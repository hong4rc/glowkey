---
phase: 4
title: "Windows input core"
status: pending
priority: P1
effort: "4d"
dependencies: [3]
---

# Phase 4: Windows input core

## Overview

The Windows keyboard backend: interception, suppression, Unicode injection,
self-event identification, and foreground-application tracking. This is where the
port either preserves GlowKey's model or loses it.

## Requirements

- Functional: full suppression — every handled key is swallowed and re-emitted.
- Functional: synthesized input is never reprocessed.
- Functional: the hook callback makes no call that can block.
- Non-functional: `cargo check --target x86_64-pc-windows-msvc` green. **Behaviour
  is unproven until Phase 6.**

## Architecture

### The mechanism decision: `WH_KEYBOARD_LL`, not TSF

Write this up as `docs/decisions/0009-windows-low-level-hook.md`.

| | Low-level hook | TSF |
|---|---|---|
| Matches today's model | **Yes** — intercept, suppress, inject | No — it is a composition model |
| Layout-agnostic wrapping | Yes | No — it is an input source the user switches to |
| Elevated windows | **No** (UIPI) | Yes |
| Store/UWP apps | Partial | Yes |
| Blocking risk | Real — `LowLevelHooksTimeout` | Lower |
| Reuses `decide.rs`'s invariants | **Entirely** | Largely discards the delivery half |

The blind diff model, `backspace_visible_char`'s "land on what the screen will
show", and the full-suppression race fix all exist **because there is no marked
text**. TSF would keep the transformation logic and throw away the delivery logic
that this codebase has spent its whole life debugging. Choose the hook; record the
elevated-window gap as a known limitation with honest detection (Phase 5).

### Self-event identification

`SendInput`'s `dwExtraInfo` is the analogue of the tagged `CGEventSource`. Pick a
constant magic value, set it on every injected event, and make the hook's first act
be to check for it and pass through.

```rust
const GLOWKEY_INJECTED: usize = 0x_47_4C_4F_57; // "GLOW"
```

This is the feedback-loop guard. **Without it the hook reprocesses its own
injection and the app melts down** — the same failure the macOS source tag prevents.

### Injection

`SendInput` with `KEYEVENTF_UNICODE`, one call per batch so the array is delivered
in order. The engine's `backspaces` count is in **UTF-16 code units**, which is
already `SendInput`'s unit — a lucky alignment worth a comment so nobody "fixes" it
to `char`s.

Backspaces are `VK_BACK` key events; inserted text is `KEYEVENTF_UNICODE` with the
UTF-16 units, surrogate pairs sent as two entries.

### Foreground application

`SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` for the notification, and
`GetForegroundWindow` + `GetWindowThreadProcessId` + `QueryFullProcessImageNameW`
to resolve it. **The notification path is not optional** — `decisions/0008` exists
because a per-keystroke foreground query froze a Mac, and the identical mistake is
available here.

## Related Code Files

- Create: `app/src/platform/windows/mod.rs`, `hook.rs`, `inject.rs`, `adapt.rs`,
  `foreground.rs`, `elevation.rs`
- Create: `docs/decisions/0009-windows-low-level-hook.md`
- Modify: `app/Cargo.toml` — `[target.'cfg(target_os = "windows")'.dependencies]`
  `windows-sys` with only the needed feature groups
- Modify: `app/src/platform/mod.rs`

## Implementation Steps

1. Add `windows-sys` under a Windows-only target table. Keep the feature list
   minimal and explicit.
2. `hook.rs`: install `WH_KEYBOARD_LL` on a thread with a message loop. First act of
   the callback: check `dwExtraInfo` against the tag and return `CallNextHookEx`.
3. `adapt.rs`: `KBDLLHOOKSTRUCT` → `KeyEvent`. Resolve the character with
   `ToUnicodeEx` against the foreground thread's layout — **without disturbing dead
   key state**, which is the classic trap here.
4. `inject.rs`: `Decision` → `SendInput` batches, tag set, ordering preserved.
5. `foreground.rs`: the WinEvent hook plus a one-time bootstrap query, mirroring
   what `decisions/0008` settled on for macOS.
6. `elevation.rs`: detect that the foreground window belongs to a higher integrity
   level than us, so Phase 5 can show an honest indicator instead of silence.
7. Measure the callback. Windows kills a slow hook via `LowLevelHooksTimeout` the
   way macOS disabled the tap; add the same style of timing log.

## Success Criteria

- [ ] `cargo check --target x86_64-pc-windows-msvc` green for the workspace
- [ ] `decisions/0009` written, with the TSF trade-off argued rather than asserted
- [ ] The tag guard has a unit test that does not need Windows (pure function over
      `dwExtraInfo`)
- [ ] `adapt.rs`'s keycode table has a test per mapped key
- [ ] No allocation, no lock, no syscall that can block, in the callback path
- [ ] Timing instrumentation present, matching the macOS `EMIT took=` shape

## Risk Assessment

**`ToUnicodeEx` mutates keyboard state.** Called naively it corrupts dead-key
sequences, which matters for users typing other languages. *Signal:* dead keys stop
composing in Phase 6. *Response:* use the documented state-preserving call pattern
and test with a layout that has dead keys.

**`SendInput` ordering relative to native input is assumed, not known.** The whole
full-suppression model exists because macOS raced. Whether Windows races the same
way — especially in Chrome's multiprocess renderer path — must be **measured in
Phase 6**, not assumed. *Signal:* transposed characters, `hoongf` → `hoồng`.
*Response:* the fix already exists conceptually (suppress everything, one ordered
queue); if Windows needs a different one, that is a Phase 6 finding and a decision
record.

**A blocking hook callback freezes Windows input**, exactly as `decisions/0008`
describes for macOS. *Signal:* the hook stops being called; Windows silently
unhooks it. *Response:* the rule is already written — carry it over verbatim, and
add the timeout detection *before* the first real typing test, not after.

**UIPI is a silent-failure class with no macOS analogue.** Typing into an elevated
window will simply do nothing. *Signal:* it works everywhere except Task Manager,
regedit, elevated terminals. *Response:* detect and report (Phase 5). Do not pretend.
