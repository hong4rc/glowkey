# Phase 3 — Correctness: the Chromium guard, the dead hook, the blocked callback

**Depends on:** phase 1.
**Gated by:** decision A1 (interim behaviour of the guard); hardware for the real
fixes.
**Owns:** `platform/windows/hook.rs` — this phase must land **before** phase 4,
which also edits it.

## 3.1 — The Windows Chromium guard eats real text [A1, P0]

`needs_omnibox_guard(backspaces, app)` (`inject.rs:220-222`) returns true for
*any* Chromium app with `backspaces > 0`, and the caller then posts a
`VK_DELETE`. There is no check that a selection exists.

macOS does this properly: `emit.rs:70-86` asks accessibility whether the focused
element is an `AXTextField` with non-empty `AXSelectedText` first, and only then
clears the inline-autocomplete selection. Windows asks nothing and deletes
unconditionally.

The code's own test comment concedes the failure: *"firing it anywhere else
spends a forward-delete on a field that never had a selection, which in a normal
editor deletes a real character."* The excuse at `inject.rs:96-99` — that
reaching that state requires moving the caret without flushing — is wrong: a
mouse click flushes *and* leaves the caret mid-field, and the next
backspace-bearing edit fires into real text.

**This affects every Electron app**, since `is_chromium_app` matches them:
Slack, VS Code, Discord, Teams.

The `crossplat` audit listed this as a deliberate non-gap "user-confirmed working
in Edge". The arbiter **refuted** that: `windows-verification:231` confirms the
omnibox case only. Nobody has typed mid-paragraph.

**Interim (decision A1):** recommended — disable the unconditional guard,
accepting the omnibox `hoồng` regression, until detection exists.

**Real fix (needs hardware):** cache focus state off the hot path by extending
the existing WinEvent hook in `foreground.rs` with `EVENT_OBJECT_FOCUS`, and fire
the guard only for the omnibox. Requires a machine to learn the omnibox's UIA
identity. Must not add a synchronous UIA call to the keystroke path — that is the
macOS §6.9 freeze, re-made on Windows.

**First test to run, before any code:** type Vietnamese mid-paragraph in a Gmail
draft in Chrome and watch whether the character right of the caret disappears.
Thirty seconds; decides the interim.

## 3.2 — Windows has no hook-liveness check at all [A3, P0]

`hook.rs:182-184` claims "the indicator pairs it with a liveness check." No such
check exists. `grep SetTimer|WM_TIMER|thread::spawn` across `platform/windows/`
finds only the log writer and the UI thread. `HOOK` is zeroed only by
`uninstall()` (`:137`), so `indicator::state` reports `HookGone` only when
GlowKey itself removed the hook (`indicator.rs:74-75`, `shell.rs:47`).

Windows silently removes a low-level hook whose callback is too slow. When that
happens the tray keeps showing **VI**, typing stops transforming, and nothing
anywhere says why. macOS has `health.rs` for exactly this (decision 0007);
Windows has nothing.

**Now:** fix the comment so it stops asserting a feature that does not exist.

**Then (needs a dead-hook repro):** a message-loop timer comparing the last
callback tick against `GetLastInputInfo` — input happened but no callback means
the hook is gone → `HookGone` → offer reinstall. The risk is a false positive
causing a reinstall loop, which is why it needs a deliberately-slow-callback
build to validate against.

## 3.3 — The macOS tap callback blocks [B2, P1]

`decisions/0008` forbids file I/O and non-tap-thread locks inside the callback,
without a macOS carve-out. Both happen:

- **Per keystroke:** `dispatch.rs:105` and `emit.rs:97` call `crate::log::log`,
  which at `log.rs:145-155` takes a global mutex, `write_all`s, **`flush()`es**,
  and at 5 MB does a rename-and-reopen — all synchronous, on the thread every
  key on the machine is waiting behind.
- **ARBITER-FOUND, and larger:** on hotkey and per-app-toggle keys,
  `dispatch.rs:193,200` performs the **whole settings file write** —
  copy `.bak`, write, rename — inside the callback. `emit.rs:60-66` admits it.
  Windows already defers this to its message loop (`hook.rs:279-282`).

This is the exact shape of the §6.9 system-wide freeze: not slow work, but work
that waits on somebody else while holding up every keystroke.

Consequence is bounded on macOS — `health.rs` re-enables a timed-out tap, where
Windows would lose the hook permanently — which is why it is P1, not P0.

**Fix, already written and misfiled:** `platform/windows/hook_log.rs` is a
bounded channel plus writer thread containing **zero Win32**. Move it to
`app/src/log_queue.rs` and use it from both shells. Not literally a drop-in:
`hook_log::log` takes `String` where `log::log` takes `&str`, so ~25 macOS call
sites change, and `dispatch.rs:96-101`'s "on disk already before a panicking
emit" comment stops being true and must be rewritten honestly.

Defer the settings write to the run loop, as Windows does.

**Cannot be signed off until phase 5.** The code is small; the verification is a
Mac.

## 3.4 — League of Legends [B3, P1]

Unusable while EVKey is fine. The log rules out elevation and refused injection.
Two candidates remain: a lone `w` becoming `ư`, or Vanguard dropping injected
input.

**Run the discriminating test before designing anything.** Press **B** (then Y,
M) in a Practice Tool game with League *not* excluded and Vietnamese *on*, with
EVKey confirmed stopped, then a Notepad control run immediately after. That turns
inference into proof. The handoff is explicit: *do not build the `w` option
before running it.*

## Validation

`cargo test --workspace`, `cargo clippy -- -D warnings`, plus the two hardware
tests named above. 3.1's interim is verifiable headless (a unit test that the
guard no longer fires); its correctness is not.
