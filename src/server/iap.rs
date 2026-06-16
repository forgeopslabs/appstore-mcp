//! In-app purchase tools (App Store Connect IAP **v2**).
//!
//! Lifecycle: create the IAP, add localizations, set a price schedule, and
//! attach an App Store review screenshot.

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{push_opt, AppStoreServer};

/// The kind of in-app purchase.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IapType {
    Consumable,
    NonConsumable,
    NonRenewingSubscription,
}

impl IapType {
    fn as_api(self) -> &'static str {
        match self {
            IapType::Consumable => "CONSUMABLE",
            IapType::NonConsumable => "NON_CONSUMABLE",
            IapType::NonRenewingSubscription => "NON_RENEWING_SUBSCRIPTION",
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListIapArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// Filter by exact productId.
    #[serde(default)]
    pub product_id: Option<String>,
    /// Comma-separated includes, e.g. "inAppPurchaseLocalizations,iapPriceSchedule".
    #[serde(default)]
    pub include: Option<String>,
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateIapArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// Reference name shown in App Store Connect (not visible to customers).
    pub name: String,
    /// Unique product ID, e.g. "com.example.app.coins_100".
    pub product_id: String,
    /// Purchase type.
    pub iap_type: IapType,
    /// Optional note for App Review.
    #[serde(default)]
    pub review_note: Option<String>,
    /// Whether the purchase is family-sharable.
    #[serde(default)]
    pub family_sharable: Option<bool>,
    /// Whether to make it available in all current and future territories.
    #[serde(default)]
    pub available_in_all_territories: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateIapArgs {
    /// The in-app purchase ID.
    pub iap_id: String,
    /// Attributes to update, e.g. {"name": "...", "reviewNote": "...", "familySharable": true}.
    pub attributes: Value,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteIapArgs {
    /// The in-app purchase ID.
    pub iap_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateIapLocalizationArgs {
    /// The in-app purchase ID.
    pub iap_id: String,
    /// BCP-47 locale, e.g. "en-US".
    pub locale: String,
    /// Display name shown to customers.
    pub name: String,
    /// Optional customer-facing description.
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetIapPriceArgs {
    /// The in-app purchase ID.
    pub iap_id: String,
    /// The inAppPurchasePricePoint ID (look up with list_iap_price_points).
    pub price_point_id: String,
    /// Base territory for the price schedule (default "USA").
    #[serde(default)]
    pub base_territory: Option<String>,
    /// Optional ISO-8601 start date (YYYY-MM-DD). Null/absent means effective immediately.
    #[serde(default)]
    pub start_date: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UploadIapScreenshotArgs {
    /// The in-app purchase ID.
    pub iap_id: String,
    /// Local path to the review screenshot image file (PNG/JPEG).
    pub file_path: String,
}

#[tool_router(router = iap_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// List an app's in-app purchases (v2).
    #[tool(
        description = "List an app's in-app purchases (IAP v2), optionally filtered by productId."
    )]
    async fn list_in_app_purchases(
        &self,
        Parameters(args): Parameters<ListIapArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "filter[productId]", args.product_id);
        push_opt(&mut query, "include", args.include);
        push_opt(&mut query, "limit", args.limit);
        let value = self
            .client
            .get(
                &format!("/v1/apps/{}/inAppPurchasesV2", args.app_id),
                &query,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Create a new in-app purchase.
    #[tool(
        description = "Create an in-app purchase (v2): provide a reference name, productId, and \
type (CONSUMABLE, NON_CONSUMABLE, or NON_RENEWING_SUBSCRIPTION). Add localizations and a price \
afterward."
    )]
    async fn create_in_app_purchase(
        &self,
        Parameters(args): Parameters<CreateIapArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut attributes = json!({
            "name": args.name,
            "productId": args.product_id,
            "inAppPurchaseType": args.iap_type.as_api(),
        });
        if let Some(note) = args.review_note {
            attributes["reviewNote"] = json!(note);
        }
        if let Some(fs) = args.family_sharable {
            attributes["familySharable"] = json!(fs);
        }
        if let Some(all) = args.available_in_all_territories {
            attributes["availableInAllTerritories"] = json!(all);
        }
        let body = json!({
            "data": {
                "type": "inAppPurchases",
                "attributes": attributes,
                "relationships": {
                    "app": { "data": { "type": "apps", "id": args.app_id } }
                }
            }
        });
        let value = self
            .client
            .post("/v2/inAppPurchases", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Update an in-app purchase.
    #[tool(
        description = "Update an in-app purchase's attributes (name, reviewNote, familySharable, etc.)."
    )]
    async fn update_in_app_purchase(
        &self,
        Parameters(args): Parameters<UpdateIapArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = json!({
            "data": { "type": "inAppPurchases", "id": args.iap_id, "attributes": args.attributes }
        });
        let value = self
            .client
            .patch(&format!("/v2/inAppPurchases/{}", args.iap_id), body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Delete an in-app purchase.
    #[tool(description = "Delete an in-app purchase by ID (only allowed before it is approved).")]
    async fn delete_in_app_purchase(
        &self,
        Parameters(args): Parameters<DeleteIapArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.client
            .delete(&format!("/v2/inAppPurchases/{}", args.iap_id))
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(json!({ "deleted": args.iap_id }))
    }

    /// Add a localization to an in-app purchase.
    #[tool(
        description = "Add a localized name/description to an in-app purchase for a given locale (e.g. en-US)."
    )]
    async fn create_iap_localization(
        &self,
        Parameters(args): Parameters<CreateIapLocalizationArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut attributes = json!({ "locale": args.locale, "name": args.name });
        if let Some(desc) = args.description {
            attributes["description"] = json!(desc);
        }
        let body = json!({
            "data": {
                "type": "inAppPurchaseLocalizations",
                "attributes": attributes,
                "relationships": {
                    "inAppPurchaseV2": { "data": { "type": "inAppPurchases", "id": args.iap_id } }
                }
            }
        });
        let value = self
            .client
            .post("/v1/inAppPurchaseLocalizations", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Set the price of an in-app purchase by creating a price schedule.
    #[tool(
        description = "Set an in-app purchase's price by creating a price schedule from a price \
point (look up the price_point_id with list_iap_price_points). Defaults to base territory USA, \
effective immediately."
    )]
    async fn set_iap_price_schedule(
        &self,
        Parameters(args): Parameters<SetIapPriceArgs>,
    ) -> Result<CallToolResult, McpError> {
        let base_territory = args.base_territory.unwrap_or_else(|| "USA".to_string());
        let body = iap_price_schedule_body(
            &args.iap_id,
            &args.price_point_id,
            &base_territory,
            args.start_date.as_deref(),
        );
        let value = self
            .client
            .post("/v1/inAppPurchasePriceSchedules", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Upload an App Store review screenshot for an in-app purchase.
    #[tool(
        description = "Upload an App Store review screenshot for an in-app purchase (reserve → \
upload → commit, with MD5 verification). Provide a local image file path."
    )]
    async fn upload_iap_review_screenshot(
        &self,
        Parameters(args): Parameters<UploadIapScreenshotArgs>,
    ) -> Result<CallToolResult, McpError> {
        let relationships = json!({
            "inAppPurchaseV2": { "data": { "type": "inAppPurchases", "id": args.iap_id } }
        });
        let value = self
            .client
            .upload_asset(
                "/v1/inAppPurchaseAppStoreReviewScreenshots",
                "inAppPurchaseAppStoreReviewScreenshots",
                relationships,
                &args.file_path,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }
}

// ---- Pure JSON:API document builders (unit-tested below) --------------------

/// Build an inAppPurchasePriceSchedules document.
///
/// The price is a temporary `inAppPurchasePrices` resource in `included`, linked
/// from `manualPrices` by a temporary id. App Store Connect requires that id to
/// use the `${...}` placeholder format — plain ids with hyphens are rejected with
/// `ENTITY_ERROR.INCLUDED.INVALID_ID` (found via live integration testing).
fn iap_price_schedule_body(
    iap_id: &str,
    price_point_id: &str,
    base_territory: &str,
    start_date: Option<&str>,
) -> Value {
    const TEMP_ID: &str = "${price1}";
    json!({
        "data": {
            "type": "inAppPurchasePriceSchedules",
            "relationships": {
                "inAppPurchase": { "data": { "type": "inAppPurchases", "id": iap_id } },
                "baseTerritory": { "data": { "type": "territories", "id": base_territory } },
                "manualPrices": { "data": [ { "type": "inAppPurchasePrices", "id": TEMP_ID } ] }
            }
        },
        "included": [
            {
                "type": "inAppPurchasePrices",
                "id": TEMP_ID,
                "attributes": { "startDate": start_date },
                "relationships": {
                    "inAppPurchasePricePoint": {
                        "data": { "type": "inAppPurchasePricePoints", "id": price_point_id }
                    }
                }
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iap_type_maps_to_api_strings() {
        assert_eq!(IapType::Consumable.as_api(), "CONSUMABLE");
        assert_eq!(IapType::NonConsumable.as_api(), "NON_CONSUMABLE");
        assert_eq!(
            IapType::NonRenewingSubscription.as_api(),
            "NON_RENEWING_SUBSCRIPTION"
        );
    }

    #[test]
    fn price_schedule_uses_placeholder_temp_id_consistently() {
        let b = iap_price_schedule_body("iap-1", "pp-9", "USA", None);
        assert_eq!(b["data"]["type"], "inAppPurchasePriceSchedules");

        // The manualPrices ref and the included resource must share the SAME
        // `${...}`-formatted temporary id (Apple rejects hyphenated ids).
        let manual_id = &b["data"]["relationships"]["manualPrices"]["data"][0]["id"];
        let included_id = &b["included"][0]["id"];
        assert_eq!(manual_id, "${price1}");
        assert_eq!(included_id, "${price1}");
        assert_eq!(manual_id, included_id);

        assert_eq!(b["data"]["relationships"]["inAppPurchase"]["data"]["id"], "iap-1");
        assert_eq!(b["data"]["relationships"]["baseTerritory"]["data"]["id"], "USA");
        assert_eq!(
            b["included"][0]["relationships"]["inAppPurchasePricePoint"]["data"]["id"],
            "pp-9"
        );
        // The redundant inAppPurchaseV2 relationship must NOT be present — Apple
        // accepts the document without it.
        assert!(b["included"][0]["relationships"]
            .get("inAppPurchaseV2")
            .is_none());
        // startDate omitted -> null (effective immediately).
        assert!(b["included"][0]["attributes"]["startDate"].is_null());
    }

    #[test]
    fn price_schedule_includes_start_date_when_given() {
        let b = iap_price_schedule_body("iap-1", "pp-9", "GBR", Some("2026-07-01"));
        assert_eq!(b["included"][0]["attributes"]["startDate"], "2026-07-01");
        assert_eq!(b["data"]["relationships"]["baseTerritory"]["data"]["id"], "GBR");
    }
}
