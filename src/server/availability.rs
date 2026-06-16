//! Territory availability tools for in-app purchases, subscriptions, and apps (issue #7).
//!
//! IAP and subscription availability use a flat to-many `availableTerritories`
//! relationship. App availability uses the newer **v2** shape, where each
//! territory is an `included` `territoryAvailabilities` resource linked by a
//! developer-defined temporary id.

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::AppStoreServer;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProductAvailabilityArgs {
    /// The product ID (in-app purchase ID or subscription ID).
    pub product_id: String,
    /// Territory IDs to make the product available in, e.g. ["USA", "GBR"].
    pub territory_ids: Vec<String>,
    /// Whether to make it available in future new territories automatically.
    /// Required by the API.
    pub available_in_new_territories: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AppAvailabilityArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// Territory IDs to make the app available in, e.g. ["USA", "GBR"].
    pub territory_ids: Vec<String>,
    /// Whether to make it available in future new territories automatically.
    /// Required by the API.
    pub available_in_new_territories: bool,
}

#[tool_router(router = availability_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// Set the territories an in-app purchase is available in.
    #[tool(
        description = "Set the territories an in-app purchase is available in (territory IDs from \
list_territories)."
    )]
    async fn set_iap_availability(
        &self,
        Parameters(args): Parameters<ProductAvailabilityArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = product_availability_body(
            "inAppPurchaseAvailabilities",
            "inAppPurchase",
            "inAppPurchases",
            &args,
        );
        let value = self
            .client
            .post("/v1/inAppPurchaseAvailabilities", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Set the territories a subscription is available in.
    #[tool(
        description = "Set the territories a subscription is available in (territory IDs from \
list_territories)."
    )]
    async fn set_subscription_availability(
        &self,
        Parameters(args): Parameters<ProductAvailabilityArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = product_availability_body(
            "subscriptionAvailabilities",
            "subscription",
            "subscriptions",
            &args,
        );
        let value = self
            .client
            .post("/v1/subscriptionAvailabilities", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Set the territories an app is available in.
    #[tool(
        description = "Set the territories an app is available in (territory IDs from \
list_territories). Uses the App Availability v2 API."
    )]
    async fn set_app_availability(
        &self,
        Parameters(args): Parameters<AppAvailabilityArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = app_availability_body(&args);
        let value = self
            .client
            .post("/v2/appAvailabilities", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }
}

// ---- Pure JSON:API document builders (unit-tested below) --------------------

fn territory_refs(ids: &[String]) -> Vec<Value> {
    ids.iter()
        .map(|id| json!({ "type": "territories", "id": id }))
        .collect()
}

/// Build an IAP/subscription availability document (flat `availableTerritories`).
fn product_availability_body(
    resource_type: &str,
    relationship_key: &str,
    relationship_type: &str,
    args: &ProductAvailabilityArgs,
) -> Value {
    json!({
        "data": {
            "type": resource_type,
            "attributes": { "availableInNewTerritories": args.available_in_new_territories },
            "relationships": {
                relationship_key: {
                    "data": { "type": relationship_type, "id": args.product_id }
                },
                "availableTerritories": {
                    "data": territory_refs(&args.territory_ids)
                }
            }
        }
    })
}

/// Build an app availability v2 document with `territoryAvailabilities` includes.
fn app_availability_body(args: &AppAvailabilityArgs) -> Value {
    let mut data_refs = Vec::with_capacity(args.territory_ids.len());
    let mut included = Vec::with_capacity(args.territory_ids.len());
    for (i, territory) in args.territory_ids.iter().enumerate() {
        // App Store Connect requires temporary `included` ids in the `${...}`
        // placeholder format; hyphenated ids are rejected with INVALID_ID.
        let temp_id = format!("${{avail{i}}}");
        data_refs.push(json!({ "type": "territoryAvailabilities", "id": temp_id }));
        included.push(json!({
            "type": "territoryAvailabilities",
            "id": temp_id,
            "attributes": { "available": true },
            "relationships": {
                "territory": { "data": { "type": "territories", "id": territory } }
            }
        }));
    }

    json!({
        "data": {
            "type": "appAvailabilities",
            "attributes": { "availableInNewTerritories": args.available_in_new_territories },
            "relationships": {
                "app": { "data": { "type": "apps", "id": args.app_id } },
                "territoryAvailabilities": { "data": data_refs }
            }
        },
        "included": included
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product_args() -> ProductAvailabilityArgs {
        ProductAvailabilityArgs {
            product_id: "iap-1".into(),
            territory_ids: vec!["USA".into(), "GBR".into()],
            available_in_new_territories: true,
        }
    }

    #[test]
    fn iap_availability_body_shape() {
        let b = product_availability_body(
            "inAppPurchaseAvailabilities",
            "inAppPurchase",
            "inAppPurchases",
            &product_args(),
        );
        assert_eq!(b["data"]["type"], "inAppPurchaseAvailabilities");
        assert_eq!(b["data"]["attributes"]["availableInNewTerritories"], true);
        assert_eq!(
            b["data"]["relationships"]["inAppPurchase"]["data"]["type"],
            "inAppPurchases"
        );
        assert_eq!(
            b["data"]["relationships"]["inAppPurchase"]["data"]["id"],
            "iap-1"
        );
        let terr = b["data"]["relationships"]["availableTerritories"]["data"]
            .as_array()
            .expect("territories array");
        assert_eq!(terr.len(), 2);
        assert_eq!(terr[0]["type"], "territories");
        assert_eq!(terr[0]["id"], "USA");
        assert_eq!(terr[1]["id"], "GBR");
    }

    #[test]
    fn subscription_availability_uses_subscription_relationship() {
        let b = product_availability_body(
            "subscriptionAvailabilities",
            "subscription",
            "subscriptions",
            &product_args(),
        );
        assert_eq!(b["data"]["type"], "subscriptionAvailabilities");
        assert_eq!(
            b["data"]["relationships"]["subscription"]["data"]["type"],
            "subscriptions"
        );
        assert!(b["data"]["relationships"].get("inAppPurchase").is_none());
    }

    #[test]
    fn product_availability_always_includes_required_flag() {
        let args = ProductAvailabilityArgs {
            product_id: "iap-1".into(),
            territory_ids: vec!["USA".into()],
            available_in_new_territories: false,
        };
        let b = product_availability_body(
            "inAppPurchaseAvailabilities",
            "inAppPurchase",
            "inAppPurchases",
            &args,
        );
        // The API requires this attribute, so it must always be present.
        assert_eq!(b["data"]["attributes"]["availableInNewTerritories"], false);
    }

    #[test]
    fn app_availability_body_links_included_by_temp_id() {
        let args = AppAvailabilityArgs {
            app_id: "app-9".into(),
            territory_ids: vec!["USA".into(), "JPN".into()],
            available_in_new_territories: false,
        };
        let b = app_availability_body(&args);
        assert_eq!(b["data"]["type"], "appAvailabilities");
        assert_eq!(b["data"]["attributes"]["availableInNewTerritories"], false);
        assert_eq!(b["data"]["relationships"]["app"]["data"]["id"], "app-9");

        let refs = b["data"]["relationships"]["territoryAvailabilities"]["data"]
            .as_array()
            .expect("refs array");
        let included = b["included"].as_array().expect("included array");
        assert_eq!(refs.len(), 2);
        assert_eq!(included.len(), 2);

        // Each data ref's temp id must resolve to an included resource that
        // points at the matching territory. Apple requires the `${...}` format.
        assert_eq!(refs[0]["id"], "${avail0}");
        assert_eq!(included[0]["id"], "${avail0}");
        assert_eq!(refs[1]["id"], "${avail1}");
        assert_eq!(included[1]["id"], "${avail1}");
        assert_eq!(included[0]["type"], "territoryAvailabilities");
        assert_eq!(included[0]["attributes"]["available"], true);
        assert_eq!(
            included[0]["relationships"]["territory"]["data"]["id"],
            "USA"
        );
        assert_eq!(
            included[1]["relationships"]["territory"]["data"]["id"],
            "JPN"
        );
    }
}
