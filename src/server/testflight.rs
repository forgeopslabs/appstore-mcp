//! TestFlight tools: builds, beta groups, beta testers, beta review.

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::json;

use super::{push_opt, AppStoreServer};

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
}
