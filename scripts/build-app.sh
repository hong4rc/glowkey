#!/usr/bin/env bash
# Builds GlowKey.app — a background agent that wraps the keyboard layout with
# Vietnamese via a CGEventTap (like EVKey) — from the Rust binary.
#
# Produces build/GlowKey.app. Run it, grant Accessibility when prompted, and it
# adds Vietnamese on top of your current layout. See docs/checkpoint.md.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"
APP="$ROOT/build/GlowKey.app"
CONFIG="${1:-release}" # release (default) or debug

echo "==> Building GlowKey ($CONFIG) as a universal binary"
if [ "$CONFIG" = "release" ]; then
    CARGO_FLAGS="--release"
    TARGET_DIR="release"
else
    CARGO_FLAGS=""
    TARGET_DIR="debug"
fi

# Build both architectures and merge, so the app runs on Apple Silicon and Intel.
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null 2>&1 || true
cargo build -p glowkey $CARGO_FLAGS --target aarch64-apple-darwin
cargo build -p glowkey $CARGO_FLAGS --target x86_64-apple-darwin

echo "==> Assembling bundle at $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

lipo -create -output "$APP/Contents/MacOS/GlowKey" \
    "$ROOT/target/aarch64-apple-darwin/$TARGET_DIR/GlowKey" \
    "$ROOT/target/x86_64-apple-darwin/$TARGET_DIR/GlowKey"

cp "$ROOT/app/Resources/Info.plist" "$APP/Contents/Info.plist"
cp "$ROOT/THIRD-PARTY-NOTICES.md" "$APP/Contents/Resources/" 2>/dev/null || true

# Ad-hoc sign so macOS lets it request Accessibility and run.
codesign --force --deep -s - "$APP" >/dev/null 2>&1 || true

echo "==> Done: $APP"
echo "    Architectures: $(lipo -archs "$APP/Contents/MacOS/GlowKey")"
echo ""
echo "To run it:"
echo "    open \"$APP\"   # or: \"$APP/Contents/MacOS/GlowKey\" to see logs in the terminal"
echo "    # Grant Accessibility when prompted (System Settings → Privacy & Security"
echo "    #  → Accessibility), then it wraps your current layout with Vietnamese."
