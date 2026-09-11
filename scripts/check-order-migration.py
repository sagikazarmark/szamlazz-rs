#!/usr/bin/env python3
"""Read-only Restate 1.7.8 migration inventory; quiesce producers before running.

Exit 0 means the invocation/state inventory is empty, not that vendor effects
are settled. Any state is a blocker, even when its value cannot be decoded.
"""
import argparse
import json
import os
import sys
import urllib.request


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def query(admin, sql):
    headers = {"Content-Type": "application/json", "Accept": "application/json"}
    if token := os.environ.get("RESTATE_ADMIN_TOKEN"):
        headers["Authorization"] = f"Bearer {token}"
    request = urllib.request.Request(
        admin.rstrip("/") + "/query",
        data=json.dumps({"query": sql}).encode(), headers=headers,
    )
    with urllib.request.build_opener(NoRedirect).open(request, timeout=30) as response:
        result = json.load(response)
    rows = result["rows"]
    if not isinstance(rows, list):
        raise ValueError("admin query did not return rows")
    return rows


def inventory(admin):
    # Deliberately all scopes, not the scope field inside a possibly unreadable
    # marker. Both old and destination identities must be clear before a flag day.
    invocations = query(admin, "SELECT id, scope, target_service_name, target_service_key, status FROM sys_invocation WHERE target_service_name IN ('Szamlazz.Order', 'Szamlazz.Agent') AND status <> 'completed'")
    state = query(admin, "SELECT scope, service_name, service_key, key FROM state WHERE service_name = 'Szamlazz.Order'")
    return {"unfinished_invocations": invocations, "order_state": state}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--admin-url", required=True)
    args = parser.parse_args()
    try:
        result = inventory(args.admin_url)
    except Exception as error:
        # Do not print an authenticated URL, token or remote error body.
        print(f"INVENTORY FAILED ({type(error).__name__}); migration remains blocked", file=sys.stderr)
        return 2
    print(json.dumps(result, indent=2))
    if any(result.values()):
        print("BLOCKED: drain invocations and recover state under its original scope", file=sys.stderr)
        return 1
    print("Inventory clear. Independently settle external uncertainty before switching.", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
