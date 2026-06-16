//! Users & access management tools (#14).
//!
//! Schemas verified against Apple's generated OpenAPI models in the AvdLee
//! Swift SDK (UserInvitationCreateRequest, UserUpdateRequest).
//!
//! JSON key mapping from the SDK's `forKey:` decode lines:
//!   - attributes: email, firstName, lastName, roles, allAppsVisible,
//!     provisioningAllowed
//!   - relationships: visibleApps (to-many list of apps)

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{push_opt, AppStoreServer};

// ---- Arg structs ------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListUsersArgs {
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
    /// Comma-separated related resources to include, e.g. "visibleApps".
    #[serde(default)]
    pub include: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InviteUserArgs {
    /// Email address of the user to invite.
    pub email: String,
    /// Given name of the user.
    pub first_name: String,
    /// Family name of the user.
    pub last_name: String,
    /// Roles to assign. Common values: ADMIN, FINANCE, DEVELOPER, MARKETING,
    /// APP_MANAGER, SALES, CUSTOMER_SUPPORT, ACCESS_TO_REPORTS, CREATE_APPS.
    pub roles: Vec<String>,
    /// Whether the user can see all apps (true) or only the apps in
    /// visible_app_ids (false / omitted).
    #[serde(default)]
    pub all_apps_visible: Option<bool>,
    /// Whether the user may access provisioning/signing resources.
    #[serde(default)]
    pub provisioning_allowed: Option<bool>,
    /// App IDs the invited user should have access to (only used when
    /// all_apps_visible is false or omitted).
    #[serde(default)]
    pub visible_app_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateUserArgs {
    /// The user's App Store Connect ID.
    pub user_id: String,
    /// New roles to assign. Common values: ADMIN, FINANCE, DEVELOPER,
    /// MARKETING, APP_MANAGER, SALES, CUSTOMER_SUPPORT, ACCESS_TO_REPORTS,
    /// CREATE_APPS.
    #[serde(default)]
    pub roles: Option<Vec<String>>,
    /// Whether the user should see all apps.
    #[serde(default)]
    pub all_apps_visible: Option<bool>,
    /// Whether the user is allowed to provision certificates and devices.
    #[serde(default)]
    pub provisioning_allowed: Option<bool>,
    /// App IDs the user should have access to. Replaces the existing list.
    /// Only sent when provided and non-empty.
    #[serde(default)]
    pub visible_app_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RemoveUserArgs {
    /// The user's App Store Connect ID.
    pub user_id: String,
}

// ---- Tool impl block --------------------------------------------------------

#[tool_router(router = users_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// List users on the team.
    #[tool(
        description = "List all users on the App Store Connect team. Optionally pass \
include=visibleApps to include the apps each user can access."
    )]
    async fn list_users(
        &self,
        Parameters(args): Parameters<ListUsersArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "limit", args.limit);
        push_opt(&mut query, "include", args.include);
        let value = self
            .client
            .get("/v1/users", &query)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Invite a new user to the team.
    #[tool(
        description = "Invite a new user to the App Store Connect team. Provide email, \
first_name, last_name, and one or more roles (e.g. DEVELOPER, ADMIN). Optionally set \
all_apps_visible or supply a list of visible_app_ids."
    )]
    async fn invite_user(
        &self,
        Parameters(args): Parameters<InviteUserArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = user_invitation_body(&args);
        let value = self
            .client
            .post("/v1/userInvitations", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Update a team user's roles or app visibility.
    #[tool(
        description = "Update a team user's roles, app-visibility flag, provisioning permission, \
or visible apps. Only fields that are provided are sent to the API."
    )]
    async fn update_user(
        &self,
        Parameters(args): Parameters<UpdateUserArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = user_update_body(&args);
        let value = self
            .client
            .patch(&format!("/v1/users/{}", args.user_id), body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Remove a user from the team.
    #[tool(description = "Remove a user from the App Store Connect team by their user ID.")]
    async fn remove_user(
        &self,
        Parameters(args): Parameters<RemoveUserArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.client
            .delete(&format!("/v1/users/{}", args.user_id))
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(json!({ "deleted": args.user_id }))
    }
}

// ---- Pure JSON:API document builders (unit-tested below) --------------------

/// Build the request body for POST /v1/userInvitations.
fn user_invitation_body(args: &InviteUserArgs) -> Value {
    let mut attrs = json!({
        "email": args.email,
        "firstName": args.first_name,
        "lastName": args.last_name,
        "roles": args.roles,
    });
    if let Some(v) = args.all_apps_visible {
        attrs["allAppsVisible"] = json!(v);
    }
    if let Some(v) = args.provisioning_allowed {
        attrs["provisioningAllowed"] = json!(v);
    }

    let mut doc = json!({
        "data": {
            "type": "userInvitations",
            "attributes": attrs,
        }
    });

    // Only attach visibleApps relationship when a non-empty list is provided.
    if let Some(ids) = &args.visible_app_ids {
        if !ids.is_empty() {
            let data: Vec<Value> = ids
                .iter()
                .map(|id| json!({ "type": "apps", "id": id }))
                .collect();
            doc["data"]["relationships"] = json!({
                "visibleApps": { "data": data }
            });
        }
    }

    doc
}

/// Build the request body for PATCH /v1/users/{id}.
fn user_update_body(args: &UpdateUserArgs) -> Value {
    let mut attrs = json!({});
    if let Some(roles) = &args.roles {
        attrs["roles"] = json!(roles);
    }
    if let Some(v) = args.all_apps_visible {
        attrs["allAppsVisible"] = json!(v);
    }
    if let Some(v) = args.provisioning_allowed {
        attrs["provisioningAllowed"] = json!(v);
    }

    let mut doc = json!({
        "data": {
            "type": "users",
            "id": args.user_id,
            "attributes": attrs,
        }
    });

    // Only attach visibleApps relationship when a non-empty list is provided.
    if let Some(ids) = &args.visible_app_ids {
        if !ids.is_empty() {
            let data: Vec<Value> = ids
                .iter()
                .map(|id| json!({ "type": "apps", "id": id }))
                .collect();
            doc["data"]["relationships"] = json!({
                "visibleApps": { "data": data }
            });
        }
    }

    doc
}

// ---- Unit tests -------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- invite_user body ---

    #[test]
    fn invitation_body_required_fields() {
        let args = InviteUserArgs {
            email: "dev@example.com".into(),
            first_name: "Jane".into(),
            last_name: "Doe".into(),
            roles: vec!["DEVELOPER".into()],
            all_apps_visible: None,
            provisioning_allowed: None,
            visible_app_ids: None,
        };
        let b = user_invitation_body(&args);
        assert_eq!(b["data"]["type"], "userInvitations");
        assert_eq!(b["data"]["attributes"]["email"], "dev@example.com");
        assert_eq!(b["data"]["attributes"]["firstName"], "Jane");
        assert_eq!(b["data"]["attributes"]["lastName"], "Doe");
        assert_eq!(b["data"]["attributes"]["roles"][0], "DEVELOPER");
        // Optional fields absent when not provided
        assert!(b["data"]["attributes"].get("allAppsVisible").is_none());
        assert!(b["data"]["attributes"].get("provisioningAllowed").is_none());
        // No relationships block when no visible_app_ids
        assert!(b["data"].get("relationships").is_none());
    }

    #[test]
    fn invitation_body_with_all_apps_visible() {
        let args = InviteUserArgs {
            email: "admin@example.com".into(),
            first_name: "John".into(),
            last_name: "Smith".into(),
            roles: vec!["ADMIN".into(), "FINANCE".into()],
            all_apps_visible: Some(true),
            provisioning_allowed: None,
            visible_app_ids: None,
        };
        let b = user_invitation_body(&args);
        assert_eq!(b["data"]["attributes"]["allAppsVisible"], true);
        assert_eq!(
            b["data"]["attributes"]["roles"].as_array().unwrap().len(),
            2
        );
        assert!(b["data"].get("relationships").is_none());
    }

    #[test]
    fn invitation_body_with_provisioning_allowed() {
        let args = InviteUserArgs {
            email: "dev@example.com".into(),
            first_name: "Carol".into(),
            last_name: "Kim".into(),
            roles: vec!["DEVELOPER".into()],
            all_apps_visible: None,
            provisioning_allowed: Some(true),
            visible_app_ids: None,
        };
        let b = user_invitation_body(&args);
        assert_eq!(b["data"]["attributes"]["provisioningAllowed"], true);
        // allAppsVisible must remain absent
        assert!(b["data"]["attributes"].get("allAppsVisible").is_none());
        assert!(b["data"].get("relationships").is_none());

        // When None, the key must be absent
        let args_none = InviteUserArgs {
            email: "dev@example.com".into(),
            first_name: "Carol".into(),
            last_name: "Kim".into(),
            roles: vec!["DEVELOPER".into()],
            all_apps_visible: None,
            provisioning_allowed: None,
            visible_app_ids: None,
        };
        let b_none = user_invitation_body(&args_none);
        assert!(b_none["data"]["attributes"]
            .get("provisioningAllowed")
            .is_none());
    }

    #[test]
    fn invitation_body_with_visible_app_ids() {
        let args = InviteUserArgs {
            email: "dev@example.com".into(),
            first_name: "Alice".into(),
            last_name: "Wu".into(),
            roles: vec!["DEVELOPER".into()],
            all_apps_visible: Some(false),
            provisioning_allowed: None,
            visible_app_ids: Some(vec!["app-1".into(), "app-2".into()]),
        };
        let b = user_invitation_body(&args);
        let vis = &b["data"]["relationships"]["visibleApps"]["data"];
        let arr = vis.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["type"], "apps");
        assert_eq!(arr[0]["id"], "app-1");
        assert_eq!(arr[1]["id"], "app-2");
    }

    #[test]
    fn invitation_body_empty_visible_app_ids_omits_relationship() {
        let args = InviteUserArgs {
            email: "dev@example.com".into(),
            first_name: "Bob".into(),
            last_name: "Lee".into(),
            roles: vec!["DEVELOPER".into()],
            all_apps_visible: None,
            provisioning_allowed: None,
            visible_app_ids: Some(vec![]), // empty — must be omitted
        };
        let b = user_invitation_body(&args);
        assert!(b["data"].get("relationships").is_none());
    }

    // --- update_user body ---

    #[test]
    fn update_body_all_fields_omitted_when_none() {
        let args = UpdateUserArgs {
            user_id: "usr-1".into(),
            roles: None,
            all_apps_visible: None,
            provisioning_allowed: None,
            visible_app_ids: None,
        };
        let b = user_update_body(&args);
        assert_eq!(b["data"]["type"], "users");
        assert_eq!(b["data"]["id"], "usr-1");
        let attrs = &b["data"]["attributes"];
        assert!(attrs.get("roles").is_none());
        assert!(attrs.get("allAppsVisible").is_none());
        assert!(attrs.get("provisioningAllowed").is_none());
        assert!(b["data"].get("relationships").is_none());
    }

    #[test]
    fn update_body_includes_only_set_attributes() {
        let args = UpdateUserArgs {
            user_id: "usr-2".into(),
            roles: Some(vec!["APP_MANAGER".into()]),
            all_apps_visible: Some(true),
            provisioning_allowed: None,
            visible_app_ids: None,
        };
        let b = user_update_body(&args);
        assert_eq!(b["data"]["attributes"]["roles"][0], "APP_MANAGER");
        assert_eq!(b["data"]["attributes"]["allAppsVisible"], true);
        assert!(b["data"]["attributes"].get("provisioningAllowed").is_none());
        assert!(b["data"].get("relationships").is_none());
    }

    #[test]
    fn update_body_with_visible_apps() {
        let args = UpdateUserArgs {
            user_id: "usr-3".into(),
            roles: None,
            all_apps_visible: Some(false),
            provisioning_allowed: Some(false),
            visible_app_ids: Some(vec!["app-42".into()]),
        };
        let b = user_update_body(&args);
        assert_eq!(b["data"]["attributes"]["provisioningAllowed"], false);
        let vis = &b["data"]["relationships"]["visibleApps"]["data"];
        let arr = vis.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["type"], "apps");
        assert_eq!(arr[0]["id"], "app-42");
    }

    #[test]
    fn update_body_empty_visible_app_ids_omits_relationship() {
        let args = UpdateUserArgs {
            user_id: "usr-4".into(),
            roles: None,
            all_apps_visible: None,
            provisioning_allowed: None,
            visible_app_ids: Some(vec![]),
        };
        let b = user_update_body(&args);
        assert!(b["data"].get("relationships").is_none());
    }
}
