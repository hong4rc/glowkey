#!/usr/bin/env bash
# Builds GlowKey.app — a macOS input method bundle — from the Rust binary.
#
# Produces build/GlowKey.app. Install it by copying to ~/Library/Input Methods/
# and enabling under System Settings → Keyboard → Input Sources. See
# docs/checkpoint.md for the full first-run walkthrough.
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

echo "==> Done: $APP"
echo "    Architectures: $(lipo -archs "$APP/Contents/MacOS/GlowKey")"
echo ""
echo "To install for the current user:"
echo "    cp -R \"$APP\" ~/Library/Input\\ Methods/"
echo "    # then log out/in, and enable under System Settings → Keyboard → Input Sources"
