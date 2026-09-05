# Manual verification — Windows

The Windows counterpart of `docs/manual-verification.md`. **Nothing in the
Windows port may be called working on the strength of a build.** `cargo build`
proves Win32 signatures; `cargo test` proves the engine and a handful of pure
functions. Neither has typed a character into an application.

This checklist is executed by a person at a real machine. Its output is a
limitations list, not a green tick.

## Before you start

```powershell
cargo build --release -p glowkey
.\target\release\GlowKey.exe
```

A tray icon should appear. If it does not, stop and read
`%LOCALAPPDATA%\GlowKey\Logs\glowkey.log`.

Keep the log open in a second window throughout. Every line below has a
counterpart in it, and the log is the only way to tell "GlowKey decided not to"
from "GlowKey never saw the key".

---

## Tier 0 — the two things that make everything else measurable

Run these first. If either fails, the rest of the list is noise.

- [ ] **Modifiers reach the hook at all.** Press `Ctrl+Shift+E` in Notepad.
      Expect a `TOGGLE app` line in the log.

      This is first because of a specific doubt: GlowKey's hook thread never has
      keyboard focus, so whether its per-thread key state is updated is not
      established. If it is not, `GetKeyState` reports Shift and Ctrl as never
      held — and then the shortcut filter and *both* `Ctrl+Shift` hotkeys are
      silently dead while ordinary typing looks fine.

- [ ] **Synthesized input is not reprocessed.** Type `hoongf` in Notepad.
      Expect `hồng`. Watch for doubled characters or a runaway.

      This is the `dwExtraInfo` tag guard. It is proven as a pure function; this
      is where it is proven as a behaviour. **If it fails, stop.** A hook feeding
      on its own output produces runaway input, not a wrong diacritic.

---

## Tier 1 — the model holds

- [ ] `hoongf` → `hồng` in Notepad
- [ ] Every letter is suppressed and re-emitted — no path where the original
      character lands *and* a replacement is injected
- [ ] `hoongf` ⌫ `z` → `hôn` (mid-word backspace stays composed)
- [ ] `hồng` ␣ ⌫ `z` → `hông` (boundary re-composition)
- [ ] `hoongf,` ␣ ⌫⌫ `z` → `hông` (the second boundary in a row)
- [ ] `exit`␣ → `exit`, not `eĩt` (auto-fix, and the boundary key replayed
      rather than passed through)
- [ ] Tone changes, capitalization, VNI, Simple Telex, Quick Telex, brackets

## Tier 2 — the applications that break input methods

Notepad proves almost nothing. Every input method works in Notepad.

- [ ] **Notepad** — the baseline
- [ ] **Chrome / Edge**, and specifically the **address bar**. Measure whether
      the trailing-selection defect the macOS AX guard exists for reproduces
      here. If it does not, say so and do not port the guard.
- [ ] **Windows Terminal** — must be excluded by default and stay excluded
- [ ] **VS Code** — Electron; the macOS race showed up in exactly this class
- [ ] **An Electron app** (Slack, Discord) — multiprocess renderer path
- [ ] **Word**, or another native Win32 editor
- [ ] **Task Manager** — must fail *visibly*. The tray shows `!` and the menu
      names the window and says it is elevated. Silence here is a defect.

## Tier 3 — the blind model's edges

- [ ] Arrow keys, Home/End, mouse click mid-word all flush; the next letter does
      not eat text
- [ ] Alt-Tab updates the frontmost application (check the `FOREGROUND ->` line)
- [ ] Hotkeys: mode toggle, per-app toggle, `Ctrl+Shift+W` correction
- [ ] A custom hotkey recorded on Windows matches on Windows
- [ ] Excluded apps type plain keys

## Tier 4 — the Windows-specific doubts

Each of these is a known gap in what the code can prove about itself. They are
listed with what "wrong" looks like, because several fail quietly.

- [ ] **Caps Lock.** Turn it on and type `hoongf`. Expect `HỒNG`.
      *Wrong looks like:* lowercase output, or a lost capital.

- [ ] **Dead keys.** Switch to US-International. Type `` ` `` then `e`.
      Expect `è`. Then type `hoongf` and confirm Vietnamese still works.
      *Wrong looks like:* `e` instead of `è`, or the accent landing on the wrong
      letter. This is the risk that breaks users typing other languages, and it
      is invisible on a US layout.

- [ ] **AltGr.** On a layout that uses it (German, Spanish, US-International),
      type an AltGr character mid-word.
      *Wrong looks like:* the composition flushing on every AltGr keystroke.

- [ ] **A second keyboard layout.** Set Notepad to one layout and another app to
      a different one, switch between them, and type in both.
      *Wrong looks like:* correct characters in one app and wrong ones in the
      other — the layout being read from GlowKey's own thread rather than the
      foreground window's.

- [ ] **Long install paths.** Run something installed under `WindowsApps` (a
      Store app) and confirm `FOREGROUND ->` names it.
      *Wrong looks like:* the foreground never updating, and GlowKey silently
      not transforming, because the engine fails closed on an unknown app.

- [ ] **Timing.** Grep the log for `EMIT took=`. Anything in the tens of
      milliseconds means something that can block reached the callback.
      `LowLevelHooksTimeout` is 100 ms and Windows removes a hook that reaches it
      **without any event, error or warning**.

- [ ] **The hook surviving a stall.** Leave GlowKey running through something
      heavy (a large build, a VM starting) and type afterwards.
      *Wrong looks like:* typing stops working, with the tray still saying `VI`.
      The tray should say `!` and offer to reinstall the hook.

## Tier 5 — the shell

- [ ] Tray icon appears; all four states are reachable and visually distinct
      (`VI`, dimmed `VI`, `EN`, `!`)
- [ ] The two `!` causes read differently in the tooltip and menu
- [ ] Settings persist to `%APPDATA%\GlowKey\settings.json` and reload
- [ ] A settings file copied from a Mac still loads
- [ ] Start-at-login adds the `HKCU\...\Run` value, and **disabling removes it**
      (check with `reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Run"`)
- [ ] The three clipboard tools transform the clipboard in place
- [ ] The log rotates, lives under `%LOCALAPPDATA%`, and no typed text leaves the
      machine
- [ ] No console window appears at any point
- [ ] **Idle cost with the settings window closed** — record CPU and working set
      as numbers. This is the check on the `winit`+`egui` decision, and taking it
      with the window open measures nothing. Since 2026-09-05 a one-point
      off-screen shim window exists for the process's life (`decisions/0011`);
      the number should not have moved.
- [ ] Settings opens, closes, and reopens three times from the tray in one
      process; an edit made in the third open is saved.
- [ ] About opens from the tray: icon, name, version with commit, no button, no
      sound. Esc closes it; the title-bar X closes it; it reopens.
- [ ] About and Settings open side by side; each has its own taskbar entry and
      neither steals focus from the other on repaint.
- [ ] With About open, Ctrl+Shift+Space toggles VI/EN and the tray glyph changes
      at once; the tray-menu toggle does the same.
- [ ] Segmented controls: no hairline around the track or the selected segment;
      the selected segment is raised (white in light, lighter grey in dark) and
      its label is the normal text colour. Both themes.
- [ ] "Open this window at launch" on: Settings appears at startup. Off: it does
      not.
- [ ] Tray Quit ends the process: no `GlowKey.exe` left in Task Manager.

## Tier 6 — the shipped exclusion table

Every default was written on a Mac and has never been matched against a real
foreground window. A wrong entry is indistinguishable, to a user, from GlowKey
being broken.

For each of `windowsterminal.exe`, `conhost.exe`, `powershell.exe`, `pwsh.exe`,
`cmd.exe`, `wsl.exe`, `alacritty.exe`, `wezterm-gui.exe`, `mintty.exe`,
`code.exe`, `devenv.exe`, `idea64.exe`, `pycharm64.exe`, `webstorm64.exe`,
`sublime_text.exe`, `nvim.exe`, `vim.exe`:

- [ ] Focus it and confirm the log names it with exactly that string
- [ ] Confirm the exclusion is the right call for it

## Recording the results

Write them to `plans/reports/windows-verification-<date>.md`. Every unchecked box
becomes either a fix or a documented limitation with a reason. Anything
structural becomes a decision record.

**Do not claim an environment works without having typed in it.**
