#Requires -Version 7
# Fetch the latest successful CI build artifacts and merge them into unity_package/.
#
# Requires: gh CLI authenticated to the repo  (gh auth login)
#
# Usage:
#   pwsh scripts/fetch-artifacts.ps1                    # all artifacts
#   pwsh scripts/fetch-artifacts.ps1 wasm ios           # specific artifacts
#   pwsh scripts/fetch-artifacts.ps1 -Run 12345678      # specific run ID
param(
    [Parameter(Position = 0, ValueFromRemainingArguments)]
    [string[]] $Artifacts = @('wasm', 'windows', 'macos', 'linux', 'ios'),

    [string] $Run = ''
)

$ErrorActionPreference = 'Stop'

$artifactNames = @{
    wasm    = 'decoder-wasm'
    windows = 'wasmtime-windows-x64'
    macos   = 'wasmtime-macos-universal'
    linux   = 'wasmtime-linux-x64'
    ios     = 'wasmtime-ios-pulley'
}

$RepoRoot = Split-Path $PSScriptRoot -Parent
$TmpDir   = Join-Path ([System.IO.Path]::GetTempPath()) "mediasandbox-artifacts-$(Get-Random)"

try {
    New-Item -ItemType Directory -Force $TmpDir | Out-Null

    if (-not $Run) {
        Write-Host '==> Finding latest successful run on main...'
        $Run = (gh run list `
            --workflow build.yml `
            --branch main `
            --status success `
            --limit 1 `
            --json databaseId `
            --jq '.[0].databaseId').Trim()
        if (-not $Run) {
            Write-Error 'No successful runs found on main. Check: gh run list --workflow build.yml --branch main'
        }
        Write-Host "    Run ID: $Run"
    }

    foreach ($art in $Artifacts) {
        $name = $artifactNames[$art]
        if (-not $name) {
            Write-Warning "Unknown artifact '$art'. Valid: $($artifactNames.Keys -join ', ')"
            continue
        }

        Write-Host "==> Downloading $name..."
        $artTmp = Join-Path $TmpDir $art
        New-Item -ItemType Directory -Force $artTmp | Out-Null
        gh run download $Run --name $name --dir $artTmp
        if ($LASTEXITCODE -ne 0) {
            Write-Warning "gh run download failed for $name — skipping"
            continue
        }

        # decoder-wasm lands in Assets/StreamingAssets/; everything else in unity_package/.
        if ($art -eq 'wasm') {
            $dstSA = Join-Path $RepoRoot 'Assets\StreamingAssets\mediasandbox'
            New-Item -ItemType Directory -Force $dstSA | Out-Null
            Copy-Item (Join-Path $artTmp 'decoder.wasm') (Join-Path $dstSA 'decoder.wasm') -Force
            Write-Host '    Copied → Assets/StreamingAssets/mediasandbox/decoder.wasm'
        } else {
            $dstPkg = Join-Path $RepoRoot 'unity_package'
            Get-ChildItem $artTmp | ForEach-Object {
                Copy-Item $_.FullName (Join-Path $dstPkg $_.Name) -Recurse -Force
            }
            Write-Host '    Merged into unity_package/'
        }
    }
} finally {
    Remove-Item $TmpDir -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host ''
Write-Host 'Done.'
