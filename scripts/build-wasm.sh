#!/usr/bin/env bash
# Build decoder.wasm and stage it to unity_package/media~/decoder.wasm.
#
# Usage:
#   bash scripts/build-wasm.sh                # release-with-debuginfo (default)
#   bash scripts/build-wasm.sh --debug        # debug build
#   bash scripts/build-wasm.sh --release      # plain release (smallest wasm)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

TARGET="wasm32-wasip1"
PROFILE="release-with-debuginfo"
CARGO_ARGS=(--profile release-with-debuginfo)

while [[ $# -gt 0 ]]; do
    case "$1" in
        --debug)   PROFILE="debug";   CARGO_ARGS=() ;;
        --release) PROFILE="release"; CARGO_ARGS=(--release) ;;
        *) echo "Unknown option: $1" >&2; exit 1 ;;
    esac
    shift
done

echo "==> Adding WASM target..."
rustup target add "$TARGET"

echo "==> Building decoder ($PROFILE)..."
cargo build --manifest-path decoder/Cargo.toml --target "$TARGET" "${CARGO_ARGS[@]}"

SRC="decoder/target/$TARGET/$PROFILE/decoder.wasm"
DEST="Assets/StreamingAssets/mediasandbox"
mkdir -p "$DEST"
cp "$SRC" "$DEST/decoder.wasm"
echo "==> Staged → $DEST/decoder.wasm ($(du -k "$SRC" | cut -f1) KB)"
