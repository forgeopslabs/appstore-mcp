//! The MCP server: the [`AppStoreServer`] type, its combined tool router, and
//! shared helpers used by every tool module.
//!
//! Tools are organized by domain into submodules, each contributing a
//! `#[tool_router(router = <name>_router)]` impl block. [`AppStoreServer::new`]
//! sums those routers into the single [`ToolRouter`] stored on the struct, and
//! `#[tool_handler(router = self.tool_router)]` dispatches off it.

mod analytics;
mod apps;
mod assets;
mod availability;
mod custom_product_pages;
mod events;
mod generic;
mod iap;
mod offer_codes;
mod offers;
mod pricing;
mod promotions;
mod provisioning;
mod reviews;
mod submission;
mod subscriptions;
mod testflight;
mod users;
mod versions;
mod xcode_cloud;

use std::sync::Arc;

use rmcp::{
    handler::server::router::tool::ToolRouter, model::*, tool_handler, ErrorData as McpError,
    ServerHandler,
};
use serde::Deserialize;
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
- Curated tools exist for apps & metadata, in-app purchases, subscriptions \
  (incl. introductory/promotional/win-back offers and offer codes), versions & \
  metadata, pricing, availability, App Review submission, TestFlight, \
  provisioning & bundle-ID capabilities, asset uploads, promoted purchases, \
  customer reviews, phased release, users & access, in-app events, Xcode Cloud, \
  and Analytics reports.
- The generic tools `appstore_request` and `appstore_list` can reach ANY App \
  Store Connect endpoint (Game Center, App Clips, finance reports, etc.) using \
  raw JSON:API documents — use them for anything without a dedicated tool.

Tips:
- IDs are opaque strings returned by list/get tools; resolve them first.
- Pricing requires a price-point ID: use the pricing tools to look them up.
- Most write operations use JSON:API bodies of the form \
  {\"data\": {\"type\": ..., \"attributes\": {...}, \"relationships\": {...}}}.

Limitations (enforced by Apple, not this server):
- New apps CANNOT be created via the API (the `apps` resource allows only \
  GET and UPDATE). Create the app in the App Store Connect website first; you \
  can pre-create its bundle ID with create_bundle_id.
- Sales/finance reports return gzipped TSV, not JSON:API, and are not wrapped here.";

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
                + Self::analytics_router()
                + Self::apps_router()
                + Self::iap_router()
                + Self::subscriptions_router()
                + Self::versions_router()
                + Self::pricing_router()
                + Self::testflight_router()
                + Self::provisioning_router()
                + Self::assets_router()
                + Self::submission_router()
                + Self::availability_router()
                + Self::events_router()
                + Self::offers_router()
                + Self::offer_codes_router()
                + Self::promotions_router()
                + Self::reviews_router()
                + Self::users_router()
                + Self::xcode_cloud_router()
                + Self::custom_product_pages_router(),
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

/// Insert a string attribute into a JSON object only when the option is `Some`.
pub(crate) fn set_opt_str(obj: &mut serde_json::Value, key: &str, value: &Option<String>) {
    if let Some(v) = value {
        obj[key] = serde_json::json!(v);
    }
}

/// Coerce a tool-argument value that may have been sent as a *stringified* JSON
/// document back into the real parsed value.
///
/// Several MCP clients serialize object/array arguments as JSON strings (e.g.
/// they send `body` as `"{\"data\":{...}}"` instead of `{"data":{...}}`). Such a
/// string would otherwise be re-encoded as a quoted JSON *string* in the outbound
/// request and rejected by the API as "not a valid request document object".
/// Strings that look like a JSON object/array and parse successfully are replaced
/// with their parsed value; every other value — including ordinary non-JSON
/// strings and already-structured objects — passes through unchanged, so
/// faithfully-encoded arguments are never altered.
pub(crate) fn coerce_json(value: Value) -> Value {
    if let Value::String(s) = &value {
        let trimmed = s.trim_start();
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                return parsed;
            }
        }
    }
    value
}

/// `#[serde(deserialize_with)]` adapter applying [`coerce_json`] to a required
/// `Value` field.
pub(crate) fn de_coerce_json<'de, D>(deserializer: D) -> Result<Value, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(coerce_json(Value::deserialize(deserializer)?))
}

/// `#[serde(deserialize_with)]` adapter applying [`coerce_json`] to an optional
/// `Value` field. Pair with `#[serde(default)]`.
pub(crate) fn de_coerce_json_opt<'de, D>(deserializer: D) -> Result<Option<Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<Value>::deserialize(deserializer)?.map(coerce_json))
}

/// `#[serde(deserialize_with)]` adapter for an optional query/filter map that also
/// accepts a stringified JSON object. Pair with `#[serde(default)]`.
pub(crate) fn de_coerce_map_opt<'de, D>(
    deserializer: D,
) -> Result<Option<serde_json::Map<String, Value>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match Option::<Value>::deserialize(deserializer)?.map(coerce_json) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(map)) => Ok(Some(map)),
        Some(other) => Err(serde::de::Error::custom(format!(
            "expected an object (or a JSON-object string) but got: {other}"
        ))),
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

#[cfg(test)]
mod tests {
    use super::{coerce_json, generic, versions};
    use serde_json::{json, Value};

    #[test]
    fn coerce_json_parses_stringified_object() {
        assert_eq!(
            coerce_json(Value::String(r#"{"whatsNew":"hi"}"#.to_string())),
            json!({ "whatsNew": "hi" })
        );
    }

    #[test]
    fn coerce_json_parses_stringified_array() {
        assert_eq!(
            coerce_json(Value::String("[1, 2, 3]".to_string())),
            json!([1, 2, 3])
        );
    }

    #[test]
    fn coerce_json_passes_through_structured_value() {
        let v = json!({ "data": { "type": "apps" } });
        assert_eq!(coerce_json(v.clone()), v);
    }

    #[test]
    fn coerce_json_leaves_plain_string_untouched() {
        let v = Value::String("Bug fixes and a fresh new look.".to_string());
        assert_eq!(coerce_json(v.clone()), v);
    }

    #[test]
    fn coerce_json_leaves_unparseable_jsonish_string_untouched() {
        let v = Value::String("{not valid json".to_string());
        assert_eq!(coerce_json(v.clone()), v);
    }

    #[test]
    fn request_args_coerces_stringified_body() {
        // Reproduces the real bug: a client that sends `body` as a JSON *string*.
        let args: generic::RequestArgs = serde_json::from_value(json!({
            "method": "PATCH",
            "path": "/v1/appStoreVersionLocalizations/x",
            "body": "{\"data\":{\"type\":\"appStoreVersionLocalizations\",\"id\":\"x\",\"attributes\":{\"whatsNew\":\"hi\"}}}"
        }))
        .unwrap();
        let body = args.body.expect("body present");
        assert!(
            body.is_object(),
            "stringified body must coerce to an object, got: {body}"
        );
        assert_eq!(body["data"]["attributes"]["whatsNew"], "hi");
    }

    #[test]
    fn request_args_keeps_real_object_body() {
        let args: generic::RequestArgs = serde_json::from_value(json!({
            "method": "POST",
            "path": "/v1/apps",
            "body": { "data": { "type": "apps", "attributes": { "name": "x" } } }
        }))
        .unwrap();
        assert_eq!(args.body.unwrap()["data"]["type"], "apps");
    }

    #[test]
    fn update_localization_attributes_coerces_stringified() {
        let args: versions::UpdateVersionLocalizationArgs = serde_json::from_value(json!({
            "localization_id": "loc-1",
            "attributes": "{\"whatsNew\":\"hi\"}"
        }))
        .unwrap();
        assert_eq!(args.attributes, json!({ "whatsNew": "hi" }));
    }
}
