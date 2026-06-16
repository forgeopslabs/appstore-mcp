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

- **Curated tools** (99) for the common, multi-step, or error-prone workflows —
  apps & metadata, IAPs, subscriptions & offers, versions, pricing, availability,
  App Review submission, TestFlight, provisioning, asset uploads, promoted
  purchases, customer reviews, phased release, users, in-app events, Xcode Cloud,
  and analytics reports.
- **Two generic escape-hatch tools** — `appstore_request` and `appstore_list` —
  that can call *any* endpoint with raw JSON:API documents.

## Tools

| Group | Tools |
|------|-------|
| **Generic** | `appstore_request`, `appstore_list` |
| **Apps & metadata** | `list_apps`, `get_app`, `update_app`, `list_app_infos`, `update_app_info`, `set_age_rating`, `create_app_info_localization`, `update_app_info_localization` |
| **In-app purchases (v2)** | `list_in_app_purchases`, `create_in_app_purchase`, `update_in_app_purchase`, `delete_in_app_purchase`, `create_iap_localization`, `set_iap_price_schedule`, `upload_iap_review_screenshot` |
| **Subscriptions** | `list_subscription_groups`, `create_subscription_group`, `create_subscription`, `update_subscription`, `create_subscription_localization`, `set_subscription_price` |
| **Versions & metadata** | `list_app_store_versions`, `create_app_store_version`, `create_version_localization`, `update_version_localization` |
| **App Review submission** | `create_review_submission`, `add_review_submission_item`, `submit_review_submission`, `list_review_submissions`, `submit_in_app_purchase`, `set_app_review_detail`, `create_app_encryption_declaration`, `assign_build_encryption_declaration` |
| **Pricing** | `list_territories`, `list_iap_price_points`, `list_subscription_price_points` |
| **Availability** | `set_iap_availability`, `set_subscription_availability`, `set_app_availability` |
| **TestFlight** | `list_builds`, `list_beta_groups`, `create_beta_group`, `add_beta_tester`, `submit_build_for_beta_review`, `set_build_test_notes`, `set_build_beta_detail`, `set_beta_app_review_detail`, `expire_build`, `add_build_to_beta_group` |
| **Provisioning & signing** | `list_bundle_ids`, `create_bundle_id`, `enable_bundle_id_capability`, `disable_bundle_id_capability`, `list_certificates`, `create_certificate`, `list_devices`, `register_device`, `list_profiles`, `create_profile` |
| **Assets** | `upload_app_screenshot`, `upload_app_preview`, `create_screenshot_set`, `create_preview_set`, `delete_screenshot_set`, `delete_preview_set`, `reorder_screenshots` |
| **Subscription offers** | `create_introductory_offer`, `create_promotional_offer`, `create_winback_offer`, `list_winback_offers` |
| **Offer codes** | `create_offer_code`, `generate_one_time_use_codes`, `create_custom_offer_code`, `list_offer_codes` |
| **Promoted purchases** | `create_promoted_purchase`, `update_promoted_purchase`, `set_promoted_purchase_order`, `list_promoted_purchases` |
| **Customer reviews** | `list_customer_reviews`, `respond_to_review`, `delete_review_response` |
| **Phased release** | `start_phased_release`, `update_phased_release` |
| **Users & access** | `list_users`, `invite_user`, `update_user`, `remove_user` |
| **In-app events** | `create_app_event`, `create_app_event_localization`, `upload_app_event_screenshot` |
| **Xcode Cloud** | `list_ci_products`, `list_ci_workflows`, `start_ci_build`, `get_ci_build_run`, `list_ci_build_actions` |
| **Analytics reports** | `request_analytics_report`, `list_analytics_reports`, `list_analytics_report_instances`, `list_analytics_report_segments` |

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

## Limitations (enforced by Apple)

- **You cannot create an app via the API.** The `apps` resource only allows GET and
  UPDATE — `POST /v1/apps` returns `403 FORBIDDEN_ERROR`. Create the app record in
  the [App Store Connect website](https://appstoreconnect.apple.com) (*Apps → ➕ →
  New App*); you can pre-create its bundle ID with `create_bundle_id`. All other
  tools operate on an existing app.
- A deleted in-app purchase's `productId` is permanently reserved by Apple and
  cannot be reused.

## Development

```bash
cargo test                              # unit tests (JWT signing, MD5, body builders)
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

### Live integration tests

`scripts/integration_test.py` drives the compiled server against the real API.
Read-only by default; `--write` adds a self-cleaning IAP lifecycle.

```bash
cargo build --release
# Credentials via env (ASC_ISSUER_ID/ASC_KEY_ID/ASC_PRIVATE_KEY_PATH) or local
# appstore-connect.txt + AuthKey_*.p8 in the repo root (both gitignored).
python3 scripts/integration_test.py --app <APP_ID>          # read-only sweep
python3 scripts/integration_test.py --app <APP_ID> --write  # + write lifecycle
```

## License

MIT
