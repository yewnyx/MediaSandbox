#Requires -Version 7
# Build Wasmtime with the `pulley` feature for Windows x64 and stage
# wasmtime.dll to unity_package/Plugins/x86_64/wasmtime.dll.
#
# Usage:
#   pwsh scripts/build-wasmtime-windows.ps1 [[-Ref] <ref>]
#   Ref: tag, branch, or full commit SHA (default: v44.0.0)
param(
    [string]$Ref = 'v44.0.0'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot    = Split-Path $PSScriptRoot -Parent
$WasmtimeRepo = 'https://github.com/bytecodealliance/wasmtime.git'
$Dest        = Join-Path $RepoRoot 'unity_package\Plugins\x86_64'
Set-Location $RepoRoot

# ── Clone / update wasmtime-src ───────────────────────────────────────────────
$GitDir = Join-Path $RepoRoot 'wasmtime-src\.git'
if (-not (Test-Path $GitDir)) {
    Write-Host '==> Cloning Wasmtime...'
    git clone --filter=blob:none --no-checkout $WasmtimeRepo wasmtime-src
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

$Current   = (git -C wasmtime-src rev-parse HEAD 2>$null).Trim()
$TargetSHA = (git -C wasmtime-src rev-parse $Ref   2>$null).Trim()

if (-not $TargetSHA -or $Current -ne $TargetSHA) {
    Write-Host "==> Fetching $Ref..."
    git -C wasmtime-src fetch origin --tags
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    git -C wasmtime-src checkout $Ref
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    git -C wasmtime-src submodule update --init --recursive --filter=blob:none
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} else {
    Write-Host "==> wasmtime-src already at $Ref"
}

# ── Build ─────────────────────────────────────────────────────────────────────
Write-Host '==> Building wasmtime-c-api for Windows x64...'
cargo build `
    --manifest-path wasmtime-src/Cargo.toml `
    -p wasmtime-c-api `
    --release `
    --features wasmtime/pulley
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

New-Item -ItemType Directory -Force $Dest | Out-Null
Copy-Item 'wasmtime-src\target\release\wasmtime.dll' "$Dest\wasmtime.dll" -Force
Write-Host "==> Staged → $Dest\wasmtime.dll"
