#!/bin/bash
# Integration test: meda run + meda delete must leave host state
# untouched. iptables rules, netns, veth pairs, and vm-dirs that
# meda creates during a run must all be removed by the matching
# delete. Concurrent launches must not leak duplicate global rules.
#
# Why a shell test: iptables/netns/veth are host-level kernel state.
# Unit-mocking wouldn't catch the actual leak (we already lived
# through that). The user-visible behaviour we care about is "after
# meda delete, the host is as it was before meda run", and that's
# exactly what this test asserts.
#
# Run from the repo root:
#   ./tests/integration/no_leaks.sh
#
# Requires:
#   - meda binary on $PATH (or override with $MEDA)
#   - sudo NOPASSWD for ip/iptables (already needed by meda itself)
#   - the ubuntu:latest template must already be cached, otherwise
#     the first run pays the ~30s cold-boot tax that's not part of
#     what we're testing.

set -uo pipefail

MEDA="${MEDA:-meda}"
IMAGE="${IMAGE:-ubuntu:latest}"

snapshot_state() {
  printf 'masq_10_99=%s\n' \
    "$(sudo iptables -w -t nat -S POSTROUTING 2>/dev/null | grep -c '10.99.0.0/16')"
  printf 'fwd_vmh=%s\n' \
    "$(sudo iptables -w -S FORWARD 2>/dev/null | grep -c 'vmh-')"
  printf 'veths=%s\n' \
    "$(ip link show 2>/dev/null | grep -cE 'vmh-')"
  printf 'netns=%s\n' \
    "$(sudo ip netns list 2>/dev/null | grep -c '^meda-')"
  printf 'vms=%s\n' \
    "$(ls /home/ubuntu/.meda/vms/ 2>/dev/null | grep -cv '^__tpl')"
}

# Force a clean baseline: tear down anything from a prior failed
# test, dedupe the host-wide MASQUERADE rule. This is the state a
# fresh box would be in.
reset_baseline() {
  for v in $(ls /home/ubuntu/.meda/vms/ 2>/dev/null | grep -v '^__tpl'); do
    "$MEDA" delete "$v" >/dev/null 2>&1 || true
  done
  # Strip every duplicate of our wildcard NAT rule (we re-add a
  # single one later if needed; for now we want to verify meda
  # doesn't re-add it on every run).
  while sudo iptables -w -t nat -D POSTROUTING -s 10.99.0.0/16 ! -d 10.99.0.0/16 -j MASQUERADE 2>/dev/null; do :; done
}

# After-state must equal before-state, exactly.
assert_state_eq() {
  local label=$1 expected=$2 actual
  actual=$(snapshot_state)
  if [[ "$expected" != "$actual" ]]; then
    echo "FAIL: $label leaked / changed host state"
    diff <(echo "$expected") <(echo "$actual") || true
    exit 1
  fi
  echo "PASS: $label — host state matches expected"
}

# Test 1: two concurrent meda runs must not duplicate the host-wide
# MASQUERADE rule. With a clean baseline (0 rules), the rule should
# end up at exactly 1 — not 2 — even though both creates raced.
test_concurrent_no_dup_global_rule() {
  echo "=== test 1: 2 concurrent runs share the host-wide rule ==="
  reset_baseline
  local before; before=$(snapshot_state)
  echo "$before"

  "$MEDA" run "$IMAGE" --name tdd-leak-a --json >/tmp/a.json 2>/dev/null &
  "$MEDA" run "$IMAGE" --name tdd-leak-b --json >/tmp/b.json 2>/dev/null &
  wait

  local after_run; after_run=$(snapshot_state)
  local rules; rules=$(echo "$after_run" | grep masq_10_99 | cut -d= -f2)
  echo "after 2 concurrent runs: masq_10_99=$rules (want 1)"
  if [[ "$rules" != "1" ]]; then
    echo "FAIL: $rules duplicate host-wide MASQUERADE rules from 2 concurrent runs"
    "$MEDA" delete tdd-leak-a >/dev/null 2>&1 || true
    "$MEDA" delete tdd-leak-b >/dev/null 2>&1 || true
    exit 1
  fi

  "$MEDA" delete tdd-leak-a >/dev/null 2>&1
  "$MEDA" delete tdd-leak-b >/dev/null 2>&1
  # After delete, the global rule stays (other VMs may need it),
  # but per-VM state should be gone.
  local after_delete; after_delete=$(snapshot_state)
  for k in fwd_vmh veths netns vms; do
    bef=$(echo "$before" | grep "^$k=" | cut -d= -f2)
    aft=$(echo "$after_delete" | grep "^$k=" | cut -d= -f2)
    if [[ "$bef" != "$aft" ]]; then
      echo "FAIL: $k leaked: was $bef, now $aft"
      exit 1
    fi
  done
  echo "PASS: 2×run+delete left no per-VM resources behind"
}

# Test 2: 5 concurrent runs all expose a working sshd, host stays
# clean afterwards. This is the headline meda use case (CI/AI
# runners spawned in parallel).
test_5_concurrent_ssh_works() {
  echo "=== test 2: 5 concurrent runs are all sshd-reachable ==="
  reset_baseline
  local before; before=$(snapshot_state)
  echo "$before"

  : >/tmp/tdd-perf.log
  local pids=() t_anchor; t_anchor=$(date +%s%3N)
  for i in 1 2 3 4 5; do
    (
      local t0; t0=$(date +%s%3N)
      "$MEDA" run "$IMAGE" --name tdd-c$i --json >/tmp/tdd-c$i.json 2>/tmp/tdd-c$i.err
      local t1; t1=$(date +%s%3N)
      local ip; ip=$(python3 -c "import json;print(json.load(open('/tmp/tdd-c$i.json'))['host'])" 2>/dev/null)
      local result=FAIL_NO_IP
      if [[ -n "$ip" ]]; then
        result=TIMEOUT
        local deadline=$((SECONDS + 30))
        while (( SECONDS < deadline )); do
          if timeout 1 bash -c "exec 3<>/dev/tcp/$ip/22 && head -c 4 <&3" >/dev/null 2>&1; then
            result=OK; break
          fi
          sleep 0.02
        done
      fi
      local t2; t2=$(date +%s%3N)
      printf 'tdd-c%s ip=%s status=%s run_ms=%s sshd_ready_ms=%s\n' \
        "$i" "$ip" "$result" "$((t1 - t0))" "$((t2 - t0))" >>/tmp/tdd-perf.log
    ) &
    pids+=($!)
    sleep 0.15
  done
  for p in "${pids[@]}"; do wait "$p"; done
  local t_end; t_end=$(date +%s%3N)
  echo "--- per-VM timings ---"
  sort -V /tmp/tdd-perf.log
  echo "wall_ms_for_5 = $((t_end - t_anchor))"

  local ok; ok=$(grep -c 'status=OK' /tmp/tdd-perf.log)

  for i in 1 2 3 4 5; do
    "$MEDA" delete tdd-c$i >/dev/null 2>&1
  done

  if [[ "$ok" != 5 ]]; then
    echo "FAIL: only $ok of 5 VMs were sshd-reachable"
    exit 1
  fi

  local after; after=$(snapshot_state)
  local rules; rules=$(echo "$after" | grep masq_10_99 | cut -d= -f2)
  if [[ "$rules" != 1 && "$rules" != 0 ]]; then
    echo "FAIL: $rules duplicate host-wide rules after 5 runs"
    exit 1
  fi
  for k in fwd_vmh veths netns vms; do
    aft=$(echo "$after" | grep "^$k=" | cut -d= -f2)
    if [[ "$aft" != 0 ]]; then
      echo "FAIL: $k=$aft after 5 deletes (expected 0)"
      exit 1
    fi
  done
  echo "PASS: 5×concurrent run + delete — all sshd-reachable, no leaks"
}

test_concurrent_no_dup_global_rule
test_5_concurrent_ssh_works