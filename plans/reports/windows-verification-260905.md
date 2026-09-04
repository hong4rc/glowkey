# Windows verification — 2026-09-05

Phase 6, Tier 1, executed on the owner's Windows 11 machine against a release
build of `feat/windows-input-core`.

**GlowKey types Vietnamese on Windows.** The blind diff model survives the port.

## Method

Automated rather than by hand: a PowerShell harness launches GlowKey and Notepad,
synthesizes real virtual-key presses with `SendInput` (not `KEYEVENTF_UNICODE` —
the point is to exercise the layout and the hook, and unicode injection would
bypass exactly what is under test), then reads the edit control's text back with
`WM_GETTEXT` and compares code points.

This is stronger evidence than a person watching a screen, because the assertion
is on code points rather than on glyphs that look right. It is also weaker in
one specific way, recorded under "What this did not test".

## Tier 1 — results

| Case | Expected | Got | |
|---|---|---|---|
| `hoongf` | `hồng` | `0068 1ED3 006E 0067` | **PASS** |
| `hoongf` ⌫ `z` | `hôn` | `0068 00F4 006E` | **PASS** |
| `hoongf` ␣ ⌫ `z` | `hông` | `0068 00F4 006E 0067` | **PASS** |
| `exit` ␣ | `exit ` (not `eĩt`) | `0065 0078 0069 0074 0020` | **PASS** |
| `vieejt` | `việt` | `0076 0069 1EC7 0074` | **PASS** |

### The tag guard, proven as a behaviour

Six keys typed produced exactly six `KEY` lines and no more:

```
KEY Some('h') vk=72 mods=- app=notepad.exe | Emit bs=0 ins="h"
KEY Some('o') vk=79 mods=- app=notepad.exe | Emit bs=0 ins="o"
KEY Some('o') vk=79 mods=- app=notepad.exe | Emit bs=1 ins="ô"
KEY Some('n') vk=78 mods=- app=notepad.exe | Emit bs=0 ins="n"
KEY Some('g') vk=71 mods=- app=notepad.exe | Emit bs=0 ins="g"
KEY Some('f') vk=70 mods=- app=notepad.exe | Emit bs=3 ins="ồng"
```

Every one of those `Emit`s injected characters through `SendInput`, and **not one
of them came back into the hook.** The `dwExtraInfo` guard was previously proven
only as a pure function over an integer; this is it working against the real
system. It is also the check the issue says to stop on if it fails.

### Full suppression, visible in the log

`ins="h"` for a plain letter is the model working as designed: even an ordinary
append is swallowed and re-emitted from GlowKey's own queue. There is no line
where a character was passed through natively *and* an edit was injected — the
race that produced `hoongf` → `hoồng` on macOS has no path here.

### The foreground notification

```
FOREGROUND -> windowsterminal.exe (Ok)
FOREGROUND -> notepad.exe (Ok)
```

Resolved from the `SetWinEventHook` notification, correctly lowercased, and
matching the shipped table's spelling. `windowsterminal.exe` is a shipped default
exclusion and was correctly identified as one. `Reach::Ok` on both — neither
window is elevated, which is correct.

No per-keystroke foreground query appears anywhere in the log, which is
`decisions/0008` holding.

## The defect this found

**`SetWindowsHookExW` with a null `hmod` installs successfully and never calls
the callback.**

The first three runs of this harness produced no `KEY` lines at all. Not wrong
characters — nothing. Everything else looked healthy: the hook handle was
non-null, the log said `HOOK installed`, the WinEvent hook on the *same thread*
and the *same message pump* kept delivering foreground changes, and the process
stayed alive.

A low-level hook lives in the installing process rather than in a DLL, so the
documentation reads as though the module handle is optional. It is accepted, and
it silently does not work. Passing `GetModuleHandleW(null)` fixed it.

This is the worst available failure shape for an input method: installation
reports success and the indicator would have said the hook was live. The
`HOOK first callback received` line now in the log exists because of it — it is
the difference between "GlowKey decided not to transform" and "GlowKey never saw
the key", which are otherwise indistinguishable in a log that simply has no `KEY`
lines in it.

## Two harness bugs worth recording, because both looked like product bugs

**1. A wrong `cbSize` made `SendInput` a no-op.** The C# `INPUT` declaration was
32 bytes; on x64 Windows expects 40. `SendInput` rejects a wrong size outright
and returns 0, which presented as "GlowKey isn't transforming" when in fact no
keystroke had been synthesized at all. Notepad being *empty* rather than
containing `hoongf` was the tell.

**2. Clearing Notepad with `WM_SETTEXT` between cases broke the blind model
deliberately.** Two cases failed with residue like `hoongfhoongzhoongfz` — three
runs' text accumulating. That is correct behaviour, not a bug: the harness
deleted text behind GlowKey's back, so the engine's belief about what it rendered
at the caret was, accurately, wrong. Pressing Home between cases (a caret move,
which the ladder flushes on) fixed it. **This is a small live demonstration of
why every flush in the ladder exists.**

## What this did NOT test

Do not read the table above as more than it is.

- **Only Notepad.** Tier 2 is the whole point — Chrome's address bar, Windows
  Terminal, VS Code, an Electron app. Notepad proves almost nothing; every input
  method works in Notepad, and the macOS race appeared specifically in
  multiprocess renderers.
- **Only a US layout, no Caps Lock, no dead keys, no AltGr.** All three are
  implemented and none is verified. The dead-key path is the one that would break
  users typing other languages, and it is invisible on a US layout.
- **No elevated window.** UIPI detection is implemented and unproven against an
  actually-elevated process.
- **No hotkeys.** `Ctrl+Shift+E`/`W` and the mode toggle were not exercised, and
  there is a specific doubt about whether GlowKey's hook thread — which never has
  keyboard focus — has its per-thread key state updated at all. If it does not,
  `GetKeyState` reports the modifiers as never held and every hotkey is silently
  dead while ordinary typing looks fine. **This should be the next thing checked.**
- **No tray interaction.** The tray installs without error; nobody has clicked it.
- **No timing data.** No `EMIT took=` line appeared, meaning no callback exceeded
  the 10 ms threshold in this short run. That is encouraging and not a measurement.
- **Nothing about long-running behaviour.** Every run here was under ten seconds.

## Next

`docs/manual-verification-windows.md` Tier 0 item 1 (modifiers reach the hook),
then Tier 2. The harness is at
`scratchpad/tier1-full.ps1` and is worth keeping — it is
reproducible in a way a human pass is not, and it caught a real defect on its
first honest run.
