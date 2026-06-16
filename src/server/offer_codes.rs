//! Subscription offer-code tools: campaigns, one-time-use codes, custom codes (#9).
//!
//! Schemas verified against Apple's generated OpenAPI models.

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::offers::{inline_offer_prices, OfferDuration, OfferPrice, SubscriptionOfferMode};
use super::{push_opt, AppStoreServer};

/// Which customers an offer code applies to.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CustomerEligibility {
    New,
    Existing,
    Expired,
}

impl CustomerEligibility {
    fn as_api(self) -> &'static str {
        match self {
            CustomerEligibility::New => "NEW",
            CustomerEligibility::Existing => "EXISTING",
            CustomerEligibility::Expired => "EXPIRED",
        }
    }
}

/// How an offer code interacts with introductory offers.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OfferEligibility {
    StackWithIntroOffers,
    ReplaceIntroOffers,
}

impl OfferEligibility {
    fn as_api(self) -> &'static str {
        match self {
            OfferEligibility::StackWithIntroOffers => "STACK_WITH_INTRO_OFFERS",
            OfferEligibility::ReplaceIntroOffers => "REPLACE_INTRO_OFFERS",
        }
    }
}

/// Environment for generated one-time-use codes.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OfferCodeEnvironment {
    Production,
    Sandbox,
}

impl OfferCodeEnvironment {
    fn as_api(self) -> &'static str {
        match self {
            OfferCodeEnvironment::Production => "PRODUCTION",
            OfferCodeEnvironment::Sandbox => "SANDBOX",
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateOfferCodeArgs {
    /// The subscription ID.
    pub subscription_id: String,
    /// Reference name (not customer-facing).
    pub name: String,
    /// Which customers are eligible (at least one): NEW, EXISTING, EXPIRED.
    pub customer_eligibilities: Vec<CustomerEligibility>,
    /// How the offer interacts with introductory offers.
    pub offer_eligibility: OfferEligibility,
    /// Offer duration.
    pub duration: OfferDuration,
    /// Charge mode.
    pub offer_mode: SubscriptionOfferMode,
    /// Number of periods.
    pub number_of_periods: u32,
    /// One price per territory (at least one required).
    pub prices: Vec<OfferPrice>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GenerateOneTimeCodesArgs {
    /// The subscriptionOfferCode ID (from create_offer_code / list_offer_codes).
    pub offer_code_id: String,
    /// How many one-time-use codes to generate.
    pub number_of_codes: u32,
    /// Expiration date (YYYY-MM-DD).
    pub expiration_date: String,
    /// Optional environment (PRODUCTION or SANDBOX).
    #[serde(default)]
    pub environment: Option<OfferCodeEnvironment>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateCustomCodeArgs {
    /// The subscriptionOfferCode ID.
    pub offer_code_id: String,
    /// The custom (vanity) code value.
    pub custom_code: String,
    /// Number of redemptions allowed.
    pub number_of_codes: u32,
    /// Optional expiration date (YYYY-MM-DD).
    #[serde(default)]
    pub expiration_date: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListOfferCodesArgs {
    /// The subscription ID.
    pub subscription_id: String,
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[tool_router(router = offer_codes_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// Create a subscription offer-code campaign.
    #[tool(
        description = "Create a subscription offer-code campaign (eligibility, duration, mode, and \
one price per territory). Then generate codes with generate_one_time_use_codes or \
create_custom_offer_code."
    )]
    async fn create_offer_code(
        &self,
        Parameters(args): Parameters<CreateOfferCodeArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = offer_code_body(&args);
        let value = self
            .client
            .post("/v1/subscriptionOfferCodes", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Generate a batch of one-time-use offer codes.
    #[tool(
        description = "Generate a batch of one-time-use codes for an offer-code campaign. The \
response includes a values URL to download the codes."
    )]
    async fn generate_one_time_use_codes(
        &self,
        Parameters(args): Parameters<GenerateOneTimeCodesArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = one_time_codes_body(&args);
        let value = self
            .client
            .post("/v1/subscriptionOfferCodeOneTimeUseCodes", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Create a custom (vanity) offer code.
    #[tool(description = "Create a custom (vanity) offer code for an offer-code campaign.")]
    async fn create_custom_offer_code(
        &self,
        Parameters(args): Parameters<CreateCustomCodeArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = custom_code_body(&args);
        let value = self
            .client
            .post("/v1/subscriptionOfferCodeCustomCodes", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// List a subscription's offer codes.
    #[tool(description = "List the offer-code campaigns configured for a subscription.")]
    async fn list_offer_codes(
        &self,
        Parameters(args): Parameters<ListOfferCodesArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "limit", args.limit);
        let value = self
            .client
            .get(
                &format!("/v1/subscriptions/{}/offerCodes", args.subscription_id),
                &query,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }
}

// ---- Pure JSON:API document builders (unit-tested below) --------------------

fn offer_code_body(args: &CreateOfferCodeArgs) -> Value {
    let (refs, included) = inline_offer_prices("subscriptionOfferCodePrices", &args.prices);
    let eligibilities: Vec<&str> = args
        .customer_eligibilities
        .iter()
        .map(|e| e.as_api())
        .collect();
    json!({
        "data": {
            "type": "subscriptionOfferCodes",
            "attributes": {
                "name": args.name,
                "customerEligibilities": eligibilities,
                "offerEligibility": args.offer_eligibility.as_api(),
                "duration": args.duration.as_api(),
                "offerMode": args.offer_mode.as_api(),
                "numberOfPeriods": args.number_of_periods,
            },
            "relationships": {
                "subscription": { "data": { "type": "subscriptions", "id": args.subscription_id } },
                "prices": { "data": refs }
            }
        },
        "included": included
    })
}

fn one_time_codes_body(args: &GenerateOneTimeCodesArgs) -> Value {
    let mut attrs = json!({
        "numberOfCodes": args.number_of_codes,
        "expirationDate": args.expiration_date,
    });
    if let Some(env) = args.environment {
        attrs["environment"] = json!(env.as_api());
    }
    json!({
        "data": {
            "type": "subscriptionOfferCodeOneTimeUseCodes",
            "attributes": attrs,
            "relationships": {
                "offerCode": { "data": { "type": "subscriptionOfferCodes", "id": args.offer_code_id } }
            }
        }
    })
}

fn custom_code_body(args: &CreateCustomCodeArgs) -> Value {
    let mut attrs = json!({
        "customCode": args.custom_code,
        "numberOfCodes": args.number_of_codes,
    });
    if let Some(d) = &args.expiration_date {
        attrs["expirationDate"] = json!(d);
    }
    json!({
        "data": {
            "type": "subscriptionOfferCodeCustomCodes",
            "attributes": attrs,
            "relationships": {
                "offerCode": { "data": { "type": "subscriptionOfferCodes", "id": args.offer_code_id } }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offer_code_body_maps_eligibilities_and_prices() {
        let args = CreateOfferCodeArgs {
            subscription_id: "sub-1".into(),
            name: "Promo".into(),
            customer_eligibilities: vec![CustomerEligibility::New, CustomerEligibility::Expired],
            offer_eligibility: OfferEligibility::StackWithIntroOffers,
            duration: OfferDuration::OneMonth,
            offer_mode: SubscriptionOfferMode::FreeTrial,
            number_of_periods: 1,
            prices: vec![OfferPrice {
                territory: "USA".into(),
                price_point_id: "pp-1".into(),
            }],
        };
        let b = offer_code_body(&args);
        let attrs = &b["data"]["attributes"];
        assert_eq!(b["data"]["type"], "subscriptionOfferCodes");
        assert_eq!(attrs["customerEligibilities"][0], "NEW");
        assert_eq!(attrs["customerEligibilities"][1], "EXPIRED");
        assert_eq!(attrs["offerEligibility"], "STACK_WITH_INTRO_OFFERS");
        assert_eq!(attrs["offerMode"], "FREE_TRIAL");
        assert_eq!(b["included"][0]["type"], "subscriptionOfferCodePrices");
        assert_eq!(
            b["data"]["relationships"]["prices"]["data"][0]["id"],
            "${price0}"
        );
    }

    #[test]
    fn one_time_codes_body_includes_optional_environment() {
        let args = GenerateOneTimeCodesArgs {
            offer_code_id: "oc-1".into(),
            number_of_codes: 100,
            expiration_date: "2026-12-31".into(),
            environment: Some(OfferCodeEnvironment::Sandbox),
        };
        let b = one_time_codes_body(&args);
        assert_eq!(b["data"]["type"], "subscriptionOfferCodeOneTimeUseCodes");
        assert_eq!(b["data"]["attributes"]["numberOfCodes"], 100);
        assert_eq!(b["data"]["attributes"]["expirationDate"], "2026-12-31");
        assert_eq!(b["data"]["attributes"]["environment"], "SANDBOX");
        assert_eq!(
            b["data"]["relationships"]["offerCode"]["data"]["id"],
            "oc-1"
        );
    }

    #[test]
    fn custom_code_body_omits_absent_expiration() {
        let args = CreateCustomCodeArgs {
            offer_code_id: "oc-1".into(),
            custom_code: "FRIENDS".into(),
            number_of_codes: 50,
            expiration_date: None,
        };
        let b = custom_code_body(&args);
        assert_eq!(b["data"]["type"], "subscriptionOfferCodeCustomCodes");
        assert_eq!(b["data"]["attributes"]["customCode"], "FRIENDS");
        assert_eq!(b["data"]["attributes"]["numberOfCodes"], 50);
        assert!(b["data"]["attributes"].get("expirationDate").is_none());
    }
}
