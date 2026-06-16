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

use md5::{Digest, Md5};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Method;
use serde_json::{json, Value};

use crate::client::AscClient;
use crate::error::AscError;

impl AscClient {
    /// Run the full reserve → upload → commit flow for an asset.
    ///
    /// * `collection_path` — the reservation collection, e.g. `"/v1/appScreenshots"`.
    /// * `resource_type` — the JSON:API `type`, e.g. `"appScreenshots"`.
    /// * `relationships` — the parent relationship object, e.g.
    ///   `{"appScreenshotSet": {"data": {"type": "appScreenshotSets", "id": "..."}}}`.
    /// * `file_path` — local path to the asset file to upload.
    ///
    /// Returns the committed resource JSON.
    pub async fn upload_asset(
        &self,
        collection_path: &str,
        resource_type: &str,
        relationships: Value,
        file_path: &str,
    ) -> Result<Value, AscError> {
        let bytes = tokio::fs::read(file_path)
            .await
            .map_err(|e| AscError::Upload(format!("cannot read asset file '{file_path}': {e}")))?;
        if bytes.is_empty() {
            return Err(AscError::Upload(format!(
                "asset file '{file_path}' is empty"
            )));
        }

        let file_name = std::path::Path::new(file_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("asset")
            .to_string();
        let checksum = md5_hex(&bytes);

        // 1. Reserve.
        let reserve_body = json!({
            "data": {
                "type": resource_type,
                "attributes": { "fileName": file_name, "fileSize": bytes.len() },
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
            self.put_upload_operation(op, &bytes).await?;
        }

        // 3. Commit.
        let commit_path = format!("{}/{}", collection_path.trim_end_matches('/'), id);
        let commit_body = json!({
            "data": {
                "type": resource_type,
                "id": id,
                "attributes": { "uploaded": true, "sourceFileChecksum": checksum },
            }
        });
        self.patch(&commit_path, commit_body).await
    }

    /// Execute a single `uploadOperations` entry: a raw `PUT` of one byte range.
    async fn put_upload_operation(&self, op: &Value, bytes: &[u8]) -> Result<(), AscError> {
        let url = op["url"]
            .as_str()
            .ok_or_else(|| AscError::Upload("upload operation missing 'url'".into()))?;
        let offset = op["offset"].as_u64().unwrap_or(0) as usize;
        let length = op["length"].as_u64().unwrap_or(bytes.len() as u64) as usize;
        let method = op["method"].as_str().unwrap_or("PUT");

        let end = offset.saturating_add(length).min(bytes.len());
        if offset > bytes.len() {
            return Err(AscError::Upload(format!(
                "upload operation offset {offset} exceeds file size {}",
                bytes.len()
            )));
        }
        let chunk = bytes[offset..end].to_vec();

        let mut headers = HeaderMap::new();
        if let Some(arr) = op["requestHeaders"].as_array() {
            for h in arr {
                if let (Some(name), Some(value)) = (h["name"].as_str(), h["value"].as_str()) {
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
            }
        }

        let method = Method::from_bytes(method.as_bytes()).unwrap_or(Method::PUT);
        // Note: deliberately NO bearer auth — the URL is pre-signed.
        let resp = self
            .http
            .request(method, url)
            .headers(headers)
            .body(chunk)
            .send()
            .await
            .map_err(|e| AscError::Upload(format!("chunk upload request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AscError::Upload(format!(
                "chunk upload failed with HTTP {status}: {body}"
            )));
        }
        Ok(())
    }
}

/// Lowercase hex MD5 of the given bytes (App Store Connect's `sourceFileChecksum`).
fn md5_hex(bytes: &[u8]) -> String {
    let mut hasher = Md5::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::md5_hex;

    #[test]
    fn md5_matches_known_vector() {
        // MD5("abc") = 900150983cd24fb0d6963f7d28e17f72
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
    }
}
