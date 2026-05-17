#!/usr/bin/env bash
set -euo pipefail

port="${FAKE_API_PORT:-18080}"

echo "[fake-api] connecting to fake-db..."
sleep 2
echo "[fake-api] connected to fake-db"
echo "[fake-api] ready on http://127.0.0.1:${port}/health"

python3 -u - "$port" <<'PY'
from http.server import BaseHTTPRequestHandler, HTTPServer
import sys

port = int(sys.argv[1])

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/health":
            self.send_response(204)
            self.end_headers()
            return
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"fake-api: hello\n")

    def log_message(self, fmt, *args):
        print(f"[fake-api] {self.address_string()} {fmt % args}", flush=True)

server = HTTPServer(("127.0.0.1", port), Handler)
print(f"[fake-api] serving HTTP on {port}", flush=True)
server.serve_forever()
PY
