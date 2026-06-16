//! TestFlight tools: builds, beta groups, beta testers, beta review,
//! build localizations, build beta details, beta app review details, and expiring builds.

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{push_opt, set_opt_str, AppStoreServer};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListBuildsArgs {
    /// Filter by app ID.
    #[serde(default)]
    pub app_id: Option<String>,
    /// Filter by version (build number), e.g. "42".
    #[serde(default)]
    pub version: Option<String>,
    /// Comma-separated includes, e.g. "betaGroups,preReleaseVersion".
    #[serde(default)]
    pub include: Option<String>,
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListBetaGroupsArgs {
    /// Filter by app ID.
    #[serde(default)]
    pub app_id: Option<String>,
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateBetaGroupArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// Group name.
    pub name: String,
    /// Whether this is a public-link group.
    #[serde(default)]
    pub public_link_enabled: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AddBetaTesterArgs {
    /// Tester email address.
    pub email: String,
    /// Tester first name.
    #[serde(default)]
    pub first_name: Option<String>,
    /// Tester last name.
    #[serde(default)]
    pub last_name: Option<String>,
    /// The beta group ID to add the tester to.
    pub beta_group_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SubmitBetaReviewArgs {
    /// The build ID to submit for beta (external) review.
    pub build_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetBuildTestNotesArgs {
    /// The build ID to attach test notes to.
    pub build_id: String,
    /// BCP 47 locale code, e.g. "en-US".
    pub locale: String,
    /// What's new / test notes shown to testers in TestFlight.
    #[serde(default)]
    pub whats_new: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetBuildBetaDetailArgs {
    /// The buildBetaDetail resource ID (NOT the build ID).
    /// Find it via GET /v1/builds/{buildId}/buildBetaDetail or ?include=buildBetaDetail.
    pub build_beta_detail_id: String,
    /// Whether to automatically notify testers when the build becomes available.
    #[serde(default)]
    pub auto_notify_enabled: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetBetaAppReviewDetailArgs {
    /// The betaAppReviewDetail resource ID (NOT the app/build ID).
    /// Find it via GET /v1/apps/{appId}/betaAppReviewDetail.
    pub beta_app_review_detail_id: String,
    /// First name of the beta review contact.
    #[serde(default)]
    pub contact_first_name: Option<String>,
    /// Last name of the beta review contact.
    #[serde(default)]
    pub contact_last_name: Option<String>,
    /// Phone number of the beta review contact.
    #[serde(default)]
    pub contact_phone: Option<String>,
    /// Email address of the beta review contact.
    #[serde(default)]
    pub contact_email: Option<String>,
    /// Demo account username (if demo account is required).
    #[serde(default)]
    pub demo_account_name: Option<String>,
    /// Demo account password (if demo account is required).
    #[serde(default)]
    pub demo_account_password: Option<String>,
    /// Whether App Review requires a demo account to test the app.
    #[serde(default)]
    pub demo_account_required: Option<bool>,
    /// Free-form notes for the beta reviewer.
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExpireBuildArgs {
    /// The build ID to expire.
    pub build_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AddBuildToBetaGroupArgs {
    /// The beta group ID to add the build to.
    pub beta_group_id: String,
    /// The build ID to add to the group.
    pub build_id: String,
}

#[tool_router(router = testflight_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// List builds.
    #[tool(description = "List TestFlight builds, optionally filtered by app or build version.")]
    async fn list_builds(
        &self,
        Parameters(args): Parameters<ListBuildsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "filter[app]", args.app_id);
        push_opt(&mut query, "filter[version]", args.version);
        push_opt(&mut query, "include", args.include);
        push_opt(&mut query, "limit", args.limit);
        let value = self
            .client
            .get("/v1/builds", &query)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// List beta groups.
    #[tool(description = "List TestFlight beta groups, optionally filtered by app.")]
    async fn list_beta_groups(
        &self,
        Parameters(args): Parameters<ListBetaGroupsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "filter[app]", args.app_id);
        push_opt(&mut query, "limit", args.limit);
        let value = self
            .client
            .get("/v1/betaGroups", &query)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Create a beta group.
    #[tool(description = "Create a TestFlight beta group for an app.")]
    async fn create_beta_group(
        &self,
        Parameters(args): Parameters<CreateBetaGroupArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut attributes = json!({ "name": args.name });
        if let Some(public) = args.public_link_enabled {
            attributes["publicLinkEnabled"] = json!(public);
        }
        let body = json!({
            "data": {
                "type": "betaGroups",
                "attributes": attributes,
                "relationships": {
                    "app": { "data": { "type": "apps", "id": args.app_id } }
                }
            }
        });
        let value = self
            .client
            .post("/v1/betaGroups", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Add (invite) a beta tester to a group.
    #[tool(
        description = "Add a beta tester (by email) to a TestFlight beta group, sending an invite."
    )]
    async fn add_beta_tester(
        &self,
        Parameters(args): Parameters<AddBetaTesterArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut attributes = json!({ "email": args.email });
        if let Some(first) = args.first_name {
            attributes["firstName"] = json!(first);
        }
        if let Some(last) = args.last_name {
            attributes["lastName"] = json!(last);
        }
        let body = json!({
            "data": {
                "type": "betaTesters",
                "attributes": attributes,
                "relationships": {
                    "betaGroups": {
                        "data": [ { "type": "betaGroups", "id": args.beta_group_id } ]
                    }
                }
            }
        });
        let value = self
            .client
            .post("/v1/betaTesters", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Submit a build for beta (external) review.
    #[tool(
        description = "Submit a build for TestFlight beta app review (required before external testing)."
    )]
    async fn submit_build_for_beta_review(
        &self,
        Parameters(args): Parameters<SubmitBetaReviewArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = json!({
            "data": {
                "type": "betaAppReviewSubmissions",
                "relationships": {
                    "build": { "data": { "type": "builds", "id": args.build_id } }
                }
            }
        });
        let value = self
            .client
            .post("/v1/betaAppReviewSubmissions", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Set test notes (What's New) for a build locale.
    #[tool(
        description = "Set the TestFlight 'What's New' test notes for a build in a specific locale \
(creates a betaBuildLocalization). locale is required (e.g. \"en-US\"); whats_new is the \
tester-facing 'What to Test' text shown in the TestFlight app. \
To update an existing localization instead of creating one, use appstore_request with \
PATCH /v1/betaBuildLocalizations/{id}."
    )]
    async fn set_build_test_notes(
        &self,
        Parameters(args): Parameters<SetBuildTestNotesArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body =
            set_build_test_notes_body(&args.build_id, &args.locale, args.whats_new.as_deref());
        let value = self
            .client
            .post("/v1/betaBuildLocalizations", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Update a buildBetaDetail resource.
    #[tool(
        description = "Update the beta detail for a build, e.g. toggle auto-notify. \
The build_beta_detail_id is the buildBetaDetail resource ID — find it via \
GET /v1/builds/{buildId}/buildBetaDetail or by including ?include=buildBetaDetail on a build fetch."
    )]
    async fn set_build_beta_detail(
        &self,
        Parameters(args): Parameters<SetBuildBetaDetailArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = set_build_beta_detail_body(&args.build_beta_detail_id, args.auto_notify_enabled);
        let path = format!("/v1/buildBetaDetails/{}", args.build_beta_detail_id);
        let value = self
            .client
            .patch(&path, body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Update the beta app review detail for an app.
    #[tool(
        description = "Update the TestFlight beta app review contact info, demo account, and notes \
for an app. beta_app_review_detail_id is the betaAppReviewDetail resource ID — find it via \
GET /v1/apps/{appId}/betaAppReviewDetail. Only provided fields are sent."
    )]
    async fn set_beta_app_review_detail(
        &self,
        Parameters(args): Parameters<SetBetaAppReviewDetailArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = set_beta_app_review_detail_body(&args);
        let path = format!(
            "/v1/betaAppReviewDetails/{}",
            args.beta_app_review_detail_id
        );
        let value = self
            .client
            .patch(&path, body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Expire a build.
    #[tool(
        description = "Mark a TestFlight build as expired so it is no longer available to testers."
    )]
    async fn expire_build(
        &self,
        Parameters(args): Parameters<ExpireBuildArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = expire_build_body(&args.build_id);
        let path = format!("/v1/builds/{}", args.build_id);
        let value = self
            .client
            .patch(&path, body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Add a build to a beta group.
    #[tool(
        description = "Add a build to a TestFlight beta group (makes it available for that group's \
testers). Uses the betaGroups/{id}/relationships/builds to-many endpoint."
    )]
    async fn add_build_to_beta_group(
        &self,
        Parameters(args): Parameters<AddBuildToBetaGroupArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = add_build_to_beta_group_body(&args.build_id);
        let path = format!("/v1/betaGroups/{}/relationships/builds", args.beta_group_id);
        let value = self
            .client
            .post(&path, body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }
}

// ---- Pure JSON:API document builders (unit-tested below) --------------------

fn set_build_test_notes_body(build_id: &str, locale: &str, whats_new: Option<&str>) -> Value {
    let mut attributes = json!({ "locale": locale });
    if let Some(text) = whats_new {
        attributes["whatsNew"] = json!(text);
    }
    json!({
        "data": {
            "type": "betaBuildLocalizations",
            "attributes": attributes,
            "relationships": {
                "build": { "data": { "type": "builds", "id": build_id } }
            }
        }
    })
}

fn set_build_beta_detail_body(id: &str, auto_notify_enabled: Option<bool>) -> Value {
    let mut attributes = json!({});
    if let Some(enabled) = auto_notify_enabled {
        attributes["autoNotifyEnabled"] = json!(enabled);
    }
    json!({
        "data": {
            "type": "buildBetaDetails",
            "id": id,
            "attributes": attributes
        }
    })
}

fn set_beta_app_review_detail_body(args: &SetBetaAppReviewDetailArgs) -> Value {
    let mut attributes = json!({});
    set_opt_str(
        &mut attributes,
        "contactFirstName",
        &args.contact_first_name,
    );
    set_opt_str(&mut attributes, "contactLastName", &args.contact_last_name);
    set_opt_str(&mut attributes, "contactPhone", &args.contact_phone);
    set_opt_str(&mut attributes, "contactEmail", &args.contact_email);
    set_opt_str(&mut attributes, "demoAccountName", &args.demo_account_name);
    set_opt_str(
        &mut attributes,
        "demoAccountPassword",
        &args.demo_account_password,
    );
    if let Some(v) = args.demo_account_required {
        attributes["demoAccountRequired"] = json!(v);
    }
    set_opt_str(&mut attributes, "notes", &args.notes);
    json!({
        "data": {
            "type": "betaAppReviewDetails",
            "id": args.beta_app_review_detail_id,
            "attributes": attributes
        }
    })
}

fn expire_build_body(build_id: &str) -> Value {
    json!({
        "data": {
            "type": "builds",
            "id": build_id,
            "attributes": { "expired": true }
        }
    })
}

fn add_build_to_beta_group_body(build_id: &str) -> Value {
    json!({
        "data": [ { "type": "builds", "id": build_id } ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notes_body_required_locale_and_build_relationship() {
        let b = set_build_test_notes_body("build-1", "en-US", Some("Fixed crash on launch"));
        assert_eq!(b["data"]["type"], "betaBuildLocalizations");
        assert_eq!(b["data"]["attributes"]["locale"], "en-US");
        assert_eq!(b["data"]["attributes"]["whatsNew"], "Fixed crash on launch");
        assert_eq!(
            b["data"]["relationships"]["build"]["data"]["type"],
            "builds"
        );
        assert_eq!(b["data"]["relationships"]["build"]["data"]["id"], "build-1");
    }

    #[test]
    fn test_notes_body_whats_new_omitted_when_none() {
        let b = set_build_test_notes_body("build-2", "de-DE", None);
        assert_eq!(b["data"]["attributes"]["locale"], "de-DE");
        // whatsNew must be absent, not null
        assert!(b["data"]["attributes"].get("whatsNew").is_none());
    }

    #[test]
    fn build_beta_detail_body_sets_auto_notify() {
        let b = set_build_beta_detail_body("bbd-42", Some(true));
        assert_eq!(b["data"]["type"], "buildBetaDetails");
        assert_eq!(b["data"]["id"], "bbd-42");
        assert_eq!(b["data"]["attributes"]["autoNotifyEnabled"], true);

        let b_false = set_build_beta_detail_body("bbd-42", Some(false));
        assert_eq!(b_false["data"]["attributes"]["autoNotifyEnabled"], false);
    }

    #[test]
    fn build_beta_detail_body_empty_attributes_when_none() {
        let b = set_build_beta_detail_body("bbd-99", None);
        // attributes must be an empty object, not absent or null
        assert!(b["data"]["attributes"].is_object());
        assert!(b["data"]["attributes"].get("autoNotifyEnabled").is_none());
    }

    #[test]
    fn beta_app_review_detail_body_partial_fields() {
        let args = SetBetaAppReviewDetailArgs {
            beta_app_review_detail_id: "bard-7".into(),
            contact_first_name: Some("Ada".into()),
            contact_last_name: Some("Lovelace".into()),
            contact_phone: None,
            contact_email: Some("ada@example.com".into()),
            demo_account_name: None,
            demo_account_password: None,
            demo_account_required: Some(false),
            notes: Some("Tap Start to begin".into()),
        };
        let b = set_beta_app_review_detail_body(&args);
        assert_eq!(b["data"]["type"], "betaAppReviewDetails");
        assert_eq!(b["data"]["id"], "bard-7");
        let attrs = &b["data"]["attributes"];
        assert_eq!(attrs["contactFirstName"], "Ada");
        assert_eq!(attrs["contactLastName"], "Lovelace");
        assert_eq!(attrs["contactEmail"], "ada@example.com");
        assert_eq!(attrs["demoAccountRequired"], false);
        assert_eq!(attrs["notes"], "Tap Start to begin");
        // Absent fields must be omitted entirely
        assert!(attrs.get("contactPhone").is_none());
        assert!(attrs.get("demoAccountName").is_none());
        assert!(attrs.get("demoAccountPassword").is_none());
        // No relationships on an update
        assert!(b["data"].get("relationships").is_none());
    }

    #[test]
    fn expire_build_body_sets_expired_true() {
        let b = expire_build_body("build-99");
        assert_eq!(b["data"]["type"], "builds");
        assert_eq!(b["data"]["id"], "build-99");
        assert_eq!(b["data"]["attributes"]["expired"], true);
        // Must not accidentally set usesNonExemptEncryption
        assert!(b["data"]["attributes"]
            .get("usesNonExemptEncryption")
            .is_none());
    }

    #[test]
    fn add_build_to_beta_group_body_is_to_many_array() {
        let b = add_build_to_beta_group_body("build-5");
        // To-many relationship body: top-level "data" is an array
        assert!(b["data"].is_array());
        let arr = b["data"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["type"], "builds");
        assert_eq!(arr[0]["id"], "build-5");
    }
}
