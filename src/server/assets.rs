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
use serde_json::json;

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
                relationships,
                &args.file_path,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }
}
