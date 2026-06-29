#!/usr/bin/env bash
# Build Wasmtime with the `pulley` feature for Android (arm64-v8a, armeabi-v7a,
# x86_64) and stage .so files to unity_package/Plugins/Android/libs/<abi>/.
#
# Pulley (the interpreter) is used instead of Cranelift JIT for broad device
# compatibility — some Android vendors enforce W^X, which prevents JIT memory
# allocation, mirroring the iOS restriction.
#
# Requires: Rust, Android NDK (ANDROID_NDK_HOME set), cargo-ndk.
# Runs on macOS or Linux.
#
# Usage:
#   bash scripts/build-wasmtime-android.sh [ref]
#   ref: tag, branch, or full commit SHA (default: v44.0.0)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

REF="${1:-v44.0.0}"
WASMTIME_REPO="https://github.com/bytecodealliance/wasmtime.git"
MIN_API=21
DEST="unity_package/Plugins/Android/libs"

# Pairs of (ABI name, Rust target triple)
declare -a ABIS=("arm64-v8a"             "armeabi-v7a"             "x86_64")
declare -a TARGETS=("aarch64-linux-android" "armv7-linux-androideabi" "x86_64-linux-android")

# ── Validate environment ──────────────────────────────────────────────────────
if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
    echo "ERROR: ANDROID_NDK_HOME is not set."
    echo "       export ANDROID_NDK_HOME=\$ANDROID_SDK_ROOT/ndk/<version>"
    exit 1
fi

if ! command -v cargo-ndk &>/dev/null; then
    echo "==> Installing cargo-ndk..."
    cargo install cargo-ndk
fi

# ── Add Rust targets ──────────────────────────────────────────────────────────
echo "==> Adding Android Rust targets..."
rustup target add "${TARGETS[@]}"

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
for i in "${!ABIS[@]}"; do
    ABI="${ABIS[$i]}"
    TARGET="${TARGETS[$i]}"

    echo "==> Building wasmtime-c-api for $ABI ($TARGET)..."
    (cd wasmtime-src && cargo ndk \
        -t "$ABI" \
        --platform "$MIN_API" \
        -- build \
        -p wasmtime-c-api \
        --release \
        --features pulley)

    OUT_DIR="$DEST/$ABI"
    mkdir -p "$OUT_DIR"
    cp "wasmtime-src/target/$TARGET/release/libwasmtime.so" "$OUT_DIR/libwasmtime.so"
    echo "    Staged → $OUT_DIR/libwasmtime.so"
done

echo "==> Done."
