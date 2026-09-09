# GlowKey task runner. `just` on its own lists everything.
#
# The recipes are the front door; scripts/ holds the implementation, because
# assembling a macOS bundle is genuinely shell work — lipo, codesign, PlistBuddy,
# hdiutil. Keeping that in shell and the entry points here gives one obvious
# command per task without pretending Rust is a better tool for calling hdiutil.
#
# **Both platforms.** A recipe with no platform attribute runs anywhere — the
# whole test and lint half is plain cargo. Where the two systems genuinely
# differ the same name is defined twice, `[macos]` and `[windows]`, so one
# command means the same thing on either machine even though one of them signs a
# bundle and the other cannot. `just --list` shows only what the platform you
# are on can do, which is why `dmg`, `signing` and `uninstall` are macOS-only:
# there is no bundle to package, sign or remove on Windows yet.
#
# Rationale sits *above* each recipe rather than inside it. `just` echoes a
# recipe's body as it runs, comments included, and takes the last comment line
# above a recipe as its `--list` description — so this way the explanation is
# read where it belongs and not printed at someone running the task.

# Windows recipes run in PowerShell rather than `sh`.
#
# `just` defaults to `sh -cu` everywhere, which on Windows means a POSIX shell
# has to be installed and on PATH — a dependency the app itself does not have.
# The Windows recipes below need `Start-Process` and `Stop-Process` anyway, and
# `docs/manual-verification-windows.md` already speaks PowerShell, so this is
# the local convention rather than a second one.
set windows-shell := ["powershell.exe", "-NoProfile", "-Command"]

# Show the available tasks.
default:
    @just --list --unsorted

# Idempotent. On first run it creates the signing identity, removes the obsolete
# InputMethodKit bundle, and clears the stale ad-hoc grant once.
#
# Build, sign, install and launch the app you type with.
[macos]
install:
    bash scripts/release-install.sh

# There is no bundle to install on Windows: the executable runs from
# target/release, which is what `docs/manual-verification-windows.md` tells you
# to launch. So "install" is build-and-run, under the same name as on macOS
# because it answers the same question — *am I typing with the code I just
# changed?*
#
# Build and launch the app you type with.
[windows]
install: install-only
    Start-Process -FilePath "{{justfile_directory()}}\target\release\GlowKey.exe"
    @Write-Output "==> GlowKey is running. The tray icon shows VI or EN."

# The same, without launching it.
[macos]
install-only:
    bash scripts/release-install.sh --no-launch

# Depends on `stop`, and not for tidiness: Windows locks a running executable,
# so the build fails with `os error 5` while GlowKey is up.
#
# Build the release executable, without launching it.
[windows]
install-only: stop
    cargo build --release -p glowkey

# A separate app with its own bundle id, so it holds its own Accessibility grant
# and never disturbs the GlowKey you type with. Never run both at once: two taps
# process every keystroke twice.
#
# Build and run "GlowKey Dev" in the foreground, with debug logging.
[macos]
dev:
    bash scripts/dev-run.sh

# Stops the release instance first, for the reason macOS refuses to run both
# variants: two hooks process every keystroke twice. Ctrl-C to stop.
# `GLOWKEY_DEBUG` echoes every decision to stderr as you type, on top of the log
# file (`app/src/log.rs`).
#
# Build and run the debug binary in the foreground, with debug logging.
[windows]
dev: stop
    $env:GLOWKEY_DEBUG = "1"; cargo run -p glowkey

# Run the whole test suite — the headless proof.
test:
    cargo test --workspace

# Worth running before trusting any change to the engine's diff or restore
# paths; the default 4096 cases have twice passed over a real corruption.
#
# Search the property suite far harder than the default.
[unix]
test-hard:
    PROPTEST_CASES=60000 cargo test -p glowkey-session --release --test properties

# The same, with PowerShell's way of setting the variable.
#
# Search the property suite far harder than the default.
[windows]
test-hard:
    $env:PROPTEST_CASES = "60000"; cargo test -p glowkey-session --release --test properties

# Lint. Must be silent — this project treats a warning as a failure.
lint:
    cargo clippy --workspace --all-targets

# The engine is about 2 µs per key. This is how you find out when that stops
# being true.
#
# Measure keystroke latency.
bench:
    cargo bench -p glowkey-session

# Everything CI checks, in the order that fails fastest.
check: lint test

# Not notarized, so the recipient needs `xattr -dr com.apple.quarantine`.
#
# Package the app as a disk image to give someone else.
[macos]
dmg: install-only
    bash scripts/make-dmg.sh

# Without it every install costs an Accessibility re-grant: macOS keys the grant
# to the ad-hoc signature's cdhash, which changes with every build.
#
# Create the code-signing identity, once. `install` does this for you.
[macos]
signing:
    bash scripts/setup-signing.sh

# Follow the live log — read this first for any reported typing bug.
[macos]
log:
    tail -f ~/Library/Logs/GlowKey/glowkey.log

# `%LOCALAPPDATA%`, not roaming: the log does not follow the user between
# machines (`app/src/platform/windows/paths.rs`).
#
# Follow the live log — read this first for any reported typing bug.
[windows]
log:
    Get-Content -Wait -Tail 40 "$env:LOCALAPPDATA\GlowKey\Logs\glowkey.log"

# Quit both variants.
[macos]
stop:
    -@killall GlowKey 2>/dev/null || true
    -@killall "GlowKey Dev" 2>/dev/null || true
    @echo "stopped"

# Silent when nothing is running: `install-only`, `dev` and `clean` all depend on
# this, and none of them may fail for want of something to kill.
#
# Hence the `if` rather than a pipe into `Stop-Process`. `powershell -Command`
# reports the last statement's success as its exit status, and `Get-Process`
# sets that to false when it matches nothing — `-ErrorAction SilentlyContinue`
# silences the message, not the status — so the obvious one-liner failed the
# recipe on a machine where GlowKey simply was not running. A real failure, such
# as being refused permission to kill it, still surfaces.
#
# Quit any running instance.
[windows]
stop:
    @if (Get-Process GlowKey -ErrorAction SilentlyContinue) { Stop-Process -Name GlowKey -Force }
    @Write-Output "stopped"

# Remove the app and its grants, keeping settings and macros.
[macos]
uninstall:
    bash scripts/uninstall.sh

# Remove everything, including settings, macros and the word list.
[macos]
uninstall-all:
    bash scripts/uninstall.sh --settings

# Drop all build artifacts. The next build is slow.
[unix]
clean:
    cargo clean
    rm -rf build

# `stop` first for the same reason as `install-only`: `cargo clean` cannot
# delete a running executable. `build/` is the macOS bundle's output and is
# normally absent here, hence the test.
#
# Drop all build artifacts. The next build is slow.
[windows]
clean: stop
    cargo clean
    @if (Test-Path build) { Remove-Item -Recurse -Force build }
