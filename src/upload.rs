//! The App Store Connect asset-upload workflow.
//!
//! Screenshots, app previews, and in-app-purchase images all follow the same
//! three-step protocol:
//!
//! 1. **Reserve** — `POST` the collection with `{ fileName, fileSize }` plus the
//!    parent relationship. The response carries an `id` and an
//!    `uploadOperations` array.
//! 2. **Upload** — for each operation, `PUT` the byte slice `[offset, offset+length)`
//!    directly to Apple's pre-signed URL, with the operation's `requestHeaders`
//!    and *no* `Authorization` header.
//! 3. **Commit** — `PATCH` the resource with `{ uploaded: true, sourceFileChecksum }`
//!    where the checksum is the MD5 of the whole file.
//!
//! App Asset Library images and videos (API 4.5.1) use the same protocol except
//! that the commit carries `{ uploaded: true }` alone: their update request has
//! no checksum attribute, and Apple rejects attributes a resource doesn't have.
//!
//! The file is never held in memory in full: the checksum is computed by
//! streaming, and each operation reads only its own byte range. Peak memory is
//! one chunk — the size Apple chose — rather than the size of the asset. A
//! failed chunk is retried on its own, so one flaky `PUT` doesn't discard an
//! upload that is otherwise complete.

use bytes::Bytes;
use md5::{Digest, Md5};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Method;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::client::AscClient;
use crate::error::AscError;

/// Block size for streaming the file while checksumming it.
const CHECKSUM_BLOCK: usize = 1024 * 1024;

impl AscClient {
    /// Run the full reserve → upload → commit flow for an asset.
    ///
    /// * `collection_path` — the reservation collection, e.g. `"/v1/appScreenshots"`.
    /// * `resource_type` — the JSON:API `type`, e.g. `"appScreenshots"`.
    /// * `extra_attributes` — additional reservation attributes (a JSON object) merged
    ///   into `{ fileName, fileSize }`. Pass `serde_json::json!({})` when none are needed.
    /// * `relationships` — the parent relationship object, e.g.
    ///   `{"appScreenshotSet": {"data": {"type": "appScreenshotSets", "id": "..."}}}`.
    /// * `file_path` — local path to the asset file to upload.
    ///
    /// Returns the committed resource JSON.
    pub async fn upload_asset(
        &self,
        collection_path: &str,
        resource_type: &str,
        extra_attributes: Value,
        relationships: Value,
        file_path: &str,
    ) -> Result<Value, AscError> {
        self.reserve_upload_commit(
            collection_path,
            resource_type,
            extra_attributes,
            relationships,
            file_path,
            true,
        )
        .await
    }

    /// The same flow for App Asset Library images and videos, whose commit is
    /// `{ uploaded: true }` with no checksum.
    pub async fn upload_library_asset(
        &self,
        collection_path: &str,
        resource_type: &str,
        extra_attributes: Value,
        relationships: Value,
        file_path: &str,
    ) -> Result<Value, AscError> {
        self.reserve_upload_commit(
            collection_path,
            resource_type,
            extra_attributes,
            relationships,
            file_path,
            false,
        )
        .await
    }

    async fn reserve_upload_commit(
        &self,
        collection_path: &str,
        resource_type: &str,
        extra_attributes: Value,
        relationships: Value,
        file_path: &str,
        send_checksum: bool,
    ) -> Result<Value, AscError> {
        let metadata = tokio::fs::metadata(file_path)
            .await
            .map_err(|e| AscError::Upload(format!("cannot read asset file '{file_path}': {e}")))?;
        let file_size = metadata.len();
        if file_size == 0 {
            return Err(AscError::Upload(format!(
                "asset file '{file_path}' is empty"
            )));
        }

        let file_name = std::path::Path::new(file_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("asset")
            .to_string();
        let checksum = if send_checksum {
            Some(md5_file(file_path).await?)
        } else {
            None
        };

        // 1. Reserve — build attributes from { fileName, fileSize } merged with any extras.
        if !extra_attributes.is_null() && !extra_attributes.is_object() {
            return Err(AscError::Upload(format!(
                "extra_attributes must be a JSON object or null, got: {extra_attributes}"
            )));
        }
        let mut reserve_attrs = json!({ "fileName": file_name, "fileSize": file_size });
        // Callers must not include `fileName`/`fileSize` in extras — they would overwrite the computed values.
        if let (Some(base), Some(extra)) =
            (reserve_attrs.as_object_mut(), extra_attributes.as_object())
        {
            for (k, v) in extra {
                base.insert(k.clone(), v.clone());
            }
        }
        let reserve_body = json!({
            "data": {
                "type": resource_type,
                "attributes": reserve_attrs,
                "relationships": relationships,
            }
        });
        let reserved = self.post(collection_path, reserve_body).await?;

        let id = reserved["data"]["id"]
            .as_str()
            .ok_or_else(|| AscError::Upload("reservation response missing data.id".into()))?
            .to_string();
        let operations = reserved["data"]["attributes"]["uploadOperations"]
            .as_array()
            .cloned()
            .ok_or_else(|| {
                AscError::Upload("reservation response missing attributes.uploadOperations".into())
            })?;

        // 2. Upload each chunk to its pre-signed URL.
        for op in &operations {
            self.put_upload_operation(op, file_path, file_size).await?;
        }

        // 3. Commit.
        let commit_path = format!("{}/{}", collection_path.trim_end_matches('/'), id);
        let mut commit_attrs = json!({ "uploaded": true });
        if let Some(checksum) = checksum {
            commit_attrs["sourceFileChecksum"] = json!(checksum);
        }
        let commit_body = json!({
            "data": {
                "type": resource_type,
                "id": id,
                "attributes": commit_attrs,
            }
        });
        self.patch(&commit_path, commit_body).await
    }

    /// Execute a single `uploadOperations` entry: a raw `PUT` of one byte range.
    async fn put_upload_operation(
        &self,
        op: &Value,
        file_path: &str,
        file_size: u64,
    ) -> Result<(), AscError> {
        let url = op["url"]
            .as_str()
            .ok_or_else(|| AscError::Upload("upload operation missing 'url'".into()))?;
        let range = chunk_range(op, file_size)?;
        let method = Method::from_bytes(op["method"].as_str().unwrap_or("PUT").as_bytes())
            .unwrap_or(Method::PUT);
        let headers = operation_headers(op)?;
        // `Bytes` rather than `Vec<u8>`: the retry closure has to hand the body
        // over on every attempt, and cloning a `Vec` would copy the whole chunk
        // even on the first, successful try.
        let chunk = Bytes::from(read_range(file_path, range.0, range.1).await?);

        // A pre-signed PUT of a fixed byte range is idempotent, so a flaky chunk
        // can be replayed without corrupting the assembled file.
        let response = self
            .send_with_retry(true, url, || async {
                // Note: deliberately NO bearer auth — the URL is pre-signed.
                Ok(self.with_transfer_timeout(
                    self.http
                        .request(method.clone(), url)
                        .headers(headers.clone())
                        .body(chunk.clone()),
                ))
            })
            .await
            .map_err(|e| AscError::Upload(format!("chunk upload request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AscError::Upload(format!(
                "chunk upload failed with HTTP {status}: {body}"
            )));
        }
        Ok(())
    }
}

/// The `[offset, end)` byte range an upload operation covers, validated against
/// the file it will be read from.
fn chunk_range(op: &Value, file_size: u64) -> Result<(u64, u64), AscError> {
    let offset = op["offset"].as_u64().unwrap_or(0);
    let length = op["length"].as_u64().unwrap_or(file_size);
    if offset > file_size {
        return Err(AscError::Upload(format!(
            "upload operation offset {offset} exceeds file size {file_size}"
        )));
    }
    Ok((offset, offset.saturating_add(length).min(file_size)))
}

/// Translate an operation's `requestHeaders` into a `HeaderMap`.
fn operation_headers(op: &Value) -> Result<HeaderMap, AscError> {
    let mut headers = HeaderMap::new();
    let Some(entries) = op["requestHeaders"].as_array() else {
        return Ok(headers);
    };
    for h in entries {
        let (Some(name), Some(value)) = (h["name"].as_str(), h["value"].as_str()) else {
            continue;
        };
        match (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            (Ok(n), Ok(v)) => {
                headers.insert(n, v);
            }
            _ => {
                return Err(AscError::Upload(format!(
                    "invalid upload header returned by API: {name}: {value}"
                )))
            }
        }
    }
    Ok(headers)
}

/// Read `[start, end)` from a file without loading the rest of it.
async fn read_range(path: &str, start: u64, end: u64) -> Result<Vec<u8>, AscError> {
    let len = end.saturating_sub(start) as usize;
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| AscError::Upload(format!("cannot read asset file '{path}': {e}")))?;
    file.seek(std::io::SeekFrom::Start(start))
        .await
        .map_err(|e| AscError::Upload(format!("cannot seek '{path}' to {start}: {e}")))?;
    let mut buf = vec![0u8; len];
    file.read_exact(&mut buf)
        .await
        .map_err(|e| AscError::Upload(format!("cannot read {len} bytes of '{path}': {e}")))?;
    Ok(buf)
}

/// Lowercase hex MD5 of a file's contents (App Store Connect's `sourceFileChecksum`),
/// computed a block at a time so the file is never fully resident.
async fn md5_file(path: &str) -> Result<String, AscError> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| AscError::Upload(format!("cannot read asset file '{path}': {e}")))?;
    let mut hasher = Md5::new();
    let mut buf = vec![0u8; CHECKSUM_BLOCK];
    loop {
        let read = file
            .read(&mut buf)
            .await
            .map_err(|e| AscError::Upload(format!("cannot read asset file '{path}': {e}")))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lowercase hex MD5 of a whole buffer — the reference the streaming
    /// implementation has to agree with.
    fn md5_hex(bytes: &[u8]) -> String {
        let mut hasher = Md5::new();
        hasher.update(bytes);
        hex::encode(hasher.finalize())
    }

    fn temp_file(name: &str, bytes: &[u8]) -> String {
        let path = std::env::temp_dir().join(format!("appstore-mcp-test-{name}"));
        std::fs::write(&path, bytes).expect("write temp file");
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn md5_matches_known_vector() {
        // MD5("abc") = 900150983cd24fb0d6963f7d28e17f72
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[tokio::test]
    async fn streamed_checksum_matches_the_whole_file_checksum() {
        // Spans several read blocks, so a bug in the loop would show up here.
        let bytes: Vec<u8> = (0..(CHECKSUM_BLOCK * 2 + 12345))
            .map(|i| (i % 251) as u8)
            .collect();
        let path = temp_file("checksum", &bytes);
        assert_eq!(md5_file(&path).await.unwrap(), md5_hex(&bytes));
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn read_range_returns_only_the_requested_slice() {
        let path = temp_file("range", b"0123456789");
        assert_eq!(read_range(&path, 3, 7).await.unwrap(), b"3456".to_vec());
        assert_eq!(
            read_range(&path, 0, 10).await.unwrap(),
            b"0123456789".to_vec()
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn chunk_range_defaults_to_the_whole_file() {
        assert_eq!(chunk_range(&json!({}), 500).unwrap(), (0, 500));
    }

    #[test]
    fn chunk_range_clamps_a_length_past_the_end() {
        assert_eq!(
            chunk_range(&json!({ "offset": 400, "length": 500 }), 500).unwrap(),
            (400, 500)
        );
    }

    #[test]
    fn chunk_range_rejects_an_offset_past_the_end() {
        let err = chunk_range(&json!({ "offset": 900, "length": 10 }), 500).unwrap_err();
        assert!(err.to_string().contains("exceeds file size"), "{err}");
    }

    // ---- The whole reserve → upload → commit protocol, against a mock API ----

    mod protocol {
        use super::*;
        use crate::client::AscClient;
        use crate::testing::{fast_retries, test_client, test_config};
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        const CONTENT: &[u8] = b"0123456789";

        /// Reserve a screenshot in two chunks pointed at the mock server.
        async fn mock_reserve(server: &MockServer) {
            let base = server.uri();
            Mock::given(method("POST"))
                .and(path("/v1/appScreenshots"))
                .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                    "data": {
                        "id": "scr-1",
                        "attributes": { "uploadOperations": [
                            { "method": "PUT", "url": format!("{base}/upload/0"),
                              "offset": 0, "length": 5,
                              "requestHeaders": [{ "name": "Content-Type", "value": "image/png" }] },
                            { "method": "PUT", "url": format!("{base}/upload/1"),
                              "offset": 5, "length": 5, "requestHeaders": [] }
                        ]}
                    }
                })))
                .expect(1)
                .mount(server)
                .await;
        }

        async fn mock_commit(server: &MockServer) {
            Mock::given(method("PATCH"))
                .and(path("/v1/appScreenshots/scr-1"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "data": { "id": "scr-1", "attributes": { "assetDeliveryState": "COMPLETE" } }
                })))
                .expect(1)
                .mount(server)
                .await;
        }

        async fn upload(client: &AscClient, path: &str) -> Result<Value, AscError> {
            client
                .upload_asset(
                    "/v1/appScreenshots",
                    "appScreenshots",
                    json!({}),
                    json!({ "appScreenshotSet": { "data": { "type": "appScreenshotSets", "id": "set-1" } } }),
                    path,
                )
                .await
        }

        #[tokio::test]
        async fn each_operation_receives_exactly_its_own_byte_range() {
            let server = MockServer::start().await;
            mock_reserve(&server).await;
            mock_commit(&server).await;
            Mock::given(method("PUT"))
                .respond_with(ResponseTemplate::new(200))
                .expect(2)
                .mount(&server)
                .await;

            let file = temp_file("upload-ranges", CONTENT);
            let result = upload(&test_client(&server.uri()), &file).await.unwrap();
            assert_eq!(
                result["data"]["attributes"]["assetDeliveryState"],
                "COMPLETE"
            );

            let puts: Vec<_> = server
                .received_requests()
                .await
                .unwrap()
                .into_iter()
                .filter(|r| r.method == wiremock::http::Method::PUT)
                .collect();
            assert_eq!(puts.len(), 2);
            let mut bodies: Vec<Vec<u8>> = puts.iter().map(|r| r.body.clone()).collect();
            bodies.sort();
            assert_eq!(bodies, vec![b"01234".to_vec(), b"56789".to_vec()]);

            // Pre-signed URLs reject an unexpected Authorization header.
            for put in &puts {
                assert!(
                    !put.headers.contains_key("authorization"),
                    "chunk upload carried the bearer token"
                );
            }
            let _ = std::fs::remove_file(file);
        }

        #[tokio::test]
        async fn the_commit_sends_the_checksum_of_the_whole_file() {
            let server = MockServer::start().await;
            mock_reserve(&server).await;
            mock_commit(&server).await;
            Mock::given(method("PUT"))
                .respond_with(ResponseTemplate::new(200))
                .mount(&server)
                .await;

            let file = temp_file("upload-checksum", CONTENT);
            upload(&test_client(&server.uri()), &file).await.unwrap();

            let commit = server
                .received_requests()
                .await
                .unwrap()
                .into_iter()
                .find(|r| r.method == wiremock::http::Method::PATCH)
                .expect("commit request");
            let body: Value = serde_json::from_slice(&commit.body).unwrap();
            assert_eq!(body["data"]["attributes"]["uploaded"], true);
            assert_eq!(
                body["data"]["attributes"]["sourceFileChecksum"],
                md5_hex(CONTENT)
            );
            let _ = std::fs::remove_file(file);
        }

        #[tokio::test]
        async fn a_library_asset_commit_carries_no_checksum() {
            // appAssetLibraryImages/Videos have no sourceFileChecksum attribute,
            // and Apple rejects an attribute the resource doesn't define.
            let server = MockServer::start().await;
            let base = server.uri();
            Mock::given(method("POST"))
                .and(path("/v1/appAssetLibraryImages"))
                .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                    "data": { "id": "img-1", "attributes": { "uploadOperations": [
                        { "method": "PUT", "url": format!("{base}/upload/0"),
                          "offset": 0, "length": 10, "requestHeaders": [] }
                    ]}}
                })))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("PUT"))
                .respond_with(ResponseTemplate::new(200))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("PATCH"))
                .and(path("/v1/appAssetLibraryImages/img-1"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(json!({ "data": { "id": "img-1" } })),
                )
                .expect(1)
                .mount(&server)
                .await;

            let file = temp_file("upload-library", CONTENT);
            test_client(&server.uri())
                .upload_library_asset(
                    "/v1/appAssetLibraryImages",
                    "appAssetLibraryImages",
                    json!({ "category": "APP_SCREENSHOTS_AND_PREVIEWS" }),
                    json!({ "assetLibrary": { "data": { "type": "appAssetLibraries", "id": "lib-1" } } }),
                    &file,
                )
                .await
                .unwrap();

            let requests = server.received_requests().await.unwrap();
            let reserve: Value = serde_json::from_slice(
                &requests
                    .iter()
                    .find(|r| r.method == wiremock::http::Method::POST)
                    .unwrap()
                    .body,
            )
            .unwrap();
            assert_eq!(reserve["data"]["attributes"]["fileSize"], 10);
            assert_eq!(
                reserve["data"]["attributes"]["category"],
                "APP_SCREENSHOTS_AND_PREVIEWS"
            );
            let commit: Value = serde_json::from_slice(
                &requests
                    .iter()
                    .find(|r| r.method == wiremock::http::Method::PATCH)
                    .unwrap()
                    .body,
            )
            .unwrap();
            assert_eq!(
                commit["data"]["attributes"],
                json!({ "uploaded": true }),
                "library commit must not send sourceFileChecksum"
            );
            let _ = std::fs::remove_file(file);
        }

        #[tokio::test]
        async fn a_flaky_chunk_is_retried_rather_than_failing_the_whole_upload() {
            let server = MockServer::start().await;
            mock_reserve(&server).await;
            mock_commit(&server).await;
            Mock::given(method("PUT"))
                .and(path("/upload/0"))
                .respond_with(ResponseTemplate::new(500))
                .up_to_n_times(1)
                .with_priority(1)
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("PUT"))
                .respond_with(ResponseTemplate::new(200))
                .with_priority(2)
                .expect(2)
                .mount(&server)
                .await;

            let file = temp_file("upload-retry", CONTENT);
            upload(&test_client(&server.uri()), &file).await.unwrap();
            let _ = std::fs::remove_file(file);
        }

        #[tokio::test]
        async fn a_chunk_that_keeps_failing_reports_an_upload_error() {
            let server = MockServer::start().await;
            mock_reserve(&server).await;
            Mock::given(method("PUT"))
                .respond_with(ResponseTemplate::new(500))
                .mount(&server)
                .await;

            let file = temp_file("upload-failure", CONTENT);
            let config = test_config(&server.uri()).with_http(fast_retries(1));
            let err = upload(&AscClient::new(config), &file).await.unwrap_err();
            assert!(matches!(err, AscError::Upload(_)), "{err:?}");
            assert!(err.to_string().contains("500"), "{err}");
            let _ = std::fs::remove_file(file);
        }

        #[tokio::test]
        async fn an_empty_file_is_rejected_before_anything_is_reserved() {
            let server = MockServer::start().await;
            let file = temp_file("upload-empty", b"");
            let err = upload(&test_client(&server.uri()), &file)
                .await
                .unwrap_err();
            assert!(err.to_string().contains("empty"), "{err}");
            assert!(
                server.received_requests().await.unwrap().is_empty(),
                "reserved an upload slot for an empty file"
            );
            let _ = std::fs::remove_file(file);
        }

        #[tokio::test]
        async fn a_reservation_without_upload_operations_is_reported_clearly() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(
                    ResponseTemplate::new(201).set_body_json(json!({ "data": { "id": "scr-1" } })),
                )
                .mount(&server)
                .await;

            let file = temp_file("upload-no-ops", CONTENT);
            let err = upload(&test_client(&server.uri()), &file)
                .await
                .unwrap_err();
            assert!(err.to_string().contains("uploadOperations"), "{err}");
            let _ = std::fs::remove_file(file);
        }
    }

    #[test]
    fn operation_headers_are_translated_and_validated() {
        let headers = operation_headers(&json!({
            "requestHeaders": [{ "name": "Content-Type", "value": "image/png" }]
        }))
        .unwrap();
        assert_eq!(headers["content-type"], "image/png");

        let err = operation_headers(&json!({
            "requestHeaders": [{ "name": "Bad Header", "value": "x" }]
        }))
        .unwrap_err();
        assert!(err.to_string().contains("invalid upload header"), "{err}");
    }
}
