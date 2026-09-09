# Windows live check, 2026-09-05 18:41

Attempted while the user was away, on their machine, against
`hardening/phase-1-safety-fixes` @ `5ce4c0c`. No other input method was running
(checked first — the 2026-09-05 round was contaminated by EVKey).

## Result in one line

**The automated Tier 1/2 typing checks could not be run by an agent, for an
environment reason that will not go away.** Everything provable without
synthesizing keystrokes was proved, and the user's own typing — already in the
log — is good evidence on the central question.

## What was established

### GlowKey is healthy at runtime

From the log of the user's real typing (their keystrokes, not synthetic):

- `HOOK installed WH_KEYBOARD_LL`, `MOUSE installed WH_MOUSE_LL`,
  `HOOK first callback received (code=0) — the hook is live`.
- **The transformation is correct.** `hoongf` in Edge, keys `h o o n g f`:

  | Line | Key | Emit | Screen |
  |---|---|---|---|
  | `#51` | h | `bs=0 ins=1` | `h` |
  | `#52` | o | `bs=0 ins=1` | `ho` |
  | `#53` | o | `bs=1 ins=2` | `hô` |
  | `#54` | n | `bs=0 ins=1` | `hôn` |
  | `#55` | g | `bs=0 ins=1` | `hông` |
  | `#56` | f | `bs=3 ins=5` | `hồng` |

  The last step is the one to read: screen `hông`, common prefix `h`, so three
  code units go and `ồng` comes back. **That is the correct diff.** The engine
  suite pins the same word independently
  (`crates/glowkey-engine/tests/telex.rs`).
- **The per-app ignore list works.** `INDICATOR ExcludedApp — off in
  windowsterminal.exe (ignore list)` and every key there `Passthrough`.
- **Foreground tracking works**, by notification, with no per-keystroke query.
- **No `PANIC`, no `RUNAWAY`, no injection refusal, no `TAP disabled`** anywhere
  in 54,660 lines.

### The `hoồng` the user reported is host-side

GlowKey emitted `bs=3` + `ồng` (line `#56` above) and Edge's address bar rendered
`hoồng`. The edit was right; the host applied it wrong, because the omnibox holds
a trailing inline-autocomplete selection and the first Backspace deletes that
instead of a character. This is the documented Edge defect, and it is the
regression accepted when the unconditional forward-delete was disabled.

Still unconfirmed by the user: whether the `hoồng` they saw was in the **address
bar** (expected) or a **page body** (would mean the fix failed). Nothing in the
log distinguishes them — `app=msedge.exe` either way.

### A tray failure that reports itself

The harness-spawned GlowKeys logged `TRAY FAILED to add the notify icon`
**followed by** `STARTUP no tray icon — the indicator will not be visible`. That
is the right behaviour and worth recording as a pass: a global keyboard hook with
no visible affordance is exactly the silent-failure class `decisions/0007`
exists for, and it announces itself rather than looking dead.

## What could not be established, and why

**`SendInput` failed with `ERROR_ACCESS_DENIED` (5).** The conclusion drawn
from that — that an agent cannot synthesize keystrokes here — was **wrong, and is
corrected below.**

**Correction, 2026-09-06.** `SendInput` works from an agent-spawned process:
`sent=2 err=0` with an ordinary window in the foreground. The original failure
was **UIPI**, not the window station — an elevated Windows Terminal was the
foreground window, and Windows blocks injection into a higher-integrity
foreground from any ordinary process. One observation was generalised into a
platform limit without testing the alternative explanation, and it was written
into this report and the handoff before anything checked it.

What is true: taking the foreground from a background process needs
`AttachThreadInput` (`SetForegroundWindow` alone is refused), and focusing the
Chromium address bar is unreliable while a page holds the keyboard.
`scripts/probe-sendinput.ps1` now reports the foreground window and its elevation
alongside the result, so the same inference is harder to repeat.

**`verify-windows-isolated.ps1` cannot run a typing harness at all** — not an
implementation gap but a contradiction: `SendInput` needs the created desktop to
be the *input* desktop, which requires `SwitchDesktop`, which is the visible
screen switch the script exists to avoid. It remains valid for observational
harnesses. The handoff listed it as "written, not yet verified"; it is now
verified as unusable for this purpose.

**This half survived the correction above.** Re-tested 2026-09-06 with
`scripts/probe-sendinput.ps1` on a created desktop: `foreground: Idle (pid 0)`,
`sent=0/2 err=5`. Nothing is in the foreground on a desktop that is not receiving
input, so there is no UIPI explanation to reach for here — the limit is real.

**Tier 1 had no target on Windows 11.** Notepad is a WinUI application with no
classic `Edit` control, so `WM_GETTEXT` reads nothing and the smoke test had
silently had no target. Fixed by falling back to `verify-windows-target.ps1`, a
WinForms `TextBox` in its own process. Verified up to the input step: the
fallback is found, reports its handle, and takes focus. The typing step is
blocked by the `SendInput` limit above, not by the target.

## What the user needs to run

One command, with the machine to themselves:

```powershell
.\scripts\verify-windows-tier1.ps1
```

It launches its own GlowKey and target, types, compares against code points, and
kills everything in a `finally`. Stop the running GlowKey first — the
single-instance mutex is session-wide.

Then the two checks no test reaches, both now in
`docs/manual-verification-windows.md`:

1. **Chrome/Edge page body** — type Vietnamese mid-paragraph in a Gmail draft;
   no character may vanish to the right of the caret. This is the one that
   matters.
2. **Chrome/Edge address bar** — `hoongf`; known to give `hoồng`.

## Housekeeping

One GlowKey running (PID 16476, the build at `5ce4c0c`). No stray Notepad or
PowerShell; the harnesses' `finally` blocks cleaned up correctly. Temp probe
files removed.

## Unresolved

1. Was the reported `hoồng` in the address bar or a page body? Decides whether
   the guard fix worked.
2. Tier 1's typing assertions remain unrun on this machine — blocked on a person,
   not on code.
3. The log is 54,660 lines / ~4.6 MB and approaching the 5 MB rotation; the
   redaction now in place only affects lines written from this build onward, so
   the older lines still contain typed text until it rotates twice.
