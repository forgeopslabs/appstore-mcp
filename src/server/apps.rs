//! Tools for apps and app-level metadata (`appInfos`).

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{de_coerce_json, push_opt, set_opt_str, AppStoreServer};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListAppsArgs {
    /// Filter by bundle ID, e.g. "com.example.app".
    #[serde(default)]
    pub bundle_id: Option<String>,
    /// Filter by app name.
    #[serde(default)]
    pub name: Option<String>,
    /// Filter by SKU.
    #[serde(default)]
    pub sku: Option<String>,
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetAppArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// Comma-separated related resources to include, e.g. "appInfos,appStoreVersions".
    #[serde(default)]
    pub include: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateAppArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
    /// Attributes to update, e.g. {"primaryLocale": "en-US", "availableInNewTerritories": true}.
    #[serde(deserialize_with = "de_coerce_json")]
    pub attributes: Value,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListAppInfosArgs {
    /// The app's App Store Connect ID.
    pub app_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateAppInfoArgs {
    /// The appInfo ID (from list_app_infos).
    pub app_info_id: String,
    /// Attributes to update, e.g. category/content-rights relationships are set separately.
    #[serde(deserialize_with = "de_coerce_json")]
    pub attributes: Value,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetAgeRatingArgs {
    /// The ageRatingDeclaration ID (from appInfo's ageRatingDeclaration relationship —
    /// fetch via get_app or list_app_infos with include=ageRatingDeclaration).
    pub age_rating_declaration_id: String,
    /// Questionnaire answers, e.g. {"violenceCartoonOrFantasy": "NONE",
    /// "gamblingSimulated": "FREQUENT_OR_INTENSE", "unrestrictedWebAccess": false}.
    #[serde(deserialize_with = "de_coerce_json")]
    pub attributes: Value,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateAppInfoLocalizationArgs {
    /// The appInfo ID (from list_app_infos).
    pub app_info_id: String,
    /// BCP-47 locale, e.g. "en-US".
    pub locale: String,
    /// Localized app name (required by the API). To change only privacy fields on
    /// an existing localization, use update_app_info_localization instead.
    pub name: String,
    /// Localized subtitle.
    #[serde(default)]
    pub subtitle: Option<String>,
    /// Privacy policy URL.
    #[serde(default)]
    pub privacy_policy_url: Option<String>,
    /// Privacy policy text (for platforms that show inline text, e.g. tvOS).
    #[serde(default)]
    pub privacy_policy_text: Option<String>,
    /// Privacy choices URL.
    #[serde(default)]
    pub privacy_choices_url: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateAppInfoLocalizationArgs {
    /// The appInfoLocalization ID.
    pub localization_id: String,
    /// Attributes to update (name, subtitle, privacyPolicyUrl, privacyPolicyText, privacyChoicesUrl).
    #[serde(deserialize_with = "de_coerce_json")]
    pub attributes: Value,
}

#[tool_router(router = apps_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// List apps in the account.
    #[tool(
        description = "List apps on the account, optionally filtered by bundle ID, name, or SKU."
    )]
    async fn list_apps(
        &self,
        Parameters(args): Parameters<ListAppsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "filter[bundleId]", args.bundle_id);
        push_opt(&mut query, "filter[name]", args.name);
        push_opt(&mut query, "filter[sku]", args.sku);
        push_opt(&mut query, "limit", args.limit);
        let value = self
            .client
            .get("/v1/apps", &query)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Get a single app by ID.
    #[tool(description = "Get a single app by its App Store Connect ID, with optional includes.")]
    async fn get_app(
        &self,
        Parameters(args): Parameters<GetAppArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut query = Vec::new();
        push_opt(&mut query, "include", args.include);
        let value = self
            .client
            .get(&format!("/v1/apps/{}", args.app_id), &query)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Update an app's attributes.
    #[tool(
        description = "Update an app's attributes (e.g. primaryLocale, availableInNewTerritories, \
contentRightsDeclaration)."
    )]
    async fn update_app(
        &self,
        Parameters(args): Parameters<UpdateAppArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = json!({
            "data": { "type": "apps", "id": args.app_id, "attributes": args.attributes }
        });
        let value = self
            .client
            .patch(&format!("/v1/apps/{}", args.app_id), body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// List an app's appInfos (metadata containers per app version state).
    #[tool(
        description = "List an app's appInfos — metadata containers holding category and \
age-rating relationships for the app."
    )]
    async fn list_app_infos(
        &self,
        Parameters(args): Parameters<ListAppInfosArgs>,
    ) -> Result<CallToolResult, McpError> {
        let value = self
            .client
            .get(&format!("/v1/apps/{}/appInfos", args.app_id), &[])
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Update an appInfo's attributes.
    #[tool(description = "Update an appInfo's attributes by appInfo ID.")]
    async fn update_app_info(
        &self,
        Parameters(args): Parameters<UpdateAppInfoArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = json!({
            "data": { "type": "appInfos", "id": args.app_info_id, "attributes": args.attributes }
        });
        let value = self
            .client
            .patch(&format!("/v1/appInfos/{}", args.app_info_id), body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Set the age-rating questionnaire answers.
    #[tool(
        description = "Set an app's age-rating questionnaire answers (required before submission). \
Pass the ageRatingDeclaration ID and the questionnaire attributes. Enum values are typically \
NONE / INFREQUENT_OR_MILD / FREQUENT_OR_INTENSE, plus booleans for items like gambling and \
unrestrictedWebAccess."
    )]
    async fn set_age_rating(
        &self,
        Parameters(args): Parameters<SetAgeRatingArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = age_rating_body(&args.age_rating_declaration_id, args.attributes);
        let value = self
            .client
            .patch(
                &format!(
                    "/v1/ageRatingDeclarations/{}",
                    args.age_rating_declaration_id
                ),
                body,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Create a localized app-info entry (name/subtitle/privacy).
    #[tool(
        description = "Create a localized app name, subtitle, and privacy policy for a locale \
(appInfoLocalizations). This is the app-level name/subtitle, distinct from per-version metadata."
    )]
    async fn create_app_info_localization(
        &self,
        Parameters(args): Parameters<CreateAppInfoLocalizationArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = app_info_localization_create_body(&args);
        let value = self
            .client
            .post("/v1/appInfoLocalizations", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Update a localized app-info entry.
    #[tool(
        description = "Update an appInfoLocalization by ID (name, subtitle, privacy URLs/text)."
    )]
    async fn update_app_info_localization(
        &self,
        Parameters(args): Parameters<UpdateAppInfoLocalizationArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = app_info_localization_update_body(&args.localization_id, args.attributes);
        let value = self
            .client
            .patch(
                &format!("/v1/appInfoLocalizations/{}", args.localization_id),
                body,
            )
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }
}

// ---- Pure JSON:API document builders (unit-tested below) --------------------

fn age_rating_body(id: &str, attributes: Value) -> Value {
    json!({
        "data": { "type": "ageRatingDeclarations", "id": id, "attributes": attributes }
    })
}

fn app_info_localization_create_body(args: &CreateAppInfoLocalizationArgs) -> Value {
    let mut attrs = json!({ "locale": args.locale, "name": args.name });
    set_opt_str(&mut attrs, "subtitle", &args.subtitle);
    set_opt_str(&mut attrs, "privacyPolicyUrl", &args.privacy_policy_url);
    set_opt_str(&mut attrs, "privacyPolicyText", &args.privacy_policy_text);
    set_opt_str(&mut attrs, "privacyChoicesUrl", &args.privacy_choices_url);
    json!({
        "data": {
            "type": "appInfoLocalizations",
            "attributes": attrs,
            "relationships": {
                "appInfo": { "data": { "type": "appInfos", "id": args.app_info_id } }
            }
        }
    })
}

fn app_info_localization_update_body(id: &str, attributes: Value) -> Value {
    json!({
        "data": { "type": "appInfoLocalizations", "id": id, "attributes": attributes }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn age_rating_body_shape() {
        let b = age_rating_body("ard-1", json!({ "gamblingSimulated": "NONE" }));
        assert_eq!(b["data"]["type"], "ageRatingDeclarations");
        assert_eq!(b["data"]["id"], "ard-1");
        assert_eq!(b["data"]["attributes"]["gamblingSimulated"], "NONE");
    }

    #[test]
    fn app_info_localization_create_includes_required_and_set_fields() {
        let args = CreateAppInfoLocalizationArgs {
            app_info_id: "ai-1".into(),
            locale: "en-US".into(),
            name: "My App".into(),
            subtitle: None,
            privacy_policy_url: Some("https://example.com/privacy".into()),
            privacy_policy_text: None,
            privacy_choices_url: None,
        };
        let b = app_info_localization_create_body(&args);
        assert_eq!(b["data"]["type"], "appInfoLocalizations");
        assert_eq!(b["data"]["attributes"]["locale"], "en-US");
        assert_eq!(b["data"]["attributes"]["name"], "My App");
        assert_eq!(
            b["data"]["attributes"]["privacyPolicyUrl"],
            "https://example.com/privacy"
        );
        assert!(b["data"]["attributes"].get("subtitle").is_none());
        assert_eq!(b["data"]["relationships"]["appInfo"]["data"]["id"], "ai-1");
    }

    #[test]
    fn app_info_localization_update_has_id_no_relationship() {
        let b = app_info_localization_update_body("loc-2", json!({ "subtitle": "Now faster" }));
        assert_eq!(b["data"]["id"], "loc-2");
        assert_eq!(b["data"]["attributes"]["subtitle"], "Now faster");
        assert!(b["data"].get("relationships").is_none());
    }
}
