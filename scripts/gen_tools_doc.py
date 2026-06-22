#!/usr/bin/env python3
"""Generate docs/TOOLS.md from the server's live `tools/list` schemas.

Needs the release binary built (`cargo build --release`) but NO credentials —
`tools/list` works without them. Run from anywhere:

    python3 scripts/gen_tools_doc.py
"""
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from integration_test import McpClient  # noqa: E402  (reuse the stdio client)

# group title -> (one-line blurb, ordered tool names) — mirrors the README table.
GROUPS = [
    ("Generic", "Reach any endpoint with raw JSON:API.",
     ["appstore_request", "appstore_list"]),
    ("Apps & metadata", "Read/update apps, app-level metadata, age rating, and localized app name/subtitle.",
     ["list_apps", "get_app", "update_app", "list_app_infos", "update_app_info",
      "set_age_rating", "create_app_info_localization", "update_app_info_localization"]),
    ("In-app purchases (v2)", "Create and manage non-subscription in-app purchases.",
     ["list_in_app_purchases", "create_in_app_purchase", "update_in_app_purchase",
      "delete_in_app_purchase", "create_iap_localization", "set_iap_price_schedule",
      "upload_iap_review_screenshot"]),
    ("Subscriptions", "Subscription groups, subscriptions, localizations, and prices.",
     ["list_subscription_groups", "create_subscription_group", "create_subscription",
      "update_subscription", "create_subscription_localization", "set_subscription_price"]),
    ("Versions & metadata", "App Store versions and their localized metadata.",
     ["list_app_store_versions", "create_app_store_version", "create_version_localization",
      "update_version_localization"]),
    ("App Review submission", "Submit versions/IAPs for review and satisfy the metadata gates.",
     ["create_review_submission", "add_review_submission_item", "submit_review_submission",
      "list_review_submissions", "submit_in_app_purchase", "set_app_review_detail",
      "create_app_encryption_declaration", "assign_build_encryption_declaration"]),
    ("Pricing", "Territories and price-point lookups.",
     ["list_territories", "list_iap_price_points", "list_subscription_price_points"]),
    ("Availability", "Control which territories products/apps are sold in.",
     ["set_iap_availability", "set_subscription_availability", "set_app_availability"]),
    ("TestFlight", "Builds, beta groups, testers, beta review, and build details.",
     ["list_builds", "list_beta_groups", "create_beta_group", "add_beta_tester",
      "submit_build_for_beta_review", "set_build_test_notes", "set_build_beta_detail",
      "set_beta_app_review_detail", "expire_build", "add_build_to_beta_group"]),
    ("Provisioning & signing", "Bundle IDs (+ capabilities), certificates, devices, profiles.",
     ["list_bundle_ids", "create_bundle_id", "enable_bundle_id_capability",
      "disable_bundle_id_capability", "list_certificates", "create_certificate",
      "list_devices", "register_device", "list_profiles", "create_profile"]),
    ("Assets", "Screenshot/preview sets and uploads (reserve -> upload -> commit).",
     ["upload_app_screenshot", "upload_app_preview", "create_screenshot_set",
      "create_preview_set", "delete_screenshot_set", "delete_preview_set", "reorder_screenshots"]),
    ("Subscription offers", "Introductory, promotional, and win-back offers.",
     ["create_introductory_offer", "create_promotional_offer", "create_winback_offer",
      "list_winback_offers"]),
    ("Offer codes", "Offer-code campaigns plus one-time-use and custom codes.",
     ["create_offer_code", "generate_one_time_use_codes", "create_custom_offer_code",
      "list_offer_codes"]),
    ("Promoted purchases", "Promote IAPs/subscriptions on the product page.",
     ["create_promoted_purchase", "update_promoted_purchase", "set_promoted_purchase_order",
      "list_promoted_purchases"]),
    ("Customer reviews", "Read reviews and post/delete developer responses.",
     ["list_customer_reviews", "respond_to_review", "delete_review_response"]),
    ("Phased release", "Staged 7-day rollout control.",
     ["start_phased_release", "update_phased_release"]),
    ("Users & access", "Team users and invitations.",
     ["list_users", "invite_user", "update_user", "remove_user"]),
    ("In-app events", "App Store in-app events, localizations, and media.",
     ["create_app_event", "create_app_event_localization", "upload_app_event_screenshot"]),
    ("Xcode Cloud", "Inspect and trigger Xcode Cloud (CI) builds.",
     ["list_ci_products", "list_ci_workflows", "start_ci_build", "get_ci_build_run",
      "list_ci_build_actions"]),
    ("Analytics reports", "Request and read App Store analytics reports.",
     ["request_analytics_report", "list_analytics_reports", "list_analytics_report_instances",
      "list_analytics_report_segments"]),
    ("Custom product pages", "Marketing product-page variants: pages, versions, localized text, and image sets.",
     ["list_custom_product_pages", "get_custom_product_page", "create_custom_product_page",
      "update_custom_product_page", "delete_custom_product_page",
      "list_custom_product_page_versions", "create_custom_product_page_version",
      "list_custom_product_page_localizations", "create_custom_product_page_localization",
      "update_custom_product_page_localization", "create_cpp_screenshot_set",
      "create_cpp_preview_set"]),
]


def type_label(schema, defs):
    s = schema
    ref = s.get("$ref") or (s.get("allOf", [{}])[0].get("$ref") if s.get("allOf") else None)
    if ref:
        s = defs.get(ref.split("/")[-1], s)
    if "enum" in s:
        return "`" + " \\| ".join(str(v) for v in s["enum"]) + "`"
    t = s.get("type")
    if isinstance(t, list):
        t = next((x for x in t if x != "null"), t[0])
    if t == "array":
        items = s.get("items", {})
        iref = items.get("$ref")
        if iref:
            it = defs.get(iref.split("/")[-1], {})
            if "enum" in it:
                return "array of `" + " \\| ".join(str(v) for v in it["enum"]) + "`"
            return f"array of {iref.split('/')[-1]}"
        return f"array of {items.get('type', 'object')}"
    return t or "object"


def fetch_tools():
    mc = McpClient(dict(os.environ))  # tools/list needs no credentials
    mc.handshake()
    resp = mc._read(mc._send("tools/list", {}))
    mc.close()
    return {t["name"]: t for t in resp["result"]["tools"]}


def render(tools):
    covered, out = set(), []
    total = len(tools)
    out.append("# Tool reference\n")
    out.append(f"All **{total}** tools exposed by `appstore-mcp`, grouped by domain. "
               "Auto-generated from the server's live `tools/list` schemas by "
               "`scripts/gen_tools_doc.py` — regenerate after changing tools.\n")
    out.append("> Required parameters are marked **yes**. IDs are opaque strings returned by the "
               "`list_*`/`get_*` tools — resolve them first. Anything not covered here is reachable "
               "via the generic `appstore_request` / `appstore_list` tools.\n")
    out.append("## Contents\n")
    for name, _desc, names in GROUPS:
        anchor = name.lower().replace(" & ", "--").replace(" ", "-").replace("(", "").replace(")", "")
        out.append(f"- [{name}](#{anchor}) ({len(names)})")
    out.append("")
    for name, desc, names in GROUPS:
        out.append(f"## {name}\n")
        out.append(f"{desc}\n")
        for tn in names:
            t = tools.get(tn)
            if not t:
                out.append(f"### `{tn}`\n\n_MISSING from server tools/list!_\n")
                continue
            covered.add(tn)
            out.append(f"### `{tn}`\n")
            out.append((t.get("description") or "").strip() + "\n")
            schema = t.get("inputSchema", {})
            props = schema.get("properties", {})
            required = set(schema.get("required", []))
            defs = schema.get("$defs", {})
            if props:
                out.append("| Parameter | Type | Required | Description |")
                out.append("|---|---|---|---|")
                for p in sorted(props, key=lambda p: (p not in required, p)):
                    ps = props[p]
                    dp = (ps.get("description") or "").replace("\n", " ").strip()
                    out.append(f"| `{p}` | {type_label(ps, defs)} | "
                               f"{'**yes**' if p in required else 'no'} | {dp} |")
                out.append("")
            else:
                out.append("_No parameters._\n")
    missing = set(tools) - covered
    if missing:
        out.append("## Ungrouped (update GROUPS in gen_tools_doc.py)\n")
        out.extend(f"- `{m}`" for m in sorted(missing))
        print("WARNING: ungrouped tools:", sorted(missing), file=sys.stderr)
    return "\n".join(out) + "\n", total, len(covered), len(missing)


def main():
    tools = fetch_tools()
    body, total, grouped, missing = render(tools)
    os.makedirs(os.path.join(ROOT, "docs"), exist_ok=True)
    with open(os.path.join(ROOT, "docs", "TOOLS.md"), "w") as f:
        f.write(body)
    print(f"wrote docs/TOOLS.md: {total} tools, {grouped} grouped, {missing} ungrouped")
    sys.exit(1 if missing else 0)


if __name__ == "__main__":
    main()
