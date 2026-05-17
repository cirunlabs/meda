#!/bin/bash
# Integration test: HTTP API's POST /api/v1/images/run must go through
# the snapshot/restore fast path (run_instant_core), not cold-boot
# cloud-init (run_from_image). The CLI's `meda run` already defaults
# to fast path since PR 6 (~120ms return, ~1.3s sshd, ~3.7s for 5
# concurrent). The HTTP API regressed: it still called run_from_image,
# so every API-launched VM paid the full cold-boot cost.
#
# This test fails on the regressed handler and passes once the API is
# wired to run_instant_capture.
#
# Run from the repo root on a linux meda host (cannot run on macOS,
# needs cloud-hypervisor):
#   ./tests/integration/instant_api.sh
#
# Requires:
#   - meda binary on $PATH (or override with $MEDA)
#   - sudo NOPASSWD for ip/iptables
#   - the ubuntu:latest template must already be cached AND
#     snapshotted once (i.e. one prior CLI `meda run ubuntu:latest`
#     succeeded so __tpl_*ubuntu* exists). Without that, the very
#     first API call still pays the ~15s template-build tax.
#   - curl, jq

set -uo pipefail

MEDA="${MEDA:-meda}"
IMAGE="${IMAGE:-ubuntu:latest}"
PORT="${PORT:-17777}"
HOST="127.0.0.1"

cleanup() {
  for vm in $(curl -s "http://$HOST:$PORT/api/v1/vms" 2>/dev/null \
              | jq -r '.vms[]?.name' 2>/dev/null | grep '^api-instant-'); do
    curl -s -X DELETE "http://$HOST:$PORT/api/v1/vms/$vm" >/dev/null 2>&1 || true
  done
  if [[ -n "${API_PID:-}" ]]; then
    kill "$API_PID" 2>/dev/null || true
    wait "$API_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

start_api() {
  "$MEDA" serve --host "$HOST" --port "$PORT" >/tmp/instant-api.log 2>&1 &
  API_PID=$!
  for _ in $(seq 1 50); do
    if curl -fsS "http://$HOST:$PORT/api/v1/health" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.1
  done
  echo "FAIL: API did not come up on $HOST:$PORT"
  cat /tmp/instant-api.log
  exit 1
}

# Test 1: single API call returns fast.
# Cold path: ~15-60s. Fast path: <2s (already-built template).
test_api_single_fast() {
  echo "=== test 1: single API call returns <2s (fast path) ==="
  local name="api-instant-1"
  local t0 t1
  t0=$(date +%s%3N)
  local body
  body=$(curl -fsS -X POST "http://$HOST:$PORT/api/v1/images/run" \
    -H 'Content-Type: application/json' \
    -d "{\"image\":\"$IMAGE\",\"name\":\"$name\"}" 2>/tmp/instant-api-err)
  local rc=$?
  t1=$(date +%s%3N)
  local elapsed=$((t1 - t0))
  echo "elapsed_ms=$elapsed"
  echo "body=$body"
  if (( rc != 0 )); then
    echo "FAIL: API call errored"
    cat /tmp/instant-api-err
    exit 1
  fi
  if (( elapsed > 2000 )); then
    echo "FAIL: API call took ${elapsed}ms — fast path not wired (cold-boot regression)"
    exit 1
  fi
  echo "PASS: API returned in ${elapsed}ms"
}

# Test 2: API-launched VM is sshd-reachable within 3s of POST.
# Fast path target per PR 6: ~1.3s. Cold path: 15-90s.
test_api_sshd_reachable_fast() {
  echo "=== test 2: API-launched VM sshd-reachable <3s ==="
  local name="api-instant-2"
  local t0 t1
  t0=$(date +%s%3N)
  curl -fsS -X POST "http://$HOST:$PORT/api/v1/images/run" \
    -H 'Content-Type: application/json' \
    -d "{\"image\":\"$IMAGE\",\"name\":\"$name\"}" >/dev/null

  # Discover the IP via the API (mirrors what cirun-agent does).
  local ip=""
  for _ in $(seq 1 30); do
    ip=$(curl -fsS "http://$HOST:$PORT/api/v1/vms/$name" 2>/dev/null | jq -r '.ip // empty')
    [[ -n "$ip" && "$ip" != "null" ]] && break
    sleep 0.1
  done
  if [[ -z "$ip" || "$ip" == "null" ]]; then
    echo "FAIL: no IP from API after VM run"
    exit 1
  fi

  local deadline=$((t0 + 3000))
  while (( $(date +%s%3N) < deadline )); do
    if timeout 1 bash -c "exec 3<>/dev/tcp/$ip/22 && head -c 4 <&3" >/dev/null 2>&1; then
      t1=$(date +%s%3N)
      echo "PASS: sshd reachable in $((t1 - t0))ms at $ip:22"
      return 0
    fi
    sleep 0.05
  done
  echo "FAIL: sshd at $ip:22 not reachable within 3s (cold-boot still wired)"
  exit 1
}

start_api
test_api_single_fast
test_api_sshd_reachable_fast
