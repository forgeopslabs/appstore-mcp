//! The authenticated App Store Connect HTTP client.
//!
//! [`AscClient`] owns a `reqwest` client, the resolved [`Config`], and a
//! [`TokenProvider`]. Every API call goes through [`AscClient::request`], which
//! injects the bearer token, sends the request, and maps non-2xx responses into
//! [`AscError::Api`] with parsed JSON:API error details.

use std::sync::Arc;

use reqwest::{Client, Method};
use serde_json::Value;

use crate::auth::TokenProvider;
use crate::config::Config;
use crate::error::{api_error, AscError};

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
        let http = Client::builder()
            .user_agent(concat!("appstore-mcp/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("failed to build reqwest client");
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
        let url = self.resolve_url(path);
        let token = self.auth.token().await?;

        let mut req = self.http.request(method, url).bearer_auth(token);
        if !query.is_empty() {
            req = req.query(query);
        }
        if let Some(b) = &body {
            req = req.json(b);
        }

        let resp = req.send().await?;
        let status = resp.status();
        let text = resp.text().await?;

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

    /// Join a relative path onto the configured base URL, or pass an absolute URL through.
    fn resolve_url(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            path.to_string()
        } else {
            format!(
                "{}/{}",
                self.config.base_url.trim_end_matches('/'),
                path.trim_start_matches('/')
            )
        }
    }
}
