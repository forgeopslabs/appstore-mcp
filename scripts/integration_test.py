#!/usr/bin/env python3
"""Integration test harness for appstore-mcp against the live App Store Connect API.

Drives the compiled MCP server over stdio with real credentials and reports
pass/fail per tool. Read-only by default; pass --write to also run a reversible
in-app-purchase lifecycle (create -> localize -> price -> delete) that cleans up
after itself.

Credentials (never commit these):
  - Set ASC_ISSUER_ID / ASC_KEY_ID / ASC_PRIVATE_KEY_PATH in the environment, OR
  - Drop `appstore-connect.txt` (lines "Key ID : ..." and "Issuer ID : ...") and
    an `AuthKey_<KEYID>.p8` file in the repo root; this script will read them.

Usage:
  python3 scripts/integration_test.py --app <APP_ID> [--write]

Note: Apple permanently reserves a deleted IAP's productId, so each --write run
burns one throwaway product-id string.
"""
import argparse
import glob
import json
import os
import re
import select
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(ROOT, "target/release/appstore-mcp")


def load_credentials():
    """Resolve credentials from the environment, falling back to local files."""
    env = dict(os.environ)
    if env.get("ASC_ISSUER_ID") and env.get("ASC_KEY_ID") and (
        env.get("ASC_PRIVATE_KEY") or env.get("ASC_PRIVATE_KEY_PATH")
    ):
        return env

    creds_file = os.path.join(ROOT, "appstore-connect.txt")
    if not os.path.exists(creds_file):
        sys.exit(
            "No credentials: set ASC_ISSUER_ID/ASC_KEY_ID/ASC_PRIVATE_KEY_PATH "
            "or provide appstore-connect.txt + AuthKey_*.p8 in the repo root."
        )
    text = open(creds_file).read()
    key_id = re.search(r"Key ID\s*:\s*(\S+)", text)
    issuer = re.search(r"Iss\w*\s*ID\s*:\s*(\S+)", text)
    if not key_id or not issuer:
        sys.exit("Could not parse Key ID / Issuer ID from appstore-connect.txt")
    p8s = glob.glob(os.path.join(ROOT, "AuthKey_*.p8")) or glob.glob(os.path.join(ROOT, "*.p8"))
    if not p8s:
        sys.exit("No .p8 private key found in the repo root.")
    env.update({
        "ASC_KEY_ID": key_id.group(1),
        "ASC_ISSUER_ID": issuer.group(1),
        "ASC_PRIVATE_KEY_PATH": p8s[0],
        "ASC_LOG": env.get("ASC_LOG", "warn"),
    })
    return env


class McpClient:
    """Minimal JSON-RPC-over-stdio client for the MCP server."""

    def __init__(self, env):
        if not os.path.exists(BIN):
            sys.exit(f"Binary not found: {BIN}\nRun: cargo build --release")
        self.proc = subprocess.Popen(
            [BIN], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, env=env, bufsize=1, text=True,
        )
        self._id = 0

    def _send(self, method, params=None, notification=False):
        msg = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            msg["params"] = params
        if not notification:
            self._id += 1
            msg["id"] = self._id
        self.proc.stdin.write(json.dumps(msg) + "\n")
        self.proc.stdin.flush()
        return self._id if not notification else None

    def _read(self, want_id, timeout=30.0):
        deadline = time.time() + timeout
        while time.time() < deadline:
            r, _, _ = select.select([self.proc.stdout], [], [], deadline - time.time())
            if not r:
                break
            line = self.proc.stdout.readline()
            if not line:
                break
            line = line.strip()
            if not line:
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue
            if obj.get("id") == want_id:
                return obj
        return {"error": {"message": "timeout"}}

    def handshake(self):
        rid = self._send("initialize", {
            "protocolVersion": "2024-11-05", "capabilities": {},
            "clientInfo": {"name": "integration_test", "version": "0"},
        })
        info = self._read(rid).get("result", {}).get("serverInfo", {})
        self._send("notifications/initialized", notification=True)
        return info

    def call(self, name, args):
        resp = self._read(self._send("tools/call", {"name": name, "arguments": args}))
        if "error" in resp:
            return False, resp["error"].get("message", "")
        result = resp.get("result", {})
        text = "".join(c.get("text", "") for c in result.get("content", []))
        if result.get("isError"):
            return False, text
        try:
            return True, json.loads(text)
        except json.JSONDecodeError:
            return True, text

    def close(self):
        try:
            self.proc.stdin.close()
            self.proc.wait(timeout=5)
        except Exception:
            self.proc.kill()


def summarize(val):
    if isinstance(val, dict):
        data = val.get("data")
        if isinstance(data, list):
            return f"{len(data)} item(s)"
        if isinstance(data, dict):
            return f"{data.get('type')} id={data.get('id')}"
    return str(val)[:80]


class Report:
    def __init__(self):
        self.rows = []

    def add(self, tool, status, detail):  # status: True / False / None(skip)
        self.rows.append((tool, status, detail))
        mark = {True: "PASS", False: "FAIL", None: "SKIP"}[status]
        print(f"  [{mark}] {tool}: {detail}")

    def finish(self):
        p = sum(1 for _, s, _ in self.rows if s is True)
        f = sum(1 for _, s, _ in self.rows if s is False)
        sk = sum(1 for _, s, _ in self.rows if s is None)
        print(f"\n== Summary: PASS={p} FAIL={f} SKIP={sk} ==")
        for tool, s, detail in self.rows:
            if s is False:
                print(f"  FAIL {tool}: {detail}")
        return f


def read_only_sweep(mc, app, rep):
    print("\n== Read-only sweep ==")
    iap_id = subscription_id = None

    ok, v = mc.call("list_apps", {"limit": 200})
    detail = summarize(v)
    if ok and isinstance(v, dict):
        match = next((a for a in v.get("data", []) if a["id"] == app), None)
        detail = (f"{len(v.get('data', []))} apps; target {app} = "
                  f"'{match['attributes'].get('name')}'" if match
                  else f"{len(v.get('data', []))} apps; WARNING {app} not found")
    rep.add("list_apps", ok, detail)

    rep.add("list_territories", *_simple(mc, "list_territories", {"limit": 5}))
    rep.add("get_app", *_simple(mc, "get_app", {"app_id": app, "include": "appInfos"}))
    rep.add("list_app_infos", *_simple(mc, "list_app_infos", {"app_id": app}))
    rep.add("list_app_store_versions", *_simple(mc, "list_app_store_versions", {"app_id": app}))

    ok, v = mc.call("list_in_app_purchases", {"app_id": app})
    if ok and isinstance(v, dict) and v.get("data"):
        iap_id = v["data"][0]["id"]
    rep.add("list_in_app_purchases", ok, summarize(v))

    ok, v = mc.call("list_subscription_groups", {"app_id": app, "include": "subscriptions"})
    if ok and isinstance(v, dict):
        for inc in v.get("included", []) or []:
            if inc.get("type") == "subscriptions":
                subscription_id = inc["id"]
                break
    rep.add("list_subscription_groups", ok, summarize(v))

    for tool, args in [
        ("list_builds", {"app_id": app}), ("list_beta_groups", {"app_id": app}),
        ("list_bundle_ids", {}), ("list_certificates", {}), ("list_devices", {}),
        ("list_profiles", {}), ("list_review_submissions", {"app_id": app}),
        ("appstore_list", {"path": "/v1/apps", "limit": 1}),
        ("appstore_request", {"method": "GET", "path": f"/v1/apps/{app}"}),
    ]:
        rep.add(tool, *_simple(mc, tool, args))

    # Error path: a bad endpoint should surface a clean API error (PASS = it errored).
    ok, v = mc.call("appstore_request", {"method": "GET", "path": "/v1/thisDoesNotExist"})
    rep.add("error-path (404)", (not ok), f"surfaced: {str(v)[:80]}" if not ok else "UNEXPECTED success")

    if subscription_id:
        rep.add("list_subscription_price_points",
                *_simple(mc, "list_subscription_price_points",
                         {"subscription_id": subscription_id, "territory": "USA", "limit": 3}))
    else:
        rep.add("list_subscription_price_points", None, "no subscription to read")

    if iap_id:
        rep.add("list_iap_price_points",
                *_simple(mc, "list_iap_price_points", {"iap_id": iap_id, "territory": "USA", "limit": 3}))
    else:
        rep.add("list_iap_price_points", None, "no existing IAP (covered by --write)")

    return subscription_id


def tier2_read_sweep(mc, app, subscription_id, rep):
    """Read-only checks of the Tier 2 monetization/engagement endpoints."""
    print("\n== Tier 2 read-only sweep ==")
    rep.add("list_customer_reviews", *_simple(mc, "list_customer_reviews",
            {"app_id": app, "sort": "-createdDate", "limit": 5}))
    rep.add("list_promoted_purchases", *_simple(mc, "list_promoted_purchases",
            {"app_id": app, "limit": 5}))
    if subscription_id:
        rep.add("list_offer_codes", *_simple(mc, "list_offer_codes",
                {"subscription_id": subscription_id, "limit": 5}))
        rep.add("list_winback_offers", *_simple(mc, "list_winback_offers",
                {"subscription_id": subscription_id, "limit": 5}))
    else:
        rep.add("list_offer_codes", None, "no subscription to read")
        rep.add("list_winback_offers", None, "no subscription to read")


def tier3_read_sweep(mc, app, rep):
    """Read-only checks of the Tier 3 + custom-product-page endpoints."""
    print("\n== Tier 3 read-only sweep ==")
    rep.add("list_users", *_simple(mc, "list_users", {"limit": 5}))
    rep.add("list_ci_products", *_simple(mc, "list_ci_products", {"limit": 5}))
    rep.add("list_custom_product_pages",
            *_simple(mc, "list_custom_product_pages", {"app_id": app, "limit": 5}))


def _simple(mc, tool, args):
    ok, v = mc.call(tool, args)
    return ok, summarize(v) if ok else str(v)[:160]


def write_lifecycle(mc, app, rep):
    print("\n== Write lifecycle (reversible, auto-cleanup) ==")
    product_id = f"integ.test.deleteme.{int(time.time())}"
    iap = None
    try:
        ok, v = mc.call("create_in_app_purchase", {
            "app_id": app, "name": "Integration Test (delete me)",
            "product_id": product_id, "iap_type": "CONSUMABLE",
        })
        if ok and isinstance(v, dict):
            iap = v["data"]["id"]
        rep.add("create_in_app_purchase", ok, f"id={iap} product={product_id}" if ok else str(v)[:160])
        if not iap:
            return

        rep.add("create_iap_localization", *_simple(mc, "create_iap_localization", {
            "iap_id": iap, "locale": "en-US", "name": "Integration Test",
            "description": "temporary",
        }))

        ok, v = mc.call("list_iap_price_points", {"iap_id": iap, "territory": "USA", "limit": 3})
        pp = v["data"][0]["id"] if ok and v.get("data") else None
        rep.add("list_iap_price_points", ok, f"price_point={pp}" if pp else summarize(v))

        if pp:
            rep.add("set_iap_price_schedule", *_simple(mc, "set_iap_price_schedule", {
                "iap_id": iap, "price_point_id": pp, "base_territory": "USA",
            }))
    finally:
        if iap:
            ok, v = mc.call("delete_in_app_purchase", {"iap_id": iap})
            rep.add("delete_in_app_purchase", ok,
                    "cleaned up" if ok else f"MANUAL CLEANUP NEEDED id={iap}: {str(v)[:120]}")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--app", default=os.environ.get("ASC_TEST_APP_ID"),
                    help="App Store Connect app ID for app-specific tools.")
    ap.add_argument("--write", action="store_true",
                    help="Also run the reversible IAP write lifecycle.")
    args = ap.parse_args()
    if not args.app:
        sys.exit("Provide --app <APP_ID> (or set ASC_TEST_APP_ID).")

    mc = McpClient(load_credentials())
    info = mc.handshake()
    print(f"== {info.get('name')} {info.get('version')} | app {args.app} ==")
    rep = Report()
    try:
        subscription_id = read_only_sweep(mc, args.app, rep)
        tier2_read_sweep(mc, args.app, subscription_id, rep)
        tier3_read_sweep(mc, args.app, rep)
        if args.write:
            write_lifecycle(mc, args.app, rep)
    finally:
        failures = rep.finish()
        mc.close()
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
