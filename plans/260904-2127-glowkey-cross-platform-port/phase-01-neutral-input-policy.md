---
phase: 1
title: "Neutral input policy crate"
status: complete
priority: P1
effort: "3d"
dependencies: []
---

# Phase 1: Neutral input policy crate

## Overview

Lift the decision ladder out of `app/src/tap/decide.rs` and the key predicates out of
`app/src/tap/keys.rs` into a new platform-free crate, `crates/glowkey-input`, so that
every platform runs the *same* policy rather than three re-implementations of it.

## Requirements

- Functional: the ladder's behaviour is bit-identical to today's. This phase changes
  no observable behaviour on macOS.
- Functional: the crate compiles and tests on Linux and Windows targets.
- Non-functional: no OS crate, no `cfg(target_os)`, no `unsafe` — enforced by CI the
  same way the engine is.
- Non-functional: no allocation on the hot path beyond what `decide` already does.

## Architecture

Three types in, one out.

```rust
/// What the platform saw. Character and identity are separate because a key can
/// have an identity and no character (Backspace) or a character and no special
/// identity (a letter).
pub struct KeyEvent {
    pub ch: Option<char>,
    pub key: Key,
    pub mods: Modifiers,
}

/// Only the identities the ladder actually branches on. Deliberately not a full
/// keyboard enum — see the risk note.
pub enum Key {
    Backspace, ForwardDelete, Escape, Space, Return, Tab,
    CaretMove,          // arrows, Home/End, PageUp/PageDown — one class, as today
    Letter(char),       // the layout-produced character
    Other,
}

pub struct Modifiers { pub control: bool, pub shift: bool, pub option: bool, pub command: bool }
```

`Decision` moves across unchanged — all five variants, including
`EmitThenReplayKey`, which is load-bearing for the boundary replay.

The ladder becomes:

```rust
pub fn decide(session: &mut Session, event: KeyEvent, ctx: &Ctx) -> Decision
```

`Ctx` carries what the platform must supply but the policy must not fetch itself:
whether a hotkey recording is armed, and the frontmost application identity. It is
*data*, not a trait object — the policy must stay callable from a test with no OS.

**The ordering is the specification.** `decide.rs`'s current order is not incidental;
each step is a fixed bug:

1. hotkeys **before** the shortcut filter — a flush destroys the memory ⌃⇧W needs
2. the shortcut filter flushes and passes through
3. the Backspace ladder, five cases, exhaustive with no catch-all arm
4. caret moves flush
5. word-character vs boundary, with every letter suppressed and re-emitted
6. the boundary key replayed after a restore, never passed through natively

Port it as a unit and pin the order with tests before touching macOS.

## Related Code Files

- Create: `crates/glowkey-input/Cargo.toml`, `src/lib.rs`, `src/event.rs`,
  `src/decision.rs`, `src/ladder.rs`, `src/hotkey.rs`
- Create: `crates/glowkey-input/tests/ladder.rs`
- Read (do not yet modify): `app/src/tap/decide.rs`, `app/src/tap/keys.rs`
- Modify: `.github/workflows/ci.yml` — add `-p glowkey-input` to the Linux job

## Implementation Steps

1. Create the crate. Dependencies: `glowkey-engine` only.
2. Define `KeyEvent`, `Key`, `Modifiers`, and move `Decision` over verbatim.
3. Move the pure predicates from `keys.rs`: `is_caret_move`, `is_shortcut`,
   `is_ctrl_shift`, `is_toggle_hotkey`, `is_app_toggle_hotkey`,
   `is_correction_hotkey` — rewritten over `Key`/`Modifiers` instead of macOS
   virtual key codes and `CGEventFlags`.
4. Move `TapState::decide`'s body into `ladder::decide`, substituting the neutral
   types. Keep every comment: they record why each step is where it is.
5. Port the 34 `CGEvent`-driven tests in `app/src/tap/tests.rs` that exercise
   *policy* rather than *plumbing* into `crates/glowkey-input/tests/`, driving
   `KeyEvent` directly. `type_with_deletes` becomes platform-free and gets a lot
   more useful.
6. Extend CI's Linux job to cover the new crate with `-D warnings`.

## Success Criteria

- [x] `cargo test -p glowkey-input` green on macOS
- [x] `cargo check -p glowkey-input --target x86_64-pc-windows-msvc` green
- [x] `cargo check -p glowkey-input --target x86_64-unknown-linux-gnu` green
- [x] The five-case Backspace ladder is pinned by tests naming each case
- [x] `hoongf, ⌫⌫z → hông` and `hoongf vieet s⌫⌫⌫⌫⌫⌫⌫z → hồngz` both pass here,
      not only at the tap
- [x] No `cfg(target_os)` anywhere in the crate

## Risk Assessment

**The ladder is ported by hand and the ordering is invisible in a diff.** A
reordering that looks harmless reintroduces a shipped bug. *Signal:* a ported test
fails, or worse, passes while the tap-level equivalent fails after Phase 3.
*Response:* port the tests first, watch them fail against an empty ladder, then fill
it in. Do not write the ladder and the tests in the same pass.

**`Key` is a judgement call about how much identity to model.** Too little and the
Windows adapter cannot express what it saw; too much and it becomes a speculative
universal keyboard enum, which the request explicitly forbids. *Signal:* Phase 4
finds itself adding variants for Windows. *Response:* adding a variant then is
correct and cheap; guessing now is not. Start from exactly what `decide.rs` branches
on today.

**`Ctx` could become a god-object.** *Signal:* it grows a field per phase.
*Response:* anything the policy needs that is not a plain value is a sign the
boundary is wrong — push it back to the platform.
