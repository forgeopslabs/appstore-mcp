//! Environment-based configuration for the App Store Connect MCP server.
//!
//! Reading config is intentionally **infallible** ([`Config::from_env`] never
//! errors) so the MCP server always starts and can advertise its tools. Missing
//! or invalid credentials only surface when a tool actually needs to call the
//! API, via [`Config::credentials`], which returns an actionable error.

use crate::error::AscError;

/// Default App Store Connect API origin.
pub const DEFAULT_BASE_URL: &str = "https://api.appstoreconnect.apple.com";

/// Resolved configuration captured from the process environment.
#[derive(Debug, Clone)]
pub struct Config {
    issuer_id: Option<String>,
    key_id: Option<String>,
    /// The private key PEM contents, resolved from `ASC_PRIVATE_KEY` or by
    /// reading the file at `ASC_PRIVATE_KEY_PATH`.
    private_key_pem: Option<String>,
    /// A problem encountered while resolving the key (e.g. file unreadable),
    /// surfaced lazily so it can be reported when credentials are requested.
    private_key_error: Option<String>,
    /// API origin; overridable via `ASC_BASE_URL`.
    pub base_url: String,
}

/// Borrowed, fully-validated credentials ready to mint a JWT.
pub struct Credentials<'a> {
    pub issuer_id: &'a str,
    pub key_id: &'a str,
    pub private_key_pem: &'a str,
}

impl Config {
    /// Read configuration from the environment. Never fails.
    ///
    /// Recognized variables:
    /// - `ASC_ISSUER_ID` (required) — your App Store Connect issuer UUID.
    /// - `ASC_KEY_ID` (required) — the API key ID.
    /// - `ASC_PRIVATE_KEY` — the `.p8` PEM contents, or
    /// - `ASC_PRIVATE_KEY_PATH` — a path to the `.p8` file (one of the two is required).
    /// - `ASC_BASE_URL` (optional) — overrides the API origin.
    pub fn from_env() -> Self {
        let issuer_id = non_empty(std::env::var("ASC_ISSUER_ID").ok());
        let key_id = non_empty(std::env::var("ASC_KEY_ID").ok());

        let (private_key_pem, private_key_error) = resolve_private_key();

        let base_url = non_empty(std::env::var("ASC_BASE_URL").ok())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        Self {
            issuer_id,
            key_id,
            private_key_pem,
            private_key_error,
            base_url,
        }
    }

    /// Validate and borrow the credential triple, or report exactly what's missing.
    pub fn credentials(&self) -> Result<Credentials<'_>, AscError> {
        let mut missing = Vec::new();
        if self.issuer_id.is_none() {
            missing.push("ASC_ISSUER_ID");
        }
        if self.key_id.is_none() {
            missing.push("ASC_KEY_ID");
        }
        if self.private_key_pem.is_none() {
            if let Some(err) = &self.private_key_error {
                return Err(AscError::Config(err.clone()));
            }
            missing.push("ASC_PRIVATE_KEY or ASC_PRIVATE_KEY_PATH");
        }

        if !missing.is_empty() {
            return Err(AscError::Config(format!(
                "set the following environment variable(s): {}",
                missing.join(", ")
            )));
        }

        Ok(Credentials {
            issuer_id: self.issuer_id.as_deref().unwrap(),
            key_id: self.key_id.as_deref().unwrap(),
            private_key_pem: self.private_key_pem.as_deref().unwrap(),
        })
    }

    /// Whether the required credentials appear to be present (for diagnostics).
    pub fn is_configured(&self) -> bool {
        self.credentials().is_ok()
    }
}

/// Resolve the PEM either from `ASC_PRIVATE_KEY` (inline) or `ASC_PRIVATE_KEY_PATH` (file).
fn resolve_private_key() -> (Option<String>, Option<String>) {
    if let Some(pem) = non_empty(std::env::var("ASC_PRIVATE_KEY").ok()) {
        return (Some(pem), None);
    }
    if let Some(path) = non_empty(std::env::var("ASC_PRIVATE_KEY_PATH").ok()) {
        return match std::fs::read_to_string(&path) {
            Ok(pem) => (Some(pem), None),
            Err(e) => (
                None,
                Some(format!(
                    "could not read ASC_PRIVATE_KEY_PATH ('{path}'): {e}"
                )),
            ),
        };
    }
    (None, None)
}

/// Treat empty/whitespace-only env values as absent.
fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
impl Config {
    /// Build a config directly from explicit parts, bypassing the environment.
    /// Avoids env-var races between parallel tests.
    pub(crate) fn from_parts(
        issuer_id: Option<&str>,
        key_id: Option<&str>,
        private_key_pem: Option<&str>,
    ) -> Self {
        Self {
            issuer_id: issuer_id.map(String::from),
            key_id: key_id.map(String::from),
            private_key_pem: private_key_pem.map(String::from),
            private_key_error: None,
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }
}
