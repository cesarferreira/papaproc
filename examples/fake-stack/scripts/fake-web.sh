#!/usr/bin/env bash
set -euo pipefail

echo "[fake-web] installing frontend dependencies..."
sleep 1
echo "[fake-web] waiting 2s for bundler warmup..."
sleep 2
echo "[fake-web] ready at http://127.0.0.1:5173"

counter=0
while true; do
  counter=$((counter + 1))
  echo "[fake-web] hot reload heartbeat ${counter}"
  if [ $((counter % 5)) -eq 0 ]; then
    echo "[fake-web] warning: fake transient error for diagnostics demo"
  fi
  sleep 1
done
