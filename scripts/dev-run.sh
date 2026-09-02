#!/usr/bin/env bash
# Quick dev loop: rebuild the app bundle and relaunch it with debug logging in the
# foreground, so each emit is printed as you type. Ctrl-C to stop.
#
#   bash scripts/dev-run.sh
#
# Accessibility: the permission is granted per bundle path, so once you allow
# GlowKey in System Settings -> Privacy -> Accessibility it persists across
# rebuilds to the same path -- no re-granting each run.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_BIN="$ROOT/build/GlowKey.app/Contents/MacOS/GlowKey"

echo "==> Stopping any running GlowKey"
killall -9 GlowKey 2>/dev/null || true

echo "==> Building (release)"
bash "$ROOT/scripts/build-app.sh" release >/dev/null

echo "==> Launching with GLOWKEY_DEBUG=1 (Ctrl-C to stop)"
echo "    Type in any app; each synthesized edit prints below."
exec env GLOWKEY_DEBUG=1 "$APP_BIN"
