//! App Store version and localized metadata tools.

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{push_opt, AppStoreServer};

/// Target platform for an App Store version.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Platform {
    Ios,
    MacOs,
    TvOs,
    VisionOs,
}

impl Platform {
    pub(crate) fn as_api(self) -> &'static str {
        match self {
            Platform::Ios => "IOS",
            Platform::MacOs => "MAC_OS",
            Platform::TvOs => "TV_OS",
            Platform::VisionOs => "VISION_OS",
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListVersionsArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// Filter by version state, e.g. "PREPARE_FOR_SUBMISSION", "READY_FOR_SALE".
    #[serde(default)]
    pub state: Option<String>,
    /// Filter by platform, e.g. "IOS".
    #[serde(default)]
    pub platform: Option<String>,
    /// Comma-separated includes, e.g. "appStoreVersionLocalizations,build".
    #[serde(default)]
    pub include: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateVersionArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// Target platform.
    pub platform: Platform,
    /// Version string, e.g. "1.2.0".
    pub version_string: String,
    /// Optional release type: "MANUAL", "AFTER_APPROVAL", or "SCHEDULED".
    #[serde(default)]
    pub release_type: Option<String>,
    /// Optional copyright string.
    #[serde(default)]
    pub copyright: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateVersionLocalizationArgs {
    /// The appStoreVersion ID.
    pub version_id: String,
    /// BCP-47 locale, e.g. "en-US".
    pub locale: String,
    /// Optional fields: description, keywords, whatsNew, promotionalText, marketingUrl, supportUrl.
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub keywords: Option<String>,
    #[serde(default)]
    pub whats_new: Option<String>,
    #[serde(default)]
    pub promotional_text: Option<String>,
    #[serde(default)]
    pub marketing_url: Option<String>,
    #[serde(default)]
    pub support_url: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateVersionLocalizationArgs {
    /// The appStoreVersionLocalization ID.
    pub localization_id: String,
    /// Attributes to update (description, keywords, whatsNew, promotionalText, marketingUrl, supportUrl).
    pub attributes: Value,
}

#[tool_router(router = versions_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// List an app's App Store versions.
    #[tool(
        description = "List an app's App Store versions, optionally filtered by state or platform."
    )]
    async fn list_app_store_versions(
        &self,
        Parameters(args): Parameters<ListVersionsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "filter[appStoreState]", args.state);
        push_opt(&mut query, "filter[platform]", args.platform);
        push_opt(&mut query, "include", args.include);
        let value = self
            .client
            .get(
                &format!("/v1/apps/{}/appStoreVersions", args.app_id),
                &query,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Create a new App Store version.
    #[tool(
        description = "Create a new App Store version for an app (platform + version string, with \
optional release type and copyright)."
    )]
    async fn create_app_store_version(
        &self,
        Parameters(args): Parameters<CreateVersionArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut attributes = json!({
            "platform": args.platform.as_api(),
            "versionString": args.version_string,
        });
        if let Some(rt) = args.release_type {
            attributes["releaseType"] = json!(rt);
        }
        if let Some(c) = args.copyright {
            attributes["copyright"] = json!(c);
        }
        let body = json!({
            "data": {
                "type": "appStoreVersions",
                "attributes": attributes,
                "relationships": {
                    "app": { "data": { "type": "apps", "id": args.app_id } }
                }
            }
        });
        let value = self
            .client
            .post("/v1/appStoreVersions", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Create a localized metadata entry for an App Store version.
    #[tool(
        description = "Create localized App Store metadata (description, keywords, whatsNew, URLs) \
for a version + locale."
    )]
    async fn create_version_localization(
        &self,
        Parameters(args): Parameters<CreateVersionLocalizationArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut attributes = json!({ "locale": args.locale });
        set_opt(&mut attributes, "description", args.description);
        set_opt(&mut attributes, "keywords", args.keywords);
        set_opt(&mut attributes, "whatsNew", args.whats_new);
        set_opt(&mut attributes, "promotionalText", args.promotional_text);
        set_opt(&mut attributes, "marketingUrl", args.marketing_url);
        set_opt(&mut attributes, "supportUrl", args.support_url);
        let body = json!({
            "data": {
                "type": "appStoreVersionLocalizations",
                "attributes": attributes,
                "relationships": {
                    "appStoreVersion": { "data": { "type": "appStoreVersions", "id": args.version_id } }
                }
            }
        });
        let value = self
            .client
            .post("/v1/appStoreVersionLocalizations", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Update a version's localized metadata.
    #[tool(
        description = "Update an App Store version localization by ID (description, keywords, whatsNew, URLs)."
    )]
    async fn update_version_localization(
        &self,
        Parameters(args): Parameters<UpdateVersionLocalizationArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = json!({
            "data": {
                "type": "appStoreVersionLocalizations",
                "id": args.localization_id,
                "attributes": args.attributes
            }
        });
        let value = self
            .client
            .patch(
                &format!("/v1/appStoreVersionLocalizations/{}", args.localization_id),
                body,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }
}

/// Insert `key: value` into a JSON object when the option is `Some`.
fn set_opt(obj: &mut Value, key: &str, value: Option<String>) {
    if let Some(v) = value {
        obj[key] = json!(v);
    }
}
