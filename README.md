# appstore-mcp

An [MCP](https://modelcontextprotocol.io) server, written in Rust, that exposes
the **Apple App Store Connect API** to AI agents. It can create and manage apps,
in-app purchases, subscriptions, pricing, App Store versions and metadata,
TestFlight, provisioning, and upload screenshots/previews — and reach *any* other
App Store Connect endpoint through two generic JSON:API tools.

Built on the official [`rmcp`](https://crates.io/crates/rmcp) SDK over stdio.

## Design: hybrid coverage

The App Store Connect API has hundreds of endpoints but is uniformly
[JSON:API](https://jsonapi.org). Rather than a tool per endpoint, this server is
**hybrid**:

- **Curated tools** (42 total) for the common, multi-step, or error-prone
  workflows — apps, IAPs, subscriptions, versions/metadata, pricing, TestFlight,
  provisioning, and asset uploads.
- **Two generic escape-hatch tools** — `appstore_request` and `appstore_list` —
  that can call *any* endpoint (Game Center, customer reviews, app events, etc.)
  with raw JSON:API documents.

## Tools

| Group | Tools |
|------|-------|
| **Generic** | `appstore_request`, `appstore_list` |
| **Apps** | `list_apps`, `get_app`, `update_app`, `list_app_infos`, `update_app_info` |
| **In-app purchases (v2)** | `list_in_app_purchases`, `create_in_app_purchase`, `update_in_app_purchase`, `delete_in_app_purchase`, `create_iap_localization`, `set_iap_price_schedule`, `upload_iap_review_screenshot` |
| **Subscriptions** | `list_subscription_groups`, `create_subscription_group`, `create_subscription`, `update_subscription`, `create_subscription_localization`, `set_subscription_price` |
| **Versions & metadata** | `list_app_store_versions`, `create_app_store_version`, `create_version_localization`, `update_version_localization` |
| **Pricing** | `list_territories`, `list_iap_price_points`, `list_subscription_price_points` |
| **TestFlight** | `list_builds`, `list_beta_groups`, `create_beta_group`, `add_beta_tester`, `submit_build_for_beta_review` |
| **Provisioning & signing** | `list_bundle_ids`, `create_bundle_id`, `list_certificates`, `create_certificate`, `list_devices`, `register_device`, `list_profiles`, `create_profile` |
| **Assets** | `upload_app_screenshot`, `upload_app_preview` |

## Credentials

Generate a **Team Key** in App Store Connect → *Users and Access → Integrations →
App Store Connect API*, and download the `.p8` file. Then set:

| Variable | Required | Description |
|----------|----------|-------------|
| `ASC_ISSUER_ID` | ✅ | Issuer UUID shown above the keys table. |
| `ASC_KEY_ID` | ✅ | The API key's Key ID. |
| `ASC_PRIVATE_KEY` | one of | Inline `.p8` PEM contents. |
| `ASC_PRIVATE_KEY_PATH` | one of | Path to the downloaded `.p8` file. |
| `ASC_BASE_URL` | optional | Override the API origin. |
| `ASC_LOG` | optional | Log filter (to stderr). Default `info`. |

See [`.env.example`](.env.example). The server authenticates each request with a
short-lived **ES256 JWT** signed by your key (cached and refreshed automatically).

> The server starts even without credentials so a client can list its tools;
> tool calls then return an actionable configuration error until creds are set.

## Build & run

```bash
cargo build --release
ASC_ISSUER_ID=... ASC_KEY_ID=... ASC_PRIVATE_KEY_PATH=/path/AuthKey_XXX.p8 \
  ./target/release/appstore-mcp
```

The server speaks MCP over **stdio**. Logs go to **stderr**; stdout is the
protocol channel.

### Use with an MCP client

Example client config (e.g. Claude Desktop's `mcpServers`):

```json
{
  "mcpServers": {
    "appstore": {
      "command": "/absolute/path/to/appstore-mcp/target/release/appstore-mcp",
      "env": {
        "ASC_ISSUER_ID": "00000000-0000-0000-0000-000000000000",
        "ASC_KEY_ID": "ABCD123456",
        "ASC_PRIVATE_KEY_PATH": "/absolute/path/to/AuthKey_ABCD123456.p8"
      }
    }
  }
}
```

### Inspect with the MCP Inspector

```bash
npx @modelcontextprotocol/inspector ./target/release/appstore-mcp
```

## Usage notes

- **IDs are opaque.** List/get first to resolve app, IAP, subscription, set, and
  price-point IDs, then pass them to create/update tools.
- **Pricing needs a price point.** Use `list_iap_price_points` /
  `list_subscription_price_points` to get the `id` for `set_iap_price_schedule` /
  `set_subscription_price`.
- **Asset uploads** (`upload_*`) take a local file path and run the full reserve →
  chunked upload → MD5 commit flow in one call. Screenshots/previews require an
  existing `appScreenshotSet` / `appPreviewSet`; create those with the generic
  tools if needed.
- **Anything not listed** is reachable via `appstore_request` (raw method + path +
  JSON:API body) or `appstore_list` (paginated GET). Example:
  `appstore_request { "method": "GET", "path": "/v1/apps/123/customerReviews" }`.
- **Not covered:** sales/finance report endpoints return gzipped TSV (not JSON:API)
  and are out of scope for these tools.

## Development

```bash
cargo test                              # unit tests (JWT signing, MD5)
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## License

MIT
