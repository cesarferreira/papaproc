#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp="$(mktemp -d)"
pids=""

cleanup() {
  for pid in $pids; do
    kill "$pid" >/dev/null 2>&1 || true
  done
  rm -rf "$tmp"
}
trap cleanup EXIT

wait_for_tcp() {
  local host="$1"
  local port="$2"
  local label="$3"
  local deadline=$((SECONDS + 8))

  until bash -c "echo >/dev/tcp/${host}/${port}" >/dev/null 2>&1; do
    if [ "$SECONDS" -ge "$deadline" ]; then
      echo "[smoke] timed out waiting for ${label} on ${host}:${port}" >&2
      return 1
    fi
    sleep 0.25
  done
}

wait_for_http() {
  local url="$1"
  local label="$2"
  local deadline=$((SECONDS + 8))

  until curl -fsS "$url" >/dev/null 2>&1; do
    if [ "$SECONDS" -ge "$deadline" ]; then
      echo "[smoke] timed out waiting for ${label} at ${url}" >&2
      return 1
    fi
    sleep 0.25
  done
}

wait_for_log() {
  local file="$1"
  local needle="$2"
  local label="$3"
  local deadline=$((SECONDS + 8))

  until grep -F "$needle" "$file" >/dev/null 2>&1; do
    if [ "$SECONDS" -ge "$deadline" ]; then
      echo "[smoke] timed out waiting for ${label} log: ${needle}" >&2
      echo "[smoke] current log:" >&2
      cat "$file" >&2 || true
      return 1
    fi
    sleep 0.25
  done
}

echo "[smoke] starting fake-db (contains intentional 2s wait)"
bash "$root/scripts/fake-db.sh" >"$tmp/db.log" 2>&1 &
pids="$pids $!"
wait_for_tcp 127.0.0.1 15432 fake-db

echo "[smoke] starting fake-api after fake-db readiness (contains intentional 2s wait)"
bash "$root/scripts/fake-api.sh" >"$tmp/api.log" 2>&1 &
pids="$pids $!"
wait_for_http http://127.0.0.1:18080/health fake-api

echo "[smoke] starting fake-web after fake-api readiness (contains intentional 2s wait)"
bash "$root/scripts/fake-web.sh" >"$tmp/web.log" 2>&1 &
pids="$pids $!"
wait_for_log "$tmp/web.log" "[fake-web] ready" fake-web

echo "[smoke] starting noisy log generator"
bash "$root/scripts/noisy-logs.sh" >"$tmp/noisy.log" 2>&1 &
pids="$pids $!"
wait_for_log "$tmp/noisy.log" "ERROR fake transient error" noisy-logs

echo "[smoke] fake-stack example passed"
