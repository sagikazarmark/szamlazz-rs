#!/usr/bin/env python3
"""Failure-closed checks for the read-only migration inventory."""
import importlib.util
import unittest
import sys
import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from unittest.mock import patch

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("inventory", Path(__file__).with_name("check-order-migration.py"))
inventory = importlib.util.module_from_spec(spec)
spec.loader.exec_module(inventory)


class InventoryTests(unittest.TestCase):
    def test_http_redirects_never_forward_credentials_or_pass(self):
        received = []
        class Handler(BaseHTTPRequestHandler):
            def do_POST(self):
                self.send_response(302)
                self.send_header("Location", f"http://localhost:{self.server.server_port}/forwarded")
                self.end_headers()
            def do_GET(self):
                received.append(self.headers.get("Authorization"))
                self.send_response(200)
                self.end_headers()
                self.wfile.write(json.dumps({"rows": []}).encode())
            def log_message(self, *args):
                pass
        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever)
        thread.start()
        try:
            with patch.dict("os.environ", {"RESTATE_ADMIN_TOKEN": "synthetic-token"}):
                with self.assertRaises(Exception):
                    inventory.query(f"http://127.0.0.1:{server.server_port}", "SELECT 1")
            self.assertEqual(received, [])
        finally:
            server.shutdown()
            thread.join()
            server.server_close()

    def run_main(self, rows):
        with patch("sys.argv", ["inventory", "--admin-url", "http://localhost:9070"]), patch.object(inventory, "query", side_effect=rows), patch("builtins.print"):
            return inventory.main()

    def test_absence_is_only_a_clean_inventory(self):
        self.assertEqual(self.run_main([[], []]), 0)

    def test_every_state_row_blocks_without_decoding_its_value(self):
        for scope in [None, "acme"]:
            for key in ["unresolved-write", "unknown-state-key"]:
                self.assertEqual(self.run_main([[], [{"scope": scope, "key": key}]]), 1)

    def test_unfinished_invocations_block(self):
        self.assertEqual(self.run_main([[{"status": "paused"}], []]), 1)

    def test_query_failures_never_pass(self):
        self.assertEqual(self.run_main([ValueError("bad response")]), 2)
        self.assertEqual(self.run_main([[], TimeoutError()]), 2)


if __name__ == "__main__":
    unittest.main()
