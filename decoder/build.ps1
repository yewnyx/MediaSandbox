param(
    [switch]$Debug
)

$target = "wasm32-wasip1"
$profile = if ($Debug) { "debug" } else { "release" }
$cargoArgs = if ($Debug) { @("build", "--target", $target) } else { @("build", "--release", "--target", $target) }

Write-Host "Adding WASM target..."
rustup target add $target

Write-Host "Building ($profile)..."
cargo @cargoArgs
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$src = "target/$target/$profile/decoder.wasm"
$dst = "../Assets/media~"
New-Item -ItemType Directory -Force $dst | Out-Null
Copy-Item $src "$dst/decoder.wasm" -Force

Write-Host "Built: $dst/decoder.wasm ($([math]::Round((Get-Item "$dst/decoder.wasm").Length / 1KB))KB)"
