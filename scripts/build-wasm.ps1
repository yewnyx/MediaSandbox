#Requires -Version 7
# Build decoder.wasm and stage it to unity_package/media~/decoder.wasm.
#
# Usage:
#   pwsh scripts/build-wasm.ps1           # release-with-debuginfo (default)
#   pwsh scripts/build-wasm.ps1 -Debug    # debug build
#   pwsh scripts/build-wasm.ps1 -Release  # plain release (smallest wasm)
param(
    [switch]$Debug,
    [switch]$Release
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path $PSScriptRoot -Parent
Set-Location $RepoRoot

$Target = 'wasm32-wasip1'
if ($Debug) {
    $Profile   = 'debug'
    $CargoArgs = @()
} elseif ($Release) {
    $Profile   = 'release'
    $CargoArgs = @('--release')
} else {
    $Profile   = 'release-with-debuginfo'
    $CargoArgs = @('--profile', 'release-with-debuginfo')
}

Write-Host '==> Adding WASM target...'
rustup target add $Target

# Build gen_cs for the host and regenerate SandboxLayout.g.cs.
# This must run before the WASM build so C# gets fresh layout constants on every rebuild.
Write-Host '==> Building gen_cs (host)...'
cargo build --manifest-path decoder/Cargo.toml --bin gen_cs 2>&1 | Write-Host
if ($LASTEXITCODE -ne 0) { Write-Error 'gen_cs build failed'; exit $LASTEXITCODE }

$GenExe  = "decoder\target\debug\gen_cs.exe"
$GenDest = 'unity_package\Runtime\Generated'
New-Item -ItemType Directory -Force $GenDest | Out-Null
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
& $GenExe | Set-Content "$GenDest\SandboxLayout.g.cs" -Encoding UTF8
if ($LASTEXITCODE -ne 0) { Write-Error 'gen_cs execution failed'; exit $LASTEXITCODE }
Write-Host "==> Generated → $GenDest\SandboxLayout.g.cs"

Write-Host "==> Building decoder WASM ($Profile)..."
cargo build --manifest-path decoder/Cargo.toml --target $Target --lib @CargoArgs
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$Src  = "decoder\target\$Target\$Profile\decoder.wasm"
$Dest = 'Assets\StreamingAssets\mediasandbox'
New-Item -ItemType Directory -Force $Dest | Out-Null
Copy-Item $Src "$Dest\decoder.wasm" -Force
$SizeKB = [math]::Round((Get-Item $Src).Length / 1KB)
Write-Host "==> Staged → $Dest\decoder.wasm ($SizeKB KB)"
