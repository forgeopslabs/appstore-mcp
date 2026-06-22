//! Auto-renewable subscription tools: groups, subscriptions, localizations, prices.

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{de_coerce_json, push_opt, AppStoreServer};

/// Renewal period of a subscription.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SubscriptionPeriod {
    OneWeek,
    OneMonth,
    TwoMonths,
    ThreeMonths,
    SixMonths,
    OneYear,
}

impl SubscriptionPeriod {
    fn as_api(self) -> &'static str {
        match self {
            SubscriptionPeriod::OneWeek => "ONE_WEEK",
            SubscriptionPeriod::OneMonth => "ONE_MONTH",
            SubscriptionPeriod::TwoMonths => "TWO_MONTHS",
            SubscriptionPeriod::ThreeMonths => "THREE_MONTHS",
            SubscriptionPeriod::SixMonths => "SIX_MONTHS",
            SubscriptionPeriod::OneYear => "ONE_YEAR",
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListSubGroupsArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// Comma-separated includes, e.g. "subscriptions".
    #[serde(default)]
    pub include: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateSubGroupArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// Reference name for the subscription group (not customer-facing).
    pub reference_name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateSubscriptionArgs {
    /// The subscription group ID to add this subscription to.
    pub group_id: String,
    /// Reference name (not customer-facing).
    pub name: String,
    /// Unique product ID, e.g. "com.example.app.pro_monthly".
    pub product_id: String,
    /// Renewal period.
    pub subscription_period: SubscriptionPeriod,
    /// Rank within the group (1 = highest level/most features). Defaults to 1.
    #[serde(default)]
    pub group_level: Option<u32>,
    /// Whether the subscription is family-sharable.
    #[serde(default)]
    pub family_sharable: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateSubscriptionArgs {
    /// The subscription ID.
    pub subscription_id: String,
    /// Attributes to update, e.g. {"name": "...", "groupLevel": 2}.
    #[serde(deserialize_with = "de_coerce_json")]
    pub attributes: Value,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateSubLocalizationArgs {
    /// The subscription ID.
    pub subscription_id: String,
    /// BCP-47 locale, e.g. "en-US".
    pub locale: String,
    /// Display name shown to customers.
    pub name: String,
    /// Optional customer-facing description.
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetSubPriceArgs {
    /// The subscription ID.
    pub subscription_id: String,
    /// The subscriptionPricePoint ID (look up with list_subscription_price_points).
    pub price_point_id: String,
    /// Territory ID (default "USA").
    #[serde(default)]
    pub territory: Option<String>,
    /// Optional ISO-8601 start date (YYYY-MM-DD). Absent means effective immediately.
    #[serde(default)]
    pub start_date: Option<String>,
    /// Preserve the current price for existing subscribers (no price increase consent).
    #[serde(default)]
    pub preserve_current_price: Option<bool>,
}

#[tool_router(router = subscriptions_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// List an app's subscription groups.
    #[tool(description = "List an app's subscription groups.")]
    async fn list_subscription_groups(
        &self,
        Parameters(args): Parameters<ListSubGroupsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "include", args.include);
        let value = self
            .client
            .get(
                &format!("/v1/apps/{}/subscriptionGroups", args.app_id),
                &query,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Create a subscription group.
    #[tool(
        description = "Create a subscription group for an app (subscriptions live inside a group)."
    )]
    async fn create_subscription_group(
        &self,
        Parameters(args): Parameters<CreateSubGroupArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = json!({
            "data": {
                "type": "subscriptionGroups",
                "attributes": { "referenceName": args.reference_name },
                "relationships": {
                    "app": { "data": { "type": "apps", "id": args.app_id } }
                }
            }
        });
        let value = self
            .client
            .post("/v1/subscriptionGroups", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Create an auto-renewable subscription within a group.
    #[tool(
        description = "Create an auto-renewable subscription inside a group: reference name, \
productId, and renewal period (ONE_WEEK..ONE_YEAR)."
    )]
    async fn create_subscription(
        &self,
        Parameters(args): Parameters<CreateSubscriptionArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut attributes = json!({
            "name": args.name,
            "productId": args.product_id,
            "subscriptionPeriod": args.subscription_period.as_api(),
        });
        if let Some(level) = args.group_level {
            attributes["groupLevel"] = json!(level);
        }
        if let Some(fs) = args.family_sharable {
            attributes["familySharable"] = json!(fs);
        }
        let body = json!({
            "data": {
                "type": "subscriptions",
                "attributes": attributes,
                "relationships": {
                    "group": { "data": { "type": "subscriptionGroups", "id": args.group_id } }
                }
            }
        });
        let value = self
            .client
            .post("/v1/subscriptions", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Update a subscription.
    #[tool(
        description = "Update a subscription's attributes (name, groupLevel, familySharable, etc.)."
    )]
    async fn update_subscription(
        &self,
        Parameters(args): Parameters<UpdateSubscriptionArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = json!({
            "data": { "type": "subscriptions", "id": args.subscription_id, "attributes": args.attributes }
        });
        let value = self
            .client
            .patch(&format!("/v1/subscriptions/{}", args.subscription_id), body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Add a localization to a subscription.
    #[tool(description = "Add a localized name/description to a subscription for a given locale.")]
    async fn create_subscription_localization(
        &self,
        Parameters(args): Parameters<CreateSubLocalizationArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut attributes = json!({ "locale": args.locale, "name": args.name });
        if let Some(desc) = args.description {
            attributes["description"] = json!(desc);
        }
        let body = json!({
            "data": {
                "type": "subscriptionLocalizations",
                "attributes": attributes,
                "relationships": {
                    "subscription": { "data": { "type": "subscriptions", "id": args.subscription_id } }
                }
            }
        });
        let value = self
            .client
            .post("/v1/subscriptionLocalizations", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Set a subscription's price in a territory.
    #[tool(
        description = "Set a subscription's price from a price point in a territory (look up the \
price_point_id with list_subscription_price_points). Defaults to territory USA."
    )]
    async fn set_subscription_price(
        &self,
        Parameters(args): Parameters<SetSubPriceArgs>,
    ) -> Result<CallToolResult, McpError> {
        let territory = args.territory.unwrap_or_else(|| "USA".to_string());
        let mut attributes = json!({});
        if let Some(start) = args.start_date {
            attributes["startDate"] = json!(start);
        }
        if let Some(preserve) = args.preserve_current_price {
            attributes["preserveCurrentPrice"] = json!(preserve);
        }
        let body = json!({
            "data": {
                "type": "subscriptionPrices",
                "attributes": attributes,
                "relationships": {
                    "subscription": { "data": { "type": "subscriptions", "id": args.subscription_id } },
                    "subscriptionPricePoint": {
                        "data": { "type": "subscriptionPricePoints", "id": args.price_point_id }
                    },
                    "territory": { "data": { "type": "territories", "id": territory } }
                }
            }
        });
        let value = self
            .client
            .post("/v1/subscriptionPrices", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }
}
