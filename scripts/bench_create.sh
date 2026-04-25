#!/usr/bin/env bash
# bench_create.sh — measure restore-to-SSH latency in ms.
#
# Default: snapshot a fresh template VM once (one-off warm cost, not
# counted), then repeatedly `meda stop` + `meda restore` + wait for SSH,
# N times. Median of N reported on stdout as an integer.
#
# Env:
#   BENCH_RUNS           number of restore cycles to time (default 5)
#   BENCH_TEMPLATE_VM    name of the template VM (default bench-tpl-$$)
#   BENCH_IMAGE          image to cold-boot into the template (default ubuntu:latest)
#   BENCH_SSH_TIMEOUT_MS max ms to wait for SSH after resume (default 30000)
#   MEDA_BIN             meda binary path (default: meda on PATH)
#
# Exit 0 on success. Exit 2 if the snapshot feature isn't compiled in
# (used by the loop harness as "feature missing — bench cold proxy").

set -euo pipefail

RUNS="${BENCH_RUNS:-5}"
TEMPLATE_VM="${BENCH_TEMPLATE_VM:-bench-tpl-$$}"
IMAGE="${BENCH_IMAGE:-ubuntu:latest}"
SSH_TIMEOUT_MS="${BENCH_SSH_TIMEOUT_MS:-30000}"
MEDA="${MEDA_BIN:-meda}"
# When set (any non-empty value), use the smoltcp userspace stack
# instead of kernel tap+iptables. Restore passes --smoltcp, and the
# bench's SSH probe hits 127.0.0.1:$BENCH_SMOLTCP_PORT (default 40022)
# which netd forwards into the guest via smoltcp TCP.
SMOLTCP="${BENCH_SMOLTCP:-}"
SMOLTCP_PORT="${BENCH_SMOLTCP_PORT:-40022}"
# Memory of the template VM. CH's --restore cost scales with the
# memory-ranges file size (~= configured RAM). 512M is the smallest
# size that reliably boots Ubuntu 22.04 cloud-init on this host;
# 256M hit OOM during cloud-init. 1024M restored in ~924ms; 512M
# restores in ~481ms (both measured on this host).
TEMPLATE_MEMORY="${BENCH_TEMPLATE_MEMORY:-512M}"

log() { printf '[bench] %s\n' "$*" >&2; }
die() { log "ERROR: $*"; exit 1; }

command -v "$MEDA" >/dev/null || die "$MEDA not found on PATH"
# snapshot/restore presence check — loop treats exit 2 as "not yet implemented".
"$MEDA" snapshot --help >/dev/null 2>&1 || exit 2

now_ms() {
    # EPOCHREALTIME is a bash 5+ builtin (seconds.microseconds, zero fork).
    local er="${EPOCHREALTIME/./}"
    printf '%s\n' "${er:0:-3}"
}

wait_for_ssh() {
    wait_for_ssh_on "$1" 22
}

wait_for_ssh_on() {
    # Single-shot connect + 1-byte banner read.
    local host="$1" port="$2"
    exec 3<>/dev/tcp/"$host"/"$port" || return 1
    if IFS= read -r -n 1 -t 1 -u 3 _; then
        exec 3<&- 3>&- || true
        return 0
    fi
    exec 3<&- 3>&- || true
    return 1
}

median() {
    sort -n | awk '
        { a[NR]=$1 }
        END {
            if (NR == 0) { print -1; exit }
            if (NR % 2) print a[(NR+1)/2]
            else        print int((a[NR/2] + a[NR/2+1]) / 2)
        }
    '
}

ensure_template() {
    if "$MEDA" get "$TEMPLATE_VM" >/dev/null 2>&1; then
        log "reusing existing template VM $TEMPLATE_VM"
        return
    fi
    log "cold-booting template VM $TEMPLATE_VM (mem=$TEMPLATE_MEMORY, one-off cost)"
    "$MEDA" run "$IMAGE" --name "$TEMPLATE_VM" --memory "$TEMPLATE_MEMORY" >/dev/null
    local ip
    ip="$("$MEDA" ip "$TEMPLATE_VM")"
    local deadline=$(( $(now_ms) + 60000 ))
    while [[ $(now_ms) -lt $deadline ]]; do
        if wait_for_ssh "$ip"; then break; fi
        sleep 0.1
    done
    wait_for_ssh "$ip" || die "template VM never reached SSH"
    log "template SSH-ready; snapshotting"
    "$MEDA" snapshot "$TEMPLATE_VM" >/dev/null
    "$MEDA" stop "$TEMPLATE_VM" >/dev/null
}

measure_restore() {
    local probe_host
    if [[ -n "$SMOLTCP" ]]; then
        probe_host="127.0.0.1"
        # In smoltcp mode the "port" is the host-side forward, not the
        # guest's SSH port. wait_for_ssh probes $probe_host:$probe_port.
        local probe_port="$SMOLTCP_PORT"
    else
        probe_host="$("$MEDA" ip "$TEMPLATE_VM")"
        local probe_port=22
    fi
    for _ in $(seq 1 "$RUNS"); do
        # Ensure stopped between runs. `meda stop` is a few hundred ms —
        # timed separately below, not part of the metric. Also kill any
        # netd from the previous restore, since it pins the tap.
        "$MEDA" stop "$TEMPLATE_VM" >/dev/null 2>&1 || true
        if [[ -n "$SMOLTCP" ]]; then
            pkill -f "meda netd" 2>/dev/null || true
        fi
        local t0 t1
        t0=$(now_ms)
        if [[ -n "$SMOLTCP" ]]; then
            "$MEDA" restore "$TEMPLATE_VM" --smoltcp --forward-ssh "$SMOLTCP_PORT" \
                >/dev/null
        else
            "$MEDA" restore "$TEMPLATE_VM" >/dev/null
        fi
        local deadline=$(( $(now_ms) + SSH_TIMEOUT_MS ))
        while [[ $(now_ms) -lt $deadline ]]; do
            if wait_for_ssh_on "$probe_host" "$probe_port"; then break; fi
        done
        wait_for_ssh_on "$probe_host" "$probe_port" || die "ssh not up after restore"
        t1=$(now_ms)
        echo "$(( t1 - t0 ))"
    done
}

main() {
    ensure_template
    measure_restore | median
}

main "$@"
