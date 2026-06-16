//! Generic JSON:API escape-hatch tools.
//!
//! These two tools can reach *any* App Store Connect endpoint, covering
//! everything the curated domain tools don't.

use reqwest::Method;
use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::{Map, Value};

use super::{flatten_query, AppStoreServer};
use crate::error::AscError;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RequestArgs {
    /// HTTP method: GET, POST, PATCH, PUT, or DELETE.
    pub method: String,
    /// API path or full URL, e.g. "/v1/apps", "v2/inAppPurchases/{id}",
    /// or a `next` link returned by a previous list call.
    pub path: String,
    /// Optional query parameters, e.g. {"filter[bundleId]": "com.example.app", "limit": 50}.
    /// Array values are comma-joined.
    #[serde(default)]
    pub query: Option<Map<String, Value>>,
    /// Optional JSON:API request body for POST/PATCH/PUT — the full document,
    /// e.g. {"data": {"type": "apps", "id": "123", "attributes": {...}}}.
    #[serde(default)]
    pub body: Option<Value>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListArgs {
    /// Collection path or full URL to GET, e.g. "/v1/apps".
    pub path: String,
    /// Optional filters, e.g. {"filter[name]": "MyApp"}.
    #[serde(default)]
    pub filters: Option<Map<String, Value>>,
    /// Comma-separated sort keys, e.g. "-createdDate".
    #[serde(default)]
    pub sort: Option<String>,
    /// Comma-separated related resources to include, e.g. "appStoreVersions".
    #[serde(default)]
    pub include: Option<String>,
    /// Page size (App Store Connect maximum is 200). Sparse-fieldset selections
    /// (`fields[...]`) can be passed via `filters` if needed.
    #[serde(default)]
    pub limit: Option<u32>,
    /// Opaque pagination cursor from a previous response's `data.links.next`.
    #[serde(default)]
    pub cursor: Option<String>,
}

#[tool_router(router = generic_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// Make a raw, authenticated request to any App Store Connect endpoint.
    #[tool(
        description = "Make a raw authenticated request to ANY App Store Connect API endpoint \
(method + path + optional query + optional JSON:API body). Use this for operations without a \
dedicated tool. Returns the parsed JSON response."
    )]
    async fn appstore_request(
        &self,
        Parameters(args): Parameters<RequestArgs>,
    ) -> Result<CallToolResult, McpError> {
        let method = Method::from_bytes(args.method.trim().to_uppercase().as_bytes())
            .map_err(|_| {
                AscError::InvalidRequest(format!(
                    "invalid HTTP method '{}'; use GET, POST, PATCH, PUT, or DELETE",
                    args.method
                ))
            })
            .map_err(AppStoreServer::map_err)?;

        let query = args.query.as_ref().map(flatten_query).unwrap_or_default();
        let value = self
            .client
            .request(method, &args.path, &query, args.body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// List a collection with filters/sort/pagination.
    #[tool(
        description = "List any App Store Connect collection with optional filters, sort, include, \
and pagination. Returns one page; pass the `next` link (from the response's links.next) back as \
`cursor` to fetch subsequent pages."
    )]
    async fn appstore_list(
        &self,
        Parameters(args): Parameters<ListArgs>,
    ) -> Result<CallToolResult, McpError> {
        // If a cursor (a full `next` URL) is supplied, follow it verbatim.
        if let Some(cursor) = &args.cursor {
            let value = self
                .client
                .get(cursor, &[])
                .await
                .map_err(AppStoreServer::map_err)?;
            return AppStoreServer::ok_json(value);
        }

        let mut query: Vec<(String, String)> = Vec::new();
        if let Some(filters) = &args.filters {
            query.extend(flatten_query(filters));
        }
        if let Some(sort) = &args.sort {
            query.push(("sort".into(), sort.clone()));
        }
        if let Some(include) = &args.include {
            query.push(("include".into(), include.clone()));
        }
        if let Some(limit) = args.limit {
            query.push(("limit".into(), limit.to_string()));
        }

        let value = self
            .client
            .get(&args.path, &query)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }
}
