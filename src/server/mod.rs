//! The MCP server: the [`AppStoreServer`] type, its combined tool router, and
//! shared helpers used by every tool module.
//!
//! Tools are organized by domain into submodules, each contributing a
//! `#[tool_router(router = <name>_router)]` impl block. [`AppStoreServer::new`]
//! sums those routers into the single [`ToolRouter`] stored on the struct, and
//! `#[tool_handler(router = self.tool_router)]` dispatches off it.

mod analytics;
mod apps;
mod asset_library;
mod assets;
mod availability;
pub mod catalog;
mod custom_product_pages;
mod discovery;
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
use catalog::Group;

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
- `appstore_list` can walk pages for you: pass `max_pages` instead of calling it \
  again with each `cursor`.
- Responses are trimmed to fit a context budget. A `_truncated` key means items \
  were dropped — narrow the query with `limit`/`filter[...]`/`fields[...]` rather \
  than assuming you saw everything.
- Analytics report data is downloaded with `download_analytics_segment` using a \
  segment URL from `list_analytics_report_segments`.

Limitations (enforced by Apple, not this server):
- New apps CANNOT be created via the API (the `apps` resource allows only \
  GET and UPDATE). Create the app in the App Store Connect website first; you \
  can pre-create its bundle ID with create_bundle_id.
- Sales/finance reports return gzipped TSV, not JSON:API, and are not wrapped here.";

const DISCOVERY_INSTRUCTIONS: &str = "\
This server wraps the Apple App Store Connect API in discovery mode. Use \
search_tools to find operations, get_tool_details to inspect an operation's \
input schema and safety annotations, then call_discovered_tool with its exact \
name and arguments. call_discovered_tool can write or delete account data; \
check the inspected annotations and obtain approval before writes. \
ASC_TOOLS and ASC_READ_ONLY still restrict which operations are available. \
Credentials come from ASC_ISSUER_ID, ASC_KEY_ID, and ASC_PRIVATE_KEY or \
ASC_PRIVATE_KEY_PATH. A _truncated response means narrow the request.";

/// The App Store Connect MCP server.
#[derive(Clone)]
pub struct AppStoreServer {
    pub(crate) client: Arc<AscClient>,
    tool_router: ToolRouter<AppStoreServer>,
    discovery_router: Option<ToolRouter<AppStoreServer>>,
}

impl AppStoreServer {
    /// Build the server from configuration, assembling the per-domain routers
    /// into the subset this configuration serves (see [`catalog`]).
    pub fn new(config: Config) -> Self {
        let tools = config.tools.clone();
        let assembled = catalog::assemble(Self::domain_routers(), &tools);

        for warning in &assembled.warnings {
            tracing::warn!("{warning}");
        }
        if !assembled.unclassified.is_empty() {
            tracing::error!(
                tools = ?assembled.unclassified,
                "these tools' names start with a verb `server::catalog::VERBS` doesn't know, so \
                 they are served without safety annotations; classify them there"
            );
        }
        let domain_tool_count = assembled.router.map.len();
        let (tool_router, discovery_router) = if tools.discovery {
            let mut visible = Self::discovery_router();
            discovery::annotate(&mut visible, tools.read_only);
            (visible, Some(assembled.router))
        } else {
            (assembled.router, None)
        };
        tracing::info!(
            served = tool_router.map.len(),
            discoverable = domain_tool_count,
            withheld = assembled.withheld,
            read_only = tools.read_only,
            discovery = tools.discovery,
            groups = assembled
                .groups
                .iter()
                .map(|g| g.as_str())
                .collect::<Vec<_>>()
                .join(","),
            "assembled tool router"
        );

        Self {
            client: Arc::new(AscClient::new(config)),
            tool_router,
            discovery_router,
        }
    }

    /// Every domain router, tagged with the group it belongs to.
    fn domain_routers() -> Vec<(Group, ToolRouter<Self>)> {
        vec![
            (Group::Generic, Self::generic_router()),
            (Group::Apps, Self::apps_router()),
            (Group::Iap, Self::iap_router()),
            (Group::Subscriptions, Self::subscriptions_router()),
            (Group::Versions, Self::versions_router()),
            (Group::Pricing, Self::pricing_router()),
            (Group::Availability, Self::availability_router()),
            (Group::Submission, Self::submission_router()),
            (Group::TestFlight, Self::testflight_router()),
            (Group::Provisioning, Self::provisioning_router()),
            (
                Group::Assets,
                Self::assets_router() + Self::asset_library_router(),
            ),
            (Group::Offers, Self::offers_router()),
            (Group::OfferCodes, Self::offer_codes_router()),
            (Group::Promotions, Self::promotions_router()),
            (Group::Reviews, Self::reviews_router()),
            (Group::Users, Self::users_router()),
            (Group::Events, Self::events_router()),
            (Group::XcodeCloud, Self::xcode_cloud_router()),
            (Group::Analytics, Self::analytics_router()),
            (
                Group::CustomProductPages,
                Self::custom_product_pages_router(),
            ),
        ]
    }

    /// The tools this server serves, sorted by name.
    pub fn tools(&self) -> Vec<Tool> {
        let mut tools: Vec<Tool> = self
            .tool_router
            .map
            .values()
            .map(|route| route.attr.clone())
            .collect();
        tools.sort_by(|a, b| a.name.cmp(&b.name));
        tools
    }

    /// Render a JSON value as a text tool result, compacted and size-capped per
    /// [`crate::json`] so one verbose page can't swamp the agent's context.
    pub(crate) fn ok_json(&self, value: Value) -> Result<CallToolResult, McpError> {
        let text = crate::json::render(value, &self.client.config.output);
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Refuse an operation that would modify the account on a read-only server.
    pub(crate) fn ensure_writable(&self, what: &str) -> Result<(), McpError> {
        if self.client.config.tools.read_only {
            return Err(AscError::InvalidRequest(format!(
                "this server is running in read-only mode (ASC_READ_ONLY), so {what} is not \
                 permitted; unset ASC_READ_ONLY to allow writes"
            ))
            .into_mcp_error());
        }
        Ok(())
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
            .with_instructions(if self.discovery_router.is_some() {
                DISCOVERY_INSTRUCTIONS.to_string()
            } else {
                INSTRUCTIONS.to_string()
            })
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
