//! Pricing reference tools: territories and price points.
//!
//! Setting prices (in iap.rs / subscriptions.rs) requires a *price point* ID.
//! These tools resolve territories and the available price points for an IAP or
//! subscription so the agent can pick the right one.

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;

use super::{push_opt, AppStoreServer};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListTerritoriesArgs {
    /// Page size (max 200). Defaults to 200 to return all territories in one page.
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IapPricePointsArgs {
    /// The in-app purchase ID.
    pub iap_id: String,
    /// Filter to a single territory, e.g. "USA". Recommended to keep results small.
    #[serde(default)]
    pub territory: Option<String>,
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SubPricePointsArgs {
    /// The subscription ID.
    pub subscription_id: String,
    /// Filter to a single territory, e.g. "USA". Recommended to keep results small.
    #[serde(default)]
    pub territory: Option<String>,
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[tool_router(router = pricing_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// List all App Store territories.
    #[tool(
        description = "List App Store territories (territory IDs like \"USA\", \"GBR\", used for pricing)."
    )]
    async fn list_territories(
        &self,
        Parameters(args): Parameters<ListTerritoriesArgs>,
    ) -> Result<CallToolResult, McpError> {
        let limit = args.limit.unwrap_or(200);
        let value = self
            .client
            .get("/v1/territories", &[("limit".into(), limit.to_string())])
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// List available price points for an in-app purchase.
    #[tool(
        description = "List the available price points for an in-app purchase (each has an id and \
customerPrice). Use the id with set_iap_price_schedule. Filter by territory to narrow results."
    )]
    async fn list_iap_price_points(
        &self,
        Parameters(args): Parameters<IapPricePointsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "filter[territory]", args.territory);
        push_opt(&mut query, "limit", args.limit);
        let value = self
            .client
            .get(
                &format!("/v2/inAppPurchases/{}/pricePoints", args.iap_id),
                &query,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// List available price points for a subscription.
    #[tool(
        description = "List the available price points for a subscription (each has an id and \
customerPrice). Use the id with set_subscription_price. Filter by territory to narrow results."
    )]
    async fn list_subscription_price_points(
        &self,
        Parameters(args): Parameters<SubPricePointsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "filter[territory]", args.territory);
        push_opt(&mut query, "limit", args.limit);
        let value = self
            .client
            .get(
                &format!("/v1/subscriptions/{}/pricePoints", args.subscription_id),
                &query,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }
}
