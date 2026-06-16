//! The MCP server: the [`AppStoreServer`] type, its combined tool router, and
//! shared helpers used by every tool module.
//!
//! Tools are organized by domain into submodules, each contributing a
//! `#[tool_router(router = <name>_router)]` impl block. [`AppStoreServer::new`]
//! sums those routers into the single [`ToolRouter`] stored on the struct, and
//! `#[tool_handler(router = self.tool_router)]` dispatches off it.

mod apps;
mod assets;
mod availability;
mod generic;
mod iap;
mod pricing;
mod provisioning;
mod submission;
mod subscriptions;
mod testflight;
mod versions;

use std::sync::Arc;

use rmcp::{
    handler::server::router::tool::ToolRouter, model::*, tool_handler, ErrorData as McpError,
    ServerHandler,
};
use serde_json::Value;

use crate::client::AscClient;
use crate::config::Config;
use crate::error::AscError;

/// Server instructions shown to MCP clients to orient the agent.
const INSTRUCTIONS: &str = "\
This server wraps the Apple App Store Connect API.

Credentials come from the environment (ASC_ISSUER_ID, ASC_KEY_ID, and either \
ASC_PRIVATE_KEY or ASC_PRIVATE_KEY_PATH). If they are unset, tools return a \
configuration error.

Coverage is hybrid:
- Curated tools exist for apps, in-app purchases, subscriptions, versions & \
  metadata, pricing, TestFlight, provisioning, and asset uploads.
- The generic tools `appstore_request` and `appstore_list` can reach ANY App \
  Store Connect endpoint (TestFlight, Game Center, finance reports, etc.) using \
  raw JSON:API documents — use them for anything without a dedicated tool.

Tips:
- IDs are opaque strings returned by list/get tools; resolve them first.
- Pricing requires a price-point ID: use the pricing tools to look them up.
- Most write operations use JSON:API bodies of the form \
  {\"data\": {\"type\": ..., \"attributes\": {...}, \"relationships\": {...}}}.";

/// The App Store Connect MCP server.
#[derive(Clone)]
pub struct AppStoreServer {
    pub(crate) client: Arc<AscClient>,
    tool_router: ToolRouter<AppStoreServer>,
}

impl AppStoreServer {
    /// Build the server from configuration, assembling all per-domain routers.
    pub fn new(config: Config) -> Self {
        Self {
            client: Arc::new(AscClient::new(config)),
            tool_router: Self::generic_router()
                + Self::apps_router()
                + Self::iap_router()
                + Self::subscriptions_router()
                + Self::versions_router()
                + Self::pricing_router()
                + Self::testflight_router()
                + Self::provisioning_router()
                + Self::assets_router()
                + Self::submission_router()
                + Self::availability_router(),
        }
    }

    /// Render a JSON value as a pretty-printed text tool result.
    pub(crate) fn ok_json(value: Value) -> Result<CallToolResult, McpError> {
        let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Map a client error into an MCP tool error.
    pub(crate) fn map_err(err: AscError) -> McpError {
        err.into_mcp_error()
    }
}

/// Flatten a JSON object of query parameters into pre-stringified pairs.
///
/// Array values are comma-joined (App Store Connect's convention for repeated
/// filters and `include`/`fields` lists); scalars are stringified.
pub(crate) fn flatten_query(map: &serde_json::Map<String, Value>) -> Vec<(String, String)> {
    map.iter()
        .map(|(k, v)| (k.clone(), value_to_query_string(v)))
        .collect()
}

fn value_to_query_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Array(arr) => arr
            .iter()
            .map(value_to_query_string)
            .collect::<Vec<_>>()
            .join(","),
        other => other.to_string(),
    }
}

/// Push `(key, value)` onto a query vector when the option is `Some`.
pub(crate) fn push_opt(query: &mut Vec<(String, String)>, key: &str, value: Option<impl ToString>) {
    if let Some(v) = value {
        query.push((key.to_string(), v.to_string()));
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AppStoreServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_instructions(INSTRUCTIONS.to_string())
    }
}
