#!/usr/bin/env bash
# Ships the current source as the real app: builds GlowKey.app and installs it
# to /Applications, replacing what is there, then launches it.
#
#   bash scripts/release-install.sh              # build, install, launch
#   bash scripts/release-install.sh --no-launch  # build and install only
#
# This is the app you actually type with. Its Accessibility grant is tied to the
# ad-hoc signature, so installing a build that changed the code drops the grant:
# the app comes up asking for it and starts by itself once you flip the switch in
# System Settings -> Privacy & Security -> Accessibility.
#
# The dev loop (scripts/dev-run.sh) builds a separate "GlowKey Dev" app with its
# own identity, so iterating there never disturbs this grant.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SOURCE="$ROOT/build/GlowKey.app"
TARGET="/Applications/GlowKey.app"

LAUNCH=true
if [ "${1:-}" = "--no-launch" ]; then
    LAUNCH=false
elif [ -n "${1:-}" ]; then
    echo "unknown option '$1' (expected '--no-launch')" >&2
    exit 2
fi

echo "==> Stopping any running GlowKey (both variants)"
killall -9 "GlowKey" 2>/dev/null || true
killall -9 "GlowKey Dev" 2>/dev/null || true

bash "$ROOT/scripts/build-app.sh" release

# Replace rather than copy over the top, so files dropped from the bundle do not
# linger. Checked first: only ever remove something that really is this app.
if [ -e "$TARGET" ]; then
    if [ ! -x "$TARGET/Contents/MacOS/GlowKey" ]; then
        echo "refusing to replace $TARGET — it does not look like GlowKey.app" >&2
        exit 1
    fi
    rm -rf "$TARGET"
fi

echo "==> Installing to $TARGET"
cp -R "$SOURCE" "$TARGET"

if [ "$LAUNCH" = true ]; then
    echo "==> Launching"
    open "$TARGET"
    echo "    If it asks for Accessibility, grant it — the app starts by itself."
else
    echo "==> Installed (not launched)"
fi
