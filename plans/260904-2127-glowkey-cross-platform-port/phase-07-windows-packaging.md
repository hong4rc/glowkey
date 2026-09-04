---
phase: 7
title: "Windows packaging and CI"
status: in-progress
priority: P2
effort: "2d"
dependencies: [0]
---

# Phase 7: Windows packaging and CI

## Overview

Make the Windows build reproducible in CI and installable by someone who is not the
author.

## The CI job does not wait for Phase 6

This phase splits in two, and the halves have different dependencies:

- **The `windows-latest` CI job depends on Phase 0 only.** The moment the engine's
  tests pass on Windows, CI should be the thing that keeps them passing — the six
  failures Phase 0 fixes were introduced by Phase 2 and went unnoticed precisely
  because no job ran them on Windows. Land the job with Phase 0 or immediately after.
  It starts by running the engine and input tests, and grows to build the app as
  Phase 4 lands. Issue #1 lists it as unblocked work, and it is.
- **Packaging and the release artifact depend on Phase 6.** Shipping a binary to
  someone before a human has typed into real applications with it is the one thing
  this plan's verification ceiling exists to prevent.

## Requirements

- Functional: a tagged commit produces a downloadable Windows artifact.
- Functional: CI builds and checks all three targets on every push.
- Non-functional: no code signing certificate is assumed — the SmartScreen story is
  documented honestly, as the notarization stance already is on macOS.

## Architecture

CI matrix:

| Job | Runner | What it proves |
|---|---|---|
| engine + input, `-D warnings` | `ubuntu-latest` | no platform code leaked into the shared crates |
| macOS build + full test suite | `macos-latest` | the shipping macOS app |
| Windows build + test | `windows-latest` | the Windows app actually links and its tests run |
| `cargo check` Linux target | `ubuntu-latest` | the future Linux backend keeps compiling |

The `windows-latest` job is worth more than it looks: it is the first place Windows
code gets **linked and its tests run**, which no amount of `cargo check` on a Mac
achieves.

Packaging: a portable ZIP first — it is honest, needs no installer framework, and
matches how a background utility is usually tried. An MSI via WiX only if the owner
wants Add/Remove Programs integration.

## Related Code Files

- Create: `.github/workflows/ci.yml` — extend the existing matrix
- Create: `scripts/build-windows.ps1`, `scripts/package-windows.ps1`
- Modify: `.github/workflows/release.yml` — attach the Windows artifact to a `v*` tag
- Modify: `README.md`, `docs/handoff.md` §8 — build commands per platform

## Implementation Steps

1. Extend CI with the `windows-latest` job; make it run the full test suite there.
   Do this first and do not hold it behind Phase 6 — see the split above. Mirror the
   existing `engine` job's shape: `cargo fmt --check`, `cargo clippy --all-targets
   -- -D warnings`, `cargo test -p glowkey-engine`, `cargo test -p glowkey-input`,
   `cargo check -p glowkey`. Add `cargo build -p glowkey` once Phase 4 lands.
2. Add the Linux-target `cargo check` job so Phases 8-10 cannot silently rot.
3. `build-windows.ps1`: release build, version stamped from `app/Cargo.toml`, the
   commit stamped by the existing `build.rs`.
4. `package-windows.ps1`: ZIP with the executable and a README naming the
   SmartScreen behaviour and the elevated-window limitation.
5. Extend the release workflow to attach it to a tag.
6. Document per-platform build commands in one place.

## Success Criteria

- [ ] CI green on all four jobs
- [ ] `git tag v0.3.0` produces a Windows ZIP and a macOS DMG
- [ ] A person who is not the author can download, unzip, run, and type Vietnamese
- [ ] SmartScreen behaviour documented, not worked around
- [ ] The Windows limitations list ships inside the artifact

## Risk Assessment

**Unsigned binaries hit SmartScreen**, which for a keyboard hook looks exactly as
alarming as it should. *Signal:* users report "Windows protected your PC".
*Response:* document the exact click path, as the macOS side documents
`xattr -dr com.apple.quarantine`. Signing is a purchase decision, not an engineering
one.

**An antivirus may flag a low-level keyboard hook.** It is, structurally, what a
keylogger does. *Signal:* false positives. *Response:* the defence is the privacy
posture being true and checkable — local-only, no network, open source. Say it in
the README.

**CI on `windows-latest` may pass while the app is unusable**, since it cannot type
into applications. *Signal:* green CI, broken app. *Response:* CI's job is regression
detection, never a substitute for Phase 6. Landing the job early makes this risk
easier to fall into, not harder — a green Windows badge is exactly the thing that
tempts someone to skip the manual tier. The badge means the tests run, and the tests
do not type.
