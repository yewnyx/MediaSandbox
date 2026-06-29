#!/usr/bin/env bash
# Fetch the latest successful CI build artifacts and merge them into unity_package/.
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
        *)       echo "" ;;
    esac
}

ALL_ARTIFACTS=(wasm windows macos linux ios)
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

    # decoder-wasm lands in Assets/StreamingAssets/; everything else in unity_package/.
    if [[ "$ART" == "wasm" ]]; then
        mkdir -p "$REPO_ROOT/Assets/StreamingAssets/mediasandbox"
        cp "$ART_TMP/decoder.wasm" "$REPO_ROOT/Assets/StreamingAssets/mediasandbox/decoder.wasm"
        echo "    Copied → Assets/StreamingAssets/mediasandbox/decoder.wasm"
    else
        cp -r "$ART_TMP/." "$REPO_ROOT/unity_package/"
        echo "    Merged into unity_package/"
    fi
done

echo ""
echo "Done."
