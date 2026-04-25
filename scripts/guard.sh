#!/usr/bin/env bash
# guard.sh — must pass every autoresearch iteration. Same contract as on
# the boot-ms branch: fmt, clippy, and cargo test --bins (meda is bin-only,
# unit tests live in `#[cfg(test)]` under src/). Integration tests that
# boot real VMs are left to the bench.

set -euo pipefail

log() { printf '[guard] %s\n' "$*" >&2; }

cd "$(dirname "$0")/.."

log "cargo fmt --check"
cargo fmt --all -- --check

log "cargo clippy -- -D warnings"
cargo clippy --all-targets --all-features -- -D warnings

log "cargo test --bins"
cargo test --bins --all-features

log "guard OK"
