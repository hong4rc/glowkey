#!/usr/bin/env bash
# Builds a GlowKey app bundle from the Rust binary.
#
#   bash scripts/build-app.sh            # GlowKey.app       (release profile)
#   bash scripts/build-app.sh dev        # "GlowKey Dev.app" (release profile)
#   bash scripts/build-app.sh dev debug  # "GlowKey Dev.app" (debug profile)
#
# The dev variant is a genuinely separate app to macOS: its own bundle
# identifier, display name and executable. That is what lets the shipped copy in
# /Applications and the working copy in build/ each hold their own Accessibility
# permission. Sharing one identity means every dev rebuild changes the signature
# under the app you actually type with, and you lose Vietnamese until you
# re-grant it.
#
# Both variants read the same settings and write the same log, so the dev build
# behaves like your real setup. Do not run them at once: two taps means every
# keystroke is processed twice. The wrapper scripts stop both first.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"

VARIANT="${1:-release}" # release | dev
PROFILE="${2:-release}" # release | debug

case "$VARIANT" in
release)
    APP_NAME="GlowKey"
    BUNDLE_ID="io.glowkey.GlowKey"
    ;;
dev)
    APP_NAME="GlowKey Dev"
    BUNDLE_ID="io.glowkey.GlowKey.dev"
    ;;
*)
    echo "unknown variant '$VARIANT' (expected 'release' or 'dev')" >&2
    exit 2
    ;;
esac

case "$PROFILE" in
release)
    CARGO_FLAGS="--release"
    TARGET_DIR="release"
    ;;
debug)
    CARGO_FLAGS=""
    TARGET_DIR="debug"
    ;;
*)
    echo "unknown profile '$PROFILE' (expected 'release' or 'debug')" >&2
    exit 2
    ;;
esac

APP="$ROOT/build/$APP_NAME.app"

echo "==> Building $APP_NAME ($PROFILE) as a universal binary"

# Build both architectures and merge, so the app runs on Apple Silicon and Intel.
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null 2>&1 || true
cargo build -p glowkey $CARGO_FLAGS --target aarch64-apple-darwin
cargo build -p glowkey $CARGO_FLAGS --target x86_64-apple-darwin

echo "==> Assembling bundle at $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

# The executable is named after the variant too, so `ps`, Activity Monitor and
# killall can tell the two apart.
lipo -create -output "$APP/Contents/MacOS/$APP_NAME" \
    "$ROOT/target/aarch64-apple-darwin/$TARGET_DIR/GlowKey" \
    "$ROOT/target/x86_64-apple-darwin/$TARGET_DIR/GlowKey"

cp "$ROOT/app/Resources/Info.plist" "$APP/Contents/Info.plist"
plist="$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $BUNDLE_ID" "$plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleName $APP_NAME" "$plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleDisplayName $APP_NAME" "$plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleExecutable $APP_NAME" "$plist"

# The icon is a committed artifact (regenerate with scripts/make-icon.sh), so a
# plain build needs no image tooling. CFBundleIconFile in Info.plist names it.
cp "$ROOT/app/Resources/AppIcon.icns" "$APP/Contents/Resources/"

cp "$ROOT/THIRD-PARTY-NOTICES.md" "$APP/Contents/Resources/" 2>/dev/null || true

# Stamp the version from Cargo.toml so it exists in exactly one place. The
# literal in Info.plist is a placeholder that is always overwritten, the same way
# the bundle identifier above is.
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/app/Cargo.toml" | head -1)"
if [ -z "$VERSION" ]; then
    echo "could not read package.version from app/Cargo.toml" >&2
    exit 1
fi
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" "$plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $VERSION" "$plist"

# Sign with a stable identity when one exists, ad-hoc otherwise.
#
# This is what decides whether the Accessibility grant survives a rebuild. TCC
# keys an ad-hoc-signed app to its **cdhash**, which changes with every code
# change — so each install of a changed build was a new app to macOS and the
# grant was dropped. A self-signed certificate gives the bundle a designated
# requirement that names the identifier and the certificate rather than the
# hash, and that does not move when the code does.
#
# Create the certificate once, by hand: Keychain Access → Certificate Assistant
# → Create a Certificate, name "GlowKey Developer", type "Code Signing",
# self-signed, ten-year validity. There is no reliable non-interactive way to
# make a self-signed certificate the system trusts for code signing, so this
# stays a documented manual step rather than a script that pretends otherwise.
# See docs/decisions/0006-stable-signing-identity.md.
SIGN_IDENTITY="${GLOWKEY_SIGN_IDENTITY:-GlowKey Developer}"
# Captured into a variable rather than piped into `grep -q`. Under `pipefail`,
# `grep -q` exits at the first match without draining its input, so `security`
# can take SIGPIPE and the pipeline reports 141 — a *failure* — even though the
# identity was found. That would silently fall back to ad-hoc signing, which is
# precisely the problem this signing work exists to remove, announced by nothing
# but one line of build output.
# `-p codesigning` without `-v`: a self-signed certificate is not *trusted*, so
# `-v` ("valid identities only") hides it — but trust governs signature
# verification, not signing. codesign uses it happily, and the designated
# requirement it produces names the identifier and the certificate rather than a
# hash of the code, which is the entire point.
AVAILABLE_IDENTITIES="$(security find-identity -p codesigning 2>/dev/null || true)"
if [ "${AVAILABLE_IDENTITIES#*"$SIGN_IDENTITY"}" != "$AVAILABLE_IDENTITIES" ]; then
    # Deliberately not silenced: with a certificate present, a signing failure is
    # a real problem and must be loud rather than falling back behind your back.
    codesign --force --deep --sign "$SIGN_IDENTITY" "$APP"
    SIGNED_WITH="$SIGN_IDENTITY"
else
    codesign --force --deep -s - "$APP" >/dev/null 2>&1 || true
    SIGNED_WITH="ad-hoc — expect to re-grant Accessibility after every code change"
fi

echo "==> Done: $APP"
echo "    Identifier:    $BUNDLE_ID"
echo "    Version:       $VERSION"
echo "    Architectures: $(lipo -archs "$APP/Contents/MacOS/$APP_NAME")"
echo "    Signed with:   $SIGNED_WITH"
# The designated requirement is what TCC actually matches on, so print it: a
# broken signature is then visible at build time instead of at permission time.
# The prefix differs by signature kind — ad-hoc prints "# designated => ", a
# certificate prints "designated => " — so accept either rather than silently
# printing nothing, which is what made this line useless exactly when it was
# most interesting.
echo "    Requirement:   $(codesign -d -r- "$APP" 2>&1 | sed -n 's/^#* *designated => //p')"
