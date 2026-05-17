#!/usr/bin/env bash
set -euo pipefail

echo "[noisy-logs] manual log generator started"
sleep 1
echo "[noisy-logs] INFO worker connected"
sleep 1
echo "[noisy-logs] WARN retrying fake request"
sleep 1
echo "[noisy-logs] ERROR fake transient error: connection refused"
sleep 1
echo "[noisy-logs] INFO recovered after retry"

while true; do
  echo "[noisy-logs] INFO heartbeat"
  sleep 2
done
