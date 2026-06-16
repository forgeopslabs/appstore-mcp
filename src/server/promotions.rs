//! Promoted in-app purchases (#11): surface IAPs/subscriptions on the product page.
//!
//! Schemas verified against Apple's generated OpenAPI models — note
//! `visibleForAllUsers` is a required attribute on create.

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{push_opt, AppStoreServer};
use crate::error::AscError;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreatePromotedPurchaseArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// Whether the promotion is visible to all users (required).
    pub visible_for_all_users: bool,
    /// Whether the promotion is enabled.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// The in-app purchase ID to promote (provide this OR subscription_id).
    #[serde(default)]
    pub in_app_purchase_id: Option<String>,
    /// The subscription ID to promote (provide this OR in_app_purchase_id).
    #[serde(default)]
    pub subscription_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdatePromotedPurchaseArgs {
    /// The promotedPurchase ID.
    pub promoted_purchase_id: String,
    /// New visibility (optional).
    #[serde(default)]
    pub visible_for_all_users: Option<bool>,
    /// New enabled state (optional).
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetPromotedOrderArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// The promotedPurchase IDs in the desired display order.
    pub ordered_promoted_purchase_ids: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListPromotedPurchasesArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[tool_router(router = promotions_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// Promote an in-app purchase or subscription on the product page.
    #[tool(
        description = "Create a promoted purchase for an app, referencing either an in-app \
purchase or a subscription. visible_for_all_users is required."
    )]
    async fn create_promoted_purchase(
        &self,
        Parameters(args): Parameters<CreatePromotedPurchaseArgs>,
    ) -> Result<CallToolResult, McpError> {
        if args.in_app_purchase_id.is_none() && args.subscription_id.is_none() {
            return Err(AppStoreServer::map_err(AscError::InvalidRequest(
                "provide either in_app_purchase_id or subscription_id".into(),
            )));
        }
        let body = promoted_purchase_body(&args);
        let value = self
            .client
            .post("/v1/promotedPurchases", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Update a promoted purchase's visibility/enabled state.
    #[tool(description = "Update a promoted purchase (visibility and/or enabled state).")]
    async fn update_promoted_purchase(
        &self,
        Parameters(args): Parameters<UpdatePromotedPurchaseArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = promoted_purchase_update_body(&args);
        let value = self
            .client
            .patch(
                &format!("/v1/promotedPurchases/{}", args.promoted_purchase_id),
                body,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Set the display order of an app's promoted purchases.
    #[tool(
        description = "Set the order of an app's promoted purchases by passing the promotedPurchase \
IDs in the desired order."
    )]
    async fn set_promoted_purchase_order(
        &self,
        Parameters(args): Parameters<SetPromotedOrderArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = promoted_order_body(&args.ordered_promoted_purchase_ids);
        let path = format!("/v1/apps/{}/relationships/promotedPurchases", args.app_id);
        self.client
            .patch(&path, body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(json!({ "ordered": args.ordered_promoted_purchase_ids }))
    }

    /// List an app's promoted purchases.
    #[tool(description = "List an app's promoted purchases (in display order).")]
    async fn list_promoted_purchases(
        &self,
        Parameters(args): Parameters<ListPromotedPurchasesArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "limit", args.limit);
        let value = self
            .client
            .get(
                &format!("/v1/apps/{}/promotedPurchases", args.app_id),
                &query,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }
}

// ---- Pure JSON:API document builders (unit-tested below) --------------------

fn promoted_purchase_body(args: &CreatePromotedPurchaseArgs) -> Value {
    let mut attrs = json!({ "visibleForAllUsers": args.visible_for_all_users });
    if let Some(enabled) = args.enabled {
        attrs["enabled"] = json!(enabled);
    }
    let mut rels = json!({ "app": { "data": { "type": "apps", "id": args.app_id } } });
    if let Some(iap) = &args.in_app_purchase_id {
        rels["inAppPurchaseV2"] = json!({ "data": { "type": "inAppPurchases", "id": iap } });
    } else if let Some(sub) = &args.subscription_id {
        rels["subscription"] = json!({ "data": { "type": "subscriptions", "id": sub } });
    }
    json!({
        "data": {
            "type": "promotedPurchases",
            "attributes": attrs,
            "relationships": rels
        }
    })
}

fn promoted_purchase_update_body(args: &UpdatePromotedPurchaseArgs) -> Value {
    let mut attrs = json!({});
    if let Some(v) = args.visible_for_all_users {
        attrs["visibleForAllUsers"] = json!(v);
    }
    if let Some(e) = args.enabled {
        attrs["enabled"] = json!(e);
    }
    json!({
        "data": {
            "type": "promotedPurchases",
            "id": args.promoted_purchase_id,
            "attributes": attrs
        }
    })
}

fn promoted_order_body(ordered_ids: &[String]) -> Value {
    let data: Vec<Value> = ordered_ids
        .iter()
        .map(|id| json!({ "type": "promotedPurchases", "id": id }))
        .collect();
    json!({ "data": data })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn promoted_purchase_uses_iap_relationship() {
        let args = CreatePromotedPurchaseArgs {
            app_id: "app-1".into(),
            visible_for_all_users: true,
            enabled: Some(true),
            in_app_purchase_id: Some("iap-9".into()),
            subscription_id: None,
        };
        let b = promoted_purchase_body(&args);
        assert_eq!(b["data"]["type"], "promotedPurchases");
        assert_eq!(b["data"]["attributes"]["visibleForAllUsers"], true);
        assert_eq!(b["data"]["attributes"]["enabled"], true);
        assert_eq!(b["data"]["relationships"]["app"]["data"]["id"], "app-1");
        assert_eq!(
            b["data"]["relationships"]["inAppPurchaseV2"]["data"]["id"],
            "iap-9"
        );
        assert!(b["data"]["relationships"].get("subscription").is_none());
    }

    #[test]
    fn promoted_purchase_uses_subscription_relationship() {
        let args = CreatePromotedPurchaseArgs {
            app_id: "app-1".into(),
            visible_for_all_users: false,
            enabled: None,
            in_app_purchase_id: None,
            subscription_id: Some("sub-2".into()),
        };
        let b = promoted_purchase_body(&args);
        assert_eq!(b["data"]["attributes"]["visibleForAllUsers"], false);
        assert!(b["data"]["attributes"].get("enabled").is_none());
        assert_eq!(
            b["data"]["relationships"]["subscription"]["data"]["id"],
            "sub-2"
        );
        assert!(b["data"]["relationships"].get("inAppPurchaseV2").is_none());
    }

    #[test]
    fn update_body_includes_only_set_fields() {
        let args = UpdatePromotedPurchaseArgs {
            promoted_purchase_id: "pp-1".into(),
            visible_for_all_users: None,
            enabled: Some(false),
        };
        let b = promoted_purchase_update_body(&args);
        assert_eq!(b["data"]["id"], "pp-1");
        assert_eq!(b["data"]["attributes"]["enabled"], false);
        assert!(b["data"]["attributes"].get("visibleForAllUsers").is_none());
    }

    #[test]
    fn order_body_preserves_sequence() {
        let b = promoted_order_body(&["a".into(), "b".into()]);
        let arr = b["data"].as_array().unwrap();
        assert_eq!(arr[0]["type"], "promotedPurchases");
        assert_eq!(arr[0]["id"], "a");
        assert_eq!(arr[1]["id"], "b");
    }
}
