#!/usr/bin/env bash
# Packages build/GlowKey.app into a distributable disk image.
#
#   bash scripts/make-dmg.sh            # build/GlowKey-<version>.dmg
#
# Deliberately plain: the app and a symlink to /Applications, no background
# image, no window geometry, no layout to maintain. A drag target is all this
# needs to be, and every decoration is something that rots.
#
# **Gatekeeper.** Unless a Developer ID signed the app, macOS refuses a
# downloaded copy with "GlowKey is damaged and can't be opened" until the
# quarantine attribute is stripped. That is expected, not a defect — the owner
# chose self-signed over a paid Apple Developer account (see
# docs/decisions/0006-stable-signing-identity.md), so the release notes and the
# README carry the one command that clears it. Do not try to work around
# Gatekeeper; the only real fix is notarization.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"

APP="$ROOT/build/GlowKey.app"
if [ ! -d "$APP" ]; then
    echo "no app bundle at $APP — run scripts/build-app.sh first" >&2
    exit 1
fi

VERSION="$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "$APP/Contents/Info.plist")"
if [ -z "$VERSION" ]; then
    echo "could not read CFBundleShortVersionString from the bundle" >&2
    exit 1
fi

DMG="$ROOT/build/GlowKey-$VERSION.dmg"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

echo "==> Staging"
command cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"

echo "==> Creating $DMG"
rm -f "$DMG"
hdiutil create \
    -volname "GlowKey $VERSION" \
    -srcfolder "$STAGE" \
    -ov -format UDZO \
    "$DMG" >/dev/null

echo "==> Verifying"
hdiutil verify "$DMG" >/dev/null

echo "==> Done: $DMG"
echo "    Size:    $(du -h "$DMG" | cut -f1)"
AUTHORITY="$(codesign -dv "$APP" 2>&1 | sed -n 's/^Authority=//p' | head -1)"
# `|| echo` would never fire here: `head` exits 0 even with no input, so an
# ad-hoc bundle printed an empty field rather than saying so.
echo "    Signed:  ${AUTHORITY:-ad-hoc (Gatekeeper will need the xattr command)}"
