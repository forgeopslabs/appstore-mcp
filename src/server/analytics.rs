//! Analytics Reports tools (App Store Connect Analytics Reports API).
//!
//! Schemas verified against Apple's generated OpenAPI models in the AvdLee
//! Swift SDK (AnalyticsReportRequestCreateRequest).
//!
//! JSON key mapping from the SDK's `forKey:` decode lines:
//!   - data.type: "analyticsReportRequests"
//!   - attributes: accessType (required, "ONGOING" | "ONE_TIME_SNAPSHOT")
//!   - relationships: app → data { type: "apps", id }

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{push_opt, AppStoreServer};

/// The access type for an analytics report request.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AccessType {
    Ongoing,
    OneTimeSnapshot,
}

impl AccessType {
    fn as_api(self) -> &'static str {
        match self {
            AccessType::Ongoing => "ONGOING",
            AccessType::OneTimeSnapshot => "ONE_TIME_SNAPSHOT",
        }
    }
}

/// The granularity of an analytics report instance.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Granularity {
    Daily,
    Weekly,
    Monthly,
}

impl Granularity {
    fn as_api(self) -> &'static str {
        match self {
            Granularity::Daily => "DAILY",
            Granularity::Weekly => "WEEKLY",
            Granularity::Monthly => "MONTHLY",
        }
    }
}

// ---- Arg structs ------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RequestAnalyticsReportArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// The access type for the report request: ONGOING or ONE_TIME_SNAPSHOT.
    pub access_type: AccessType,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListAnalyticsReportsArgs {
    /// The analytics report request ID.
    pub report_request_id: String,
    /// Filter by report category (e.g. "APP_USAGE", "COMMERCE", "ENGAGEMENT",
    /// "FRAMEWORK_USAGE", "PERFORMANCE").
    #[serde(default)]
    pub category: Option<String>,
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListAnalyticsReportInstancesArgs {
    /// The analytics report ID.
    pub report_id: String,
    /// Filter by granularity: DAILY, WEEKLY, or MONTHLY.
    #[serde(default)]
    pub granularity: Option<Granularity>,
    /// Filter by processing date in YYYY-MM-DD format.
    #[serde(default)]
    pub processing_date: Option<String>,
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListAnalyticsReportSegmentsArgs {
    /// The analytics report instance ID.
    pub instance_id: String,
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

// ---- Tool impl block --------------------------------------------------------

#[tool_router(router = analytics_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// Request an analytics report.
    #[tool(
        description = "Create an analytics report request for an app. Use access_type ONGOING for \
a recurring report or ONE_TIME_SNAPSHOT for a one-time snapshot. Returns the report request \
resource including its ID, which you then pass to list_analytics_reports."
    )]
    async fn request_analytics_report(
        &self,
        Parameters(args): Parameters<RequestAnalyticsReportArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = analytics_report_request_body(&args.app_id, args.access_type);
        let value = self
            .client
            .post("/v1/analyticsReportRequests", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// List analytics reports for a report request.
    #[tool(
        description = "List the analytics reports available for a report request. Optionally \
filter by category (e.g. APP_USAGE, COMMERCE, ENGAGEMENT, FRAMEWORK_USAGE, PERFORMANCE). \
Returns report resources whose IDs you pass to list_analytics_report_instances."
    )]
    async fn list_analytics_reports(
        &self,
        Parameters(args): Parameters<ListAnalyticsReportsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "filter[category]", args.category);
        push_opt(&mut query, "limit", args.limit);
        let value = self
            .client
            .get(
                &format!(
                    "/v1/analyticsReportRequests/{}/reports",
                    args.report_request_id
                ),
                &query,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// List instances of an analytics report.
    #[tool(
        description = "List instances of an analytics report, optionally filtered by granularity \
(DAILY, WEEKLY, or MONTHLY) and/or processing date (YYYY-MM-DD). Returns instance resources \
whose IDs you pass to list_analytics_report_segments."
    )]
    async fn list_analytics_report_instances(
        &self,
        Parameters(args): Parameters<ListAnalyticsReportInstancesArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        if let Some(g) = args.granularity {
            query.push(("filter[granularity]".into(), g.as_api().to_string()));
        }
        push_opt(&mut query, "filter[processingDate]", args.processing_date);
        push_opt(&mut query, "limit", args.limit);
        let value = self
            .client
            .get(
                &format!("/v1/analyticsReports/{}/instances", args.report_id),
                &query,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// List segments of an analytics report instance.
    #[tool(
        description = "List the downloadable segments for an analytics report instance. Each \
segment's attributes include a presigned `url` pointing to a gzipped CSV file, plus \
`sizeInBytes` and `checksum` — this tool surfaces those download URLs rather than \
downloading the data itself."
    )]
    async fn list_analytics_report_segments(
        &self,
        Parameters(args): Parameters<ListAnalyticsReportSegmentsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "limit", args.limit);
        let value = self
            .client
            .get(
                &format!("/v1/analyticsReportInstances/{}/segments", args.instance_id),
                &query,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }
}

// ---- Pure JSON:API document builders (unit-tested below) --------------------

/// Build the request body for POST /v1/analyticsReportRequests.
///
/// Schema verified against AnalyticsReportRequestCreateRequest in the AvdLee
/// Swift SDK: data.type = "analyticsReportRequests", required attribute
/// `accessType`, and an `app` relationship.
fn analytics_report_request_body(app_id: &str, access_type: AccessType) -> Value {
    json!({
        "data": {
            "type": "analyticsReportRequests",
            "attributes": {
                "accessType": access_type.as_api()
            },
            "relationships": {
                "app": {
                    "data": {
                        "type": "apps",
                        "id": app_id
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
    fn access_type_ongoing_maps_to_api_string() {
        assert_eq!(AccessType::Ongoing.as_api(), "ONGOING");
    }

    #[test]
    fn access_type_one_time_snapshot_maps_to_api_string() {
        assert_eq!(AccessType::OneTimeSnapshot.as_api(), "ONE_TIME_SNAPSHOT");
    }

    #[test]
    fn analytics_report_request_body_data_type() {
        let b = analytics_report_request_body("app-123", AccessType::Ongoing);
        assert_eq!(b["data"]["type"], "analyticsReportRequests");
    }

    #[test]
    fn analytics_report_request_body_access_type_ongoing() {
        let b = analytics_report_request_body("app-123", AccessType::Ongoing);
        assert_eq!(b["data"]["attributes"]["accessType"], "ONGOING");
    }

    #[test]
    fn analytics_report_request_body_access_type_one_time_snapshot() {
        let b = analytics_report_request_body("app-456", AccessType::OneTimeSnapshot);
        assert_eq!(b["data"]["attributes"]["accessType"], "ONE_TIME_SNAPSHOT");
    }

    #[test]
    fn analytics_report_request_body_app_relationship() {
        let b = analytics_report_request_body("app-789", AccessType::Ongoing);
        let rel = &b["data"]["relationships"]["app"]["data"];
        assert_eq!(rel["type"], "apps");
        assert_eq!(rel["id"], "app-789");
    }

    #[test]
    fn granularity_daily_maps_to_api_string() {
        assert_eq!(Granularity::Daily.as_api(), "DAILY");
    }

    #[test]
    fn granularity_weekly_maps_to_api_string() {
        assert_eq!(Granularity::Weekly.as_api(), "WEEKLY");
    }

    #[test]
    fn granularity_monthly_maps_to_api_string() {
        assert_eq!(Granularity::Monthly.as_api(), "MONTHLY");
    }

    #[test]
    fn analytics_report_request_body_no_id_field() {
        let b = analytics_report_request_body("app-123", AccessType::Ongoing);
        assert!(b["data"].get("id").is_none());
    }
}
