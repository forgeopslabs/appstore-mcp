//! App Review submission gates: review submissions, App Review details, and
//! export-compliance (encryption) declarations.
//!
//! These complete the create → **submit** lifecycle for issues #1, #2, and #4.
//! Each tool delegates JSON:API document construction to a small pure
//! `*_body` function so the request shape is unit-testable without the network.

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::versions::Platform;
use super::{push_opt, AppStoreServer};

/// What a review-submission item points at.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewItemKind {
    /// An App Store version (the common case).
    AppStoreVersion,
    /// An in-app event.
    AppEvent,
}

impl ReviewItemKind {
    /// (relationship key, JSON:API resource type) for this item kind.
    fn relationship(self) -> (&'static str, &'static str) {
        match self {
            ReviewItemKind::AppStoreVersion => ("appStoreVersion", "appStoreVersions"),
            ReviewItemKind::AppEvent => ("appEvent", "appEvents"),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateReviewSubmissionArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// Target platform.
    pub platform: Platform,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AddReviewItemArgs {
    /// The review submission ID (from create_review_submission).
    pub review_submission_id: String,
    /// What kind of item to attach.
    pub item_kind: ReviewItemKind,
    /// The ID of the version/event being submitted.
    pub item_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReviewSubmissionIdArgs {
    /// The review submission ID.
    pub review_submission_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListReviewSubmissionsArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// Filter by state, e.g. "READY_FOR_REVIEW", "WAITING_FOR_REVIEW", "IN_REVIEW".
    #[serde(default)]
    pub state: Option<String>,
    /// Filter by platform, e.g. "IOS".
    #[serde(default)]
    pub platform: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SubmitIapArgs {
    /// The in-app purchase ID to submit for review.
    pub iap_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetReviewDetailArgs {
    /// The appStoreVersion ID the review details belong to.
    pub version_id: String,
    /// If updating an existing detail, its ID (PATCH instead of POST).
    #[serde(default)]
    pub review_detail_id: Option<String>,
    #[serde(default)]
    pub contact_first_name: Option<String>,
    #[serde(default)]
    pub contact_last_name: Option<String>,
    #[serde(default)]
    pub contact_phone: Option<String>,
    #[serde(default)]
    pub contact_email: Option<String>,
    /// Whether App Review needs a demo account to use the app.
    #[serde(default)]
    pub demo_account_required: Option<bool>,
    #[serde(default)]
    pub demo_account_name: Option<String>,
    #[serde(default)]
    pub demo_account_password: Option<String>,
    /// Free-form notes for the reviewer.
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateEncryptionDeclarationArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// A description of how the app uses encryption.
    pub app_description: String,
    /// Whether the app implements any proprietary/non-standard encryption algorithms.
    pub contains_proprietary_cryptography: bool,
    /// Whether the app uses any third-party encryption.
    pub contains_third_party_cryptography: bool,
    /// Whether the app will be available on the French App Store.
    pub available_on_french_store: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AssignEncryptionDeclarationArgs {
    /// The appEncryptionDeclaration ID.
    pub declaration_id: String,
    /// The build ID to associate with the declaration.
    pub build_id: String,
}

#[tool_router(router = submission_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// Open a review submission for an app.
    #[tool(
        description = "Open a new App Review submission for an app + platform. Then attach items \
with add_review_submission_item and submit with submit_review_submission."
    )]
    async fn create_review_submission(
        &self,
        Parameters(args): Parameters<CreateReviewSubmissionArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = review_submission_body(&args.app_id, args.platform.as_api());
        let value = self
            .client
            .post("/v1/reviewSubmissions", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Attach an item (version or event) to a review submission.
    #[tool(
        description = "Attach an App Store version or in-app event to an open review submission."
    )]
    async fn add_review_submission_item(
        &self,
        Parameters(args): Parameters<AddReviewItemArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = review_item_body(&args.review_submission_id, args.item_kind, &args.item_id);
        let value = self
            .client
            .post("/v1/reviewSubmissionItems", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Submit a review submission to App Review.
    #[tool(
        description = "Submit a prepared review submission to App Review (sets submitted=true). All \
metadata gates (age rating, export compliance, review details) must be satisfied first."
    )]
    async fn submit_review_submission(
        &self,
        Parameters(args): Parameters<ReviewSubmissionIdArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = submit_review_submission_body(&args.review_submission_id);
        let path = format!("/v1/reviewSubmissions/{}", args.review_submission_id);
        let value = self
            .client
            .patch(&path, body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// List an app's review submissions.
    #[tool(
        description = "List an app's App Review submissions, optionally filtered by state or platform."
    )]
    async fn list_review_submissions(
        &self,
        Parameters(args): Parameters<ListReviewSubmissionsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = vec![("filter[app]".to_string(), args.app_id.clone())];
        push_opt(&mut query, "filter[state]", args.state);
        push_opt(&mut query, "filter[platform]", args.platform);
        let value = self
            .client
            .get("/v1/reviewSubmissions", &query)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Submit an in-app purchase for review.
    #[tool(
        description = "Submit a single in-app purchase for App Review (inAppPurchaseSubmissions)."
    )]
    async fn submit_in_app_purchase(
        &self,
        Parameters(args): Parameters<SubmitIapArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = iap_submission_body(&args.iap_id);
        let value = self
            .client
            .post("/v1/inAppPurchaseSubmissions", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Create or update the App Review details for a version.
    #[tool(
        description = "Set the App Review contact info, optional demo account, and notes for a \
version. Pass review_detail_id to update an existing detail; omit it to create one."
    )]
    async fn set_app_review_detail(
        &self,
        Parameters(args): Parameters<SetReviewDetailArgs>,
    ) -> Result<CallToolResult, McpError> {
        let attributes = review_detail_attributes(&args);
        let value = match &args.review_detail_id {
            Some(id) => {
                let body = review_detail_update_body(id, attributes);
                self.client
                    .patch(&format!("/v1/appStoreReviewDetails/{id}"), body)
                    .await
            }
            None => {
                let body = review_detail_create_body(&args.version_id, attributes);
                self.client.post("/v1/appStoreReviewDetails", body).await
            }
        }
        .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Create an app encryption (export-compliance) declaration.
    #[tool(
        description = "Create an app encryption / export-compliance declaration for an app. Then \
attach a build with assign_build_encryption_declaration."
    )]
    async fn create_app_encryption_declaration(
        &self,
        Parameters(args): Parameters<CreateEncryptionDeclarationArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = encryption_declaration_body(&args);
        let value = self
            .client
            .post("/v1/appEncryptionDeclarations", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Assign a build to an encryption declaration.
    #[tool(description = "Associate a build with an existing app encryption declaration.")]
    async fn assign_build_encryption_declaration(
        &self,
        Parameters(args): Parameters<AssignEncryptionDeclarationArgs>,
    ) -> Result<CallToolResult, McpError> {
        // A build has a to-one `appEncryptionDeclaration` relationship; set it
        // from the build side (the declaration's `builds` relationship is not
        // a PATCH target).
        let body = build_encryption_linkage_body(&args.declaration_id);
        let path = format!(
            "/v1/builds/{}/relationships/appEncryptionDeclaration",
            args.build_id
        );
        let value = self
            .client
            .patch(&path, body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }
}

// ---- Pure JSON:API document builders (unit-tested below) --------------------

fn review_submission_body(app_id: &str, platform: &str) -> Value {
    json!({
        "data": {
            "type": "reviewSubmissions",
            "attributes": { "platform": platform },
            "relationships": {
                "app": { "data": { "type": "apps", "id": app_id } }
            }
        }
    })
}

fn review_item_body(review_submission_id: &str, kind: ReviewItemKind, item_id: &str) -> Value {
    let (rel_key, rel_type) = kind.relationship();
    json!({
        "data": {
            "type": "reviewSubmissionItems",
            "relationships": {
                "reviewSubmission": {
                    "data": { "type": "reviewSubmissions", "id": review_submission_id }
                },
                rel_key: { "data": { "type": rel_type, "id": item_id } }
            }
        }
    })
}

fn submit_review_submission_body(id: &str) -> Value {
    json!({
        "data": {
            "type": "reviewSubmissions",
            "id": id,
            "attributes": { "submitted": true }
        }
    })
}

fn iap_submission_body(iap_id: &str) -> Value {
    json!({
        "data": {
            "type": "inAppPurchaseSubmissions",
            "relationships": {
                "inAppPurchaseV2": { "data": { "type": "inAppPurchases", "id": iap_id } }
            }
        }
    })
}

fn review_detail_attributes(args: &SetReviewDetailArgs) -> Value {
    let mut attrs = json!({});
    set_opt_str(&mut attrs, "contactFirstName", &args.contact_first_name);
    set_opt_str(&mut attrs, "contactLastName", &args.contact_last_name);
    set_opt_str(&mut attrs, "contactPhone", &args.contact_phone);
    set_opt_str(&mut attrs, "contactEmail", &args.contact_email);
    set_opt_str(&mut attrs, "demoAccountName", &args.demo_account_name);
    set_opt_str(
        &mut attrs,
        "demoAccountPassword",
        &args.demo_account_password,
    );
    set_opt_str(&mut attrs, "notes", &args.notes);
    if let Some(required) = args.demo_account_required {
        attrs["demoAccountRequired"] = json!(required);
    }
    attrs
}

fn review_detail_create_body(version_id: &str, attributes: Value) -> Value {
    json!({
        "data": {
            "type": "appStoreReviewDetails",
            "attributes": attributes,
            "relationships": {
                "appStoreVersion": { "data": { "type": "appStoreVersions", "id": version_id } }
            }
        }
    })
}

fn review_detail_update_body(id: &str, attributes: Value) -> Value {
    json!({
        "data": {
            "type": "appStoreReviewDetails",
            "id": id,
            "attributes": attributes
        }
    })
}

fn encryption_declaration_body(args: &CreateEncryptionDeclarationArgs) -> Value {
    json!({
        "data": {
            "type": "appEncryptionDeclarations",
            "attributes": {
                "appDescription": args.app_description,
                "containsProprietaryCryptography": args.contains_proprietary_cryptography,
                "containsThirdPartyCryptography": args.contains_third_party_cryptography,
                "availableOnFrenchStore": args.available_on_french_store,
            },
            "relationships": {
                "app": { "data": { "type": "apps", "id": args.app_id } }
            }
        }
    })
}

/// To-one linkage body for setting a build's `appEncryptionDeclaration`.
fn build_encryption_linkage_body(declaration_id: &str) -> Value {
    json!({ "data": { "type": "appEncryptionDeclarations", "id": declaration_id } })
}

/// Insert a string attribute only when present.
fn set_opt_str(obj: &mut Value, key: &str, value: &Option<String>) {
    if let Some(v) = value {
        obj[key] = json!(v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_submission_body_shape() {
        let b = review_submission_body("123", "IOS");
        assert_eq!(b["data"]["type"], "reviewSubmissions");
        assert_eq!(b["data"]["attributes"]["platform"], "IOS");
        assert_eq!(b["data"]["relationships"]["app"]["data"]["type"], "apps");
        assert_eq!(b["data"]["relationships"]["app"]["data"]["id"], "123");
    }

    #[test]
    fn review_item_body_maps_version_kind() {
        let b = review_item_body("sub-1", ReviewItemKind::AppStoreVersion, "ver-9");
        assert_eq!(b["data"]["type"], "reviewSubmissionItems");
        let rels = &b["data"]["relationships"];
        assert_eq!(rels["reviewSubmission"]["data"]["id"], "sub-1");
        assert_eq!(rels["appStoreVersion"]["data"]["type"], "appStoreVersions");
        assert_eq!(rels["appStoreVersion"]["data"]["id"], "ver-9");
        // The event relationship key must NOT be present for a version item.
        assert!(rels.get("appEvent").is_none());
    }

    #[test]
    fn review_item_body_maps_event_kind() {
        let b = review_item_body("sub-1", ReviewItemKind::AppEvent, "evt-2");
        let rels = &b["data"]["relationships"];
        assert_eq!(rels["appEvent"]["data"]["type"], "appEvents");
        assert_eq!(rels["appEvent"]["data"]["id"], "evt-2");
        assert!(rels.get("appStoreVersion").is_none());
    }

    #[test]
    fn submit_body_sets_submitted_true() {
        let b = submit_review_submission_body("sub-7");
        assert_eq!(b["data"]["id"], "sub-7");
        assert_eq!(b["data"]["attributes"]["submitted"], true);
    }

    #[test]
    fn iap_submission_body_shape() {
        let b = iap_submission_body("iap-5");
        assert_eq!(b["data"]["type"], "inAppPurchaseSubmissions");
        assert_eq!(
            b["data"]["relationships"]["inAppPurchaseV2"]["data"]["type"],
            "inAppPurchases"
        );
        assert_eq!(
            b["data"]["relationships"]["inAppPurchaseV2"]["data"]["id"],
            "iap-5"
        );
    }

    #[test]
    fn review_detail_create_includes_relationship_and_only_set_fields() {
        let args = SetReviewDetailArgs {
            version_id: "v1".into(),
            review_detail_id: None,
            contact_first_name: Some("Ada".into()),
            contact_last_name: Some("Lovelace".into()),
            contact_phone: None,
            contact_email: Some("ada@example.com".into()),
            demo_account_required: Some(false),
            demo_account_name: None,
            demo_account_password: None,
            notes: Some("Tap start".into()),
        };
        let attrs = review_detail_attributes(&args);
        let b = review_detail_create_body(&args.version_id, attrs);
        assert_eq!(b["data"]["type"], "appStoreReviewDetails");
        assert_eq!(
            b["data"]["relationships"]["appStoreVersion"]["data"]["id"],
            "v1"
        );
        assert_eq!(b["data"]["attributes"]["contactFirstName"], "Ada");
        assert_eq!(b["data"]["attributes"]["contactEmail"], "ada@example.com");
        assert_eq!(b["data"]["attributes"]["demoAccountRequired"], false);
        assert_eq!(b["data"]["attributes"]["notes"], "Tap start");
        // Unset fields must be absent (not null).
        assert!(b["data"]["attributes"].get("contactPhone").is_none());
        assert!(b["data"]["attributes"].get("demoAccountName").is_none());
    }

    #[test]
    fn review_detail_update_has_id_and_no_relationship() {
        let attrs = json!({ "notes": "updated" });
        let b = review_detail_update_body("rd-3", attrs);
        assert_eq!(b["data"]["id"], "rd-3");
        assert_eq!(b["data"]["attributes"]["notes"], "updated");
        assert!(b["data"].get("relationships").is_none());
    }

    #[test]
    fn encryption_declaration_body_has_required_attributes_only() {
        let args = CreateEncryptionDeclarationArgs {
            app_id: "app-1".into(),
            app_description: "Uses HTTPS only".into(),
            contains_proprietary_cryptography: false,
            contains_third_party_cryptography: true,
            available_on_french_store: false,
        };
        let b = encryption_declaration_body(&args);
        assert_eq!(b["data"]["type"], "appEncryptionDeclarations");
        let attrs = &b["data"]["attributes"];
        assert_eq!(attrs["appDescription"], "Uses HTTPS only");
        assert_eq!(attrs["containsProprietaryCryptography"], false);
        assert_eq!(attrs["containsThirdPartyCryptography"], true);
        assert_eq!(attrs["availableOnFrenchStore"], false);
        // The API has no usesEncryption/platform/exempt on create — must be absent.
        assert!(attrs.get("usesEncryption").is_none());
        assert!(attrs.get("platform").is_none());
        assert!(attrs.get("exempt").is_none());
        assert_eq!(b["data"]["relationships"]["app"]["data"]["id"], "app-1");
    }

    #[test]
    fn build_encryption_linkage_is_to_one() {
        let b = build_encryption_linkage_body("aed-42");
        // To-one linkage: a single object, not an array.
        assert!(b["data"].is_object());
        assert_eq!(b["data"]["type"], "appEncryptionDeclarations");
        assert_eq!(b["data"]["id"], "aed-42");
    }
}
