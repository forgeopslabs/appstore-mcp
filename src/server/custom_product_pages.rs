//! Custom Product Page (CPP) tools.
//!
//! Apps can have up to 35 custom product pages, each a marketing variant of the
//! default product page with its own promotional text and screenshots/previews,
//! reachable by a unique URL. The hierarchy is:
//!   appCustomProductPages -> appCustomProductPageVersions
//!     -> appCustomProductPageLocalizations (per-locale promotional text)
//!        -> appScreenshotSets / appPreviewSets (images, uploaded with the
//!           existing upload_app_screenshot / upload_app_preview tools).
//!
//! Schemas verified against Apple's generated OpenAPI models. Each tool delegates
//! JSON:API document construction to a pure `*_body` function (unit-tested below).
//! Submitting a CPP version for review is done with add_review_submission_item
//! (item_kind = app_custom_product_page_version).

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{push_opt, AppStoreServer};
use crate::error::AscError;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListPagesArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// Comma-separated includes, e.g. "appCustomProductPageVersions".
    #[serde(default)]
    pub include: Option<String>,
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetPageArgs {
    /// The appCustomProductPage ID.
    pub page_id: String,
    /// Comma-separated includes, e.g. "appCustomProductPageVersions".
    #[serde(default)]
    pub include: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreatePageArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// Reference name for the page (not customer-facing).
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdatePageArgs {
    /// The appCustomProductPage ID.
    pub page_id: String,
    /// New reference name.
    #[serde(default)]
    pub name: Option<String>,
    /// Whether the page is visible/active.
    #[serde(default)]
    pub visible: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PageIdArgs {
    /// The appCustomProductPage ID.
    pub page_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListVersionsArgs {
    /// The appCustomProductPage ID.
    pub page_id: String,
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateVersionArgs {
    /// The appCustomProductPage ID.
    pub page_id: String,
    /// Optional deep link URL the page opens to in the app.
    #[serde(default)]
    pub deep_link: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListLocalizationsArgs {
    /// The appCustomProductPageVersion ID.
    pub version_id: String,
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateLocalizationArgs {
    /// The appCustomProductPageVersion ID.
    pub version_id: String,
    /// BCP-47 locale, e.g. "en-US".
    pub locale: String,
    /// Optional promotional text shown on the page for this locale.
    #[serde(default)]
    pub promotional_text: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateLocalizationArgs {
    /// The appCustomProductPageLocalization ID.
    pub localization_id: String,
    /// New promotional text.
    pub promotional_text: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateScreenshotSetArgs {
    /// The appCustomProductPageLocalization ID.
    pub localization_id: String,
    /// Display type, e.g. "APP_IPHONE_67", "APP_IPAD_PRO_129".
    pub screenshot_display_type: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreatePreviewSetArgs {
    /// The appCustomProductPageLocalization ID.
    pub localization_id: String,
    /// Preview type, e.g. "IPHONE_67", "IPAD_PRO_129".
    pub preview_type: String,
}

#[tool_router(router = custom_product_pages_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// List an app's custom product pages.
    #[tool(description = "List an app's custom product pages (CPP).")]
    async fn list_custom_product_pages(
        &self,
        Parameters(args): Parameters<ListPagesArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "include", args.include);
        push_opt(&mut query, "limit", args.limit);
        let value = self
            .client
            .get(
                &format!("/v1/apps/{}/appCustomProductPages", args.app_id),
                &query,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Get a custom product page by ID.
    #[tool(description = "Get a custom product page by ID, with optional includes.")]
    async fn get_custom_product_page(
        &self,
        Parameters(args): Parameters<GetPageArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "include", args.include);
        let value = self
            .client
            .get(
                &format!("/v1/appCustomProductPages/{}", args.page_id),
                &query,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Create a custom product page.
    #[tool(
        description = "Create a custom product page for an app (reference name). Then add a \
version, localizations, and screenshot/preview sets."
    )]
    async fn create_custom_product_page(
        &self,
        Parameters(args): Parameters<CreatePageArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = page_create_body(&args.app_id, &args.name);
        let value = self
            .client
            .post("/v1/appCustomProductPages", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Update a custom product page (name and/or visibility).
    #[tool(description = "Update a custom product page's name and/or visibility.")]
    async fn update_custom_product_page(
        &self,
        Parameters(args): Parameters<UpdatePageArgs>,
    ) -> Result<CallToolResult, McpError> {
        if args.name.is_none() && args.visible.is_none() {
            return Err(AppStoreServer::map_err(AscError::InvalidRequest(
                "provide name and/or visible to update".into(),
            )));
        }
        let body = page_update_body(&args.page_id, args.name.as_deref(), args.visible);
        let value = self
            .client
            .patch(&format!("/v1/appCustomProductPages/{}", args.page_id), body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Delete a custom product page.
    #[tool(description = "Delete a custom product page by ID.")]
    async fn delete_custom_product_page(
        &self,
        Parameters(args): Parameters<PageIdArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.client
            .delete(&format!("/v1/appCustomProductPages/{}", args.page_id))
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(json!({ "deleted": args.page_id }))
    }

    /// List a page's versions.
    #[tool(description = "List a custom product page's versions.")]
    async fn list_custom_product_page_versions(
        &self,
        Parameters(args): Parameters<ListVersionsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "limit", args.limit);
        let value = self
            .client
            .get(
                &format!(
                    "/v1/appCustomProductPages/{}/appCustomProductPageVersions",
                    args.page_id
                ),
                &query,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Create a new version of a custom product page.
    #[tool(
        description = "Create a new version of a custom product page (optionally with a deep link). \
A new version is the editable draft you add localizations and images to."
    )]
    async fn create_custom_product_page_version(
        &self,
        Parameters(args): Parameters<CreateVersionArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = version_create_body(&args.page_id, args.deep_link.as_deref());
        let value = self
            .client
            .post("/v1/appCustomProductPageVersions", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// List a version's localizations.
    #[tool(description = "List a custom product page version's localizations.")]
    async fn list_custom_product_page_localizations(
        &self,
        Parameters(args): Parameters<ListLocalizationsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "limit", args.limit);
        let value = self
            .client
            .get(
                &format!(
                    "/v1/appCustomProductPageVersions/{}/appCustomProductPageLocalizations",
                    args.version_id
                ),
                &query,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Add a localization (promotional text) to a page version.
    #[tool(
        description = "Add a localized promotional text to a custom product page version for a \
given locale. Create screenshot/preview sets against the returned localization ID."
    )]
    async fn create_custom_product_page_localization(
        &self,
        Parameters(args): Parameters<CreateLocalizationArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = localization_create_body(
            &args.version_id,
            &args.locale,
            args.promotional_text.as_deref(),
        );
        let value = self
            .client
            .post("/v1/appCustomProductPageLocalizations", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Update a localization's promotional text.
    #[tool(description = "Update a custom product page localization's promotional text.")]
    async fn update_custom_product_page_localization(
        &self,
        Parameters(args): Parameters<UpdateLocalizationArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = localization_update_body(&args.localization_id, &args.promotional_text);
        let value = self
            .client
            .patch(
                &format!(
                    "/v1/appCustomProductPageLocalizations/{}",
                    args.localization_id
                ),
                body,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Create a screenshot set on a custom product page localization.
    #[tool(
        description = "Create an appScreenshotSet on a custom product page localization (e.g. \
display type APP_IPHONE_67). Upload images into it with upload_app_screenshot."
    )]
    async fn create_cpp_screenshot_set(
        &self,
        Parameters(args): Parameters<CreateScreenshotSetArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = screenshot_set_body(&args.localization_id, &args.screenshot_display_type);
        let value = self
            .client
            .post("/v1/appScreenshotSets", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Create a preview set on a custom product page localization.
    #[tool(
        description = "Create an appPreviewSet on a custom product page localization (e.g. preview \
type IPHONE_67). Upload videos into it with upload_app_preview."
    )]
    async fn create_cpp_preview_set(
        &self,
        Parameters(args): Parameters<CreatePreviewSetArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = preview_set_body(&args.localization_id, &args.preview_type);
        let value = self
            .client
            .post("/v1/appPreviewSets", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }
}

// ---- Pure JSON:API document builders (unit-tested below) --------------------

fn page_create_body(app_id: &str, name: &str) -> Value {
    json!({
        "data": {
            "type": "appCustomProductPages",
            "attributes": { "name": name },
            "relationships": {
                "app": { "data": { "type": "apps", "id": app_id } }
            }
        }
    })
}

fn page_update_body(page_id: &str, name: Option<&str>, visible: Option<bool>) -> Value {
    let mut attrs = json!({});
    if let Some(n) = name {
        attrs["name"] = json!(n);
    }
    if let Some(v) = visible {
        attrs["visible"] = json!(v);
    }
    json!({
        "data": { "type": "appCustomProductPages", "id": page_id, "attributes": attrs }
    })
}

fn version_create_body(page_id: &str, deep_link: Option<&str>) -> Value {
    let mut attrs = json!({});
    if let Some(d) = deep_link {
        attrs["deepLink"] = json!(d);
    }
    json!({
        "data": {
            "type": "appCustomProductPageVersions",
            "attributes": attrs,
            "relationships": {
                "appCustomProductPage": {
                    "data": { "type": "appCustomProductPages", "id": page_id }
                }
            }
        }
    })
}

fn localization_create_body(
    version_id: &str,
    locale: &str,
    promotional_text: Option<&str>,
) -> Value {
    let mut attrs = json!({ "locale": locale });
    if let Some(t) = promotional_text {
        attrs["promotionalText"] = json!(t);
    }
    json!({
        "data": {
            "type": "appCustomProductPageLocalizations",
            "attributes": attrs,
            "relationships": {
                "appCustomProductPageVersion": {
                    "data": { "type": "appCustomProductPageVersions", "id": version_id }
                }
            }
        }
    })
}

fn localization_update_body(localization_id: &str, promotional_text: &str) -> Value {
    json!({
        "data": {
            "type": "appCustomProductPageLocalizations",
            "id": localization_id,
            "attributes": { "promotionalText": promotional_text }
        }
    })
}

fn screenshot_set_body(localization_id: &str, display_type: &str) -> Value {
    json!({
        "data": {
            "type": "appScreenshotSets",
            "attributes": { "screenshotDisplayType": display_type },
            "relationships": {
                "appCustomProductPageLocalization": {
                    "data": { "type": "appCustomProductPageLocalizations", "id": localization_id }
                }
            }
        }
    })
}

fn preview_set_body(localization_id: &str, preview_type: &str) -> Value {
    json!({
        "data": {
            "type": "appPreviewSets",
            "attributes": { "previewType": preview_type },
            "relationships": {
                "appCustomProductPageLocalization": {
                    "data": { "type": "appCustomProductPageLocalizations", "id": localization_id }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_create_shape() {
        let b = page_create_body("app-1", "Summer Campaign");
        assert_eq!(b["data"]["type"], "appCustomProductPages");
        assert_eq!(b["data"]["attributes"]["name"], "Summer Campaign");
        assert_eq!(b["data"]["relationships"]["app"]["data"]["type"], "apps");
        assert_eq!(b["data"]["relationships"]["app"]["data"]["id"], "app-1");
    }

    #[test]
    fn page_update_includes_only_set_fields() {
        let b = page_update_body("pg-1", None, Some(true));
        assert_eq!(b["data"]["id"], "pg-1");
        assert_eq!(b["data"]["attributes"]["visible"], true);
        assert!(b["data"]["attributes"].get("name").is_none());
        // and name-only
        let b2 = page_update_body("pg-1", Some("New"), None);
        assert_eq!(b2["data"]["attributes"]["name"], "New");
        assert!(b2["data"]["attributes"].get("visible").is_none());
    }

    #[test]
    fn version_create_with_and_without_deep_link() {
        let b = version_create_body("pg-1", Some("myapp://promo"));
        assert_eq!(b["data"]["type"], "appCustomProductPageVersions");
        assert_eq!(b["data"]["attributes"]["deepLink"], "myapp://promo");
        assert_eq!(
            b["data"]["relationships"]["appCustomProductPage"]["data"]["id"],
            "pg-1"
        );
        let b2 = version_create_body("pg-1", None);
        assert!(b2["data"]["attributes"].get("deepLink").is_none());
    }

    #[test]
    fn localization_create_shape() {
        let b = localization_create_body("ver-1", "en-US", Some("Big sale!"));
        assert_eq!(b["data"]["type"], "appCustomProductPageLocalizations");
        assert_eq!(b["data"]["attributes"]["locale"], "en-US");
        assert_eq!(b["data"]["attributes"]["promotionalText"], "Big sale!");
        assert_eq!(
            b["data"]["relationships"]["appCustomProductPageVersion"]["data"]["type"],
            "appCustomProductPageVersions"
        );
        assert_eq!(
            b["data"]["relationships"]["appCustomProductPageVersion"]["data"]["id"],
            "ver-1"
        );
        // promotional_text omitted when None
        let b2 = localization_create_body("ver-1", "fr-FR", None);
        assert!(b2["data"]["attributes"].get("promotionalText").is_none());
    }

    #[test]
    fn localization_update_shape() {
        let b = localization_update_body("loc-1", "Updated");
        assert_eq!(b["data"]["id"], "loc-1");
        assert_eq!(b["data"]["attributes"]["promotionalText"], "Updated");
        assert!(b["data"].get("relationships").is_none());
    }

    #[test]
    fn cpp_sets_use_cpp_localization_relationship() {
        let s = screenshot_set_body("loc-1", "APP_IPHONE_67");
        assert_eq!(s["data"]["type"], "appScreenshotSets");
        assert_eq!(
            s["data"]["attributes"]["screenshotDisplayType"],
            "APP_IPHONE_67"
        );
        assert_eq!(
            s["data"]["relationships"]["appCustomProductPageLocalization"]["data"]["type"],
            "appCustomProductPageLocalizations"
        );
        let p = preview_set_body("loc-1", "IPHONE_67");
        assert_eq!(p["data"]["type"], "appPreviewSets");
        assert_eq!(p["data"]["attributes"]["previewType"], "IPHONE_67");
        assert_eq!(
            p["data"]["relationships"]["appCustomProductPageLocalization"]["data"]["id"],
            "loc-1"
        );
    }
}
