//! Provisioning & signing tools: bundle IDs, capabilities, certificates,
//! devices, profiles.
//!
//! Schemas verified against Apple's generated OpenAPI models in the AvdLee
//! Swift SDK (BundleIDCapabilityCreateRequest):
//!   - attributes: capabilityType (required), settings (optional array)
//!   - relationships: bundleId → { data: { type: "bundleIds", id } }

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{push_opt, AppStoreServer};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListArgs {
    /// Page size (max 200).
    #[serde(default)]
    pub limit: Option<u32>,
    /// Comma-separated includes.
    #[serde(default)]
    pub include: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateBundleIdArgs {
    /// Display name for the bundle ID.
    pub name: String,
    /// The reverse-DNS identifier, e.g. "com.example.app".
    pub identifier: String,
    /// Platform: "IOS", "MAC_OS", or "UNIVERSAL".
    pub platform: String,
    /// Optional team/seed ID.
    #[serde(default)]
    pub seed_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateCertificateArgs {
    /// Certificate type, e.g. "IOS_DISTRIBUTION", "IOS_DEVELOPMENT", "DISTRIBUTION".
    pub certificate_type: String,
    /// PEM-encoded Certificate Signing Request (CSR) content.
    pub csr_content: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RegisterDeviceArgs {
    /// Device name.
    pub name: String,
    /// Platform: "IOS" or "MAC_OS".
    pub platform: String,
    /// The device UDID.
    pub udid: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EnableBundleIdCapabilityArgs {
    /// The bundle ID resource ID to enable the capability on.
    pub bundle_id: String,
    /// The capability type to enable. Common values: PUSH_NOTIFICATIONS,
    /// APP_GROUPS, ICLOUD, ASSOCIATED_DOMAINS, IN_APP_PURCHASE, GAME_CENTER,
    /// SIGN_IN_WITH_APPLE, MAPS, WALLET, HEALTHKIT, HOMEKIT, WIRELESS_ACCESSORY_CONFIGURATION,
    /// DATA_PROTECTION, SIRIKIT, NETWORK_EXTENSIONS, MULTIPATH, HOT_SPOT, NFC_TAG_READING,
    /// CLASSKIT, AUTOFILL_CREDENTIAL_PROVIDER, ACCESS_WIFI_INFORMATION,
    /// COREMEDIA_HLS_LOW_LATENCY, FONT_INSTALLATION, EXTENDED_VIRTUAL_ADDRESS_SPACE,
    /// USER_MANAGEMENT, APPLE_ID_AUTH.
    pub capability_type: String,
    /// Optional array of capability-setting objects (free-form JSON array).
    /// Only send when the capability requires settings (e.g. iCloud containers,
    /// App Groups identifiers). Structure matches Apple's `CapabilitySetting` model.
    #[serde(default)]
    pub settings: Option<Value>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DisableBundleIdCapabilityArgs {
    /// The bundleIdCapabilities resource ID to delete.
    pub capability_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateProfileArgs {
    /// Profile name.
    pub name: String,
    /// Profile type, e.g. "IOS_APP_DEVELOPMENT", "IOS_APP_STORE", "IOS_APP_ADHOC".
    pub profile_type: String,
    /// The bundle ID resource ID this profile is for.
    pub bundle_id: String,
    /// Certificate resource IDs to include.
    pub certificate_ids: Vec<String>,
    /// Device resource IDs to include (required for development/ad-hoc profiles).
    #[serde(default)]
    pub device_ids: Option<Vec<String>>,
}

#[tool_router(router = provisioning_router, vis = "pub(crate)")]
impl AppStoreServer {
    /// List bundle IDs.
    #[tool(description = "List registered bundle IDs.")]
    async fn list_bundle_ids(
        &self,
        Parameters(args): Parameters<ListArgs>,
    ) -> Result<CallToolResult, McpError> {
        let value = self
            .client
            .get("/v1/bundleIds", &list_query(&args))
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Register a new bundle ID.
    #[tool(description = "Register a new bundle ID (name, reverse-DNS identifier, platform).")]
    async fn create_bundle_id(
        &self,
        Parameters(args): Parameters<CreateBundleIdArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut attributes = json!({
            "name": args.name,
            "identifier": args.identifier,
            "platform": args.platform,
        });
        if let Some(seed) = args.seed_id {
            attributes["seedId"] = json!(seed);
        }
        let body = json!({ "data": { "type": "bundleIds", "attributes": attributes } });
        let value = self
            .client
            .post("/v1/bundleIds", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Enable a capability on a bundle ID.
    #[tool(
        description = "Enable a capability on a registered bundle ID. Provide the bundle_id \
resource ID and the capability_type (e.g. PUSH_NOTIFICATIONS, ICLOUD, APP_GROUPS, \
ASSOCIATED_DOMAINS, SIGN_IN_WITH_APPLE). Pass settings only for capabilities that require \
extra configuration (e.g. iCloud containers)."
    )]
    async fn enable_bundle_id_capability(
        &self,
        Parameters(args): Parameters<EnableBundleIdCapabilityArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = bundle_id_capability_body(
            &args.bundle_id,
            &args.capability_type,
            args.settings.as_ref(),
        );
        let value = self
            .client
            .post("/v1/bundleIdCapabilities", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Disable (delete) a capability from a bundle ID.
    #[tool(
        description = "Disable a capability on a bundle ID by deleting the bundleIdCapabilities \
resource. Pass the capability_id returned from enable_bundle_id_capability or list_bundle_ids \
(include=bundleIdCapabilities)."
    )]
    async fn disable_bundle_id_capability(
        &self,
        Parameters(args): Parameters<DisableBundleIdCapabilityArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.client
            .delete(&format!("/v1/bundleIdCapabilities/{}", args.capability_id))
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(json!({ "deleted": args.capability_id }))
    }

    /// List certificates.
    #[tool(description = "List signing certificates.")]
    async fn list_certificates(
        &self,
        Parameters(args): Parameters<ListArgs>,
    ) -> Result<CallToolResult, McpError> {
        let value = self
            .client
            .get("/v1/certificates", &list_query(&args))
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Create a certificate from a CSR.
    #[tool(
        description = "Create a signing certificate from a CSR (certificate type + PEM CSR content)."
    )]
    async fn create_certificate(
        &self,
        Parameters(args): Parameters<CreateCertificateArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = json!({
            "data": {
                "type": "certificates",
                "attributes": {
                    "certificateType": args.certificate_type,
                    "csrContent": args.csr_content,
                }
            }
        });
        let value = self
            .client
            .post("/v1/certificates", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// List registered devices.
    #[tool(description = "List registered devices.")]
    async fn list_devices(
        &self,
        Parameters(args): Parameters<ListArgs>,
    ) -> Result<CallToolResult, McpError> {
        let value = self
            .client
            .get("/v1/devices", &list_query(&args))
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Register a device.
    #[tool(
        description = "Register a device for development/ad-hoc distribution (name, platform, UDID)."
    )]
    async fn register_device(
        &self,
        Parameters(args): Parameters<RegisterDeviceArgs>,
    ) -> Result<CallToolResult, McpError> {
        let body = json!({
            "data": {
                "type": "devices",
                "attributes": {
                    "name": args.name,
                    "platform": args.platform,
                    "udid": args.udid,
                }
            }
        });
        let value = self
            .client
            .post("/v1/devices", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// List provisioning profiles.
    #[tool(description = "List provisioning profiles.")]
    async fn list_profiles(
        &self,
        Parameters(args): Parameters<ListArgs>,
    ) -> Result<CallToolResult, McpError> {
        let value = self
            .client
            .get("/v1/profiles", &list_query(&args))
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }

    /// Create a provisioning profile.
    #[tool(
        description = "Create a provisioning profile (name, type, bundle ID, certificate IDs, and \
device IDs for development/ad-hoc profiles)."
    )]
    async fn create_profile(
        &self,
        Parameters(args): Parameters<CreateProfileArgs>,
    ) -> Result<CallToolResult, McpError> {
        let certificates: Vec<Value> = args
            .certificate_ids
            .iter()
            .map(|id| json!({ "type": "certificates", "id": id }))
            .collect();
        let mut relationships = json!({
            "bundleId": { "data": { "type": "bundleIds", "id": args.bundle_id } },
            "certificates": { "data": certificates },
        });
        if let Some(devices) = args.device_ids {
            let devices: Vec<Value> = devices
                .iter()
                .map(|id| json!({ "type": "devices", "id": id }))
                .collect();
            relationships["devices"] = json!({ "data": devices });
        }
        let body = json!({
            "data": {
                "type": "profiles",
                "attributes": { "name": args.name, "profileType": args.profile_type },
                "relationships": relationships
            }
        });
        let value = self
            .client
            .post("/v1/profiles", body)
            .await
            .map_err(AppStoreServer::map_err)?;
        AppStoreServer::ok_json(value)
    }
}

/// Build a common list query (limit + include) for provisioning collections.
fn list_query(args: &ListArgs) -> Vec<(String, String)> {
    let mut query = Vec::new();
    push_opt(&mut query, "limit", args.limit);
    push_opt(&mut query, "include", args.include.clone());
    query
}

/// Build the request body for POST /v1/bundleIdCapabilities.
///
/// `capabilityType` is always included; `settings` is only included when
/// `Some` (Apple's schema marks it optional).
fn bundle_id_capability_body(
    bundle_id: &str,
    capability_type: &str,
    settings: Option<&Value>,
) -> Value {
    let mut attributes = json!({ "capabilityType": capability_type });
    if let Some(s) = settings {
        attributes["settings"] = json!(s);
    }
    json!({
        "data": {
            "type": "bundleIdCapabilities",
            "attributes": attributes,
            "relationships": {
                "bundleId": {
                    "data": { "type": "bundleIds", "id": bundle_id }
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
    fn capability_body_capability_type_present() {
        let body = bundle_id_capability_body("bid-123", "PUSH_NOTIFICATIONS", None);
        assert_eq!(body["data"]["type"], "bundleIdCapabilities");
        assert_eq!(
            body["data"]["attributes"]["capabilityType"],
            "PUSH_NOTIFICATIONS"
        );
    }

    #[test]
    fn capability_body_bundle_id_relationship_correct() {
        let body = bundle_id_capability_body("bid-456", "ICLOUD", None);
        let rel = &body["data"]["relationships"]["bundleId"]["data"];
        assert_eq!(rel["type"], "bundleIds");
        assert_eq!(rel["id"], "bid-456");
    }

    #[test]
    fn capability_body_settings_included_when_some() {
        let settings = json!([
            {
                "key": "ICLOUD_VERSION",
                "options": [{ "key": "XCODE_6", "enabled": true }]
            }
        ]);
        let body = bundle_id_capability_body("bid-789", "ICLOUD", Some(&settings));
        assert_eq!(body["data"]["attributes"]["settings"], settings);
    }

    #[test]
    fn capability_body_settings_omitted_when_none() {
        let body = bundle_id_capability_body("bid-999", "ASSOCIATED_DOMAINS", None);
        // The "settings" key must be absent — not null — when not provided.
        assert!(body["data"]["attributes"].get("settings").is_none());
    }
}
