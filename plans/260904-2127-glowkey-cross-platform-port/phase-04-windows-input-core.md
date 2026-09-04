---
phase: 4
title: "Windows input core"
status: complete
priority: P1
effort: "4d"
dependencies: [0, 3]
---

# Phase 4: Windows input core

Tracked by [issue #1](https://github.com/hong4rc/glowkey/issues/1).

## Overview

The Windows keyboard backend: interception, suppression, Unicode injection,
self-event identification, and foreground-application tracking. This is where the
port either preserves GlowKey's model or loses it.

## What changed since this phase was first written

Phases 1-3 have landed on `main`. The issue that tracks this work was written while
they were still in flight and lists three items as blocked; **all three are now
unblocked** and are in scope here:

| Was blocked on | Now |
|---|---|
| `adapt.rs` — "the target type does not exist yet" | `glowkey_input::{KeyEvent, Key, Modifiers}` exist and are stable. Build the adapter against them. |
| The hook callback body — "`glowkey_input::decide` does not exist yet" | `decide(&mut Session, &KeyEvent, &Ctx, &mut Effects) -> Decision` exists, with `Ctx { toggle_hotkey }`. Write the real body, not `todo!()`. |
| The Windows exclusion table — "that is Phase 2" | Shipped. `crate::exclusion_defaults::DEFAULT_EXCLUSIONS` already returns `windowsterminal.exe`, `conhost.exe`, `pwsh.exe`, `cmd.exe`, `wsl.exe`, `code.exe`, `devenv.exe`, `nvim.exe` and the rest on Windows. Consume it; do not write a second one. |

Also settled by Phase 3 and not to be re-litigated here: `app/src/platform/mod.rs`
exists and already carries the "exactly one backend compiles at a time" comment. Add
one `#[cfg(target_os = "windows")] pub mod windows;` line to it. The old instruction
not to touch that file was a merge-conflict avoidance measure for a parallel session
that has since merged.

`cargo check --workspace` is green on Windows today. That is the floor this phase
must not drop below, not an achievement.

## Requirements

- Functional: full suppression — every handled key is swallowed and re-emitted.
- Functional: synthesized input is never reprocessed.
- Functional: the hook callback makes no call that can block.
- Functional: the frontmost application is resolved from a notification, never from a
  per-keystroke query.
- Non-functional: `cargo check --workspace` stays green on Windows, and
  `cargo check -p glowkey` stays green on Linux and macOS — the `cfg` gating has to be
  right or every other platform breaks silently.
- Non-functional: **behaviour is unproven until Phase 6.** Nothing here may be called
  done on a `cargo check`.

## Architecture

### The mechanism decision: `WH_KEYBOARD_LL`, not TSF

Write this up as `docs/decisions/0009-windows-low-level-hook.md`, arguing it rather
than asserting it.

| | Low-level hook | TSF |
|---|---|---|
| Matches today's model | **Yes** — intercept, suppress, inject | No — it is a composition model |
| Layout-agnostic wrapping | Yes | No — it is an input source the user switches to |
| Elevated windows | **No** (UIPI) | Yes |
| Store/UWP apps | Partial | Yes |
| Blocking risk | Real — `LowLevelHooksTimeout` | Lower |
| Reuses the decision ladder's invariants | **Entirely** | Largely discards the delivery half |

The blind diff model, `backspace_visible_char`'s "land on what the screen will show",
and the full-suppression race fix all exist **because there is no marked text**. TSF
would keep the transformation logic and throw away the delivery logic this codebase
has spent its whole life debugging. Choose the hook; record the elevated-window gap as
a known limitation with honest detection (Phase 5).

The decision record should argue the cost as well as the benefit. UIPI means a
non-elevated hook cannot inject into an elevated window, and the answer is to detect
and display that, not to request elevation. An input method asking for administrator
rights is a red flag, correctly. Say so in the record so nobody re-opens it as an
oversight.

### Self-event identification

`SendInput`'s `dwExtraInfo` is the analogue of the tagged `CGEventSource`. Pick a
constant magic value, set it on every injected event, and make the hook's first act be
to check for it and pass through.

```rust
const GLOWKEY_INJECTED: usize = 0x_47_4C_4F_57; // "GLOW"
```

This is the feedback-loop guard. **Without it the hook reprocesses its own injection
and the app melts down** — the same failure the macOS source tag prevents. If this
does not work, nothing else in the phase is worth testing.

Structure it so the guard is a pure function over the `dwExtraInfo` value, separate
from the callback that calls it, because that is the only part of this phase that can
be unit-tested without Windows and the issue's definition of done requires such a test.

### The callback body

Now that `decide` exists, the callback is a translator and nothing else, matching what
Phase 3 did to the macOS tap:

```text
KBDLLHOOKSTRUCT ──adapt──▶ KeyEvent ──decide──▶ Decision ──inject──▶ SendInput
                                          └────▶ Effects ──▶ queued, not run here
```

`Effects` is plain data on purpose. Anything in it that touches disk — `save_settings`,
`personal_words_changed` — is exactly the kind of work `decisions/0008` forbids inside
the callback. Hand it to another thread and return. Read that decision record properly
before writing this function; it is the rule you are most likely to break.

### Injection

`SendInput` with `KEYEVENTF_UNICODE`, one call per batch so the array is delivered in
order. The engine's `backspaces` count is in **UTF-16 code units**, which is already
`SendInput`'s unit — a lucky alignment worth a comment so nobody "fixes" it to `char`s.

Backspaces are `VK_BACK` key events; inserted text is `KEYEVENTF_UNICODE` with the
UTF-16 units, surrogate pairs sent as two entries.

### Foreground application

`SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` for the notification, and
`GetForegroundWindow` + `GetWindowThreadProcessId` + `QueryFullProcessImageNameW` to
resolve it, lowercased to match the shipped table's spelling. **The notification path
is not optional** — `decisions/0008` exists because a per-keystroke foreground query
froze a Mac, and the identical mistake is available here.

Feed the resolved name to `Session::set_frontmost_app`, the same entry point macOS
uses. The exclusion comparison itself is already written and already platform-neutral.

## Related Code Files

- Create: `app/src/platform/windows/mod.rs`, `hook.rs`, `inject.rs`, `adapt.rs`,
  `foreground.rs`, `elevation.rs`
- Create: `docs/decisions/0009-windows-low-level-hook.md`
- Modify: `app/Cargo.toml` — `[target.'cfg(target_os = "windows")'.dependencies]`
  `windows-sys` with only the needed feature groups
- Modify: `app/src/platform/mod.rs` — one gated `pub mod windows;`
- Modify: `app/src/main.rs` — the Windows entry point; keep the non-macOS stub honest
- Do not create: a second Windows exclusion list. It exists.

## Implementation Steps

1. Add `windows-sys` under a Windows-only target table. Keep the feature list minimal
   and explicit — this is a keystroke-observing process and its dependency surface is
   part of its privacy claim.
2. `hook.rs`: install `WH_KEYBOARD_LL` on a thread with a message loop. First
   statement of the callback: check `dwExtraInfo` against the tag and return
   `CallNextHookEx`.
3. `adapt.rs`: `KBDLLHOOKSTRUCT` → `glowkey_input::KeyEvent`. Resolve the character
   with `ToUnicodeEx` against the foreground thread's layout — **without disturbing
   dead-key state**, which is the classic trap here. Map `Key::Backspace`,
   `Key::CaretMove` (arrows, Home/End, Page keys) and the modifier set the ladder
   expects; the ladder's behaviour depends on these being right.
4. `inject.rs`: `Decision` → `SendInput` batches, tag set, ordering preserved.
5. Wire the callback: adapt, `decide`, carry out the `Decision`, hand `Effects` off.
6. `foreground.rs`: the WinEvent hook plus a one-time bootstrap query, mirroring what
   `decisions/0008` settled on for macOS.
7. `elevation.rs`: detect that the foreground window belongs to a higher integrity
   level than us, so Phase 5 can show an honest indicator instead of silence.
8. Measure the callback. Windows kills a slow hook via `LowLevelHooksTimeout` the way
   macOS disabled the tap; add a timing log matching the existing `EMIT took=` shape.
9. Verify the gating from the other side: `cargo check -p glowkey` must still pass on
   Linux, which is what CI's `engine` job runs.

## Success Criteria

- [ ] `cargo check --workspace` green on Windows; `cargo check -p glowkey` green on
      Linux and macOS
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` silent on Windows
- [ ] `decisions/0009` written, with the TSF trade-off argued rather than asserted,
      and the elevation stance stated as a decision rather than a gap
- [ ] The tag guard has a unit test that does not need Windows (a pure function over
      `dwExtraInfo`)
- [ ] `adapt.rs`'s keycode table has a test per mapped key
- [ ] The callback calls `glowkey_input::decide` — no second reading of the ladder,
      no reimplemented policy, no `todo!()` left behind
- [ ] No allocation, no lock, no syscall that can block, in the callback path
- [ ] Timing instrumentation present, matching the macOS `EMIT took=` shape
- [ ] The exclusion path consumes `exclusion_defaults` and adds no table of its own

## Risk Assessment

**`ToUnicodeEx` mutates keyboard state.** Called naively it corrupts dead-key
sequences, which matters for users typing other languages. *Signal:* dead keys stop
composing in Phase 6. *Response:* use the documented state-preserving call pattern and
test with a layout that has dead keys.

**`SendInput` ordering relative to native input is assumed, not known.** The whole
full-suppression model exists because macOS raced. Whether Windows races the same way
— especially in Chrome's multiprocess renderer path — must be **measured in Phase 6**,
not assumed. *Signal:* transposed characters, `hoongf` → `hoồng`. *Response:* the fix
already exists conceptually (suppress everything, one ordered queue); if Windows needs
a different one, that is a Phase 6 finding and a decision record.

**A blocking hook callback freezes Windows input**, exactly as `decisions/0008`
describes for macOS. *Signal:* the hook stops being called; Windows silently unhooks
it. *Response:* the rule is already written — carry it over verbatim, and add the
timeout detection *before* the first real typing test, not after.

**UIPI is a silent-failure class with no macOS analogue.** Typing into an elevated
window will simply do nothing. *Signal:* it works everywhere except Task Manager,
regedit, elevated terminals. *Response:* detect and report (Phase 5). Do not pretend.

**Now that the callback body is unblocked, the temptation is to re-read the ladder
rather than call it.** A Windows-shaped copy of the decision sequence would compile,
pass a smoke test, and quietly lose the ordering that five fixed bugs are encoded in.
*Signal:* any `match` on key kind inside `hook.rs`, any Windows-side notion of
"boundary key". *Response:* the callback translates and dispatches. Every question
about *what to do* belongs to `glowkey-input`, and if the ladder cannot answer one,
that is a change to the ladder, tested on Linux, not a Windows special case.
