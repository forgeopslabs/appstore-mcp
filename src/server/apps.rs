//! Tools for apps and app-level metadata (`appInfos`).

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{push_opt, AppStoreServer};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListAppsArgs {
    /// Filter by bundle ID, e.g. "com.example.app".
    #[serde(default)]
    pub bundle_id: Option<String>,
    /// Filter by app name.
    #[serde(default)]
    pub name: Option<String>,
    /// Filter by SKU.
    #[serde(default)]
    pub sku: Option<String>,
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetAppArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// Comma-separated related resources to include, e.g. "appInfos,appStoreVersions".
    #[serde(default)]
    pub include: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateAppArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// Attributes to update, e.g. {"primaryLocale": "en-US", "availableInNewTerritories": true}.
    pub attributes: Value,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListAppInfosArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateAppInfoArgs {
    /// The appInfo ID (from list_app_infos).
    pub app_info_id: String,
    /// Attributes to update, e.g. category/content-rights relationships are set separately.
    pub attributes: Value,
}

#[tool_router(router = apps_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// List apps in the account.
    #[tool(
        description = "List apps on the account, optionally filtered by bundle ID, name, or SKU."
    )]
    async fn list_apps(
        &self,
        Parameters(args): Parameters<ListAppsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "filter[bundleId]", args.bundle_id);
        push_opt(&mut query, "filter[name]", args.name);
        push_opt(&mut query, "filter[sku]", args.sku);
        push_opt(&mut query, "limit", args.limit);
        let value = self
            .client
            .get("/v1/apps", &query)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Get a single app by ID.
    #[tool(description = "Get a single app by its App Store Connect ID, with optional includes.")]
    async fn get_app(
        &self,
        Parameters(args): Parameters<GetAppArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "include", args.include);
        let value = self
            .client
            .get(&format!("/v1/apps/{}", args.app_id), &query)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Update an app's attributes.
    #[tool(
        description = "Update an app's attributes (e.g. primaryLocale, availableInNewTerritories, \
contentRightsDeclaration)."
    )]
    async fn update_app(
        &self,
        Parameters(args): Parameters<UpdateAppArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = json!({
            "data": { "type": "apps", "id": args.app_id, "attributes": args.attributes }
        });
        let value = self
            .client
            .patch(&format!("/v1/apps/{}", args.app_id), body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// List an app's appInfos (metadata containers per app version state).
    #[tool(
        description = "List an app's appInfos — metadata containers holding category and \
age-rating relationships for the app."
    )]
    async fn list_app_infos(
        &self,
        Parameters(args): Parameters<ListAppInfosArgs>,
    ) -> Result<CallToolResult, McpError> {
        let value = self
            .client
            .get(&format!("/v1/apps/{}/appInfos", args.app_id), &[])
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Update an appInfo's attributes.
    #[tool(description = "Update an appInfo's attributes by appInfo ID.")]
    async fn update_app_info(
        &self,
        Parameters(args): Parameters<UpdateAppInfoArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = json!({
            "data": { "type": "appInfos", "id": args.app_info_id, "attributes": args.attributes }
        });
        let value = self
            .client
            .patch(&format!("/v1/appInfos/{}", args.app_info_id), body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }
}
