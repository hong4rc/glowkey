# Windows verification — 2026-09-05

Phase 6, Tier 1, executed on the owner's Windows 11 machine against a release
build of `feat/windows-input-core`.

**GlowKey types Vietnamese on Windows.** The blind diff model survives the port.

## Read this first: the test machine had a second Vietnamese IME running

**EVKey64 was running throughout.** It was not noticed until after Tier 1, and it
is the single biggest caveat on everything below.

It surfaced through an anomaly rather than a check: in the Tier 0 run, GlowKey had
`notepad.exe` excluded (persisted from the previous run's `Ctrl+Shift+E`), every
log line said `Passthrough`, and `hoongf` still became `hồng` — accompanied by
inbound `vk=231` (`VK_PACKET`) and synthetic backspace events GlowKey had not
sent. Something else was transforming.

**Why Tier 1 is still valid.** Low-level hooks are called most-recently-installed
first, and GlowKey installs after EVKey, so GlowKey sees each keystroke first.
Every handled key returns non-zero, which swallows it and never reaches
`CallNextHookEx` — so EVKey is starved of the keystrokes entirely. The Tier 1 log
shows GlowKey computing each edit itself (`Emit bs=1 ins="ô"`, `Emit bs=3
ins="ồng"`) and the resulting text matches those edits exactly, code point for
code point. Two IMEs both transforming would produce corruption, not a clean
match.

**What it does mean.** GlowKey's injected output *does* reach EVKey — injected
events re-enter at the top of the chain, GlowKey's tag check passes them on, and
EVKey sees them. It evidently ignores them (they are `VK_PACKET` and `VK_BACK`,
not the letter keystrokes it composes from), but that is an observation, not a
guarantee.

**Tier 1 should be re-run with EVKey stopped before any of this is called
settled.** That was not done here: EVKey is the owner's own running application
and stopping it was not mine to do while they were away.

It is also, incidentally, a real-world finding worth keeping: GlowKey's
full-suppression model means it wins the hook chain against an already-running
competitor, rather than fighting with it.

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

## The test machine runs US-International, which matters

Discovered while writing an AltGr test that assumed "a US layout has no AltGr
mappings" and failed. Probing `ToUnicodeEx` under Ctrl+Alt on this machine:

```
vk=0x41 (A) -> á     vk=0x46 (F) -> ã     vk=0x35 (5) -> €
```

`Get-WinUserLanguageList` confirms `0409:00060409` — **United States-International**,
alongside plain US.

Two consequences:

1. **AltGr is live here**, so the AltGr handling is not theoretical on this
   machine and the Tier 1 results above were produced on a layout where it
   matters.
2. **US-International is a dead-key layout.** It is exactly the layout Tier 4
   calls for to test `ToUnicodeEx` dead-key preservation, and it is already
   installed. That check is therefore cheap and should be done next — `` ` ``
   then `e` should give `è`, with GlowKey running and with it stopped, and the
   two compared.

It also means the earlier assumption in the code — that a US layout is the
uninteresting default case — was wrong about the very machine the code was being
written on.

## Tier 0 — modifiers reach the hook

The doubt that mattered most, because it fails silently: GlowKey's hook thread
never has keyboard focus, so whether `GetKeyState` sees held modifiers at all was
unestablished. If it did not, the shortcut filter and both `Ctrl+Shift` hotkeys
would be dead while ordinary typing looked perfect.

**They arrive.**

```
KEY None vk=160 mods=Ctrl       app=notepad.exe | Passthrough
KEY None vk=69  mods=Ctrl+Shift app=notepad.exe | ToggleApp
TOGGLE app "notepad.exe" -> Excluded
KEY None vk=32  mods=Ctrl+Shift app=notepad.exe | Consume
TOGGLE mode -> English
```

- `Ctrl+Shift+E` reaches the ladder as `ToggleApp` and the exclusion takes effect
  immediately — the keys after it log `Passthrough`.
- `Ctrl+Shift+Space` reaches it as `Consume` and flips the mode.
- Modifier reporting is correct: `mods=Ctrl` while only Control is down,
  `mods=Ctrl+Shift` once both are.

## Settings persistence, proven incidentally

`%APPDATA%\GlowKey\settings.json` was created by these runs and contains exactly
the shipped defaults, with an empty `removed_default_exclusions` — the toggles
that ran during testing netted out correctly rather than leaving residue.

That file existing at all is the proof that the first review's finding 8 is fixed.
A hook callback does not make `GetMessageW` return, so `Effects::save_settings`
was previously set and never drained: the settings file would never have been
written on a run where the user typed, pressed a hotkey and never switched
windows. The `PostThreadMessageW(WM_GLOWKEY_SAVE)` wake is what turns the flag
into a write, and this file is that path completing.

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
- **`Ctrl+Shift+W` (the correction hotkey) was not exercised**, though the other
  two were — see Tier 0 above.
- **A second Vietnamese IME was running throughout.** See the note at the top.
- **No tray interaction.** The tray installs without error; nobody has clicked it.
- **No timing data.** No `EMIT took=` line appeared, meaning no callback exceeded
  the 10 ms threshold in this short run. That is encouraging and not a measurement.
- **Nothing about long-running behaviour.** Every run here was under ten seconds.

## Next, in order

1. **Re-run Tier 1 with EVKey stopped.** Until then every result here carries the
   caveat at the top. This is the owner's call to make, not the agent's.
2. **Tier 2** — Chrome's address bar, Windows Terminal, VS Code, an Electron app.
   This is where the macOS race appeared and where Notepad's evidence runs out.
3. **An elevated window** (Task Manager), to exercise the UIPI path that is
   implemented and entirely unproven.
4. The Tier 4 layout cases: Caps Lock, dead keys, AltGr, two layouts at once.

The harness is kept at `scripts/verify-windows-tier1.ps1`. It is reproducible in
a way a human pass is not, and it earned its place by catching a real defect on
its first honest run — one that a person watching a screen would have reported as
"nothing happens", with no way to tell that from a hundred other causes.

## Unresolved questions

1. Does EVKey's presence change any Tier 1 result? Untested (see above).
2. Does the keyboard hook keep working while the settings window is open? The
   window runs `eframe` on the message-loop thread, which is the thread the hook
   is delivered on. Not yet exercised.
3. Does GlowKey's injected output disturb any other input method that is also
   watching? EVKey appears to ignore it; nothing establishes that in general.
