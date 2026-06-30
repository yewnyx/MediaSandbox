#Requires -Version 7
# Fetch the latest successful CI build artifacts and place them in the repo.
#
# Requires: gh CLI authenticated to the repo  (gh auth login)
#
# Usage:
#   pwsh scripts/fetch-artifacts.ps1                    # all artifacts
#   pwsh scripts/fetch-artifacts.ps1 wasm ios           # specific artifacts
#   pwsh scripts/fetch-artifacts.ps1 -Run 12345678      # specific run ID
param(
    [Parameter(Position = 0, ValueFromRemainingArguments)]
    [string[]] $Artifacts = @('wasm', 'windows', 'macos', 'linux', 'ios', 'android'),

    [string] $Run = ''
)

$ErrorActionPreference = 'Stop'

$artifactNames = @{
    wasm    = 'decoder-wasm'
    windows = 'wasmtime-windows-x64'
    macos   = 'wasmtime-macos-universal'
    linux   = 'wasmtime-linux-x64'
    ios     = 'wasmtime-ios-pulley'
    android = 'wasmtime-android'
}

$RepoRoot = Split-Path $PSScriptRoot -Parent
$TmpDir   = Join-Path ([System.IO.Path]::GetTempPath()) "mediasandbox-artifacts-$(Get-Random)"

function Install-Artifact([string]$art, [string]$src) {
    switch ($art) {
        'wasm' {
            $dst = Join-Path $RepoRoot 'Assets\StreamingAssets\mediasandbox'
            New-Item -ItemType Directory -Force $dst | Out-Null
            Copy-Item (Join-Path $src 'decoder.wasm') (Join-Path $dst 'decoder.wasm') -Force
            Write-Host '    → Assets/StreamingAssets/mediasandbox/decoder.wasm'
        }
        'windows' {
            $dst = Join-Path $RepoRoot 'unity_package\Plugins\x86_64'
            New-Item -ItemType Directory -Force $dst | Out-Null
            Copy-Item (Join-Path $src 'wasmtime.dll') (Join-Path $dst 'wasmtime.dll') -Force
            Write-Host '    → unity_package/Plugins/x86_64/wasmtime.dll'
        }
        'macos' {
            $dst = Join-Path $RepoRoot 'unity_package\Plugins\x86_64'
            New-Item -ItemType Directory -Force $dst | Out-Null
            Copy-Item (Join-Path $src 'libwasmtime.dylib') (Join-Path $dst 'libwasmtime.dylib') -Force
            Write-Host '    → unity_package/Plugins/x86_64/libwasmtime.dylib'
        }
        'linux' {
            $dst = Join-Path $RepoRoot 'unity_package\Plugins\x86_64'
            New-Item -ItemType Directory -Force $dst | Out-Null
            Copy-Item (Join-Path $src 'libwasmtime.so') (Join-Path $dst 'libwasmtime.so') -Force
            Write-Host '    → unity_package/Plugins/x86_64/libwasmtime.so'
        }
        'ios' {
            $dst = Join-Path $RepoRoot 'unity_package\Plugins\iOS\Wasmtime.xcframework'
            if (Test-Path $dst) { Remove-Item $dst -Recurse -Force }
            New-Item -ItemType Directory -Force (Split-Path $dst) | Out-Null
            Copy-Item $src $dst -Recurse -Force
            Write-Host '    → unity_package/Plugins/iOS/Wasmtime.xcframework/'
        }
        'android' {
            $dst = Join-Path $RepoRoot 'unity_package\Plugins\Android\libs'
            New-Item -ItemType Directory -Force $dst | Out-Null
            Get-ChildItem $src | ForEach-Object {
                # Pre-create the ABI subdir so Copy-Item merges into it rather than
                # nesting the source dir inside an existing destination directory.
                $abiDst = Join-Path $dst $_.Name
                New-Item -ItemType Directory -Force $abiDst | Out-Null
                Copy-Item "$($_.FullName)\*" $abiDst -Recurse -Force
            }
            Write-Host '    → unity_package/Plugins/Android/libs/'
        }
    }
}

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

        Install-Artifact $art $artTmp
    }
} finally {
    Remove-Item $TmpDir -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host ''
Write-Host 'Done.'
