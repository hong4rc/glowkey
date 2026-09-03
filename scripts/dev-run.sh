#!/usr/bin/env bash
# Dev loop: rebuild the "GlowKey Dev" bundle and run it in the foreground with
# debug logging, so each emit is printed as you type. Ctrl-C to stop.
#
#   bash scripts/dev-run.sh          # release profile (fast to run)
#   bash scripts/dev-run.sh debug    # debug profile (fast to build)
#
# This is a separate app from the GlowKey in /Applications — separate bundle
# identifier, so nothing done here touches that app's Accessibility grant.
#
# You do not need to grant this one anything. Because the binary is exec'd from
# your shell rather than launched via `open`, macOS attributes Accessibility to
# the responsible process — your terminal — and the tap inherits its grant. (Run
# the same bundle with `open` and it waits for a permission of its own, which is
# how this was verified.) So keep your terminal granted and the dev loop stays
# friction-free, however often the signature changes underneath it.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_BIN="$ROOT/build/GlowKey Dev.app/Contents/MacOS/GlowKey Dev"

# Stop both variants: two taps would process every keystroke twice.
echo "==> Stopping any running GlowKey (both variants)"
killall -9 "GlowKey" 2>/dev/null || true
killall -9 "GlowKey Dev" 2>/dev/null || true

echo "==> Building"
bash "$ROOT/scripts/build-app.sh" dev "${1:-release}" >/dev/null

echo "==> Launching with GLOWKEY_DEBUG=1 (Ctrl-C to stop)"
echo "    Type in any app; each synthesized edit prints below."
exec env GLOWKEY_DEBUG=1 "$APP_BIN"
