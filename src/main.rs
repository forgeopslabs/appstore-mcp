//! `appstore-mcp` — an MCP server exposing the Apple App Store Connect API.
//!
//! Communicates over stdio (JSON-RPC). All diagnostic logging goes to **stderr**;
//! stdout is reserved for the MCP protocol.

mod auth;
mod client;
mod config;
mod error;
mod server;
mod upload;

use anyhow::Context;
use rmcp::{transport::stdio, ServiceExt};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // stdout is the JSON-RPC channel; logs MUST go to stderr or they corrupt it.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_env_filter(
            EnvFilter::try_from_env("ASC_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = config::Config::from_env();
    if !config.is_configured() {
        tracing::warn!(
            "App Store Connect credentials are not fully configured; \
             tools will return a configuration error until ASC_ISSUER_ID, ASC_KEY_ID, \
             and ASC_PRIVATE_KEY (or ASC_PRIVATE_KEY_PATH) are set."
        );
    }

    let server = server::AppStoreServer::new(config);
    tracing::info!("starting appstore-mcp on stdio");

    let service = server
        .serve(stdio())
        .await
        .context("failed to start the MCP server over stdio")?;
    service.waiting().await?;
    Ok(())
}
