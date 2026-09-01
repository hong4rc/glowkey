# 0002 — CGEventTap wrap (EVKey model), replacing the InputMethodKit shell

## Status

Accepted (2026-09-01). Supersedes the InputMethodKit shell from decision 0001's
implementation (the all-Rust-via-objc2 choice itself still holds).

## Context

The InputMethodKit shell made GlowKey a separate input source the user switches
to. When selected, it would not respect the user's Colemak layout without extra
mapping, and it is not the "wrap on top of my layout" model the user wanted.

The user explicitly asked for EVKey's behavior: keep the Colemak (or US) layout
active, and add Vietnamese on top, toggled — no input-source switching. They chose
this over the InputMethodKit approach with full knowledge of the trade-offs.

## Decision

Replace the InputMethodKit shell with a **CGEventTap background agent**, the same
architecture EVKey and OpenKey use. GlowKey installs a session-level event tap,
sees each key *after* the active layout maps it (so Colemak/US is honored
automatically — "whatever layout is active"), and when the engine transforms it
suppresses the original keystroke and re-emits the result by posting backspaces
plus the Vietnamese text. Synthesized events are tagged so the tap ignores its own
output.

The engine crate is unchanged — its `(backspaces, insert)` diff is exactly what an
event-tap shell needs. All engine tests still pass.

## Consequences

- **Colemak/US "just works"** — the tap reads the already-mapped character, so
  GlowKey follows whatever system layout is active with no per-layout code.
- **No input-source switching** — GlowKey is a background agent, not an input
  method. It installs nowhere special; the user runs it and grants Accessibility.
- **Requires an Accessibility permission** (prompted on first run).
- **Does not work in secure/password fields** — macOS withholds those events from
  all event taps. This is the exact limitation EVKey has, which the user accepts.
- The InputMethodKit advantages GlowKey was earlier differentiated on (no prompt,
  works in password fields) are **given up** by this choice. That was the user's
  call, made explicitly.
- Verified to compile and link (CoreGraphics/CoreFoundation via objc2). The tap
  behavior itself can only be verified by granting Accessibility and typing.

## Not yet built

- VN/EN toggle hotkey (the user wants toggling). Deferred — can be detected in the
  tap itself or via a hotkey API. Default is Vietnamese-on.
- Menu bar item and ignore-list editor UI.
