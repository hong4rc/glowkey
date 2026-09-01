# 0001 — All-Rust InputMethodKit shell via objc2

## Status

Accepted (2026-09-01).

## Context

The plan assumed a Rust engine behind a C ABI with a Swift InputMethodKit shell.
Scouting the sibling projects showed `marau` already builds a macOS app whose Rust
side calls Apple frameworks directly through `objc2` (Core WLAN, Bluetooth,
Foundation) — no Swift, no hand-written FFI boundary. Only Command Line Tools are
installed on this machine, not full Xcode.

## Decision

Write the whole thing in Rust. The InputMethodKit shell subclasses
`IMKInputController` via `objc2::define_class!`, using the
`objc2-input-method-kit` bindings. No Swift, no `cbindgen`, no C ABI to maintain.

## Consequences

- One language, one crate graph, matching the established `marau` pattern.
- No `.xcodeproj` — the app bundle is assembled by `scripts/build-app.sh`, which is
  more reproducible in CI anyway.
- **Verified:** the subclass compiles and links against `InputMethodKit.framework`.
  This was the architecture's biggest unknown and it is resolved.
- Novel territory — no known prior Rust IMK input method. The rendering layer
  (event decode, `insertText`/`setMarkedText`) is conventional objc2 but can only
  be verified by installing and typing, not by unit tests.

## Alternatives rejected

- **Rust core + Swift shell** (original plan): closer to `xkey`/OpenKey, but adds a
  second language and a hand-maintained FFI boundary `marau` shows is unnecessary,
  and needs full Xcode which is not installed.
