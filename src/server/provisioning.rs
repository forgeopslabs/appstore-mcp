//! Provisioning & signing tools: bundle IDs, certificates, devices, profiles.

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
