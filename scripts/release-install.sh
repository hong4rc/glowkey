#!/usr/bin/env bash
# Ships the current source as the real app: builds GlowKey.app and installs it
# to /Applications, replacing what is there, then launches it.
#
#   bash scripts/release-install.sh               # everything
#   bash scripts/release-install.sh --no-launch   # build and install only
#   bash scripts/release-install.sh --no-signing  # skip the identity check
#
# One command does the lot, idempotently: creates the code-signing identity if it
# is missing, removes the obsolete InputMethodKit bundle if it is still there,
# builds, installs, and launches.
#
# This is the app you actually type with. Its Accessibility grant follows the
# code signature: with the identity in place a rebuild keeps the grant, and
# without one (--no-signing, or a machine with no keychain access) the ad-hoc
# signature changes with every code change and the grant is dropped. Either way
# the app asks on screen and starts by itself once you flip the switch in
# System Settings -> Privacy & Security -> Accessibility.
#
# The dev loop (scripts/dev-run.sh) builds a separate "GlowKey Dev" app with its
# own identity, so iterating there never disturbs this grant.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SOURCE="$ROOT/build/GlowKey.app"
TARGET="/Applications/GlowKey.app"

LAUNCH=true
SIGNING=true
for arg in "$@"; do
    case "$arg" in
    --no-launch) LAUNCH=false ;;
    --no-signing) SIGNING=false ;;
    *)
        echo "unknown option '$arg' (expected '--no-launch' or '--no-signing')" >&2
        exit 2
        ;;
    esac
done

# One command, everything: signing identity, stale leftovers, build, install.
# Each step is idempotent and says what it did, so running it twice is safe and
# running it the first time explains itself.

# 1. The signing identity, without which every install costs an Accessibility
#    re-grant (see scripts/setup-signing.sh for why).
RESET_GRANT=false
if [ "$SIGNING" = true ]; then
    # Exit 10 means the identity was created just now, so the app is about to be
    # signed differently than the grant on file was issued for. That stale entry
    # never matches again and has to be cleared once — the last re-grant this
    # setup needs.
    set +e
    bash "$ROOT/scripts/setup-signing.sh"
    signing_status=$?
    set -e
    if [ "$signing_status" -eq 10 ]; then
        RESET_GRANT=true
    elif [ "$signing_status" -ne 0 ]; then
        echo "signing setup failed (exit $signing_status)" >&2
        exit "$signing_status"
    fi
fi

# 2. A leftover from before the CGEventTap redesign (docs/decisions/0002).
#    GlowKey used to be an InputMethodKit input method installed here; that
#    bundle is inert now, but it is a second app with its own identifier, so it
#    shows up as a second "GlowKey" in the Accessibility list and makes the real
#    one impossible to pick out.
LEGACY_IMK="$HOME/Library/Input Methods/GlowKey.app"
if [ -d "$LEGACY_IMK" ] && [ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$LEGACY_IMK/Contents/Info.plist" 2>/dev/null)" = "io.glowkey.inputmethod.GlowKey" ]; then
    echo "==> Removing the obsolete InputMethodKit build at $LEGACY_IMK"
    rm -rf "$LEGACY_IMK"
    echo "    (that was the second \"GlowKey\" in System Settings)"
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

if [ "$RESET_GRANT" = true ]; then
    echo "==> Clearing the old ad-hoc Accessibility grant (it can never match again)"
    tccutil reset Accessibility io.glowkey.GlowKey >/dev/null 2>&1 || true
    echo "    Grant it once more when asked — with the identity in place, later"
    echo "    rebuilds keep it."
fi

echo "==> Installing to $TARGET"
# `command cp`, not `cp`: an interactive shell may alias cp to `cp -i`, and a
# copy over an existing bundle then stops to ask about every file it would
# overwrite. Scripts do not expand aliases, but this also documents why copying
# the bundle by hand at the prompt is a bad idea — and merging over a live
# bundle leaves files behind that the new signature does not cover.
command cp -R "$SOURCE" "$TARGET"

if [ "$LAUNCH" = true ]; then
    echo "==> Launching"
    open "$TARGET"
    echo "    If it asks for Accessibility, grant it — the app starts by itself."
else
    echo "==> Installed (not launched)"
fi
