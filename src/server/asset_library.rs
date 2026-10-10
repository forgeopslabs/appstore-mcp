//! App Asset Library tools (App Store Connect API 4.5.1).
//!
//! The library replaces screenshot sets, preview sets, and in-app event media:
//! an image or video is uploaded to the app's library once, then *placed* on any
//! number of localizations — an App Store version localization, a custom product
//! page localization, an in-app event localization, or a product page
//! optimization treatment localization. The order of placements within a
//! placement group is set separately.
//!
//! Upload reuses the reserve → upload → commit workflow in `crate::upload`; the
//! only difference is that the commit carries no checksum.

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{push_opt, AppStoreServer};
use crate::error::AscError;

/// Where a placement can live, and the JSON:API names Apple uses for each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlacementTarget {
    Version,
    CustomProductPage,
    Event,
    Treatment,
}

impl PlacementTarget {
    const ACCEPTED: &'static str = "app_store_version_localization, \
custom_product_page_localization, app_event_localization, \
experiment_treatment_localization";

    fn parse(name: &str) -> Result<Self, AscError> {
        match name.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "app_store_version_localization" | "version_localization" => Ok(Self::Version),
            "custom_product_page_localization" | "app_custom_product_page_localization" => {
                Ok(Self::CustomProductPage)
            }
            "app_event_localization" | "event_localization" => Ok(Self::Event),
            "experiment_treatment_localization"
            | "app_store_version_experiment_treatment_localization" => Ok(Self::Treatment),
            other => Err(AscError::InvalidRequest(format!(
                "unknown target_type '{other}'; expected one of: {}",
                Self::ACCEPTED
            ))),
        }
    }

    /// The relationship name on a placement or ordering request.
    fn relationship(self) -> &'static str {
        match self {
            Self::Version => "appStoreVersionLocalization",
            Self::CustomProductPage => "appCustomProductPageLocalization",
            Self::Event => "appEventLocalization",
            Self::Treatment => "appStoreVersionExperimentTreatmentLocalization",
        }
    }

    /// The JSON:API resource type, which is also the collection path segment.
    fn resource_type(self) -> &'static str {
        match self {
            Self::Version => "appStoreVersionLocalizations",
            Self::CustomProductPage => "appCustomProductPageLocalizations",
            Self::Event => "appEventLocalizations",
            Self::Treatment => "appStoreVersionExperimentTreatmentLocalizations",
        }
    }
}

/// Whether a placement shows an image or a video.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MediaKind {
    Image,
    Video,
}

impl MediaKind {
    fn parse(name: &str) -> Result<Self, AscError> {
        match name.trim().to_ascii_lowercase().as_str() {
            "image" => Ok(Self::Image),
            "video" => Ok(Self::Video),
            other => Err(AscError::InvalidRequest(format!(
                "unknown media_type '{other}'; expected 'image' or 'video'"
            ))),
        }
    }

    fn relationship(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
        }
    }

    fn resource_type(self) -> &'static str {
        match self {
            Self::Image => "appAssetLibraryImages",
            Self::Video => "appAssetLibraryVideos",
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetAssetLibraryArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListLibraryAssetsArgs {
    /// The appAssetLibrary ID (from get_app_asset_library).
    pub asset_library_id: String,
    /// Filter by category: "APP_SCREENSHOTS_AND_PREVIEWS" or "CREATIVE_ASSETS".
    #[serde(default)]
    pub category: Option<String>,
    /// Filter by state, e.g. "COMPLETE", "AWAITING_UPLOAD", "FAILED", "ARCHIVED".
    #[serde(default)]
    pub state: Option<String>,
    /// Filter by reference name.
    #[serde(default)]
    pub reference_name: Option<String>,
    /// Set to true to include each asset's placements.
    #[serde(default)]
    pub include_placements: Option<bool>,
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UploadLibraryImageArgs {
    /// The appAssetLibrary ID (from get_app_asset_library).
    pub asset_library_id: String,
    /// Local path to the image file (PNG/JPEG).
    pub file_path: String,
    /// "APP_SCREENSHOTS_AND_PREVIEWS" (default) or "CREATIVE_ASSETS".
    #[serde(default)]
    pub category: Option<String>,
    /// An internal name to find the asset by later.
    #[serde(default)]
    pub reference_name: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UploadLibraryVideoArgs {
    /// The appAssetLibrary ID (from get_app_asset_library).
    pub asset_library_id: String,
    /// Local path to the video file.
    pub file_path: String,
    /// "APP_SCREENSHOTS_AND_PREVIEWS" (default) or "CREATIVE_ASSETS".
    #[serde(default)]
    pub category: Option<String>,
    /// An internal name to find the asset by later.
    #[serde(default)]
    pub reference_name: Option<String>,
    /// Poster-frame timecode, e.g. "00:00:05:00".
    #[serde(default)]
    pub preview_frame_time_code: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteLibraryAssetArgs {
    /// The asset ID to delete.
    pub asset_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreatePlacementArgs {
    /// "image" or "video".
    pub media_type: String,
    /// The appAssetLibraryImage or appAssetLibraryVideo ID to place.
    pub asset_id: String,
    /// Where to place it: "app_store_version_localization",
    /// "custom_product_page_localization", "app_event_localization", or
    /// "experiment_treatment_localization".
    pub target_type: String,
    /// The ID of the target localization.
    pub target_id: String,
    /// e.g. "APP_SCREENSHOT", "APP_PREVIEW", "IMESSAGE_APP_SCREENSHOT",
    /// "EVENT_CARD_ASSET", "EVENT_DETAILS_PAGE_ASSET".
    pub placement_type: String,
    /// The device profile group, e.g. "IPHONE_DYNAMIC_ISLAND_LARGE_PROFILE"; valid
    /// values are the placementProfileGroups in list_asset_library_ref_data.
    #[serde(default)]
    pub placement_group: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListPlacementsArgs {
    /// "app_store_version_localization", "custom_product_page_localization",
    /// "app_event_localization", or "experiment_treatment_localization".
    pub target_type: String,
    /// The ID of the target localization.
    pub target_id: String,
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeletePlacementArgs {
    /// The appAssetLibraryPlacement ID to delete.
    pub placement_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetPlacementOrderArgs {
    /// "app_store_version_localization", "custom_product_page_localization", or
    /// "experiment_treatment_localization". In-app event placements have no order.
    pub target_type: String,
    /// The ID of the target localization.
    pub target_id: String,
    /// The device profile group whose order is being set, e.g.
    /// "IPHONE_DYNAMIC_ISLAND_LARGE_PROFILE".
    pub placement_group: String,
    /// Placement IDs in the desired display order.
    pub ordered_placement_ids: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListRefDataArgs {
    /// Comma-separated placement types, e.g. "APP_SCREENSHOT,APP_PREVIEW".
    #[serde(default)]
    pub placement_types: Option<String>,
    /// Comma-separated placement profile groups.
    #[serde(default)]
    pub placement_profile_groups: Option<String>,
    /// Comma-separated features, e.g. "APP_STORE_VERSIONS,CUSTOM_PRODUCT_PAGES".
    #[serde(default)]
    pub features: Option<String>,
}

#[tool_router(router = asset_library_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// Read an app's asset library.
    #[tool(
        description = "Get an app's App Asset Library (its ID is needed to upload and list \
library images and videos). The library replaces screenshot and preview sets as of API 4.5.1."
    )]
    async fn get_app_asset_library(
        &self,
        Parameters(args): Parameters<GetAssetLibraryArgs>,
    ) -> Result<CallToolResult, McpError> {
        let value = self
            .client
            .get(&format!("/v1/apps/{}/assetLibrary", args.app_id), &[])
            .await
            .map_err(AppStoreServer::map_err)?;
        self.ok_json(value)
    }

    /// List the images in an asset library.
    #[tool(
        description = "List the images in an App Asset Library, optionally filtered by category, \
state, or reference name, and optionally with their placements."
    )]
    async fn list_asset_library_images(
        &self,
        Parameters(args): Parameters<ListLibraryAssetsArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.list_library_assets("images", args).await
    }

    /// List the videos in an asset library.
    #[tool(
        description = "List the videos in an App Asset Library, optionally filtered by category, \
state, or reference name, and optionally with their placements."
    )]
    async fn list_asset_library_videos(
        &self,
        Parameters(args): Parameters<ListLibraryAssetsArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.list_library_assets("videos", args).await
    }

    /// Upload an image to an asset library.
    #[tool(
        description = "Upload an image to an App Asset Library (reserve → upload → commit). Then \
place it on a localization with create_asset_library_placement."
    )]
    async fn upload_asset_library_image(
        &self,
        Parameters(args): Parameters<UploadLibraryImageArgs>,
    ) -> Result<CallToolResult, McpError> {
        let attrs = library_asset_attributes(&args.category, &args.reference_name, &None);
        let value = self
            .client
            .upload_library_asset(
                "/v1/appAssetLibraryImages",
                "appAssetLibraryImages",
                attrs,
                library_relationship(&args.asset_library_id),
                &args.file_path,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        self.ok_json(value)
    }

    /// Upload a video to an asset library.
    #[tool(
        description = "Upload a video (app preview or creative) to an App Asset Library (reserve → \
upload → commit). Then place it on a localization with create_asset_library_placement."
    )]
    async fn upload_asset_library_video(
        &self,
        Parameters(args): Parameters<UploadLibraryVideoArgs>,
    ) -> Result<CallToolResult, McpError> {
        let attrs = library_asset_attributes(
            &args.category,
            &args.reference_name,
            &args.preview_frame_time_code,
        );
        let value = self
            .client
            .upload_library_asset(
                "/v1/appAssetLibraryVideos",
                "appAssetLibraryVideos",
                attrs,
                library_relationship(&args.asset_library_id),
                &args.file_path,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        self.ok_json(value)
    }

    /// Delete an image from an asset library.
    #[tool(description = "Delete an image from an App Asset Library by ID.")]
    async fn delete_asset_library_image(
        &self,
        Parameters(args): Parameters<DeleteLibraryAssetArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.client
            .delete(&format!("/v1/appAssetLibraryImages/{}", args.asset_id))
            .await
            .map_err(AppStoreServer::map_err)?;
        self.ok_json(json!({ "deleted": args.asset_id }))
    }

    /// Delete a video from an asset library.
    #[tool(description = "Delete a video from an App Asset Library by ID.")]
    async fn delete_asset_library_video(
        &self,
        Parameters(args): Parameters<DeleteLibraryAssetArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.client
            .delete(&format!("/v1/appAssetLibraryVideos/{}", args.asset_id))
            .await
            .map_err(AppStoreServer::map_err)?;
        self.ok_json(json!({ "deleted": args.asset_id }))
    }

    /// Place a library asset on a localization.
    #[tool(
        description = "Place an App Asset Library image or video on a localization: an App Store \
version, custom product page, in-app event, or product page optimization treatment. Use \
list_asset_library_ref_data for valid placement types and groups."
    )]
    async fn create_asset_library_placement(
        &self,
        Parameters(args): Parameters<CreatePlacementArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = placement_body(&args).map_err(AppStoreServer::map_err)?;
        let value = self
            .client
            .post("/v1/appAssetLibraryPlacements", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        self.ok_json(value)
    }

    /// List the placements on a localization.
    #[tool(
        description = "List the App Asset Library placements on a localization (App Store version, \
custom product page, in-app event, or product page optimization treatment)."
    )]
    async fn list_asset_library_placements(
        &self,
        Parameters(args): Parameters<ListPlacementsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let target = PlacementTarget::parse(&args.target_type).map_err(AppStoreServer::map_err)?;
        let mut query = Vec::new();
        push_opt(&mut query, "limit", args.limit);
        let path = format!(
            "/v1/{}/{}/placements",
            target.resource_type(),
            args.target_id
        );
        let value = self
            .client
            .get(&path, &query)
            .await
            .map_err(AppStoreServer::map_err)?;
        self.ok_json(value)
    }

    /// Remove a placement.
    #[tool(
        description = "Delete an App Asset Library placement, removing the asset from that \
localization. The asset itself stays in the library."
    )]
    async fn delete_asset_library_placement(
        &self,
        Parameters(args): Parameters<DeletePlacementArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.client
            .delete(&format!(
                "/v1/appAssetLibraryPlacements/{}",
                args.placement_id
            ))
            .await
            .map_err(AppStoreServer::map_err)?;
        self.ok_json(json!({ "deleted": args.placement_id }))
    }

    /// Set the display order of placements in a placement group.
    #[tool(
        description = "Set the display order of App Asset Library placements within one placement \
group on a localization, by passing the placement IDs in the desired order."
    )]
    async fn set_asset_library_placement_order(
        &self,
        Parameters(args): Parameters<SetPlacementOrderArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = ordering_body(&args).map_err(AppStoreServer::map_err)?;
        let value = self
            .client
            .post("/v1/appAssetLibraryPlacementOrderingRequests", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        self.ok_json(value)
    }

    /// Read the asset specifications.
    #[tool(
        description = "List App Asset Library reference data: supported placement types, placement \
groups per device, image and video specs (dimensions, file types, limits), and per-feature limits. \
Read this instead of hard-coding screenshot sizes."
    )]
    async fn list_asset_library_ref_data(
        &self,
        Parameters(args): Parameters<ListRefDataArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "filter[placementTypes]", args.placement_types);
        push_opt(
            &mut query,
            "filter[placementProfileGroups]",
            args.placement_profile_groups,
        );
        push_opt(&mut query, "filter[features]", args.features);
        let value = self
            .client
            .get("/v1/appAssetLibraryRefData", &query)
            .await
            .map_err(AppStoreServer::map_err)?;
        self.ok_json(value)
    }
}

impl AppStoreServer {
    async fn list_library_assets(
        &self,
        collection: &str,
        args: ListLibraryAssetsArgs,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "filter[category]", args.category);
        push_opt(&mut query, "filter[state]", args.state);
        push_opt(&mut query, "filter[referenceName]", args.reference_name);
        push_opt(&mut query, "limit", args.limit);
        if args.include_placements.unwrap_or(false) {
            query.push(("include".into(), "placements".into()));
        }
        let path = format!(
            "/v1/appAssetLibraries/{}/{collection}",
            args.asset_library_id
        );
        let value = self
            .client
            .get(&path, &query)
            .await
            .map_err(AppStoreServer::map_err)?;
        self.ok_json(value)
    }
}

// ---- Pure JSON:API document builders (unit-tested below) --------------------

const DEFAULT_CATEGORY: &str = "APP_SCREENSHOTS_AND_PREVIEWS";

fn library_asset_attributes(
    category: &Option<String>,
    reference_name: &Option<String>,
    preview_frame_time_code: &Option<String>,
) -> Value {
    let mut attrs = json!({
        "category": category.as_deref().unwrap_or(DEFAULT_CATEGORY),
    });
    super::set_opt_str(&mut attrs, "referenceName", reference_name);
    super::set_opt_str(&mut attrs, "previewFrameTimeCode", preview_frame_time_code);
    attrs
}

fn library_relationship(asset_library_id: &str) -> Value {
    json!({
        "assetLibrary": {
            "data": { "type": "appAssetLibraries", "id": asset_library_id }
        }
    })
}

fn placement_body(args: &CreatePlacementArgs) -> Result<Value, AscError> {
    let media = MediaKind::parse(&args.media_type)?;
    let target = PlacementTarget::parse(&args.target_type)?;
    let mut attributes = json!({ "placementType": args.placement_type });
    super::set_opt_str(&mut attributes, "placementGroup", &args.placement_group);
    Ok(json!({
        "data": {
            "type": "appAssetLibraryPlacements",
            "attributes": attributes,
            "relationships": {
                media.relationship(): {
                    "data": { "type": media.resource_type(), "id": args.asset_id }
                },
                target.relationship(): {
                    "data": { "type": target.resource_type(), "id": args.target_id }
                }
            }
        }
    }))
}

fn ordering_body(args: &SetPlacementOrderArgs) -> Result<Value, AscError> {
    let target = PlacementTarget::parse(&args.target_type)?;
    if target == PlacementTarget::Event {
        return Err(AscError::InvalidRequest(
            "in-app event placements cannot be reordered; target_type must be an App Store \
             version, custom product page, or experiment treatment localization"
                .into(),
        ));
    }
    let ordered: Vec<Value> = args
        .ordered_placement_ids
        .iter()
        .map(|id| json!({ "type": "appAssetLibraryPlacements", "id": id }))
        .collect();
    Ok(json!({
        "data": {
            "type": "appAssetLibraryPlacementOrderingRequests",
            "attributes": { "placementGroup": args.placement_group },
            "relationships": {
                "orderedPlacements": { "data": ordered },
                target.relationship(): {
                    "data": { "type": target.resource_type(), "id": args.target_id }
                }
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placement(media: &str, target: &str) -> CreatePlacementArgs {
        CreatePlacementArgs {
            media_type: media.into(),
            asset_id: "asset-1".into(),
            target_type: target.into(),
            target_id: "loc-1".into(),
            placement_type: "APP_SCREENSHOT".into(),
            placement_group: Some("IPHONE_DYNAMIC_ISLAND_LARGE_PROFILE".into()),
        }
    }

    #[test]
    fn upload_attributes_default_to_screenshots_and_previews() {
        let a = library_asset_attributes(&None, &None, &None);
        assert_eq!(a, json!({ "category": "APP_SCREENSHOTS_AND_PREVIEWS" }));
        let a = library_asset_attributes(
            &Some("CREATIVE_ASSETS".into()),
            &Some("hero".into()),
            &Some("00:00:05:00".into()),
        );
        assert_eq!(a["category"], "CREATIVE_ASSETS");
        assert_eq!(a["referenceName"], "hero");
        assert_eq!(a["previewFrameTimeCode"], "00:00:05:00");
    }

    #[test]
    fn a_placement_links_the_asset_and_the_target() {
        let b = placement_body(&placement("image", "app_store_version_localization")).unwrap();
        let rel = &b["data"]["relationships"];
        assert_eq!(b["data"]["type"], "appAssetLibraryPlacements");
        assert_eq!(b["data"]["attributes"]["placementType"], "APP_SCREENSHOT");
        assert_eq!(
            b["data"]["attributes"]["placementGroup"],
            "IPHONE_DYNAMIC_ISLAND_LARGE_PROFILE"
        );
        assert_eq!(rel["image"]["data"]["type"], "appAssetLibraryImages");
        assert_eq!(rel["image"]["data"]["id"], "asset-1");
        assert_eq!(
            rel["appStoreVersionLocalization"]["data"]["type"],
            "appStoreVersionLocalizations"
        );
        assert_eq!(rel.as_object().unwrap().len(), 2);
    }

    #[test]
    fn every_target_maps_to_apples_relationship_names() {
        for (target, relationship, resource) in [
            (
                "custom_product_page_localization",
                "appCustomProductPageLocalization",
                "appCustomProductPageLocalizations",
            ),
            (
                "app_event_localization",
                "appEventLocalization",
                "appEventLocalizations",
            ),
            (
                "experiment_treatment_localization",
                "appStoreVersionExperimentTreatmentLocalization",
                "appStoreVersionExperimentTreatmentLocalizations",
            ),
        ] {
            let b = placement_body(&placement("video", target)).unwrap();
            let rel = &b["data"]["relationships"];
            assert_eq!(rel[relationship]["data"]["type"], resource, "{target}");
            assert_eq!(rel["video"]["data"]["type"], "appAssetLibraryVideos");
        }
    }

    #[test]
    fn unknown_media_or_target_is_rejected_before_any_request() {
        assert!(matches!(
            placement_body(&placement("audio", "app_store_version_localization")),
            Err(AscError::InvalidRequest(_))
        ));
        assert!(matches!(
            placement_body(&placement("image", "app_store_version")),
            Err(AscError::InvalidRequest(_))
        ));
    }

    #[test]
    fn ordering_preserves_order_and_names_the_group() {
        let args = SetPlacementOrderArgs {
            target_type: "custom_product_page_localization".into(),
            target_id: "cpp-loc".into(),
            placement_group: "IPHONE_DYNAMIC_ISLAND_LARGE_PROFILE".into(),
            ordered_placement_ids: vec!["c".into(), "a".into(), "b".into()],
        };
        let b = ordering_body(&args).unwrap();
        assert_eq!(
            b["data"]["attributes"]["placementGroup"],
            "IPHONE_DYNAMIC_ISLAND_LARGE_PROFILE"
        );
        let ids: Vec<&str> = b["data"]["relationships"]["orderedPlacements"]["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["c", "a", "b"]);
        assert_eq!(
            b["data"]["relationships"]["appCustomProductPageLocalization"]["data"]["id"],
            "cpp-loc"
        );
    }

    #[test]
    fn event_placements_cannot_be_ordered() {
        let args = SetPlacementOrderArgs {
            target_type: "app_event_localization".into(),
            target_id: "e".into(),
            placement_group: "g".into(),
            ordered_placement_ids: vec![],
        };
        assert!(matches!(
            ordering_body(&args),
            Err(AscError::InvalidRequest(_))
        ));
    }
}
