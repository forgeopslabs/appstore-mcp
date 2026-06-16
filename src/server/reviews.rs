//! Customer reviews and developer responses (#12).

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{push_opt, AppStoreServer};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListCustomerReviewsArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// Filter by star rating (1-5).
    #[serde(default)]
    pub rating: Option<u32>,
    /// Filter by territory, e.g. "USA".
    #[serde(default)]
    pub territory: Option<String>,
    /// Sort, e.g. "-createdDate" (newest first) or "rating".
    #[serde(default)]
    pub sort: Option<String>,
    /// Set to true to include each review's existing response.
    #[serde(default)]
    pub include_response: Option<bool>,
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RespondToReviewArgs {
    /// The customerReview ID (from list_customer_reviews).
    pub review_id: String,
    /// The response body shown publicly under the review.
    pub response_body: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteReviewResponseArgs {
    /// The customerReviewResponse ID.
    pub response_id: String,
}

#[tool_router(router = reviews_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// List an app's customer reviews.
    #[tool(
        description = "List an app's customer reviews, optionally filtered by rating/territory and \
sorted (e.g. -createdDate). Set include_response=true to see existing responses."
    )]
    async fn list_customer_reviews(
        &self,
        Parameters(args): Parameters<ListCustomerReviewsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "filter[rating]", args.rating);
        push_opt(&mut query, "filter[territory]", args.territory);
        push_opt(&mut query, "sort", args.sort);
        push_opt(&mut query, "limit", args.limit);
        if args.include_response.unwrap_or(false) {
            query.push(("include".into(), "response".into()));
        }
        let value = self
            .client
            .get(&format!("/v1/apps/{}/customerReviews", args.app_id), &query)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Respond to a customer review.
    #[tool(
        description = "Post a developer response to a customer review. A review can have only one \
response; to change it, delete the existing one with delete_review_response first."
    )]
    async fn respond_to_review(
        &self,
        Parameters(args): Parameters<RespondToReviewArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = review_response_body(&args.review_id, &args.response_body);
        let value = self
            .client
            .post("/v1/customerReviewResponses", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Delete a developer response.
    #[tool(description = "Delete a developer response to a customer review by response ID.")]
    async fn delete_review_response(
        &self,
        Parameters(args): Parameters<DeleteReviewResponseArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.client
            .delete(&format!("/v1/customerReviewResponses/{}", args.response_id))
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(json!({ "deleted": args.response_id }))
    }
}

// ---- Pure JSON:API document builders (unit-tested below) --------------------

fn review_response_body(review_id: &str, response_body: &str) -> Value {
    json!({
        "data": {
            "type": "customerReviewResponses",
            "attributes": { "responseBody": response_body },
            "relationships": {
                "review": { "data": { "type": "customerReviews", "id": review_id } }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_response_body_shape() {
        let b = review_response_body("rev-1", "Thanks for the feedback!");
        assert_eq!(b["data"]["type"], "customerReviewResponses");
        assert_eq!(
            b["data"]["attributes"]["responseBody"],
            "Thanks for the feedback!"
        );
        assert_eq!(
            b["data"]["relationships"]["review"]["data"]["type"],
            "customerReviews"
        );
        assert_eq!(b["data"]["relationships"]["review"]["data"]["id"], "rev-1");
    }
}
