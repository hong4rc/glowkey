# GlowKey task runner. `just` on its own lists everything.
#
# The recipes are the front door; scripts/ holds the implementation, because
# assembling a macOS bundle is genuinely shell work — lipo, codesign, PlistBuddy,
# hdiutil. Keeping that in shell and the entry points here gives one obvious
# command per task without pretending Rust is a better tool for calling hdiutil.

# Show the available tasks.
default:
    @just --list --unsorted

# Build, sign, install and launch the app you type with.
install:
    # Idempotent. On first run it creates the signing identity, removes the
    # obsolete InputMethodKit bundle, and clears the stale ad-hoc grant once.
    bash scripts/release-install.sh

# The same, without launching it.
install-only:
    bash scripts/release-install.sh --no-launch

# Build and run "GlowKey Dev" in the foreground, with debug logging.
dev:
    # A separate app with its own bundle id, so it holds its own Accessibility
    # grant and never disturbs the GlowKey you type with. Never run both at
    # once: two taps process every keystroke twice.
    bash scripts/dev-run.sh

# Run the whole test suite — the headless proof.
test:
    cargo test --workspace

# Search the property suite far harder than the default.
test-hard:
    # Worth running before trusting any change to the engine's diff or restore
    # paths; the default 4096 cases have twice passed over a real corruption.
    PROPTEST_CASES=60000 cargo test -p glowkey-engine --release --test properties

# Lint. Must be silent — this project treats a warning as a failure.
lint:
    cargo clippy --workspace --all-targets

# Measure keystroke latency.
bench:
    # The engine is about 2 µs per key. This is how you find out when that
    # stops being true.
    cargo bench -p glowkey-engine

# Everything CI checks, in the order that fails fastest.
check: lint test

# Package the app as a disk image to give someone else.
dmg: install-only
    # Not notarized, so the recipient needs `xattr -dr com.apple.quarantine`.
    bash scripts/make-dmg.sh

# Create the code-signing identity, once. `install` does this for you.
signing:
    # Without it every install costs an Accessibility re-grant: macOS keys the
    # grant to the ad-hoc signature's cdhash, which changes with every build.
    bash scripts/setup-signing.sh

# Follow the live log — read this first for any reported typing bug.
log:
    tail -f ~/Library/Logs/GlowKey/glowkey.log

# Quit both variants.
stop:
    -@killall GlowKey 2>/dev/null || true
    -@killall "GlowKey Dev" 2>/dev/null || true
    @echo "stopped"

# Remove the app and its grants, keeping settings and macros.
uninstall:
    bash scripts/uninstall.sh

# Remove everything, including settings, macros and the word list.
uninstall-all:
    bash scripts/uninstall.sh --settings

# Drop all build artifacts. The next build is slow.
clean:
    cargo clean
    rm -rf build
