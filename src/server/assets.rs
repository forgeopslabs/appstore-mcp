//! App screenshot and app preview upload tools.
//!
//! Both reuse the reserve → upload → commit workflow in `crate::upload`. The
//! caller supplies the target *set* ID (an `appScreenshotSet`/`appPreviewSet`),
//! which is created against a version localization beforehand (use the generic
//! tools or `appstore_request` to create the set if needed).

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::AppStoreServer;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UploadScreenshotArgs {
    /// The appScreenshotSet ID to add this screenshot to.
    pub screenshot_set_id: String,
    /// Local path to the screenshot image file (PNG/JPEG).
    pub file_path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UploadPreviewArgs {
    /// The appPreviewSet ID to add this preview to.
    pub preview_set_id: String,
    /// Local path to the preview video file.
    pub file_path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateScreenshotSetArgs {
    /// The appStoreVersionLocalization ID this set belongs to.
    pub version_localization_id: String,
    /// Display type, e.g. "APP_IPHONE_67", "APP_IPAD_PRO_129", "APP_WATCH_ULTRA".
    pub screenshot_display_type: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreatePreviewSetArgs {
    /// The appStoreVersionLocalization ID this set belongs to.
    pub version_localization_id: String,
    /// Preview type, e.g. "IPHONE_67", "IPAD_PRO_129".
    pub preview_type: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteSetArgs {
    /// The set ID to delete.
    pub set_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReorderScreenshotsArgs {
    /// The appScreenshotSet ID.
    pub set_id: String,
    /// The screenshot IDs in the desired display order.
    pub ordered_screenshot_ids: Vec<String>,
}

#[tool_router(router = assets_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// Upload an app screenshot into a screenshot set.
    #[tool(
        description = "Upload an app screenshot into an appScreenshotSet (reserve → upload → commit \
with MD5 verification). Provide the set ID and a local image path."
    )]
    async fn upload_app_screenshot(
        &self,
        Parameters(args): Parameters<UploadScreenshotArgs>,
    ) -> Result<CallToolResult, McpError> {
        let relationships = json!({
            "appScreenshotSet": {
                "data": { "type": "appScreenshotSets", "id": args.screenshot_set_id }
            }
        });
        let value = self
            .client
            .upload_asset(
                "/v1/appScreenshots",
                "appScreenshots",
                json!({}),
                relationships,
                &args.file_path,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Upload an app preview into a preview set.
    #[tool(
        description = "Upload an app preview video into an appPreviewSet (reserve → upload → commit \
with MD5 verification). Provide the set ID and a local video path."
    )]
    async fn upload_app_preview(
        &self,
        Parameters(args): Parameters<UploadPreviewArgs>,
    ) -> Result<CallToolResult, McpError> {
        let relationships = json!({
            "appPreviewSet": {
                "data": { "type": "appPreviewSets", "id": args.preview_set_id }
            }
        });
        let value = self
            .client
            .upload_asset(
                "/v1/appPreviews",
                "appPreviews",
                json!({}),
                relationships,
                &args.file_path,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Create a screenshot set for a version localization.
    #[tool(
        description = "Create an appScreenshotSet for a version localization + display type (e.g. \
APP_IPHONE_67). Upload screenshots into it with upload_app_screenshot."
    )]
    async fn create_screenshot_set(
        &self,
        Parameters(args): Parameters<CreateScreenshotSetArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body =
            screenshot_set_body(&args.version_localization_id, &args.screenshot_display_type);
        let value = self
            .client
            .post("/v1/appScreenshotSets", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Create a preview set for a version localization.
    #[tool(
        description = "Create an appPreviewSet for a version localization + preview type (e.g. \
IPHONE_67). Upload previews into it with upload_app_preview."
    )]
    async fn create_preview_set(
        &self,
        Parameters(args): Parameters<CreatePreviewSetArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = preview_set_body(&args.version_localization_id, &args.preview_type);
        let value = self
            .client
            .post("/v1/appPreviewSets", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Delete a screenshot set.
    #[tool(description = "Delete an appScreenshotSet (and its screenshots) by ID.")]
    async fn delete_screenshot_set(
        &self,
        Parameters(args): Parameters<DeleteSetArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.client
            .delete(&format!("/v1/appScreenshotSets/{}", args.set_id))
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(json!({ "deleted": args.set_id }))
    }

    /// Delete a preview set.
    #[tool(description = "Delete an appPreviewSet (and its previews) by ID.")]
    async fn delete_preview_set(
        &self,
        Parameters(args): Parameters<DeleteSetArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.client
            .delete(&format!("/v1/appPreviewSets/{}", args.set_id))
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(json!({ "deleted": args.set_id }))
    }

    /// Reorder the screenshots within a set.
    #[tool(
        description = "Set the display order of screenshots within an appScreenshotSet by passing \
the screenshot IDs in the desired order."
    )]
    async fn reorder_screenshots(
        &self,
        Parameters(args): Parameters<ReorderScreenshotsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = reorder_screenshots_body(&args.ordered_screenshot_ids);
        let path = format!(
            "/v1/appScreenshotSets/{}/relationships/appScreenshots",
            args.set_id
        );
        self.client
            .patch(&path, body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(json!({ "reordered": args.ordered_screenshot_ids }))
    }
}

// ---- Pure JSON:API document builders (unit-tested below) --------------------

fn screenshot_set_body(version_localization_id: &str, display_type: &str) -> Value {
    json!({
        "data": {
            "type": "appScreenshotSets",
            "attributes": { "screenshotDisplayType": display_type },
            "relationships": {
                "appStoreVersionLocalization": {
                    "data": { "type": "appStoreVersionLocalizations", "id": version_localization_id }
                }
            }
        }
    })
}

fn preview_set_body(version_localization_id: &str, preview_type: &str) -> Value {
    json!({
        "data": {
            "type": "appPreviewSets",
            "attributes": { "previewType": preview_type },
            "relationships": {
                "appStoreVersionLocalization": {
                    "data": { "type": "appStoreVersionLocalizations", "id": version_localization_id }
                }
            }
        }
    })
}

fn reorder_screenshots_body(ordered_ids: &[String]) -> Value {
    let data: Vec<Value> = ordered_ids
        .iter()
        .map(|id| json!({ "type": "appScreenshots", "id": id }))
        .collect();
    json!({ "data": data })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_set_body_shape() {
        let b = screenshot_set_body("loc-1", "APP_IPHONE_67");
        assert_eq!(b["data"]["type"], "appScreenshotSets");
        assert_eq!(
            b["data"]["attributes"]["screenshotDisplayType"],
            "APP_IPHONE_67"
        );
        assert_eq!(
            b["data"]["relationships"]["appStoreVersionLocalization"]["data"]["id"],
            "loc-1"
        );
    }

    #[test]
    fn preview_set_body_shape() {
        let b = preview_set_body("loc-2", "IPHONE_67");
        assert_eq!(b["data"]["type"], "appPreviewSets");
        assert_eq!(b["data"]["attributes"]["previewType"], "IPHONE_67");
        assert_eq!(
            b["data"]["relationships"]["appStoreVersionLocalization"]["data"]["type"],
            "appStoreVersionLocalizations"
        );
    }

    #[test]
    fn reorder_body_preserves_order_and_type() {
        let ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let b = reorder_screenshots_body(&ids);
        let arr = b["data"].as_array().expect("array");
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0]["id"], "a");
        assert_eq!(arr[1]["id"], "b");
        assert_eq!(arr[2]["id"], "c");
        assert_eq!(arr[0]["type"], "appScreenshots");
    }

    #[test]
    fn reorder_body_empty_is_empty_array() {
        let b = reorder_screenshots_body(&[]);
        assert_eq!(b["data"].as_array().map(|a| a.len()), Some(0));
    }
}
