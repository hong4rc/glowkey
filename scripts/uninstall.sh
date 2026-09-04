#!/usr/bin/env bash
# Removes GlowKey from this machine.
#
#   bash scripts/uninstall.sh             # app, build output, grants
#   bash scripts/uninstall.sh --settings  # the above plus settings and the log
#
# Settings are kept by default because they are the part that took effort: a
# curated exclusion list, macros, and per-word decisions are not something to
# throw away as a side effect of reinstalling.
#
# The signing identity in the keychain is always kept — it is what stops the
# next install from costing an Accessibility re-grant, and it is not GlowKey's
# to delete on the user's behalf.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"

WIPE_SETTINGS=false
if [ "${1:-}" = "--settings" ]; then
    WIPE_SETTINGS=true
elif [ -n "${1:-}" ]; then
    echo "unknown option '$1' (expected '--settings')" >&2
    exit 2
fi

echo "==> Stopping GlowKey"
killall GlowKey 2>/dev/null && echo "    quit GlowKey" || echo "    GlowKey was not running"
killall "GlowKey Dev" 2>/dev/null && echo "    quit GlowKey Dev" || echo "    GlowKey Dev was not running"
sleep 1

# Checked before removing: only ever delete something that really is this app.
TARGET="/Applications/GlowKey.app"
if [ -e "$TARGET" ]; then
    if [ -x "$TARGET/Contents/MacOS/GlowKey" ]; then
        rm -rf "$TARGET"
        echo "==> Removed $TARGET"
    else
        echo "==> Refusing to remove $TARGET — it does not look like GlowKey.app" >&2
    fi
fi

# The pre-CGEventTap InputMethodKit build, if it is still lying around. A second
# app with its own identifier, which is why the Accessibility list used to show
# two GlowKeys (docs/decisions/0002).
LEGACY_IMK="$HOME/Library/Input Methods/GlowKey.app"
if [ -d "$LEGACY_IMK" ]; then
    rm -rf "$LEGACY_IMK"
    echo "==> Removed the obsolete InputMethodKit build"
fi

if [ -d "$ROOT/build" ]; then
    rm -rf "$ROOT/build"
    echo "==> Removed build/"
fi

echo "==> Clearing Accessibility grants"
for id in io.glowkey.GlowKey io.glowkey.GlowKey.dev io.glowkey.inputmethod.GlowKey; do
    tccutil reset Accessibility "$id" >/dev/null 2>&1 || true
done
echo "    cleared"

if [ "$WIPE_SETTINGS" = true ]; then
    rm -rf "$HOME/Library/Application Support/GlowKey"
    rm -rf "$HOME/Library/Logs/GlowKey"
    echo "==> Removed settings, macros, word list and the log"
else
    SETTINGS="$HOME/Library/Application Support/GlowKey/settings.json"
    if [ -f "$SETTINGS" ]; then
        echo "==> Kept your settings at $SETTINGS"
        echo "    (exclusions, macros and word decisions come back on reinstall)"
    fi
fi

echo "==> Done. Reinstall with: just install"
