//! In-app events tools (#18).
//!
//! Schemas verified against Apple's OpenAPI models in the AvdLee Swift SDK
//! (AppEventCreateRequest, AppEventLocalizationCreateRequest,
//!  AppEventScreenshotCreateRequest, AppEventAssetType).
//!
//! JSON key mapping from the SDK's `forKey:` decode lines:
//!   AppEventCreateRequest.attributes:
//!     - referenceName (required), badge?, primaryLocale?
//!   AppEventLocalizationCreateRequest.attributes:
//!     - locale (required), name?, shortDescription?, longDescription?
//!   AppEventLocalizationCreateRequest.relationships:
//!     - appEvent → appEvents
//!   AppEventScreenshotCreateRequest.attributes:
//!     - fileSize (required), fileName (required), appEventAssetType (required)
//!   AppEventScreenshotCreateRequest.relationships:
//!     - appEventLocalization → appEventLocalizations

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{set_opt_str, AppStoreServer};

// ---- Enums ------------------------------------------------------------------

/// Asset-type slot for an in-app event screenshot.
///
/// Verified from AppEventAssetType in the AvdLee Swift SDK
/// (<https://github.com/AvdLee/appstoreconnect-swift-sdk>).
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppEventAssetType {
    EventCard,
    EventDetailsPage,
}

impl AppEventAssetType {
    fn as_api(self) -> &'static str {
        match self {
            AppEventAssetType::EventCard => "EVENT_CARD",
            AppEventAssetType::EventDetailsPage => "EVENT_DETAILS_PAGE",
        }
    }
}

/// Badge type for an in-app event (optional).
///
/// Verified from AppEventCreateRequest.Attributes.Badge in the AvdLee Swift SDK.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppEventBadge {
    LiveEvent,
    Premiere,
    Challenge,
    Competition,
    NewSeason,
    MajorUpdate,
    SpecialEvent,
}

impl AppEventBadge {
    fn as_api(self) -> &'static str {
        match self {
            AppEventBadge::LiveEvent => "LIVE_EVENT",
            AppEventBadge::Premiere => "PREMIERE",
            AppEventBadge::Challenge => "CHALLENGE",
            AppEventBadge::Competition => "COMPETITION",
            AppEventBadge::NewSeason => "NEW_SEASON",
            AppEventBadge::MajorUpdate => "MAJOR_UPDATE",
            AppEventBadge::SpecialEvent => "SPECIAL_EVENT",
        }
    }
}

// ---- Arg structs ------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateAppEventArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// Internal reference name for the event (not shown to customers).
    pub reference_name: String,
    /// Optional badge type. Common values: LIVE_EVENT, PREMIERE, CHALLENGE,
    /// COMPETITION, NEW_SEASON, MAJOR_UPDATE, SPECIAL_EVENT.
    #[serde(default)]
    pub badge: Option<AppEventBadge>,
    /// Optional BCP-47 primary locale for the event, e.g. "en-US".
    #[serde(default)]
    pub primary_locale: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateAppEventLocalizationArgs {
    /// The appEvent ID to attach this localization to.
    pub app_event_id: String,
    /// BCP-47 locale, e.g. "en-US".
    pub locale: String,
    /// Localized event name shown to customers.
    pub name: String,
    /// Short description shown to customers.
    pub short_description: String,
    /// Optional long description shown to customers.
    #[serde(default)]
    pub long_description: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UploadAppEventScreenshotArgs {
    /// The appEventLocalization ID to attach this screenshot to.
    pub app_event_localization_id: String,
    /// Asset type slot: EVENT_CARD or EVENT_DETAILS_PAGE.
    pub app_event_asset_type: AppEventAssetType,
    /// Local path to the screenshot image file (PNG/JPEG).
    pub file_path: String,
}

// ---- Tool router ------------------------------------------------------------

#[tool_router(router = events_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// Create an in-app event for an app.
    #[tool(
        description = "Create an in-app event for an app. Provide a reference_name (internal, not \
shown to customers) and optionally a badge (LIVE_EVENT, PREMIERE, CHALLENGE, COMPETITION, \
NEW_SEASON, MAJOR_UPDATE, SPECIAL_EVENT) and primary_locale (BCP-47, e.g. en-US). After \
creating, add localizations with create_app_event_localization and screenshots with \
upload_app_event_screenshot."
    )]
    async fn create_app_event(
        &self,
        Parameters(args): Parameters<CreateAppEventArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = create_app_event_body(
            &args.app_id,
            &args.reference_name,
            args.badge,
            &args.primary_locale,
        );
        let value = self
            .client
            .post("/v1/appEvents", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Add a localization to an in-app event.
    #[tool(
        description = "Add a localized name and description to an in-app event for a given locale \
(e.g. en-US). Provide the app_event_id, locale, name, and short_description; long_description is \
optional."
    )]
    async fn create_app_event_localization(
        &self,
        Parameters(args): Parameters<CreateAppEventLocalizationArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = create_app_event_localization_body(
            &args.app_event_id,
            &args.locale,
            &args.name,
            &args.short_description,
            &args.long_description,
        );
        let value = self
            .client
            .post("/v1/appEventLocalizations", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Upload a screenshot for an in-app event localization.
    #[tool(
        description = "Upload a screenshot for an in-app event localization (reserve → upload → \
commit with MD5 verification). Provide the app_event_localization_id, app_event_asset_type \
(EVENT_CARD or EVENT_DETAILS_PAGE), and a local image file_path."
    )]
    async fn upload_app_event_screenshot(
        &self,
        Parameters(args): Parameters<UploadAppEventScreenshotArgs>,
    ) -> Result<CallToolResult, McpError> {
        let value = self
            .client
            .upload_asset(
                "/v1/appEventScreenshots",
                "appEventScreenshots",
                json!({ "appEventAssetType": args.app_event_asset_type.as_api() }),
                json!({
                    "appEventLocalization": {
                        "data": {
                            "type": "appEventLocalizations",
                            "id": args.app_event_localization_id
                        }
                    }
                }),
                &args.file_path,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }
}

// ---- Pure JSON:API document builders (unit-tested below) --------------------

fn create_app_event_body(
    app_id: &str,
    reference_name: &str,
    badge: Option<AppEventBadge>,
    primary_locale: &Option<String>,
) -> Value {
    let mut attributes = json!({ "referenceName": reference_name });
    if let Some(b) = badge {
        attributes["badge"] = json!(b.as_api());
    }
    set_opt_str(&mut attributes, "primaryLocale", primary_locale);
    json!({
        "data": {
            "type": "appEvents",
            "attributes": attributes,
            "relationships": {
                "app": { "data": { "type": "apps", "id": app_id } }
            }
        }
    })
}

fn create_app_event_localization_body(
    app_event_id: &str,
    locale: &str,
    name: &str,
    short_description: &str,
    long_description: &Option<String>,
) -> Value {
    let mut attributes = json!({
        "locale": locale,
        "name": name,
        "shortDescription": short_description,
    });
    set_opt_str(&mut attributes, "longDescription", long_description);
    json!({
        "data": {
            "type": "appEventLocalizations",
            "attributes": attributes,
            "relationships": {
                "appEvent": { "data": { "type": "appEvents", "id": app_event_id } }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- AppEventAssetType::as_api tests ------------------------------------

    #[test]
    fn asset_type_as_api_returns_correct_strings() {
        assert_eq!(AppEventAssetType::EventCard.as_api(), "EVENT_CARD");
        assert_eq!(
            AppEventAssetType::EventDetailsPage.as_api(),
            "EVENT_DETAILS_PAGE"
        );
    }

    // ---- create_app_event_body tests ----------------------------------------

    #[test]
    fn event_body_required_fields_only() {
        let b = create_app_event_body("app-1", "Summer Sale", None, &None);
        assert_eq!(b["data"]["type"], "appEvents");
        assert_eq!(b["data"]["attributes"]["referenceName"], "Summer Sale");
        assert!(b["data"]["attributes"].get("badge").is_none());
        assert!(b["data"]["attributes"].get("primaryLocale").is_none());
        assert_eq!(b["data"]["relationships"]["app"]["data"]["type"], "apps");
        assert_eq!(b["data"]["relationships"]["app"]["data"]["id"], "app-1");
    }

    #[test]
    fn event_body_with_badge_and_locale() {
        let b = create_app_event_body(
            "app-2",
            "World Cup",
            Some(AppEventBadge::Challenge),
            &Some("en-US".to_string()),
        );
        assert_eq!(b["data"]["attributes"]["badge"], "CHALLENGE");
        assert_eq!(b["data"]["attributes"]["primaryLocale"], "en-US");
    }

    #[test]
    fn event_body_all_badge_variants_map_correctly() {
        let cases = [
            (AppEventBadge::LiveEvent, "LIVE_EVENT"),
            (AppEventBadge::Premiere, "PREMIERE"),
            (AppEventBadge::Challenge, "CHALLENGE"),
            (AppEventBadge::Competition, "COMPETITION"),
            (AppEventBadge::NewSeason, "NEW_SEASON"),
            (AppEventBadge::MajorUpdate, "MAJOR_UPDATE"),
            (AppEventBadge::SpecialEvent, "SPECIAL_EVENT"),
        ];
        for (variant, expected) in cases {
            let b = create_app_event_body("a", "ref", Some(variant), &None);
            assert_eq!(
                b["data"]["attributes"]["badge"], expected,
                "badge mismatch for {expected}"
            );
        }
    }

    // ---- create_app_event_localization_body tests ---------------------------

    #[test]
    fn localization_body_required_fields() {
        let b = create_app_event_localization_body(
            "evt-1",
            "en-US",
            "Summer Sale",
            "Great deals this summer",
            &None,
        );
        assert_eq!(b["data"]["type"], "appEventLocalizations");
        assert_eq!(b["data"]["attributes"]["locale"], "en-US");
        assert_eq!(b["data"]["attributes"]["name"], "Summer Sale");
        assert_eq!(
            b["data"]["attributes"]["shortDescription"],
            "Great deals this summer"
        );
        assert!(b["data"]["attributes"].get("longDescription").is_none());
        assert_eq!(
            b["data"]["relationships"]["appEvent"]["data"]["type"],
            "appEvents"
        );
        assert_eq!(
            b["data"]["relationships"]["appEvent"]["data"]["id"],
            "evt-1"
        );
    }

    #[test]
    fn localization_body_with_long_description() {
        let b = create_app_event_localization_body(
            "evt-2",
            "fr-FR",
            "Soldes d'été",
            "Bonnes affaires",
            &Some("Une description longue des soldes d'été.".to_string()),
        );
        assert_eq!(
            b["data"]["attributes"]["longDescription"],
            "Une description longue des soldes d'été."
        );
    }
}
