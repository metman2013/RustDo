<#
.SYNOPSIS
    Builds the Microsoft Store package (MSIX) from a built rustdo.exe.

.DESCRIPTION
    Fills in installer\msix\AppxManifest.xml, lays out rustdo.exe and the tile images,
    indexes the images (makepri) and packs everything with makeappx, which also checks the
    manifest. The result is unsigned: the Store signs it when it's published. Upload it in
    Partner Center (your app > Packages).

    The package identity is RustDo's, from Partner Center > Product management > Product
    identity (KoebleEffect.RustDo). These environment variables override it:
      MSIX_NAME                   Package/Identity/Name
      MSIX_PUBLISHER              Package/Identity/Publisher (CN=...)
      MSIX_PUBLISHER_DISPLAY_NAME Package/Properties/PublisherDisplayName

.EXAMPLE
    .\tools\build-msix.ps1 -Exe target\release\rustdo.exe -Version 0.3.2 -Out target\store
#>
param(
    [Parameter(Mandatory)][string]$Exe,
    [Parameter(Mandatory)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version,
    [string]$Out = 'target\store'
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$src = Join-Path $root 'installer\msix'

# RustDo's identity in Partner Center (not secret). The environment variables override it,
# e.g. to build a test package under another identity.
$name = if ($env:MSIX_NAME) { $env:MSIX_NAME } else { 'KoebleEffect.RustDo' }
$publisher = if ($env:MSIX_PUBLISHER) { $env:MSIX_PUBLISHER } else { 'CN=B91A4D6C-087A-4954-86E2-2B5ECE9D9079' }
$publisherName = if ($env:MSIX_PUBLISHER_DISPLAY_NAME) { $env:MSIX_PUBLISHER_DISPLAY_NAME } else { 'KoebleEffect' }
Write-Host "Package identity: $name / $publisher / $publisherName"

# The newest Windows SDK's tools.
$sdkBin = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.*\x64" -Directory |
    Sort-Object { [version]$_.Name } | Select-Object -Last 1
if (-not $sdkBin) { throw 'Windows SDK not found (makeappx/makepri)' }
$makeappx = Join-Path $sdkBin.FullName 'makeappx.exe'
$makepri = Join-Path $sdkBin.FullName 'makepri.exe'
Write-Host "Using Windows SDK tools from $($sdkBin.FullName)"

$layout = Join-Path $root 'target\msix-layout'
if (Test-Path $layout) { Remove-Item -Recurse -Force $layout }
New-Item -ItemType Directory -Force (Join-Path $layout 'Assets') | Out-Null

# Manifest with the identity and version filled in (XML-escaped).
$esc = { param($s) [System.Security.SecurityElement]::Escape($s) }
$manifest = [System.IO.File]::ReadAllText((Join-Path $src 'AppxManifest.xml'))
$manifest = $manifest.Replace('__NAME__', (& $esc $name)).
    Replace('__PUBLISHER__', (& $esc $publisher)).
    Replace('__PUBLISHER_DISPLAY_NAME__', (& $esc $publisherName)).
    Replace('__VERSION__', "$Version.0")
[System.IO.File]::WriteAllText((Join-Path $layout 'AppxManifest.xml'), $manifest, (New-Object System.Text.UTF8Encoding $false))

Copy-Item $Exe (Join-Path $layout 'rustdo.exe')
Copy-Item (Join-Path $src 'Assets\*.png') (Join-Path $layout 'Assets')
Copy-Item (Join-Path $root 'LICENSE') (Join-Path $layout 'LICENSE.txt')

# resources.pri: lets Windows pick the right icon size (targetsize-*) for the taskbar.
$priConfig = Join-Path $root 'target\priconfig.xml'
& $makepri createconfig /cf $priConfig /dq en-US /o | Out-Host
if ($LASTEXITCODE -ne 0) { throw 'makepri createconfig failed' }
& $makepri new /pr $layout /cf $priConfig /of (Join-Path $layout 'resources.pri') /o | Out-Host
if ($LASTEXITCODE -ne 0) { throw 'makepri new failed' }

New-Item -ItemType Directory -Force $Out | Out-Null
$msix = Join-Path $Out "RustDo-$Version.msix"
& $makeappx pack /d $layout /p $msix /o | Out-Host
if ($LASTEXITCODE -ne 0) { throw 'makeappx pack failed' }
Write-Host "Store package: $msix ($([math]::Round((Get-Item $msix).Length / 1MB, 1)) MB)"
