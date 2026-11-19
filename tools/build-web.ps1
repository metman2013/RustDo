<#
.SYNOPSIS
    Builds the web app (PWA) into .\dist, ready to upload to any static web host.

.DESCRIPTION
    Needs:  rustup target add wasm32-unknown-unknown
            cargo install wasm-bindgen-cli --version <same as wasm-bindgen in Cargo.lock>
#>
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $root 'dist'
$homeDir = if ($env:USERPROFILE) { $env:USERPROFILE } else { $HOME }
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $homeDir '.cargo' }

# getrandom (used for task ids) needs to be told to use the browser's crypto API.
# The remaps keep personal folder names out of the published files.
$env:RUSTFLAGS = @(
    '--cfg getrandom_backend="wasm_js"',
    "--remap-path-prefix=$homeDir=home",
    "--remap-path-prefix=$cargoHome=cargo",
    "--remap-path-prefix=$root=rustdo"
) -join ' '

# wasm-bindgen needs the feature section that `strip = true` would remove.
$env:CARGO_PROFILE_RELEASE_STRIP = 'false'

Push-Location $root
try {
    # These tools print progress on stderr, which Windows PowerShell treats as errors.
    $ErrorActionPreference = 'Continue'
    cargo rustc --lib --release --target wasm32-unknown-unknown --crate-type cdylib
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

    if (Test-Path $dist) { Remove-Item -Recurse -Force $dist }
    New-Item -ItemType Directory $dist | Out-Null
    wasm-bindgen --target web --no-typescript --out-dir (Join-Path $dist 'pkg') `
        (Join-Path $root 'target/wasm32-unknown-unknown/release/rustdo_app.wasm')
    if ($LASTEXITCODE -ne 0) { throw 'wasm-bindgen failed' }
    $ErrorActionPreference = 'Stop'

    Copy-Item -Recurse (Join-Path $root 'web/*') $dist
    # A new cache name per build makes installed copies pick up the update.
    $version = (Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
    $stamp = Get-Date -Format 'yyyyMMddHHmmss'
    $sw = Join-Path $dist 'sw.js'
    (Get-Content $sw -Raw).Replace('__VERSION__', "$version-$stamp") | Set-Content $sw -NoNewline -Encoding ascii

    $size = [math]::Round((Get-Item (Join-Path $dist 'pkg/rustdo_app_bg.wasm')).Length / 1MB, 1)
    Write-Host "Web app built in $dist (wasm: $size MB)"
}
finally {
    Pop-Location
}
