#!/usr/bin/env bash
# Build Wasmtime with the `pulley` feature for iOS device (arm64) and
# simulator (arm64), then package as Wasmtime.xcframework.
# Output: unity_package/Plugins/iOS/Wasmtime.xcframework/
#
# Must run on macOS. Requires Xcode command-line tools and Rust.
#
# Usage:
#   bash scripts/build-wasmtime-ios.sh [ref]
#   ref: tag, branch, or full commit SHA (default: v44.0.0)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

REF="${1:-v44.0.0}"
WASMTIME_REPO="https://github.com/bytecodealliance/wasmtime.git"
DEVICE="aarch64-apple-ios"
SIM="aarch64-apple-ios-sim"
DEST="unity_package/Plugins/iOS"

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

echo "==> Adding iOS Rust targets..."
rustup target add "$DEVICE" "$SIM"

# ── Build iOS device (arm64) ──────────────────────────────────────────────────
echo "==> Building wasmtime-c-api for $DEVICE..."
SDKROOT="$(xcrun --sdk iphoneos --show-sdk-path)" \
    cargo build \
        --manifest-path wasmtime-src/Cargo.toml \
        -p wasmtime-c-api \
        --release \
        --target "$DEVICE" \
        --features wasmtime/pulley

# ── Build iOS simulator (arm64) ───────────────────────────────────────────────
echo "==> Building wasmtime-c-api for $SIM..."
SDKROOT_SIM="$(xcrun --sdk iphonesimulator --show-sdk-path)"
SDKROOT="$SDKROOT_SIM" \
BINDGEN_EXTRA_CLANG_ARGS="--target=arm64-apple-ios-simulator --sysroot=$SDKROOT_SIM" \
    cargo build \
        --manifest-path wasmtime-src/Cargo.toml \
        -p wasmtime-c-api \
        --release \
        --target "$SIM" \
        --features wasmtime/pulley

# ── Verify Pulley symbols ─────────────────────────────────────────────────────
echo "==> Verifying Pulley symbols in device lib..."
nm "wasmtime-src/target/$DEVICE/release/libwasmtime.a" 2>/dev/null \
    | grep -q pulley \
    && echo "    OK" \
    || { echo "ERROR: Pulley symbols not found — was --features pulley applied?"; exit 1; }

# ── Package xcframework ───────────────────────────────────────────────────────
echo "==> Creating Wasmtime.xcframework..."
mkdir -p "$DEST"
rm -rf "$DEST/Wasmtime.xcframework"
xcodebuild -create-xcframework \
    -library "wasmtime-src/target/$DEVICE/release/libwasmtime.a" \
    -library "wasmtime-src/target/$SIM/release/libwasmtime.a" \
    -output "$DEST/Wasmtime.xcframework"

echo "==> Staged → $DEST/Wasmtime.xcframework"
