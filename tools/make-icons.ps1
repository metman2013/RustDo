<#
.SYNOPSIS
    Draws RustDo's icon (white check mark on a blue rounded square) at every size the
    web app, Android app and Windows installer need. Re-run after changing the design.
#>
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$root = Split-Path -Parent $PSScriptRoot

function New-Icon([int]$size, [string]$path, [switch]$Maskable) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'
    $g.Clear([System.Drawing.Color]::Transparent)
    $blue = [System.Drawing.Color]::FromArgb(255, 70, 125, 230)
    $brush = New-Object System.Drawing.SolidBrush $blue
    if ($Maskable) {
        # Maskable icons must fill the whole square; the OS applies its own shape.
        $g.FillRectangle($brush, 0, 0, $size, $size)
    } else {
        $r = [int]($size * 0.22)
        $gp = New-Object System.Drawing.Drawing2D.GraphicsPath
        $gp.AddArc(0, 0, $r * 2, $r * 2, 180, 90)
        $gp.AddArc($size - $r * 2, 0, $r * 2, $r * 2, 270, 90)
        $gp.AddArc($size - $r * 2, $size - $r * 2, $r * 2, $r * 2, 0, 90)
        $gp.AddArc(0, $size - $r * 2, $r * 2, $r * 2, 90, 90)
        $gp.CloseFigure()
        $g.FillPath($brush, $gp)
    }
    # Check mark, kept inside the central safe zone so maskable crops don't clip it.
    $s = $size / 100.0
    $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::White), ([float](9 * $s))
    $pen.StartCap = 'Round'; $pen.EndCap = 'Round'; $pen.LineJoin = 'Round'
    $pts = [System.Drawing.PointF[]]@(
        (New-Object System.Drawing.PointF ([float](30 * $s)), ([float](52 * $s))),
        (New-Object System.Drawing.PointF ([float](44 * $s)), ([float](66 * $s))),
        (New-Object System.Drawing.PointF ([float](71 * $s)), ([float](37 * $s)))
    )
    $g.DrawLines($pen, $pts)
    New-Item -ItemType Directory -Force (Split-Path -Parent $path) | Out-Null
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
}

# Web app
New-Icon 192 "$root\web\icons\icon-192.png"
New-Icon 512 "$root\web\icons\icon-512.png"
New-Icon 512 "$root\web\icons\icon-maskable-512.png" -Maskable

# Android launcher icons
$densities = @{ mdpi = 48; hdpi = 72; xhdpi = 96; xxhdpi = 144; xxxhdpi = 192 }
foreach ($d in $densities.Keys) {
    New-Icon $densities[$d] "$root\android\res\mipmap-$d\ic_launcher.png"
}

# Linux (.deb / AppImage)
New-Icon 512 "$root\assets\rustdo.png"

# Windows .ico (PNG-compressed entries, supported since Windows Vista)
$sizes = 16, 24, 32, 48, 64, 128, 256
$pngs = foreach ($sz in $sizes) {
    $tmp = [System.IO.Path]::GetTempFileName()
    New-Icon $sz $tmp
    , [System.IO.File]::ReadAllBytes($tmp)
    Remove-Item $tmp
}
$ico = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter $ico
$w.Write([uint16]0); $w.Write([uint16]1); $w.Write([uint16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $sz = $sizes[$i]; $len = $pngs[$i].Length
    $w.Write([byte]($sz % 256)); $w.Write([byte]($sz % 256)); $w.Write([byte]0); $w.Write([byte]0)
    $w.Write([uint16]1); $w.Write([uint16]32); $w.Write([uint32]$len); $w.Write([uint32]$offset)
    $offset += $len
}
foreach ($p in $pngs) { $w.Write($p) }
New-Item -ItemType Directory -Force "$root\assets" | Out-Null
[System.IO.File]::WriteAllBytes("$root\assets\rustdo.ico", $ico.ToArray())
Write-Host 'Icons written.'
