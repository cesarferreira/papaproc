#!/usr/bin/env bash
set -euo pipefail

port="${FAKE_DB_PORT:-15432}"

echo "[fake-db] booting database container..."
sleep 2
echo "[fake-db] ready on 127.0.0.1:${port}"

python3 -u - "$port" <<'PY'
import socket
import sys
import time

port = int(sys.argv[1])
sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
sock.bind(("127.0.0.1", port))
sock.listen(16)
print(f"[fake-db] accepting TCP connections on {port}", flush=True)

while True:
    conn, addr = sock.accept()
    print(f"[fake-db] client connected from {addr[0]}:{addr[1]}", flush=True)
    conn.sendall(b"fake-db: ok\n")
    conn.close()
    time.sleep(0.05)
PY
