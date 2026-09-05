# Windows port — handoff, 2026-09-05

For a session picking this up cold. Branch `feat/windows-input-core`, 16 commits
ahead of `main`, **not pushed**. 285 tests green, clippy silent at `-D warnings`,
release binary builds and runs.

Read `docs/handoff.md` §5b first for the shape of the port. This file is the
current state, the open questions, and the things that cost time to learn.

---

## Where the port is

| Phase | State |
|---|---|
| 0 — engine tests pass on Windows | Complete |
| 4 — input core (hook, injection, adapt, foreground, elevation) | Complete |
| 5 — shell (tray, settings, startup, clipboard, indicator) | Complete, two open UI defects below |
| 6 — verification on real hardware | **Tier 0 and Tier 1 done. Tier 2 barely started.** |
| 7 — CI job | Done. Packaging deliberately waits for Phase 6. |

Plan: `plans/260904-2127-glowkey-cross-platform-port/`.
Verification results: `plans/reports/windows-verification-260905.md`.

## What is actually verified

- `hoongf` → `hồng`, mid-word backspace, boundary re-composition, auto-fix, tone
  placement — **in Notepad, compared against code points**, not glyphs.
- Modifiers and both `Ctrl+Shift` hotkeys reach the hook.
- The `dwExtraInfo` self-event guard holds against the real system.
- **The Chromium omnibox fix works** — the user confirmed `hoongf` → `hồng` in
  Edge's address bar after the forward-delete guard landed.
- Single-instance guard: launch three, one survives.
- Settings persistence, the startup registry entry, and the tray indicator
  tracking mode and foreground changes.

## What is NOT verified

Chrome, Windows Terminal, VS Code, Electron apps, elevated windows (the UIPI
path is implemented and has never met one), dead-key layouts, AltGr, two layouts
at once, long-running behaviour, idle cost. Hotkey *recording* is not implemented.

---

## Open defects

### 1. The settings window renders dark on a light system — UNRESOLVED

The user's machine reports `AppsUseLightTheme = 1` and the window still came up
black, with white text and a black Done button.

Two causes were found; **one is fixed and one is not**:

- **Fixed.** `eframe`'s default `clear_color` is a hardcoded
  `rgba(12, 12, 12, 180)` that ignores the theme entirely, and the tab strip and
  button bar were `Frame::none()` — transparent — so it showed through. Both now
  paint a real surface, and `clear_color` is overridden to `visuals.window_fill()`.
- **Not fixed.** The *content* panel already filled with `window_fill` and was
  also black, which means the theme is resolving to **Dark** despite the registry
  saying light. `apply_theme` reads `theme::apps_are_light()` and calls
  `ctx.set_theme` every frame, and it looks correct by inspection.

**A diagnostic is in place and has not been read yet.** Opening the settings
window logs, once per open:

```
SETTINGS theme: apps_are_light=<bool> -> Light|Dark
```

That line decides where to look: if `apps_are_light=true` the registry read is
fine and `ctx.set_theme` is not taking effect; if `false`, the registry read in
`theme.rs` is wrong. **Do not guess between those — open the window and read the
log.**

**Update, 2026-09-05 09:50 — verified headlessly, pending one look at the window.**
The log line was never produced (Settings was not opened in the running process),
so both halves were checked without a window instead:

- The Rust registry read returns `apps_are_light=true` on this machine (a
  throwaway test printed it; PowerShell agrees, both values are `1`).
- egui 0.29.1 resolves `ThemePreference::Light` to the light style with no
  system-theme input at all (`Options::theme` in `memory/mod.rs`), and eframe
  0.29.1 never touches `theme_preference`. The existing headless test
  `caption_colour_contrasts_in_both_themes` already exercises this path.
- The registry read, `apply_theme`, and the diagnostic all landed in one commit
  (`40a7e4d`). The running binary was built at 08:57, after that commit, so the
  "still dark" observation most likely predates the fix.

Every link in `registry → set_theme(Light) → light style` is therefore verified.
Remaining step: the user opens Settings once. If it is light, close this defect.
If it is still dark, the log line will say `apps_are_light=true -> Light` and the
cause is outside egui's theme (e.g. a panel painting an explicit dark colour).

### 2. The settings window opens once per process — CLOSED 2026-09-05

Closed by `docs/decisions/0011`: one eframe loop on a dedicated thread, Settings
and About as deferred viewports. Open-at-launch is honoured. Plan:
`plans/260905-1039-windows-ui-parity/`. The paragraphs below are the history.

**Update 2026-09-05 10:30.** Because of this, "Open this window at launch" is
**not honored on Windows** (`platform/windows/mod.rs::run` never calls
`shell::open_settings`), although the checkbox is shown there by the shared
spec. Honoring it would spend the process's one window at startup and leave the
tray's Settings item dead. Fix both together: a long-lived UI thread or a
separate process, then call `open_settings` at startup when the setting is on.

Also closed today: defect 1. The log line read `SETTINGS theme:
apps_are_light=true -> Light` on the previous build.

winit permits one event loop per process and there is no reset outside its web
backend. The second time a user picks Settings in a long-running GlowKey, it
returns `RecreationAttempt`, which is now logged rather than silent. Fixing it
properly is a design decision — a separate process, or a dedicated long-lived UI
thread — not a patch.

### 3. The list editors are overlays, not windows

Excluded apps, Macros and Personal words are `egui::Window` overlays inside the
main window; on macOS they are genuinely separate windows. They were inset
(372×320, 396×340) so they stop covering their parent. `ctx.show_viewport_immediate`
would be the faithful answer; it changes window lifetime, which is what the
"nothing outlives `show`" rule protects, so it was left as a deliberate choice
rather than shipped unseen.

---

## The open question that matters most: League of Legends

The user reports GlowKey is unusable in League while EVKey is fine. **The
evidence rules out the obvious explanations**, and the remaining two need one
test to separate.

What the log established:

```
FOREGROUND -> league of legends.exe (Ok)      <- reach is fine; UIPI is NOT blocking
INJECT REFUSED: 0                              <- SendInput never failed
KEY Some('w') vk=87 | Emit bs=0 ins="ư"        <- GlowKey transformed, twice
TOGGLE mode -> English (menu)                  <- user gave up 10s in
```

So it is **not** elevation and **not** Vanguard refusing injection. Two
candidates remain:

**(a) A lone `w` becomes `ư`.** In this engine `w` is a horn/breve modifier, and
with nothing composing it produces `ư` on its own — so pressing W to cast an
ability sends `ư` and the ability never fires. There is **no setting to disable
standalone `w`**, and UniKey/EVKey do have that option, which is the likeliest
reason EVKey behaves differently on the same machine.

**(b) Vanguard drops injected input.** GlowKey suppresses *every* key and
re-injects it, even ones it does not change. If League ignores injected
keystrokes, then with Vietnamese on *nothing* works, not just `w`. The log cannot
distinguish this because the user switched to English within ten seconds, and
English mode injects nothing at all.

**The discriminating test**, with League *not* excluded and Vietnamese *on*:
press a key that is not a Telex key — `B` for the shop, or `Y`, or `M`.

- Shop opens → cause (a). Fix is a "do not transform a standalone `w`" setting,
  which is a small engine change and useful beyond games.
- Shop does not open → cause (b). No Telex setting helps; GlowKey would need a
  different delivery path for games, which is a much larger question and probably
  its own decision record.

**Do not build the `w` option before running this test.** If it is (b), the
option fixes nothing and the time is wasted.

Note the user's own correction, which rules out a tempting shortcut: they *can*
type Vietnamese in game chat with EVKey, so shipping games in the default
exclusion list would remove something they use. The shipped Windows defaults
contain **no games** and should stay that way pending this answer.

---

## The machine this was tested on

Facts that changed results and will change them again:

- **EVKey is installed** at `D:\apps\evkey\EVKey64.exe`. Its logon task
  (`EVKey - Vietnamese Keyboard`, `RunLevel: Highest`) **was disabled at the
  user's request** this session. Re-enable with
  `Enable-ScheduledTask -TaskName "EVKey - Vietnamese Keyboard"` (needs UAC).
  Two Vietnamese IMEs running at once contaminated an entire round of
  verification before anyone noticed — check for it before trusting a result.
- **The keyboard layout is US-International**, not plain US. `Ctrl+Alt+A` is `á`,
  `Ctrl+Alt+5` is `€`. So AltGr is live here, and — more usefully — the dead-key
  layout Tier 4 needs is already installed, making the `ToUnicodeEx` dead-key
  check the cheapest remaining item.
- **Light theme**: `AppsUseLightTheme = 1`, `SystemUsesLightTheme = 1`.
- **League is excluded** in the user's `%APPDATA%\GlowKey\settings.json`
  (`league of legends.exe`, `leagueclientux.exe`, `riotclientservices.exe`).
  Backup at `settings.json.bak-before-league`. This is a workaround for the
  question above, not a decision — it also means no Vietnamese in League chat.
- **GlowKey has a startup entry** pointing at
  `D:\project\github\glowkey\target\release\GlowKey.exe`. The user enabled it
  from the tray. `cargo clean` or moving the repo breaks it silently.

## Testing rules the user set

- **Do not send synthetic keystrokes into their live session.** The harnesses
  steal focus and type; they asked for this to stop after one interrupted a video.
  Ask first, or use an isolated desktop.
- `scripts/verify-windows-isolated.ps1` creates a real Windows desktop via
  `CreateDesktop` and launches GlowKey plus a harness there with
  `STARTUPINFO.lpDesktop`, so the input queue is separate. **It is written and
  untested** — verify it before relying on it.
- `scripts/verify-windows-tier0.ps1` and `-tier1.ps1` work but type into the
  current desktop.
- `scripts/verify-windows-tier2-edge.ps1` exists and **does not work** — it gets
  stuck on Edge's first-run state and focus, and produced no keystrokes at all.

---

## Things that cost time, so they do not again

**A null `hmod` makes `SetWindowsHookExW` install and never fire.** It returns a
valid handle. Installation reports success, the WinEvent hook on the same thread
and pump keeps working, the process stays alive, and not one keystroke arrives.
Three test rounds went into this. `GetModuleHandleW(null)` fixes it, and the
`HOOK first callback received` log line exists so the next person sees it in one
line rather than three rounds.

**eframe's default `clear_color` is hardcoded near-black** and ignores the
theme; its own source note says `_visuals.window_fill() would also be a natural
choice`. Any panel without a fill shows it.

**egui's bundled font cannot draw Vietnamese at all** — no Latin Extended
Additional. A Vietnamese interface in the default font is a wall of
missing-glyph boxes. `settings_ui.rs` loads Segoe UI and Consolas from
`%SystemRoot%\Fonts` with a test asserting the alphabet is drawable.

**`INPUT` is 40 bytes on x64**, and `Marshal.SizeOf` on a trimmed C# declaration
says 32. `SendInput` rejects a wrong `cbSize` outright and returns 0, which
presents as "the hook is not firing". The tell is the target being *empty*
rather than containing the raw keys.

**Clearing a test target with `WM_SETTEXT` breaks the blind model deliberately**
— the engine still believes it rendered the previous word there. Press Home
(a caret move, which flushes) between cases.

**The single-instance test cannot claim the real slot**: it fails whenever
GlowKey is running, which is the guard working reported as a broken suite. It
uses its own mutex name. For the same reason two tests cannot both exercise a
session-wide mutex in parallel — cargo runs them on threads and they take each
other's slot.

---

## Suggested order for the next session

1. **Open the settings window and read the `SETTINGS theme:` log line.** It
   decides the whole theme investigation and takes ten seconds.
2. **Run the League `B` test** above. It decides whether the standalone-`w`
   option is worth building.
3. **Dead keys on US-International** — `` ` `` then `e` should give `è`, with
   GlowKey running and stopped, and the two compared. The layout is already
   installed.
4. **Tier 2 proper**: Windows Terminal, VS Code, an Electron app, and an elevated
   window (Task Manager) for the UIPI path.
5. Re-run Tier 1 with EVKey confirmed stopped, so the numbers are clean.

## Unresolved questions

1. Is the settings window dark because the registry read is wrong, or because
   `ctx.set_theme` is not taking effect? The log line answers it.
2. Is League broken by `w` → `ư`, or by Vanguard dropping injected input?
3. How does EVKey allow Vietnamese in game chat without breaking ability keys —
   a standalone-`w` setting, or something else?
4. Should the list editors become real child viewports?
5. What should the settings window do on a second open in one process?
