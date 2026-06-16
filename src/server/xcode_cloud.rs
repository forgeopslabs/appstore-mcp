//! Xcode Cloud (CI) tools (#17).
//!
//! Schemas verified against Apple's generated OpenAPI models in the AvdLee
//! Swift SDK (CiBuildRunCreateRequest).
//!
//! Schema findings:
//!   - data.type                          : "ciBuildRuns"
//!   - attributes                         : optional (isClean only) — omitted here
//!   - relationships.workflow.data.type   : "ciWorkflows"
//!   - relationships.sourceBranchOrTag.data.type : "scmGitReferences"
//!   - All relationships are optional at the API level; we require workflow and
//!     sourceBranchOrTag since they are the minimum needed to trigger a build.
//!
//! The `source_branch_or_tag_id` arg is a **scmGitReference** resource ID.
//! Callers obtain it by listing the workflow's repository git references via
//! the generic `appstore_list` tool:
//!   GET /v1/ciWorkflows/{id}/repository/gitReferences

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{push_opt, AppStoreServer};

// ---- Arg structs ------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListCiProductsArgs {
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListCiWorkflowsArgs {
    /// The Xcode Cloud product ID.
    pub ci_product_id: String,
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StartCiBuildArgs {
    /// The Xcode Cloud workflow ID.
    pub workflow_id: String,
    /// The scmGitReference resource ID for the branch or tag to build.
    /// Obtain it by listing the workflow's repository git references via
    /// GET /v1/ciWorkflows/{id}/repository/gitReferences (use appstore_list).
    pub source_branch_or_tag_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetCiBuildRunArgs {
    /// The build run ID.
    pub build_run_id: String,
    /// Comma-separated related resources to include, e.g. "builds,workflows".
    #[serde(default)]
    pub include: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListCiBuildActionsArgs {
    /// The build run ID.
    pub build_run_id: String,
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

// ---- Tool impl block --------------------------------------------------------

#[tool_router(router = xcode_cloud_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// List Xcode Cloud products (CI-enabled apps/frameworks).
    #[tool(
        description = "List all Xcode Cloud products (CI-enabled apps and frameworks) in the team. \
Each product corresponds to an app or framework that has been set up for Xcode Cloud."
    )]
    async fn list_ci_products(
        &self,
        Parameters(args): Parameters<ListCiProductsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "limit", args.limit);
        let value = self
            .client
            .get("/v1/ciProducts", &query)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// List workflows for an Xcode Cloud product.
    #[tool(
        description = "List all CI workflows for a given Xcode Cloud product. Use list_ci_products \
to obtain the ci_product_id."
    )]
    async fn list_ci_workflows(
        &self,
        Parameters(args): Parameters<ListCiWorkflowsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "limit", args.limit);
        let value = self
            .client
            .get(
                &format!("/v1/ciProducts/{}/workflows", args.ci_product_id),
                &query,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Start a new Xcode Cloud build run.
    #[tool(
        description = "Start a new Xcode Cloud build run for a workflow on a specific branch or tag. \
Provide the workflow_id (from list_ci_workflows) and source_branch_or_tag_id, which is a \
scmGitReference resource ID. Obtain it by listing the workflow's repository git references with: \
appstore_list { \"path\": \"/v1/ciWorkflows/{workflow_id}/repository/gitReferences\" }."
    )]
    async fn start_ci_build(
        &self,
        Parameters(args): Parameters<StartCiBuildArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = ci_build_run_body(&args.workflow_id, &args.source_branch_or_tag_id);
        let value = self
            .client
            .post("/v1/ciBuildRuns", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Get details for a specific Xcode Cloud build run.
    #[tool(
        description = "Get the details of a specific Xcode Cloud build run by its ID. \
Optionally pass include (e.g. \"builds,workflows\") to embed related resources."
    )]
    async fn get_ci_build_run(
        &self,
        Parameters(args): Parameters<GetCiBuildRunArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "include", args.include);
        let value = self
            .client
            .get(&format!("/v1/ciBuildRuns/{}", args.build_run_id), &query)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// List the build actions for a specific Xcode Cloud build run.
    #[tool(
        description = "List all CI build actions (e.g. analyze, archive, test, lint) for a given \
Xcode Cloud build run. Use get_ci_build_run or start_ci_build to obtain the build_run_id."
    )]
    async fn list_ci_build_actions(
        &self,
        Parameters(args): Parameters<ListCiBuildActionsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "limit", args.limit);
        let value = self
            .client
            .get(
                &format!("/v1/ciBuildRuns/{}/actions", args.build_run_id),
                &query,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }
}

// ---- Pure JSON:API document builders (unit-tested below) --------------------

/// Build the request body for POST /v1/ciBuildRuns.
///
/// Schema (verified against AvdLee/appstoreconnect-swift-sdk):
///   - data.type                                    : "ciBuildRuns"
///   - relationships.workflow.data.type             : "ciWorkflows"
///   - relationships.sourceBranchOrTag.data.type    : "scmGitReferences"
fn ci_build_run_body(workflow_id: &str, source_branch_or_tag_id: &str) -> Value {
    json!({
        "data": {
            "type": "ciBuildRuns",
            "relationships": {
                "workflow": {
                    "data": {
                        "type": "ciWorkflows",
                        "id": workflow_id
                    }
                },
                "sourceBranchOrTag": {
                    "data": {
                        "type": "scmGitReferences",
                        "id": source_branch_or_tag_id
                    }
                }
            }
        }
    })
}

// ---- Unit tests -------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ci_build_run_body_type_is_ci_build_runs() {
        let b = ci_build_run_body("wf-1", "ref-1");
        assert_eq!(b["data"]["type"], "ciBuildRuns");
    }

    #[test]
    fn ci_build_run_body_workflow_relationship() {
        let b = ci_build_run_body("wf-abc", "ref-xyz");
        let wf = &b["data"]["relationships"]["workflow"]["data"];
        assert_eq!(wf["type"], "ciWorkflows");
        assert_eq!(wf["id"], "wf-abc");
    }

    #[test]
    fn ci_build_run_body_source_branch_or_tag_relationship() {
        let b = ci_build_run_body("wf-abc", "ref-xyz");
        let src = &b["data"]["relationships"]["sourceBranchOrTag"]["data"];
        assert_eq!(src["type"], "scmGitReferences");
        assert_eq!(src["id"], "ref-xyz");
    }

    #[test]
    fn ci_build_run_body_no_attributes() {
        let b = ci_build_run_body("wf-1", "ref-1");
        // We intentionally omit attributes (isClean etc.) — they must be absent.
        assert!(b["data"].get("attributes").is_none());
    }

    #[test]
    fn ci_build_run_body_full_shape() {
        let b = ci_build_run_body("workflow-99", "gitref-42");
        assert_eq!(b["data"]["type"], "ciBuildRuns");
        assert_eq!(
            b["data"]["relationships"]["workflow"]["data"]["type"],
            "ciWorkflows"
        );
        assert_eq!(
            b["data"]["relationships"]["workflow"]["data"]["id"],
            "workflow-99"
        );
        assert_eq!(
            b["data"]["relationships"]["sourceBranchOrTag"]["data"]["type"],
            "scmGitReferences"
        );
        assert_eq!(
            b["data"]["relationships"]["sourceBranchOrTag"]["data"]["id"],
            "gitref-42"
        );
    }
}
