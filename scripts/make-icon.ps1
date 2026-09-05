# Regenerates app/Resources/AppIcon.ico from the vector master.
#
#   pwsh scripts/make-icon.ps1
#
# The Windows counterpart of scripts/make-icon.sh, and the same bargain: **the
# generated .ico is committed**, so building GlowKey needs no image tooling.
# Only run this when the artwork changes. It needs ImageMagick:
#
#   winget install ImageMagick.ImageMagick
#
# The sizes are the ones Windows actually asks for: 16 and 20 for the tray and
# small list views, 24 and 32 for the title bar and Alt-Tab, 48 for large icons,
# 64 for high-DPI Alt-Tab, and 256 for the extra-large Explorer view and the
# installer. Each is rendered from the vector rather than resampled from a bigger
# raster, so the 16px never turns into a smudge of the 256px.

$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

$svg = 'GlowKey_Assets/app_icon.svg'
$ico = 'app/Resources/AppIcon.ico'
$sizes = 16, 20, 24, 32, 48, 64, 256

$magick = Get-Command magick -ErrorAction SilentlyContinue
if (-not $magick) {
    Write-Error "magick not found - install it with: winget install ImageMagick.ImageMagick"
    exit 1
}

New-Item -ItemType Directory -Force -Path 'app/Resources' | Out-Null
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("glowkey-icon-" + [Guid]::NewGuid().ToString("N").Substring(0,8))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

try {
    # The master SVG asks for `-apple-system`, which is SF Pro on a Mac and does
    # not exist here — ImageMagick falls back to a generic sans and the `G` comes
    # out in the wrong typeface, which on an icon that is mostly one letter is the
    # whole icon being wrong.
    #
    # Substituted in a copy rather than in the master: the master is shared with
    # the macOS build and must keep asking for the font macOS actually has. Segoe
    # UI Semibold is the closest Windows equivalent at the same weight.
    $svgWin = Join-Path $tmp 'app_icon_win.svg'
    (Get-Content $svg -Raw) -replace '-apple-system, sans-serif', 'Segoe UI Semibold, Segoe UI, sans-serif' |
        Set-Content -Path $svgWin -Encoding UTF8

    $pngs = @()
    foreach ($size in $sizes) {
        $out = Join-Path $tmp "icon-$size.png"
        # -background none keeps the squircle's corners transparent; without it
        # ImageMagick fills them white and the icon gets a square halo on every
        # dark surface Windows puts it on.
        & magick -background none -density 384 $svgWin -resize "${size}x${size}" $out
        if ($LASTEXITCODE -ne 0) { throw "magick failed rendering ${size}px" }
        $pngs += $out
    }

    & magick @pngs $ico
    if ($LASTEXITCODE -ne 0) { throw "magick failed assembling $ico" }

    $written = Get-Item $ico
    Write-Output ("Wrote {0} ({1:N0} bytes) with sizes: {2}" -f $ico, $written.Length, ($sizes -join ', '))
}
finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
