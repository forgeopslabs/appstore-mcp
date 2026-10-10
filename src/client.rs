//! The authenticated App Store Connect HTTP client.
//!
//! [`AscClient`] owns a `reqwest` client, the resolved [`Config`], and a
//! [`TokenProvider`]. Every API call goes through [`AscClient::request`], which
//! injects the bearer token, sends the request, retries the failures worth
//! retrying (see [`crate::retry`]), and maps non-2xx responses into
//! [`AscError::Api`] with parsed JSON:API error details.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use reqwest::{header::RETRY_AFTER, Client, Method, RequestBuilder, Response, Url};
use serde_json::{json, Value};

use crate::auth::TokenProvider;
use crate::config::Config;
use crate::error::{api_error, AscError};
use crate::retry;

/// Hard ceiling on `max_pages`, so a runaway auto-paginating call can't walk a
/// collection of tens of thousands of resources.
pub const MAX_PAGES_LIMIT: u32 = 20;

/// Authenticated client for `https://api.appstoreconnect.apple.com`.
pub struct AscClient {
    /// Public within the crate so the upload workflow (a separate module) can
    /// issue un-authenticated `PUT`s to Apple's pre-signed upload URLs.
    pub(crate) http: Client,
    pub(crate) config: Arc<Config>,
    auth: TokenProvider,
}

impl AscClient {
    /// Build a client from configuration. Construction never performs I/O or
    /// validates credentials, so the server can start without them.
    pub fn new(config: Config) -> Self {
        let config = Arc::new(config);
        let mut builder =
            Client::builder().user_agent(concat!("appstore-mcp/", env!("CARGO_PKG_VERSION")));
        // Without these a stalled connection hangs the tool call forever, and an
        // MCP client on stdio has no way to cancel it.
        if let Some(t) = config.http.connect_timeout {
            builder = builder.connect_timeout(t);
        }
        if let Some(t) = config.http.request_timeout {
            builder = builder.timeout(t);
        }
        let http = builder.build().expect("failed to build reqwest client");
        let auth = TokenProvider::new(config.clone());
        Self { http, config, auth }
    }

    /// Perform an authenticated JSON request against the API.
    ///
    /// `path` may be a server-relative path (`"/v1/apps"`, `"v2/inAppPurchases/{id}"`)
    /// or an absolute URL. `query` is a list of pre-stringified key/value pairs;
    /// `body` is an optional JSON:API request document.
    ///
    /// Returns the parsed response body (`Value::Null` for empty 2xx bodies, e.g.
    /// `204 No Content` from a `DELETE`).
    pub async fn request(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
        body: Option<Value>,
    ) -> Result<Value, AscError> {
        let url = self.resolve_url(path)?;
        let idempotent = retry::is_idempotent(&method);

        let response = self
            .send_with_retry(idempotent, &url, || async {
                let token = self.auth.token().await?;
                let mut req = self.http.request(method.clone(), &url).bearer_auth(token);
                if !query.is_empty() {
                    req = req.query(query);
                }
                if let Some(b) = &body {
                    req = req.json(b);
                }
                Ok(req)
            })
            .await?;

        let status = response.status();
        let text = response.text().await?;

        if status.is_success() {
            if text.trim().is_empty() {
                Ok(Value::Null)
            } else {
                serde_json::from_str(&text).map_err(|e| AscError::Parse(e.to_string()))
            }
        } else {
            Err(api_error(status.as_u16(), &text))
        }
    }

    /// Convenience: authenticated `GET`.
    pub async fn get(&self, path: &str, query: &[(String, String)]) -> Result<Value, AscError> {
        self.request(Method::GET, path, query, None).await
    }

    /// Convenience: authenticated `POST` with a JSON:API body.
    pub async fn post(&self, path: &str, body: Value) -> Result<Value, AscError> {
        self.request(Method::POST, path, &[], Some(body)).await
    }

    /// Convenience: authenticated `PATCH` with a JSON:API body.
    pub async fn patch(&self, path: &str, body: Value) -> Result<Value, AscError> {
        self.request(Method::PATCH, path, &[], Some(body)).await
    }

    /// Convenience: authenticated `DELETE`.
    pub async fn delete(&self, path: &str) -> Result<Value, AscError> {
        self.request(Method::DELETE, path, &[], None).await
    }

    /// `GET` a collection, following `links.next` up to `max_pages` times.
    ///
    /// Walking pages here instead of returning a cursor to the agent turns an
    /// N-turn conversation into one tool call. The merged document keeps the
    /// last page's `links.next`, so a caller can always pick up where it stopped.
    pub async fn get_paged(
        &self,
        path: &str,
        query: &[(String, String)],
        max_pages: u32,
    ) -> Result<Value, AscError> {
        let mut merged = self.get(path, query).await?;
        let budget = max_pages.clamp(1, MAX_PAGES_LIMIT);
        let mut pages = 1;
        // Built once from the first page and carried across the rest. Rebuilding
        // it inside each merge re-hashed every resource accumulated so far, so
        // deduping twenty pages cost O(pages²) allocations.
        let mut seen = included_keys(&merged);

        while pages < budget {
            let Some(next) = next_link(&merged) else {
                break;
            };
            let page = self.get(&next, &[]).await?;
            merge_page(&mut merged, page, &mut seen);
            pages += 1;
        }

        let has_more = next_link(&merged).is_some();
        if let Some(obj) = merged.as_object_mut() {
            let meta = obj.entry("meta").or_insert_with(|| json!({}));
            if let Some(meta) = meta.as_object_mut() {
                meta.insert("pagesFetched".into(), json!(pages));
                meta.insert("hasMore".into(), json!(has_more));
            }
        }
        Ok(merged)
    }

    /// `GET` an absolute URL **without** the bearer token, returning raw bytes.
    ///
    /// Analytics report segments live at pre-signed URLs that reject requests
    /// carrying an unexpected `Authorization` header. Uses the bulk-transfer
    /// timeout, since report files are far larger than an API response.
    ///
    /// Returns [`Bytes`] rather than `Vec<u8>` so the body is handed on without
    /// being copied — these are whole report files, not API responses.
    pub async fn download_unauthenticated(&self, url: &str) -> Result<Bytes, AscError> {
        if !url.starts_with("https://") && !url.starts_with("http://") {
            return Err(AscError::InvalidRequest(format!(
                "expected an absolute http(s) URL to download, got '{url}'"
            )));
        }
        let response = self
            .send_with_retry(true, url, || async {
                Ok(self.with_transfer_timeout(self.http.get(url)))
            })
            .await?;

        let status = response.status();
        let bytes = response.bytes().await?;
        if !status.is_success() {
            return Err(AscError::Api {
                status: status.as_u16(),
                errors: Vec::new(),
                raw: Some(String::from_utf8_lossy(&bytes).into_owned()),
            });
        }
        Ok(bytes)
    }

    /// Apply the bulk-transfer timeout to a request that moves a whole file.
    pub(crate) fn with_transfer_timeout(&self, req: RequestBuilder) -> RequestBuilder {
        match self.config.http.transfer_timeout {
            Some(t) => req.timeout(t),
            None => req,
        }
    }

    /// Send a request, rebuilding it for each attempt, and retry the failures
    /// [`crate::retry`] considers safe for this method.
    ///
    /// `build` is a closure rather than a prepared request because a
    /// `RequestBuilder` is consumed by `send()` and a body may not be cloneable.
    pub(crate) async fn send_with_retry<F, Fut>(
        &self,
        idempotent: bool,
        url: &str,
        build: F,
    ) -> Result<Response, AscError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<RequestBuilder, AscError>>,
    {
        let max_retries = self.config.http.max_retries;
        let mut attempt = 0u32;

        loop {
            match build().await?.send().await {
                Ok(response) => {
                    let status = response.status().as_u16();
                    if attempt >= max_retries || !retry::should_retry_status(status, idempotent) {
                        return Ok(response);
                    }
                    attempt += 1;
                    let after = retry::parse_retry_after(
                        response
                            .headers()
                            .get(RETRY_AFTER)
                            .and_then(|v| v.to_str().ok()),
                    );
                    let delay = retry::retry_delay(&self.config.http, attempt, after);
                    tracing::warn!(
                        url,
                        status,
                        attempt,
                        delay_ms = delay.as_millis() as u64,
                        "App Store Connect returned a retryable status; retrying"
                    );
                    sleep(delay).await;
                }
                Err(err) => {
                    if attempt >= max_retries || !retry::should_retry_transport(&err, idempotent) {
                        return Err(err.into());
                    }
                    attempt += 1;
                    let delay = retry::retry_delay(&self.config.http, attempt, None);
                    tracing::warn!(
                        url,
                        attempt,
                        delay_ms = delay.as_millis() as u64,
                        error = %err,
                        "transport failure; retrying"
                    );
                    sleep(delay).await;
                }
            }
        }
    }

    /// Join a relative path onto the configured base URL, or accept an absolute
    /// URL **only when it points at the configured host**.
    ///
    /// Absolute URLs are accepted because the API hands them back: `links.next`
    /// on a collection response is absolute, and callers pass it through as a
    /// cursor. They are *checked* because [`AscClient::request`] attaches the
    /// bearer token to whatever this returns, and `path` can come from tool
    /// input. Without the check, a caller-supplied URL naming any host would be
    /// sent a live App Store Connect token.
    ///
    /// The comparison is same-origin — scheme, host and effective port — against
    /// `base_url`, so `ASC_BASE_URL` keeps working for a proxy or a test server.
    fn resolve_url(&self, path: &str) -> Result<String, AscError> {
        if !(path.starts_with("http://") || path.starts_with("https://")) {
            return Ok(format!(
                "{}/{}",
                self.config.base_url.trim_end_matches('/'),
                path.trim_start_matches('/')
            ));
        }

        let candidate = Url::parse(path)
            .map_err(|e| AscError::InvalidRequest(format!("could not parse URL '{path}': {e}")))?;
        let base = Url::parse(&self.config.base_url).map_err(|e| {
            AscError::InvalidRequest(format!(
                "configured base URL '{}' is not a valid URL: {e}",
                self.config.base_url
            ))
        })?;

        if same_origin(&candidate, &base) {
            Ok(path.to_string())
        } else {
            Err(AscError::InvalidRequest(format!(
                "refusing to send an authenticated request to '{}': only the configured \
                 host '{}' is allowed. Absolute URLs are accepted solely so that pagination \
                 cursors returned by the API can be followed.",
                candidate
                    .host_str()
                    .map(|h| h.to_string())
                    .unwrap_or_else(|| "<no host>".to_string()),
                base.host_str().unwrap_or("<no host>")
            )))
        }
    }
}

/// Whether two URLs share a scheme, host and effective port.
///
/// `Url::port_or_known_default` resolves the implicit 443/80 so that
/// `https://host` and `https://host:443` compare equal.
fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

async fn sleep(delay: Duration) {
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
}

/// The `links.next` URL of a collection response, if there is another page.
fn next_link(doc: &Value) -> Option<String> {
    doc.get("links")?
        .get("next")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Fold one more page into an accumulated collection document.
///
/// `data` concatenates, `included` concatenates without repeating a resource
/// already present, and `links` is replaced so `next` always points at the page
/// after the last one fetched.
///
/// `seen` carries the identities already merged; the caller owns it across the
/// whole walk so each resource is hashed once rather than once per remaining page.
fn merge_page(merged: &mut Value, page: Value, seen: &mut HashSet<String>) {
    let Value::Object(mut page) = page else {
        return;
    };
    let Some(target) = merged.as_object_mut() else {
        return;
    };

    if let (Some(Value::Array(into)), Some(Value::Array(from))) =
        (target.get_mut("data"), page.get_mut("data"))
    {
        into.append(from);
    }

    if let Some(Value::Array(from)) = page.get_mut("included") {
        let into = target
            .entry("included")
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Value::Array(into) = into {
            for item in from.drain(..) {
                if let Some(key) = identity(&item) {
                    if !seen.insert(key) {
                        continue;
                    }
                }
                into.push(item);
            }
        }
    }

    match page.remove("links") {
        Some(links) => {
            target.insert("links".into(), links);
        }
        None => {
            target.remove("links");
        }
    }
}

/// The `type`/`id` identity of a JSON:API resource, as a single allocation.
///
/// Joined with a NUL, which cannot appear in either field, so the combined form
/// is unambiguous — and one string per resource beats a tuple of two.
fn identity(item: &Value) -> Option<String> {
    let resource_type = item.get("type")?.as_str()?;
    let id = item.get("id")?.as_str()?;
    let mut key = String::with_capacity(resource_type.len() + 1 + id.len());
    key.push_str(resource_type);
    key.push('\0');
    key.push_str(id);
    Some(key)
}

/// Every resource identity already present in a document's `included`.
fn included_keys(doc: &Value) -> HashSet<String> {
    doc.get("included")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(identity).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(ids: &[&str], next: Option<&str>) -> Value {
        let mut doc = json!({
            "data": ids.iter().map(|id| json!({ "type": "apps", "id": id })).collect::<Vec<_>>(),
            "links": { "self": "https://api.example.com/v1/apps" },
        });
        if let Some(next) = next {
            doc["links"]["next"] = json!(next);
        }
        doc
    }

    #[test]
    fn next_link_is_read_only_when_present_and_non_empty() {
        assert_eq!(
            next_link(&page(&["1"], Some("https://x/page2"))).as_deref(),
            Some("https://x/page2")
        );
        assert_eq!(next_link(&page(&["1"], None)), None);
        assert_eq!(next_link(&json!({ "links": { "next": "" } })), None);
        assert_eq!(next_link(&json!({ "data": [] })), None);
    }

    /// Merge one page the way [`AscClient::get_paged`] does: seeding the seen-set
    /// from the accumulated document first.
    fn merge_one(merged: &mut Value, page: Value) {
        let mut seen = included_keys(merged);
        merge_page(merged, page, &mut seen);
    }

    #[test]
    fn merge_page_concatenates_data_and_advances_links() {
        let mut merged = page(&["1", "2"], Some("https://x/page2"));
        merge_one(&mut merged, page(&["3"], Some("https://x/page3")));
        let ids: Vec<&str> = merged["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["1", "2", "3"]);
        assert_eq!(merged["links"]["next"], "https://x/page3");
    }

    #[test]
    fn merge_page_clears_next_on_the_final_page() {
        let mut merged = page(&["1"], Some("https://x/page2"));
        merge_one(&mut merged, page(&["2"], None));
        assert!(next_link(&merged).is_none(), "stale next link kept");
    }

    #[test]
    fn identity_cannot_confuse_two_different_resources() {
        // A naive concatenation would make ("ab", "c") and ("a", "bc") collide.
        let a = identity(&json!({ "type": "ab", "id": "c" }));
        let b = identity(&json!({ "type": "a", "id": "bc" }));
        assert!(a.is_some() && b.is_some());
        assert_ne!(a, b);
        assert_eq!(identity(&json!({ "type": "builds" })), None);
        assert_eq!(identity(&json!({ "id": "b1" })), None);
    }

    #[test]
    fn the_seen_set_carries_across_every_page_not_just_the_last() {
        // The set is built once in `get_paged` and threaded through, so a
        // resource first seen on page 1 must still be recognised on page 3.
        let mut merged = json!({ "data": [], "included": [{ "type": "builds", "id": "b1" }] });
        let mut seen = included_keys(&merged);
        merge_page(
            &mut merged,
            json!({ "data": [], "included": [{ "type": "builds", "id": "b2" }] }),
            &mut seen,
        );
        merge_page(
            &mut merged,
            json!({ "data": [], "included": [{ "type": "builds", "id": "b1" }] }),
            &mut seen,
        );
        let ids: Vec<&str> = merged["included"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["b1", "b2"], "page 1's resource reappeared on page 3");
    }

    #[test]
    fn merge_page_does_not_repeat_included_resources() {
        let mut merged = json!({
            "data": [], "included": [{ "type": "builds", "id": "b1" }]
        });
        merge_one(
            &mut merged,
            json!({
                "data": [],
                "included": [{ "type": "builds", "id": "b1" }, { "type": "builds", "id": "b2" }]
            }),
        );
        let included = merged["included"].as_array().unwrap();
        assert_eq!(
            included.len(),
            2,
            "duplicate included resource: {included:?}"
        );
    }

    fn test_client() -> AscClient {
        AscClient::new(
            Config::from_parts(None, None, None).with_base_url("https://api.example.com/"),
        )
    }

    #[test]
    fn resolve_url_joins_relative_paths_onto_the_base() {
        let client = test_client();
        assert_eq!(
            client.resolve_url("/v1/apps").unwrap(),
            "https://api.example.com/v1/apps"
        );
        assert_eq!(
            client.resolve_url("v2/inAppPurchases/1").unwrap(),
            "https://api.example.com/v2/inAppPurchases/1"
        );
    }

    #[test]
    fn resolve_url_accepts_an_absolute_url_on_the_configured_host() {
        // This is what makes pagination work: `links.next` comes back absolute.
        let client = test_client();
        let cursor = "https://api.example.com/v1/apps?cursor=x";
        assert_eq!(client.resolve_url(cursor).unwrap(), cursor);
        // Explicit default port is the same origin as an implicit one.
        assert!(client
            .resolve_url("https://api.example.com:443/v1/apps")
            .is_ok());
    }

    #[test]
    fn resolve_url_refuses_to_send_the_token_to_another_host() {
        let client = test_client();
        for hostile in [
            "https://other.example.com/v1/apps?cursor=x",
            "https://api.example.com.evil.test/v1/apps",
            "https://evil.test/?x=https://api.example.com",
            "http://api.example.com/v1/apps", // scheme downgrade
            "https://api.example.com:8443/v1/apps", // different port
            "http://127.0.0.1:1/",
        ] {
            let err = match client.resolve_url(hostile) {
                Ok(url) => panic!(
                    "resolve_url must reject '{hostile}', but it returned '{url}' \
                     — that result is sent a bearer token"
                ),
                Err(e) => e,
            };
            assert!(
                matches!(err, AscError::InvalidRequest(_)),
                "expected InvalidRequest for {hostile}, got {err:?}"
            );
        }
    }

    // ---- Against a mock API ------------------------------------------------
    //
    // These cover what the pure unit tests above cannot: that a request is
    // actually replayed (or actually isn't), that the bearer token goes on
    // (or stays off), and that Apple's error envelope survives the round trip.

    mod http {
        use super::*;
        use crate::config::HttpConfig;
        use crate::testing::{fast_retries, test_client, test_config};
        use std::time::Duration;
        use wiremock::matchers::{header, header_exists, method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        #[tokio::test]
        async fn a_successful_call_carries_the_bearer_token() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/v1/apps"))
                .and(header_exists("authorization"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [] })))
                .expect(1)
                .mount(&server)
                .await;

            let value = test_client(&server.uri())
                .get("/v1/apps", &[])
                .await
                .unwrap();
            assert_eq!(value["data"], json!([]));
        }

        #[tokio::test]
        async fn query_parameters_are_sent() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/v1/apps"))
                .and(query_param("filter[bundleId]", "com.example.app"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [] })))
                .expect(1)
                .mount(&server)
                .await;

            test_client(&server.uri())
                .get(
                    "/v1/apps",
                    &[("filter[bundleId]".into(), "com.example.app".into())],
                )
                .await
                .unwrap();
        }

        #[tokio::test]
        async fn an_empty_204_body_becomes_null_rather_than_a_parse_error() {
            let server = MockServer::start().await;
            Mock::given(method("DELETE"))
                .respond_with(ResponseTemplate::new(204))
                .mount(&server)
                .await;

            let value = test_client(&server.uri())
                .delete("/v1/inAppPurchases/1")
                .await
                .unwrap();
            assert_eq!(value, Value::Null);
        }

        #[tokio::test]
        async fn apples_error_envelope_reaches_the_agent_intact() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(409).set_body_json(json!({
                    "errors": [{
                        "status": "409", "code": "ENTITY_ERROR.ATTRIBUTE.INVALID.DUPLICATE",
                        "title": "An attribute value is not acceptable",
                        "detail": "The product ID 'com.example.pro' is already in use",
                        "source": { "pointer": "/data/attributes/productId" }
                    }]
                })))
                .mount(&server)
                .await;

            let err = test_client(&server.uri())
                .post("/v2/inAppPurchases", json!({}))
                .await
                .unwrap_err();

            let AscError::Api { status, errors, .. } = err else {
                panic!("expected a parsed API error, got: {err:?}");
            };
            assert_eq!(status, 409);
            assert_eq!(errors.len(), 1);
            assert_eq!(
                errors[0].detail.as_deref(),
                Some("The product ID 'com.example.pro' is already in use")
            );
        }

        #[tokio::test]
        async fn an_unparseable_error_body_is_still_reported_verbatim() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .respond_with(
                    ResponseTemplate::new(502).set_body_string("<html>bad gateway</html>"),
                )
                .mount(&server)
                .await;

            let config = test_config(&server.uri()).with_http(fast_retries(0));
            let err = AscClient::new(config)
                .get("/v1/apps", &[])
                .await
                .unwrap_err();
            let AscError::Api { status, raw, .. } = err else {
                panic!("expected an API error, got: {err:?}");
            };
            assert_eq!(status, 502);
            assert!(raw.unwrap().contains("bad gateway"));
        }

        #[tokio::test]
        async fn a_rate_limited_request_is_retried_until_it_succeeds() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
                .up_to_n_times(2)
                .expect(2)
                .with_priority(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [] })))
                .expect(1)
                .with_priority(2)
                .mount(&server)
                .await;

            let value = test_client(&server.uri())
                .get("/v1/apps", &[])
                .await
                .unwrap();
            assert_eq!(value["data"], json!([]));
        }

        #[tokio::test]
        async fn a_rate_limited_post_is_retried_because_apple_never_applied_it() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
                .up_to_n_times(1)
                .expect(1)
                .with_priority(1)
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .respond_with(
                    ResponseTemplate::new(201).set_body_json(json!({ "data": { "id": "new" } })),
                )
                .expect(1)
                .with_priority(2)
                .mount(&server)
                .await;

            let value = test_client(&server.uri())
                .post("/v1/betaGroups", json!({}))
                .await
                .unwrap();
            assert_eq!(value["data"]["id"], "new");
        }

        #[tokio::test]
        async fn a_post_is_never_replayed_after_a_server_error() {
            // Replaying could create a second resource, and Apple permanently
            // reserves identifiers like a product ID.
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(500))
                .expect(1)
                .mount(&server)
                .await;

            let err = test_client(&server.uri())
                .post("/v2/inAppPurchases", json!({}))
                .await
                .unwrap_err();
            assert!(matches!(err, AscError::Api { status: 500, .. }));
            // `expect(1)` is verified when the server drops.
        }

        #[tokio::test]
        async fn a_get_is_replayed_after_a_server_error_up_to_the_budget() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(503))
                .expect(3) // the first attempt plus two retries
                .mount(&server)
                .await;

            let config = test_config(&server.uri()).with_http(fast_retries(2));
            let err = AscClient::new(config)
                .get("/v1/apps", &[])
                .await
                .unwrap_err();
            assert!(matches!(err, AscError::Api { status: 503, .. }));
        }

        #[tokio::test]
        async fn a_client_error_is_not_retried() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(404).set_body_json(json!({ "errors": [] })))
                .expect(1)
                .mount(&server)
                .await;

            let err = test_client(&server.uri())
                .get("/v1/apps/nope", &[])
                .await
                .unwrap_err();
            assert!(matches!(err, AscError::Api { status: 404, .. }));
        }

        #[tokio::test]
        async fn a_stalled_response_times_out_instead_of_hanging_forever() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
                .mount(&server)
                .await;

            let config = test_config(&server.uri()).with_http(HttpConfig {
                request_timeout: Some(Duration::from_millis(150)),
                ..fast_retries(0)
            });
            let started = std::time::Instant::now();
            let err = AscClient::new(config)
                .get("/v1/apps", &[])
                .await
                .unwrap_err();

            assert!(matches!(err, AscError::Http(_)), "{err:?}");
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "waited {:?}",
                started.elapsed()
            );
        }

        #[tokio::test]
        async fn paging_follows_next_and_returns_one_merged_document() {
            let server = MockServer::start().await;
            let next = format!("{}/v1/apps?cursor=PAGE2", server.uri());
            Mock::given(method("GET"))
                .and(path("/v1/apps"))
                .and(query_param("cursor", "PAGE2"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "data": [{ "type": "apps", "id": "3" }],
                    "links": {}
                })))
                .with_priority(1)
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/apps"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "data": [{ "type": "apps", "id": "1" }, { "type": "apps", "id": "2" }],
                    "links": { "next": next }
                })))
                .with_priority(2)
                .expect(1)
                .mount(&server)
                .await;

            let merged = test_client(&server.uri())
                .get_paged("/v1/apps", &[], 5)
                .await
                .unwrap();
            assert_eq!(merged["data"].as_array().unwrap().len(), 3);
            assert_eq!(merged["meta"]["pagesFetched"], 2);
            assert_eq!(merged["meta"]["hasMore"], false);
        }

        #[tokio::test]
        async fn paging_deduplicates_included_resources_across_pages() {
            // Pages routinely share sideloaded resources — the same app included
            // by every one of its builds. Returning it once per page would waste
            // most of the merged document on repeats.
            let server = MockServer::start().await;
            let next = format!("{}/v1/builds?cursor=P2", server.uri());
            Mock::given(method("GET"))
                .and(query_param("cursor", "P2"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "data": [{ "type": "builds", "id": "b2" }],
                    "included": [
                        { "type": "apps", "id": "a1" },
                        { "type": "apps", "id": "a2" }
                    ],
                    "links": {}
                })))
                .with_priority(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "data": [{ "type": "builds", "id": "b1" }],
                    "included": [{ "type": "apps", "id": "a1" }],
                    "links": { "next": next }
                })))
                .with_priority(2)
                .mount(&server)
                .await;

            let merged = test_client(&server.uri())
                .get_paged("/v1/builds", &[], 5)
                .await
                .unwrap();

            let included: Vec<&str> = merged["included"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["id"].as_str().unwrap())
                .collect();
            assert_eq!(included, ["a1", "a2"], "a1 came back twice");
            assert_eq!(merged["data"].as_array().unwrap().len(), 2);
        }

        #[tokio::test]
        async fn paging_stops_at_max_pages_and_says_there_is_more() {
            let server = MockServer::start().await;
            let next = format!("{}/v1/apps?cursor=MORE", server.uri());
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "data": [{ "type": "apps", "id": "1" }],
                    "links": { "next": next }
                })))
                .expect(1)
                .mount(&server)
                .await;

            let merged = test_client(&server.uri())
                .get_paged("/v1/apps", &[], 1)
                .await
                .unwrap();
            assert_eq!(merged["meta"]["pagesFetched"], 1);
            assert_eq!(merged["meta"]["hasMore"], true, "the caller can resume");
        }

        #[tokio::test]
        async fn paging_is_bounded_even_if_the_api_never_stops_offering_pages() {
            let server = MockServer::start().await;
            let next = format!("{}/v1/apps?cursor=FOREVER", server.uri());
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "data": [{ "type": "apps", "id": "1" }],
                    "links": { "next": next }
                })))
                .expect(MAX_PAGES_LIMIT as u64)
                .mount(&server)
                .await;

            let merged = test_client(&server.uri())
                .get_paged("/v1/apps", &[], u32::MAX)
                .await
                .unwrap();
            assert_eq!(merged["meta"]["pagesFetched"], MAX_PAGES_LIMIT);
        }

        #[tokio::test]
        async fn a_presigned_download_carries_no_authorization_header() {
            // Apple's pre-signed URLs reject requests with an unexpected
            // Authorization header, so this must not inherit the bearer token.
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/segment.csv.gz"))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(b"col\n1\n".to_vec()))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(header_exists("authorization"))
                .respond_with(ResponseTemplate::new(403))
                .with_priority(1)
                .expect(0)
                .mount(&server)
                .await;

            let bytes = test_client(&server.uri())
                .download_unauthenticated(&format!("{}/segment.csv.gz", server.uri()))
                .await
                .unwrap();
            assert_eq!(bytes, b"col\n1\n".to_vec());
        }

        #[tokio::test]
        async fn a_failed_download_reports_the_status() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(header("x-none", "x"))
                .respond_with(ResponseTemplate::new(200))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(403).set_body_string("expired"))
                .mount(&server)
                .await;

            let config = test_config(&server.uri()).with_http(fast_retries(0));
            let err = AscClient::new(config)
                .download_unauthenticated(&format!("{}/segment.csv.gz", server.uri()))
                .await
                .unwrap_err();
            assert!(matches!(err, AscError::Api { status: 403, .. }), "{err:?}");
        }

        #[tokio::test]
        async fn a_relative_download_url_is_rejected_before_any_request() {
            let err = test_client("http://127.0.0.1:1")
                .download_unauthenticated("/v1/segments/1")
                .await
                .unwrap_err();
            assert!(matches!(err, AscError::InvalidRequest(_)), "{err:?}");
        }

        #[tokio::test]
        async fn a_hostile_absolute_url_is_refused_before_a_token_is_sent() {
            // Two live servers: the configured API, and a host a caller names.
            // The credentials sign, so if validation ran after token creation
            // or dispatch, the hostile server would see a request.
            let api = MockServer::start().await;
            let hostile = MockServer::start().await;
            for server in [&api, &hostile] {
                Mock::given(wiremock::matchers::any())
                    .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [] })))
                    .expect(0)
                    .mount(server)
                    .await;
            }
            let client = test_client(&api.uri());
            let target = format!("{}/v1/apps?cursor=x", hostile.uri());

            let err = client.get(&target, &[]).await.unwrap_err();
            assert!(matches!(err, AscError::InvalidRequest(_)), "{err:?}");

            let err = client
                .request(Method::POST, &target, &[], Some(json!({ "data": {} })))
                .await
                .unwrap_err();
            assert!(matches!(err, AscError::InvalidRequest(_)), "{err:?}");

            assert!(
                hostile.received_requests().await.unwrap().is_empty(),
                "the hostile host was contacted"
            );
            assert!(api.received_requests().await.unwrap().is_empty());
        }
    }
}
