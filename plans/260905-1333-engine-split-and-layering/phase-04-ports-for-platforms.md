---
phase: 4
title: "Ports for platforms"
status: pending
priority: P2
effort: "4h"
dependencies: [3]
---

# Phase 4: Ports for platforms

## Overview
Name the port a shell must implement. The ladder already returns a `Decision`
and `Effects`; both shells carry them out in their own code. A `Platform` trait
in `glowkey-input` makes that contract explicit, so a third shell (Linux, or
someone else's) knows exactly what to write.

## Requirements
- Functional: no behaviour change on macOS or Windows; the two existing
  effect-carrying functions become the trait's two implementations.
- Non-functional: the trait is small: only what both shells already do.

## Architecture
```rust
// crates/glowkey-input/src/platform.rs
pub trait Platform {
    /// Replace `backspaces` characters before the caret with `text`.
    fn inject(&mut self, backspaces: usize, text: &str);
    /// Let the original key through untouched.
    fn pass_through(&mut self);
    /// The application in front, if known.
    fn app_in_front(&self) -> Option<AppId>;
    /// Something must reach the settings file soon (never on the keystroke path).
    fn request_save(&mut self);
    /// The indicator (tray / menu bar) should repaint.
    fn request_indicator(&mut self);
}
pub fn handle(session: &mut Session, event: &KeyEvent, ctx: &Ctx, platform: &mut impl Platform) -> Decision;
```
- `handle` calls `decide` and then applies `Decision`/`Effects` through the
  trait: `Emit` → `inject`, `Passthrough` → `pass_through`, `Effects.save` →
  `request_save`, `Effects.refresh` → `request_indicator`.
- macOS: `platform/macos/dispatch.rs` implements `Platform` on its existing
  emit/posting state. Windows: `platform/windows/hook.rs` `HookState`
  implements it with `inject::send`, the pending flags and `wake()`.
- `decide` stays public for tests that assert on the decision without a
  platform; `handle` is the one a shell calls.

## Related Code Files
- Create: `crates/glowkey-input/src/platform.rs`
- Modify: `crates/glowkey-input/src/lib.rs`, `app/src/platform/macos/dispatch.rs`,
  `app/src/platform/windows/hook.rs`, `docs/handoff.md` §3 (architecture)

## Implementation Steps
1. Trait and `handle`; a `RecordingPlatform` test double in the crate's tests
   asserting `handle` maps each `Decision`/`Effects` to the right calls.
2. Windows adapter: `HookState` implements `Platform`; `carry_out_effects`
   collapses into it. Run the Windows suite.
3. macOS adapter: `dispatch.rs`; `cargo check --target aarch64-apple-darwin`.
4. Decision record `docs/decisions/0012-engine-layering-and-ports.md`
   (covers phases 1–4: the layers, the patterns adopted and rejected).
5. Gates.

## Success Criteria
- [ ] `Platform` has exactly the five methods above (or fewer).
- [ ] `RecordingPlatform` test covers every `Decision` variant.
- [ ] Both shells compile against `handle`; Windows tests green; macOS check green.
- [ ] Decision 0012 written.

## Risk Assessment
- The Windows hook callback must not block (`decisions/0008`); `Platform`
  methods on Windows only set flags and call `SendInput`, as today. The trait
  must not tempt anyone to do I/O in `request_save`; its doc says so.
- If the two shells' effect handling differs in a way the trait cannot
  express, that difference is a bug or a platform fact; record which.
