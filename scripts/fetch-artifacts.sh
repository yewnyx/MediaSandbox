#!/usr/bin/env bash
# Fetch the latest successful CI build artifacts and place them in the repo.
#
# Requires: gh CLI authenticated to the repo  (gh auth login)
#
# Usage:
#   bash scripts/fetch-artifacts.sh                    # all artifacts
#   bash scripts/fetch-artifacts.sh wasm ios           # specific artifacts
#   bash scripts/fetch-artifacts.sh -r 12345678        # specific run ID
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

artifact_name() {
    case "$1" in
        wasm)    echo "decoder-wasm" ;;
        windows) echo "wasmtime-windows-x64" ;;
        macos)   echo "wasmtime-macos-universal" ;;
        linux)   echo "wasmtime-linux-x64" ;;
        ios)     echo "wasmtime-ios-pulley" ;;
        android) echo "wasmtime-android" ;;
        *)       echo "" ;;
    esac
}

install_artifact() {
    local ART="$1" SRC="$2"
    case "$ART" in
        wasm)
            DST="$REPO_ROOT/Assets/StreamingAssets/mediasandbox"
            mkdir -p "$DST"
            cp "$SRC/decoder.wasm" "$DST/decoder.wasm"
            echo "    → Assets/StreamingAssets/mediasandbox/decoder.wasm"
            ;;
        windows)
            DST="$REPO_ROOT/unity_package/Plugins/x86_64"
            mkdir -p "$DST"
            cp "$SRC/wasmtime.dll" "$DST/wasmtime.dll"
            echo "    → unity_package/Plugins/x86_64/wasmtime.dll"
            ;;
        macos)
            DST="$REPO_ROOT/unity_package/Plugins/x86_64"
            mkdir -p "$DST"
            cp "$SRC/libwasmtime.dylib" "$DST/libwasmtime.dylib"
            echo "    → unity_package/Plugins/x86_64/libwasmtime.dylib"
            ;;
        linux)
            DST="$REPO_ROOT/unity_package/Plugins/x86_64"
            mkdir -p "$DST"
            cp "$SRC/libwasmtime.so" "$DST/libwasmtime.so"
            echo "    → unity_package/Plugins/x86_64/libwasmtime.so"
            ;;
        ios)
            DST="$REPO_ROOT/unity_package/Plugins/iOS/Wasmtime.xcframework"
            mkdir -p "$DST"
            cp -r "$SRC/." "$DST/"
            echo "    → unity_package/Plugins/iOS/Wasmtime.xcframework/"
            ;;
        android)
            DST="$REPO_ROOT/unity_package/Plugins/Android/libs"
            mkdir -p "$DST"
            cp -r "$SRC/." "$DST/"
            echo "    → unity_package/Plugins/Android/libs/"
            ;;
    esac
}

ALL_ARTIFACTS=(wasm windows macos linux ios android)
ARTIFACTS=()
RUN_ID=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        -r|--run) RUN_ID="$2"; shift 2 ;;
        -*) echo "Unknown option: $1" >&2; exit 1 ;;
        *)  ARTIFACTS+=("$1"); shift ;;
    esac
done

if [[ ${#ARTIFACTS[@]} -eq 0 ]]; then
    ARTIFACTS=("${ALL_ARTIFACTS[@]}")
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

if [[ -z "$RUN_ID" ]]; then
    echo "==> Finding latest successful run on main..."
    RUN_ID="$(gh run list \
        --workflow build.yml \
        --branch main \
        --status success \
        --limit 1 \
        --json databaseId \
        --jq '.[0].databaseId')"
    if [[ -z "$RUN_ID" ]]; then
        echo "ERROR: No successful runs found on main." \
             "Check: gh run list --workflow build.yml --branch main" >&2
        exit 1
    fi
    echo "    Run ID: $RUN_ID"
fi

for ART in "${ARTIFACTS[@]}"; do
    NAME="$(artifact_name "$ART")"
    if [[ -z "$NAME" ]]; then
        echo "WARNING: Unknown artifact '$ART'. Valid: ${ALL_ARTIFACTS[*]}" >&2
        continue
    fi

    echo "==> Downloading $NAME..."
    ART_TMP="$TMP_DIR/$ART"
    mkdir -p "$ART_TMP"
    gh run download "$RUN_ID" --name "$NAME" --dir "$ART_TMP"
    install_artifact "$ART" "$ART_TMP"
done

echo ""
echo "Done."
