#!/bin/bash
# Integration test: a VM from POST /api/v1/images/run gets the CPUs and
# memory it asked for, even when a VM of another size from the same image
# ran first. The fast path clones a snapshotted template, and a clone keeps
# the template's size, so a template shared across sizes hands every VM
# the size of whichever request built it.
#
# Run from the repo root on a linux meda host (needs cloud-hypervisor):
#   ./tests/integration/run_size.sh
#
# Requires: meda on $PATH (or $MEDA), sudo NOPASSWD for ip/iptables,
# curl, jq, ssh, and meda's SSH key under ~/.meda/ssh.

set -uo pipefail

MEDA="${MEDA:-meda}"
IMAGE="${IMAGE:-ubuntu:latest}"
PORT="${PORT:-17778}"
HOST="127.0.0.1"
KEY="${KEY:-$HOME/.meda/ssh/id_ed25519}"

cleanup() {
  for vm in run-size-small run-size-large; do
    curl -s -X DELETE "http://$HOST:$PORT/api/v1/vms/$vm" >/dev/null 2>&1 || true
  done
  if [[ -n "${API_PID:-}" ]]; then
    kill "$API_PID" 2>/dev/null || true
    wait "$API_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

start_api() {
  "$MEDA" serve --host "$HOST" --port "$PORT" >/tmp/run-size-api.log 2>&1 &
  API_PID=$!
  for _ in $(seq 1 50); do
    curl -fsS "http://$HOST:$PORT/api/v1/health" >/dev/null 2>&1 && return 0
    sleep 0.1
  done
  echo "FAIL: API did not come up on $HOST:$PORT"
  cat /tmp/run-size-api.log
  exit 1
}

# Runs a VM of the given size and prints "<nproc> <MemTotal kB>" from inside it.
guest_size() {
  local name=$1 cpus=$2 memory=$3
  curl -fsS -X POST "http://$HOST:$PORT/api/v1/images/run" \
    -H 'Content-Type: application/json' \
    -d "{\"image\":\"$IMAGE\",\"name\":\"$name\",\"cpus\":$cpus,\"memory\":\"$memory\"}" \
    >/dev/null || { echo "FAIL: run $name errored" >&2; exit 1; }
  local ip
  ip=$(curl -fsS "http://$HOST:$PORT/api/v1/vms/$name" | jq -r '.ip // empty')
  for _ in $(seq 1 60); do
    if out=$(ssh -i "$KEY" -o BatchMode=yes -o ConnectTimeout=2 \
        -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
        "cirun@$ip" "nproc; awk '/MemTotal/{print \$2}' /proc/meminfo" 2>/dev/null); then
      echo $out
      return 0
    fi
    sleep 1
  done
  echo "FAIL: no SSH to $name at $ip" >&2
  exit 1
}

check() {
  local name=$1 cpus=$2 memory=$3 min_kb=$4
  read -r got_cpus got_kb < <(guest_size "$name" "$cpus" "$memory")
  echo "$name asked cpus=$cpus memory=$memory, guest has cpus=$got_cpus MemTotal=${got_kb}kB"
  if [[ "$got_cpus" != "$cpus" ]] || (( got_kb < min_kb )); then
    echo "FAIL: $name did not get the size it asked for"
    exit 1
  fi
}

start_api
# MemTotal sits a little below the configured size (kernel reservations).
check run-size-small 1 1G 700000
check run-size-large 4 8G 7000000
echo "PASS: each VM got its requested size"
