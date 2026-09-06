---
phase: 3
title: "Correctness: the Chromium guard, the dead hook, the blocked callback"
status: in-progress
priority: P1
effort: "3-4d, ~1d of it hardware-bound"
dependencies: [1]
---

# Phase 3 — Correctness: the Chromium guard, the dead hook, the blocked callback

**Depends on:** phase 1 (done).
**Owns:** `platform/windows/hook.rs`, `foreground.rs`, `mouse.rs` — this phase
must land **before** phase 4, which also edits `hook.rs`.
**Updated 2026-09-06** after a live typing session on Windows. What changed:
3.1's interim is verified working, 3.5 is new, and 3.6 records a rule that was
questioned and is being kept.

## Overview

Three known-wrong behaviours and one diagnostic blind spot. The blind spot goes
first: on 2026-09-06 a user report (`hoongfa` ␣ `ss` ⌫⌫⌫ not restoring) could not
be diagnosed at all, because the most likely cause — a mouse click flushing the
session — writes nothing to the log. Everything after it is easier to verify once
that is fixed.

## 3.1 — The Windows Chromium guard [A1, P0] — interim DONE, real fix open

`needs_omnibox_guard(backspaces, app)` (`inject.rs`) returned true for *any*
Chromium app with `backspaces > 0`, and the caller posted a `VK_DELETE` with no
check that a selection existed. macOS asks accessibility first (`emit.rs:70-86`,
`decisions/0003`); Windows asked nothing. Every Electron app was affected, since
`is_chromium_app` matches Slack, VS Code, Discord and Teams.

**Interim shipped (commit `4d99de4`):** the forward-delete is disabled.
`edit_inputs` was split out of `emit_edit` so the emitted key sequence is
assertable without sending input, and
`inject::tests::a_chromium_edit_sends_no_forward_delete` pins that no Chromium
edit contains a `VK_DELETE`.

**Verified live, 2026-09-06.** The user typed `hoongf` six times in Edge. GlowKey
emitted the correct diff every time (`bs=3 ins=3u`); the first attempt rendered
`hoồng` and the five after it rendered `hồng`. The arithmetic identifies the
mechanism exactly:

| Key | GlowKey emits | Correct result | Omnibox gave |
|---|---|---|---|
| `o` (2nd) | `bs=1` + `ô` | `hô` | `hoô` — backspace absorbed |
| `f` | `bs=3` + `ồng` | `hồng` | `hoồng` |

The third keystroke's backspace was consumed by the omnibox's trailing
inline-autocomplete selection instead of deleting the `o`. **Only the first word
failed**, which is the autocomplete signature — it fires on the first token into
a freshly cleared field (`#17126` is `Ctrl+A`, `#17127` Backspace) and not
afterwards. A page-body defect would fail every time.

So the interim decision holds: the silent page-body deletion is gone, and what
remains is a visible address-bar mis-render.

**Real fix (open).** Fire the forward-delete only when the focused element is
actually the omnibox.

**The design constraint that matters.** `foreground.rs:148-155` installs
`SetWinEventHook(EVENT_SYSTEM_FOREGROUND, …, WINEVENT_OUTOFCONTEXT)`, which
delivers its callback **on our own thread, through the same message loop the
keyboard hook is served by**. A UIA call there is cross-process COM and can
block, and a blocked message loop means `LowLevelHooksTimeout` removes the
keyboard hook. That is `decisions/0008` re-made on Windows, with a shorter fuse
and no recovery.

**Therefore:** the WinEvent callback must do nothing but hand the `HWND` to a
**worker thread**, which resolves "is this the omnibox" via UIA and stores the
answer in an atomic the injection path reads. The keystroke path reads a cached
`bool` and never calls UIA. Anything simpler than this is the freeze.

**Steps:**
1. Widen the WinEvent registration to include `EVENT_OBJECT_FOCUS`. The existing
   callback filters `id_object != 0`; focus events carry `OBJID_CLIENT`, so that
   filter must become event-aware rather than being dropped.
2. Add a worker thread with a bounded channel — the same shape as
   `hook_log.rs`, which already proves the pattern in this codebase. Full channel
   drops rather than blocks.
3. The worker resolves the focused element and sets
   `FOCUS_IS_OMNIBOX: AtomicBool`.
4. `needs_omnibox_guard` gains that as its second condition and is re-wired into
   `edit_inputs`. Its first condition (Chromium app + backspaces) is already
   written and tested.
5. Clear the flag on every foreground change, so a stale `true` cannot follow the
   user into another app.

**Needs the machine** to learn the omnibox's UIA identity (control type,
automation id, or class chain). Nothing here is knowable from a Mac or from CI.

**Acceptance:** address bar gives `hồng`; a Gmail draft mid-paragraph loses no
character; `EMIT took=` shows no new millisecond-scale cost; no
`LowLevelHooksTimeout`.

## 3.2 — Windows has no hook-liveness check [A3, P0]

`hook.rs:182-186` claimed "the indicator pairs it with a liveness check". No such
check exists — `grep SetTimer|WM_TIMER|thread::spawn` across `platform/windows/`
finds only the log writer and the UI thread. `HOOK` is zeroed only by
`uninstall()`, so `indicator::state` reports `HookGone` only when GlowKey itself
removed the hook (`indicator.rs:74-75`, `shell.rs:47`).

Windows silently removes a low-level hook whose callback is too slow. The tray
then keeps showing **VI**, typing stops transforming, and nothing says why. macOS
has `health.rs` for this (`decisions/0007`); Windows has nothing.

**Done (commit `5ce4c0c`):** the comment now says plainly that no liveness check
exists, instead of describing a guard that does not.

**Open:** a message-loop timer comparing the last callback tick against
`GetLastInputInfo`. Input happened with no callback ⇒ the hook is gone ⇒
`HookGone` ⇒ offer reinstall (`shell::reinstall_hook` already exists).

**Risk:** a false positive causes a reinstall loop. Needs a deliberately-slow
callback build to reproduce a dead hook and validate against. Do not ship the
watchdog on reasoning alone — a watchdog that fires wrongly is worse than none.

**Note:** elevation detection already works and was seen live on 2026-09-06
(`#17308`: `BlockedByElevation`, `REACH … cannot receive injected input`). That
is the *other* reachability failure and it is already honest; this is the gap
next to it.

## 3.3 — The macOS tap callback blocks [B2, P1]

`decisions/0008` forbids file I/O and non-tap-thread locks inside the callback,
with no macOS carve-out. Both happen:

- **Per keystroke:** `dispatch.rs:105` and `emit.rs:97` call `crate::log::log`,
  which at `log.rs` takes a global mutex, `write_all`s, **`flush()`es**, and at
  5 MB renames and reopens — synchronously, on the thread every key on the
  machine waits behind.
- **ARBITER-FOUND, larger:** on hotkey and per-app-toggle keys,
  `dispatch.rs:193,200` performs the **whole settings file write** — copy `.bak`,
  write, rename — inside the callback. `emit.rs:60-66` admits it. Windows already
  defers this to its message loop (`hook.rs:279-282`).

Bounded on macOS because `health.rs` re-enables a timed-out tap, where Windows
would lose the hook permanently — hence P1.

**Fix, already written and misfiled:** `platform/windows/hook_log.rs` is a
bounded channel plus writer thread with **zero Win32** in it. Move it to
`app/src/log_queue.rs` and use it from both shells. Not a literal drop-in:
`hook_log::log` takes `String` where `log::log` takes `&str`, so ~25 macOS call
sites change, and `dispatch.rs:96-101`'s "on disk already before a panicking
emit" comment stops being true and must be rewritten honestly.

Defer the settings write to the run loop, as Windows does.

**Cannot be signed off until phase 5.** Small code, and the verification is a Mac.

## 3.4 — League of Legends [B3, P1]

Unusable while EVKey is fine. The log rules out elevation and refused injection.
Two candidates remain: a lone `w` becoming `ư`, or Vanguard dropping injected
input.

**Run the discriminating test before designing anything.** Press **B** (then Y,
M) in a Practice Tool game with League *not* excluded and Vietnamese *on*, EVKey
confirmed stopped, then a Notepad control run immediately after. The handoff is
explicit: *do not build the `w` option before running it.*

Currently worked around by exclusion — seen live 2026-09-06 (`#17213`,
`#17216`): both `league of legends.exe` and `leagueclientux.exe` hit the ignore
list.

## 3.5 — A flush writes nothing to the log [NEW, P1]

**The diagnostic blind spot, and the reason this is first in the work order.**

On 2026-09-06 a user reported that `hoongfa` ␣ `ss` ⌫⌫⌫ did not restore `hồng`.
The log shows the feature itself working two keystrokes earlier —

```
#17610  a   Emit bs=3 ins=6u    hồng → hoongfa   (spell-check escape)
#17611  ⌫   Emit bs=7 ins=4u    hoongfa → hồng   (unescape) ✓
```

— and then, after a space committed the word and `ss` began a new one, all four
backspaces came back **`Passthrough`** (`#17616-17619`). Passthrough means
nothing was composing, so something flushed between `#17615` and `#17616`.

**Nothing in the log says what.** `mouse.rs:76` flushes the session on every
button press and logs nothing, and a click is invisible to the keyboard hook
entirely. The most likely explanation is therefore an ordinary click — correct,
deliberate behaviour (the blind model must forget when the caret may have moved)
— but it is unprovable, which makes every report of this shape unfalsifiable.

**There are six flush sites and the shell can only see two:**

| Site | Cause | Visible today |
|---|---|---|
| `ladder.rs:117` | a Ctrl/Alt shortcut | no |
| `ladder.rs:170` | mid-word Backspace the engine cannot follow | no |
| `ladder.rs:182` | caret move (arrows/Home/End/Page) | no |
| `mouse.rs:76` | any mouse button press | no |
| `macos/mod.rs:226` | app switch | no |
| `macos/settings.rs:387` | a settings change | no |

**Design.** The three ladder sites are in the platform-free crate, which cannot
log — it reports through the port. So add a `Notice::Flushed { cause: FlushCause }`
to `glowkey-input` and let each shell log it, exactly as `Notice::Decided` is
handled today. The shell-side flushes (mouse, app switch, settings) log directly.
One line, naming the cause:

```
#17616 +15272.173s  FLUSH mouse-button — composing word discarded
```

**Why this is worth doing before the rest:** it converts the whole
"GlowKey stopped mid-word and I don't know why" class from a guess into a
one-line answer, permanently, for both platforms. It is also the cheapest item in
the phase.

**Acceptance:** every flush site emits exactly one line naming its cause; the
`hoongfa`/`ss` sequence is re-run and the cause is read off the log rather than
inferred (3.7).

## 3.6 — `hoongf4` does not revert, and that is deliberate [DECIDED 2026-09-06]

Reported as a bug. It is the documented rule, and the rule is being kept —
written down here so it is not re-litigated.

`engine.rs:293-295`: in **Telex a digit is not a syllable character**, so it ends
the word exactly like a space. By the time `4` arrives, `hồng` has already
committed as valid Vietnamese and auto-fix has nothing to restore. A **letter**
behaves differently because it *extends* the syllable, so the whole word is
re-judged, fails, and escapes to raw — which is why `hoongfc` gives `hoongfc`
while `hoongf4` gives `hồng4`. Both are correct; they differ because the digit is
a boundary and the letter is not.

**Rejected:** treating a digit glued to a syllable as evidence the token is not
Vietnamese. It would break `tầng2`, `quận1`, `phường7` — forms Vietnamese people
type without a space. The cost lands on real Vietnamese text to fix a case that
is better served by the per-app ignore list or ⌃⇧W.

**Action:** none in code. Add the `hoongfc` / `hoongf4` contrast to
`crates/glowkey-engine/tests/telex.rs` so the asymmetry is pinned as intended
behaviour rather than looking like a bug to the next reader.

## 3.7 — Re-verify the `hoongfa` / `ss` sequence [NEW, P2]

After 3.5, re-run the exact user sequence: `hoongfa` ␣ `ss` then Backspace,
**without clicking**, then again **with** a click before the backspaces. Expected:
without a click the backspaces are handled and delete within `ss`; with a click
the log shows `FLUSH mouse-button` and passthrough follows.

If the no-click run *still* passes the backspaces through, the cause is not the
click and this becomes a real re-composition defect — at which point the
committed-history stack (`session.rs:775-788`, where a restored word clears the
stack deliberately) is the place to look.

## Work order

1. **3.5** flush logging — cheapest, unblocks diagnosis of everything else.
2. **3.7** re-verify the user's sequence with the new log line.
3. **3.6** the engine test pinning the digit/letter asymmetry.
4. **3.2** watchdog — first of the `hook.rs` touchers, before phase 4.
5. **3.1** real omnibox guard — the largest, and the only one needing UIA.
6. **3.3** macOS log queue — code now, sign-off waits on phase 5.
7. **3.4** League — a test, not a fix, and it is 30 seconds.

3.5, 3.6 and 3.3 are codeable without the machine. 3.1, 3.2, 3.4 and 3.7 all need
a human at a Windows desktop — and note that **an agent cannot run them**:
`SendInput` fails with `ERROR_ACCESS_DENIED` from a non-interactive window
station, so the keystrokes must come from a person
(`plans/reports/verification-260905-1841-windows-live-check.md`).

## Risk

| Risk | Signal it broke | Response |
|---|---|---|
| UIA in the WinEvent callback blocks the message loop | `LowLevelHooksTimeout`, hook removed, typing dies | Worker thread is mandatory, not an optimisation. If the worker cannot keep up, drop the event and leave the flag false — a missed guard is a mis-render, a blocked loop is a dead hook |
| Watchdog false-positive reinstall loop | Repeated `HOOK reinstall` lines with no user action | Do not ship without the slow-callback repro; require two consecutive misses before firing |
| Flush logging floods the log | Log rotates far faster than before | Flushes are user-gestures, not per-keystroke; if volume surprises, log only flushes that discarded a composing word |
| macOS log-queue port breaks the tap | Unverifiable until phase 5 | Do not merge to a release until a Mac has run it |

## Validation

`cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo clippy --target aarch64-apple-darwin -p glowkey --all-targets`, plus the
hardware checks above. 3.1's interim is verifiable headless; its correctness is
not.
