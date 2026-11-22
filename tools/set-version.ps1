<#
.SYNOPSIS
    Sets the app version everywhere it's needed (desktop/web, Android and RustDo Hub).

.EXAMPLE
    .\tools\set-version.ps1 0.2.0
#>
param([Parameter(Mandatory)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version)
$root = Split-Path -Parent $PSScriptRoot
foreach ($file in 'Cargo.toml', 'android/Cargo.toml', 'hub/Cargo.toml') {
    $path = Join-Path $root $file
    $text = [System.IO.File]::ReadAllText($path)
    # Replace only the first top-level `version = "x.y.z"` line.
    $re = [regex]'(?m)^version = "\d+\.\d+\.\d+"'
    if (-not $re.IsMatch($text)) { throw "No version line in $file" }
    $text = $re.Replace($text, "version = `"$Version`"", 1)
    [System.IO.File]::WriteAllText($path, $text, (New-Object System.Text.UTF8Encoding $false))
    Write-Host "$file -> $Version"
}
Write-Host "Now commit, then: git tag v$Version; git push origin main --tags"
