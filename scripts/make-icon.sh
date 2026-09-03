#!/usr/bin/env bash
# Regenerates app/Resources/AppIcon.icns from the vector master.
#
#   bash scripts/make-icon.sh
#
# The generated .icns is committed, so building the app needs no image tooling.
# Only run this when the artwork changes — it needs rsvg-convert:
#
#   brew install librsvg
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"

SVG="$ROOT/GlowKey_Assets/app_icon.svg"
ICNS="$ROOT/app/Resources/AppIcon.icns"
SET="$(mktemp -d)/AppIcon.iconset"

if ! command -v rsvg-convert >/dev/null; then
    echo "rsvg-convert not found — install it with: brew install librsvg" >&2
    exit 1
fi

mkdir -p "$SET"

# The ladder macOS expects. Each logical size needs a 1x and a 2x rendering, and
# the 2x of one rung is the same pixel count as the 1x of the next — rendered
# separately from the vector rather than resampled, so nothing is ever upscaled.
for size in 16 32 128 256 512; do
    rsvg-convert -w "$size" -h "$size" "$SVG" -o "$SET/icon_${size}x${size}.png"
    rsvg-convert -w "$((size * 2))" -h "$((size * 2))" "$SVG" -o "$SET/icon_${size}x${size}@2x.png"
done

iconutil --convert icns "$SET" --output "$ICNS"
rm -rf "$(dirname "$SET")"

echo "==> Wrote $ICNS ($(du -h "$ICNS" | cut -f1))"
