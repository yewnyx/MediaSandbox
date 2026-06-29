#!/usr/bin/env bash
# Build a macOS universal (arm64 + x86_64) Wasmtime dylib with the `pulley`
# feature and stage it to unity_package/Plugins/x86_64/libwasmtime.dylib.
#
# Must run on macOS. Requires Xcode command-line tools and Rust.
#
# Usage:
#   bash scripts/build-wasmtime-macos.sh [ref]
#   ref: tag, branch, or full commit SHA (default: v44.0.0)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

REF="${1:-v44.0.0}"
WASMTIME_REPO="https://github.com/bytecodealliance/wasmtime.git"
ARM="aarch64-apple-darwin"
X86="x86_64-apple-darwin"
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

echo "==> Adding macOS Rust targets..."
rustup target add "$ARM" "$X86"

# ── Build arm64 ───────────────────────────────────────────────────────────────
echo "==> Building wasmtime-c-api for $ARM..."
cargo build \
    --manifest-path wasmtime-src/Cargo.toml \
    -p wasmtime-c-api \
    --release \
    --target "$ARM" \
    --features pulley

# ── Build x86_64 ──────────────────────────────────────────────────────────────
echo "==> Building wasmtime-c-api for $X86..."
cargo build \
    --manifest-path wasmtime-src/Cargo.toml \
    -p wasmtime-c-api \
    --release \
    --target "$X86" \
    --features pulley

# ── Lipo into universal binary ────────────────────────────────────────────────
echo "==> Creating universal dylib..."
mkdir -p "$DEST"
lipo -create \
    "wasmtime-src/target/$ARM/release/libwasmtime.dylib" \
    "wasmtime-src/target/$X86/release/libwasmtime.dylib" \
    -output "$DEST/libwasmtime.dylib"

codesign --force --sign - "$DEST/libwasmtime.dylib"

echo "==> Staged → $DEST/libwasmtime.dylib"
