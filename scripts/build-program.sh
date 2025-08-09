#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# Build SBF with pinocchio’s recommended feature-gating
# Requires Solana toolchain with `cargo-build-sbf` installed.
cd programs/interest_vault
cargo build-sbf --features bpf-entrypoint
echo "Built program to target/deploy/interest_vault.so"


