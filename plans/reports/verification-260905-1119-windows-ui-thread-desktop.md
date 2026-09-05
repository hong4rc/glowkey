# Windows UI thread — desktop verification, 2026-09-05 11:25

Build: `main` at `0646c8f` (+ journal), release binary of 11:10, running as the
user's GlowKey. Driven through GlowKey's own tray window: menu commands as
`WM_COMMAND`, window closes as `WM_CLOSE`. **No keystrokes or mouse input** into
the session (`scratchpad/drive-glowkey.ps1`, not committed).

## Results

| Check | Result | Evidence |
|---|---|---|
| Settings opens, closes, reopens ×3 in one process | pass | window `GlowKey Settings` present after each open, absent after each close; no `RecreationAttempt` in log |
| About opens, closes, reopens | pass | window `About GlowKey` present/absent as commanded, twice |
| About and Settings open together | pass | both windows listed; Settings foreground, About not |
| Menu VI/EN toggle with About open, glyph follows at once | pass | `TOGGLE mode -> Vietnamese (menu)` then `INDICATOR Vietnamese` 1 ms later; same back to English |
| Root shim off-screen and receiving redraws | pass | shim at (-32000,-32000) 1×1; every command above was drained through it |
| Open-at-launch | pass | `SETTINGS theme` line 0.4 s after startup on the 11:10 launch |
| No `MessageBox` | pass | `shell.rs` has none; About is class `Window Class` (winit) |
| Gates | pass | 98 app tests; clippy Windows + macOS targets clean |

Mode was left as found (English → Vietnamese → English).

## Not verified here, for the user

- Hotkey Ctrl+Shift+Space with About open (needs a keystroke). The menu path
  proves the loop is free; the hotkey uses the same deferred refresh.
- Esc closes About (keystroke). `about_ui` tests prove the command is sent.
- No system sound on About (ear).
- Segmented controls: no hairlines, raised selection, both themes (eye).
- Taskbar: Settings and About should each have a button; the off-screen shim
  carries `WS_EX_APPWINDOW` and relies on winit's `skip_taskbar` (taskbar tab
  removal) to stay out. **Look for a stray "GlowKey" button.** If present, the
  fix is `with_taskbar(false)` not taking effect on this winit; fall back to
  `WS_EX_TOOLWINDOW` via the raw handle.
- Tray Quit leaves no process. Not run: it would restart Settings over the
  user's game.

## Observation

Opening Settings takes foreground focus, as a new window does. With
`open_settings_at_launch` on, that happens at every start, including over a
full-screen game. The setting is the user's; the checkbox is in General.
