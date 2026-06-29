#!/usr/bin/env bash
# Build Wasmtime with the `pulley` feature for Linux x86_64 and stage
# libwasmtime.so to unity_package/Plugins/x86_64/libwasmtime.so.
#
# Usage:
#   bash scripts/build-wasmtime-linux.sh [ref]
#   ref: tag, branch, or full commit SHA (default: v44.0.0)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

REF="${1:-v44.0.0}"
WASMTIME_REPO="https://github.com/bytecodealliance/wasmtime.git"
DEST="unity_package/Plugins/x86_64"

# ── Clone / update wasmtime-src ───────────────────────────────────────────────
if [[ ! -d wasmtime-src/.git ]]; then
    echo "==> Cloning Wasmtime..."
    git clone --filter=blob:none --no-checkout "$WASMTIME_REPO" wasmtime-src
fi

CURRENT="$(git -C wasmtime-src rev-parse HEAD 2>/dev/null || echo '')"
TARGET_SHA="$(git -C wasmtime-src rev-parse "$REF" 2>/dev/null || echo '')"

if [[ -z "$TARGET_SHA" || "$CURRENT" != "$TARGET_SHA" ]]; then
    echo "==> Fetching $REF..."
    git -C wasmtime-src fetch origin --tags
    git -C wasmtime-src checkout "$REF"
    git -C wasmtime-src submodule update --init --recursive --filter=blob:none
else
    echo "==> wasmtime-src already at $REF"
fi

# ── Build ─────────────────────────────────────────────────────────────────────
echo "==> Building wasmtime-c-api for Linux x86_64..."
cargo build \
    --manifest-path wasmtime-src/Cargo.toml \
    -p wasmtime-c-api \
    --release \
    --features wasmtime/pulley

mkdir -p "$DEST"
cp wasmtime-src/target/release/libwasmtime.so "$DEST/libwasmtime.so"
echo "==> Staged → $DEST/libwasmtime.so"
