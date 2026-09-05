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

## Later the same day — what the user found

Testing by the person who owns the machine, after the fixes above landed. This
is the half no harness produced.

**The Chromium omnibox fix works.** `hoongf` → `hồng` in Edge's address bar,
confirmed by the user. That closes the Phase 6 question the plan had listed as
open ("does the trailing-selection defect reproduce on Windows?"): it did, and
the forward-delete guard fixes it.

**The tray glyph was invisible.** Hardcoded near-white with a comment reading
"for a dark taskbar", on a machine whose taskbar is light. Reported as "vi may
be easy, but en is something wrong, cannot see that" — and the reason `VI`
looked fine is that the excluded state is grey, which happens to show on both.

**The settings window rendered black on a light system**, Done button included.
Two causes, one fixed (eframe's `clear_color` is a hardcoded near-black that
ignores the theme, and the unfilled chrome panels showed it) and one open (the
theme still resolves to Dark despite `AppsUseLightTheme = 1`). A diagnostic line
is in place and has not been read.

**`hồngu` "does not auto revert"** turned out to be correct behaviour: auto-fix
fires at the word boundary, and the log showed `Ctrl+A`/`Ctrl+C` where a space
would have been. Now pinned by a test so the answer lives in the suite.

**League of Legends is the open question.** GlowKey is unusable there while
EVKey is fine. The log rules out the obvious causes — `Reach::Ok`, zero injection
refusals — and leaves two candidates that need one test to separate. Full
reasoning in `windows-handoff-260905.md`; the short version is that a lone `w`
becomes `ư`, so the ability key never fires, *or* Vanguard drops injected input
and nothing works at all. **Do not build a fix before running the test**, and do
not ship games in the default exclusion list: the user types Vietnamese in game
chat and excluding games would take that away.

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

---

# Tier 5 — the shell, 2026-09-05 (engine-split build, `c348714`)

A separate run, same day, same file (the plan naming this report collided with
the Tier 1 section above; nothing there is touched). Release build of
`main` at `c348714` — the commit phase 1 (macOS renderer parity) landed on.
No macOS pass was possible this session (Windows machine); this is the
stand-in `docs/handoff.md` §11 asks for. **No synthetic keystrokes or mouse
input went into the live session anywhere in this run.**

## Method

Two channels, both posting directly to a GlowKey-owned window handle, never to
the desktop or the foreground app:

- **`WM_COMMAND` to the tray's message window** (class `GlowKeyTray`), with the
  same command ids its own context menu uses (`cmd::SETTINGS=9`,
  `cmd::ABOUT=12`, `cmd::TOGGLE_MODE=1`, `cmd::QUIT=11`). The handler at
  `tray.rs:669` was wired for exactly this case ("nothing routes through here
  in practice" — it does now) and every use below went through it.
- **`WM_CLOSE` / `WM_KEYDOWN(VK_ESCAPE)` posted straight to a viewport's own
  `HWND`**, found by title (`GlowKey Settings`, `About GlowKey`) via
  `EnumWindows` filtered to GlowKey's PID.

**One channel did not work and is the session's one negative result:**
`WM_MOUSEMOVE` / `WM_LBUTTONDOWN` / `WM_LBUTTONUP` posted at computed
client-coordinates inside a Settings viewport — checked against known-good
`ClientToScreen`/`GetWindowRect` math, tried with both `PostMessageW` and
synchronous `SendMessageW`, against three different targets (a checkbox, the
Language segmented control, the tab strip) — produced no visible effect,
not even a hover highlight, across repeated attempts. Posted keyboard
messages to the same window **do** reach it (`Esc` closed About; see below),
so the window is receiving posted messages in general — it is specifically
pointer input into an egui viewport under the decision-0011 UI thread that
this technique could not drive. This may be a regression in how the
persistent-UI-thread/deferred-viewport model (`0011`) delivers pointer events
compared to the pre-0011 per-open `eframe` model the `260905-1145` phase 4
report used successfully for the same kind of click — worth a source read by
whoever picks up the macOS pass or the next Windows UI change, but it is a
**verification-technique gap, not a product defect**: a real mouse still
drives the app normally. Every box below that needed a click inside a
viewport (not the tray, not a title bar) is left unticked with this reason,
not faked.

## Pre-state (restored at the end, all three confirmed back)

| | Before | After |
|---|---|---|
| `HKCU\...\Run\GlowKey` | `"D:\project\github\glowkey\target\release\GlowKey.exe"` | same |
| `%APPDATA%\GlowKey\settings.json` | user's real file (backed up) | restored byte-identical |
| `AppsUseLightTheme` | `0x1` (Light) | `0x1` (Light) |

## Results

| Box | Result | Evidence |
|---|---|---|
| Idle CPU / working set, no window open | **recorded** | 30 s sample: **593.75 ms** CPU (**≈2.0%** of one core), working set **~101 MB** (105,943,040 B). No earlier Windows figure exists to compare against — this is the baseline. |
| Settings persists and reloads | pass | `settings.json` unchanged byte-for-byte across a restart when untouched |
| A settings file copied from a Mac still loads | pass | `settings-real-macos.json` (bundle-id exclusions, no Windows fields) copied to `%APPDATA%\GlowKey\settings.json`, GlowKey started clean, ran normally, file left unmodified — not silently replaced with defaults. Same file is also pinned headlessly by `a_real_settings_file_loads_field_for_field` (`prefs_model.rs:347`). |
| Start-at-login adds the `Run` value, disabling removes it | pass | `startup::set_enabled(false)`/`(true)` called directly (two `#[ignore]` tests added, run, then reverted — no permanent test added); `reg query` showed the value gone after disable and back (quoted, correct path) after enable |
| Log rotates, lives under `%LOCALAPPDATA%`, no typed text leaves the machine | pass (rotation by code reading + existing headless tests, not forced live) | `glowkey.log` at `%LOCALAPPDATA%\GlowKey\Logs\`, 4.47 MB at time of check (cap is 5 MB); `rotate()` and its three tests (`log.rs:211-260`) are platform-shared code, exercised by `cargo test --workspace`; no networking dependency anywhere in `app/` or the three crates (`grep` for `reqwest`/`hyper`/`TcpStream`/`http(s)://` finds only doc URLs) |
| No console window appears | pass | `#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]` (`main.rs:27`); no console observed across every start in this session |
| Settings opens, closes, reopens ×3 in one process | pass | via `WM_COMMAND SETTINGS` to the tray + `WM_CLOSE` to the viewport, 3 rounds, each producing a new `HWND` and no `RecreationAttempt` in the log |
| About opens, closes (X), reopens | pass | opened via `WM_COMMAND ABOUT`; closed via posted `WM_CLOSE` |
| About closes by Esc | pass | `WM_KEYDOWN(VK_ESCAPE)`/`WM_KEYUP` posted straight to the About `HWND` closed it — this is the one posted-keyboard confirmation that the window is live and reading input at all |
| About: icon, name, version-with-commit, no button (but Copy), no sound | pass (sound unheard — no audio device checked) | capture: `plans/reports/tier5-captures/about.png` — `GlowKey`, `Version 0.1.0 (c348714)`, matches the exact commit under test |
| About and Settings open side by side, each its own taskbar entry | pass | both open simultaneously; both windows carry `WS_EX_APPWINDOW` (`0x40110`) in their extended style |
| With About open, tray-menu VI/EN toggle updates the indicator at once | pass | `TOGGLE mode -> English (menu)` then `INDICATOR English` **1 ms** later, and back, both via posted `WM_COMMAND TOGGLE_MODE` while About's `HWND` was open |
| Segmented control: no hairline, raised selection, normal label colour, both themes | pass | `plans/reports/tier5-captures/settings-general.png` (light) and `settings-dark.png` (dark, via a temporary `AppsUseLightTheme=0` + broadcast `WM_SETTINGCHANGE("ImmersiveColorSet")`, restored after) — General tab's Language control in both |
| Tray Quit ends the process, no `GlowKey.exe` left | pass | `WM_COMMAND QUIT` posted to the tray; `Get-Process GlowKey` found nothing after |
| Every control and caption on one vertical line (control column) | pass, General tab only (see gap below) | visible in both captures above; not re-checked on Typing/Corrections/Apps & macros this run (see unticked list) |
| The shortcut row shows keycaps, captions stay plain text | pass | visible in `settings-general.png`: `Ctrl` `Shift` `E` as keycaps, "Turns GlowKey off or on..." as plain text |

## Left unchecked, with reasons — not faked

**Blocked by the posted-mouse-input gap above** (a real click would work; the
technique in this report could not drive it):

- Tab switching to Typing / Corrections / Apps & macros — so the count rows
  (`20 apps` / `0 macros` / `0 words`) were not re-captured live this run. Not
  a new risk: this exact text was captured working in
  `verification-260905-1216-windows-settings-ux-polish.md` before phase 1,
  and phase 1 touched no Windows rendering path (only added `ListId::unit`,
  which Windows now reads instead of its old inline match — same string, new
  source).
- `Manage…` on Excluded apps, Macros, Personal words — none of the three list
  windows opened, so their own captures, their Esc, "closing Settings closes
  them too", and "an app added is saved" are all unverified this run.
- An edit made in the third Settings open being saved.
- About: Copy puts the version and commit on the clipboard.

**User-owned, needing a real keypress or a real click into the live
session**, per the phase's own rule — listed here as the walkable checklist:

1. `Ctrl+Shift+Space` with About open (the tray-menu equivalent of this was
   exercised above and passed; the hotkey path itself needs a real keypress).
2. The tray icon's own left/right click and its context menu (its command ids
   were exercised directly via `WM_COMMAND`, but the icon and menu chrome
   themselves were not clicked).
3. Tab reaching the tab strip and each segmented control; ←/→ moving the
   selection and showing a focus ring; the hotkey popup opening with
   Space/Enter. (Posting `VK_TAB`/`VK_RIGHT` to the Settings `HWND` produced
   no visible change either — consistent with the same pointer/focus gap
   above rather than a second issue, but left here rather than claimed.)
4. The three clipboard tools (remove tones, UPPERCASE, lowercase) — they act
   on the real clipboard, which this session did not want to disturb.
5. Reading a live `EMIT took=` figure for Chromium versus a plain text field
   (§7 of the handoff already lists this as open; unrelated to this plan).

## Gates

Unaffected by this phase — no source file changed. Full six-gate run is
phase 3's job, on the branch that carries this report.

## Unresolved questions (Tier 5)

1. Does the pointer-input gap above reproduce for a plain, unposted mouse
   click once the window is truly focused and foreground, or is it specific
   to posted messages? Not established — this report only proves the posted
   path doesn't work, not why.
2. Would enabling `accesskit`/UIA support in the Windows `eframe` integration
   let a future verification pass drive these controls without a person at
   the keyboard? Worth asking whoever next touches `platform/windows/ui_thread.rs`.
