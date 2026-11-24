<#
.SYNOPSIS
    Builds a release binary with no personal paths embedded.

.DESCRIPTION
    Rust embeds source file paths (for panic messages) in binaries, and these include
    your user folder (C:\Users\<name>\.cargo\...) and this project's location.
    This script rewrites those prefixes to neutral ones before building, then checks
    the result.
#>
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$homeDir = if ($env:USERPROFILE) { $env:USERPROFILE } else { $HOME }
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $homeDir '.cargo' }

# rustc applies the LAST matching rule, so list the most general prefix first.
# +crt-static: no VCRUNTIME140.dll needed (RUSTFLAGS replaces .cargo/config.toml's flags).
$env:RUSTFLAGS = @(
    "-C target-feature=+crt-static",
    "--remap-path-prefix=$homeDir=home",
    "--remap-path-prefix=$cargoHome=cargo",
    "--remap-path-prefix=$root=rustdo"
) -join ' '

Push-Location $root
try {
    # cargo prints progress on stderr, which Windows PowerShell would treat as an error.
    $ErrorActionPreference = 'Continue'
    cargo build --release
    $ErrorActionPreference = 'Stop'
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

    $exe = Join-Path $root 'target/release/rustdo.exe'
    $text = [System.Text.Encoding]::ASCII.GetString([System.IO.File]::ReadAllBytes($exe))
    $leaf = Split-Path -Leaf $root
    $needles = @($homeDir, $root, "\$leaf\", "/$leaf/")
    if ($env:USERNAME) { $needles += "\$($env:USERNAME)\" }
    $leaks = $needles | Where-Object { $_ -and $text.IndexOf($_, [StringComparison]::OrdinalIgnoreCase) -ge 0 }
    if ($leaks) { throw "Personal paths still found in ${exe}: $($leaks -join ', ')" }
    Write-Host "OK: $exe contains no user name or local paths."
}
finally {
    Pop-Location
}
