//! Error types for the App Store Connect client and their mapping to MCP errors.
//!
//! A library-style error enum ([`AscError`]) carries enough structure to render
//! Apple's own JSON:API error details back to the agent, so a failed tool call
//! tells the model *why* (e.g. a duplicate `productId`) and how to fix it.

use rmcp::ErrorData as McpError;
use serde::Deserialize;
use serde_json::json;

/// A single entry from a JSON:API `errors` array.
///
/// See <https://developer.apple.com/documentation/appstoreconnectapi/errorresponse>.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorDetail {
    pub status: Option<String>,
    pub code: Option<String>,
    pub title: Option<String>,
    pub detail: Option<String>,
    /// Apple sometimes points at the offending field via `source.pointer`/`parameter`.
    #[serde(default)]
    pub source: Option<serde_json::Value>,
}

impl std::fmt::Display for ApiErrorDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let title = self.title.as_deref().unwrap_or("error");
        let detail = self.detail.as_deref().unwrap_or("");
        let code = self.code.as_deref().unwrap_or("");
        write!(f, "- [{code}] {title}")?;
        if !detail.is_empty() {
            write!(f, ": {detail}")?;
        }
        if let Some(src) = &self.source {
            write!(f, " (source: {src})")?;
        }
        Ok(())
    }
}

/// All the ways an App Store Connect operation can fail.
#[derive(Debug, thiserror::Error)]
pub enum AscError {
    /// Credentials are missing or unreadable; the message lists what to set.
    #[error("App Store Connect credentials are not configured: {0}")]
    Config(String),

    /// The JWT could not be built (bad key, signing failure).
    #[error("failed to build the App Store Connect auth token: {0}")]
    Auth(String),

    /// Network / transport failure before a response was received.
    #[error("HTTP transport error: {0}")]
    Http(#[from] reqwest::Error),

    /// The API returned a non-2xx response with parsed JSON:API error details.
    #[error("App Store Connect API returned HTTP {status}:\n{}", format_details(.errors))]
    Api {
        status: u16,
        errors: Vec<ApiErrorDetail>,
        /// Raw body, kept when it could not be parsed as a JSON:API error envelope.
        raw: Option<String>,
    },

    /// The caller supplied an argument that cannot form a valid request.
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    /// An asset (screenshot/preview/image) upload step failed.
    #[error("asset upload failed: {0}")]
    Upload(String),

    /// A successful response could not be parsed as JSON.
    #[error("failed to parse API response: {0}")]
    Parse(String),

    /// Local I/O failure (reading a key or an asset file).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl AscError {
    /// Convert into an MCP tool error, choosing a code that reflects whether the
    /// agent can fix the request, and attaching structured data for clients.
    pub fn into_mcp_error(self) -> McpError {
        match self {
            AscError::Config(msg) => McpError::invalid_request(
                format!("App Store Connect credentials are not configured: {msg}"),
                None,
            ),
            AscError::InvalidRequest(msg) => McpError::invalid_params(msg, None),
            AscError::Api {
                status,
                errors,
                raw,
            } => {
                let message = format!(
                    "App Store Connect API returned HTTP {status}:\n{}",
                    format_details(&errors)
                );
                let data = json!({
                    "status": status,
                    "errors": errors.iter().map(|e| json!({
                        "status": e.status,
                        "code": e.code,
                        "title": e.title,
                        "detail": e.detail,
                        "source": e.source,
                    })).collect::<Vec<_>>(),
                    "raw": raw,
                });
                // 4xx -> the agent can likely fix the request; 5xx -> server-side.
                if (400..500).contains(&status) {
                    McpError::invalid_params(message, Some(data))
                } else {
                    McpError::internal_error(message, Some(data))
                }
            }
            other => McpError::internal_error(other.to_string(), None),
        }
    }
}

/// Render a JSON:API `errors` array as a readable, multi-line block.
fn format_details(errors: &[ApiErrorDetail]) -> String {
    if errors.is_empty() {
        return "(no error details returned)".to_string();
    }
    errors
        .iter()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The JSON:API error envelope: `{ "errors": [ ... ] }`.
#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    errors: Vec<ApiErrorDetail>,
}

/// Build an [`AscError::Api`] from a status code and raw response body, parsing
/// the JSON:API `errors` array when present.
pub fn api_error(status: u16, body: &str) -> AscError {
    match serde_json::from_str::<ErrorEnvelope>(body) {
        Ok(env) if !env.errors.is_empty() => AscError::Api {
            status,
            errors: env.errors,
            raw: None,
        },
        _ => AscError::Api {
            status,
            errors: Vec::new(),
            raw: Some(body.to_string()),
        },
    }
}
