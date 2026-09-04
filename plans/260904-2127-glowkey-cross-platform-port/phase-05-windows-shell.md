---
phase: 5
title: "Windows application shell"
status: complete
priority: P2
effort: "5d"
dependencies: [4]
---

# Phase 5: Windows application shell

Tracked by [issue #1](https://github.com/hong4rc/glowkey/issues/1).

## Overview

Everything around the input core that makes it an application: tray icon, settings UI,
startup, clipboard tools, paths, and — the part that is not cosmetic — an indicator
that tells the truth about whether GlowKey is actually working.

## Requirements

- Functional: feature parity with the macOS menu for the things that transfer.
- Functional: the indicator distinguishes active / English / excluded / broken.
- Functional: no console window; a background tray process.
- Non-functional: no web runtime. No Electron, no Tauri, no bundled browser.

## Architecture

| macOS | Windows |
|---|---|
| `NSStatusItem` + `NSMenu` | `Shell_NotifyIcon` + `TrackPopupMenu` |
| AppKit Settings window (~1,900 lines) | `winit` + `egui` window |
| `SMAppService` | `HKCU\...\Run` registry value |
| `NSPasteboard` | `OpenClipboard` / `CF_UNICODETEXT` |
| `NSLocale::preferredLanguages` | `GetUserPreferredUILanguages` |
| `~/Library/Application Support`, `~/Library/Logs` | `%APPDATA%\GlowKey`, `%LOCALAPPDATA%\GlowKey\Logs` |
| `LSUIElement` | `/SUBSYSTEM:WINDOWS`, no console |
| Accessibility gate + health poll | Hook-alive check + **UIPI/elevation notice** |

### The settings UI: `winit` + `egui`, decided

The earlier draft of this phase recommended raw Win32 dialogs and left the choice
open until one pane had been prototyped. **That is now decided: `winit` + `egui`.**
Recording it here so it is decided once rather than oscillated over.

The reasoning. The settings surface is not small — it is the macOS window's tabs
(general, exclusions, macros, personal words, about) and roughly 1,900 lines of AppKit
behind them. In raw Win32 that is dialog templates, `WM_COMMAND` routing, owner-drawn
list views and manual DPI handling for every one of those panes, and this phase's
named schedule risk is exactly that the UI absorbs the budget before the input core is
proven. `egui` collapses the list-and-checkbox panes that make up most of that surface
into something a fraction of the size, and the panes are where the tedium is.

The cost is a real dependency: `egui` pulls a rendering stack into a process that must
never be the reason a machine feels slow. Contain it, and the containment is part of
the phase, not an afterthought:

- **The tray stays native.** `Shell_NotifyIcon` and `TrackPopupMenu` directly, with no
  `egui` involvement. The tray is the always-resident half; the settings window is
  transient.
- **The window is created on demand and destroyed on close.** GlowKey at rest is a
  hook, a message loop and a tray icon. No renderer, no swapchain, no event loop
  spinning on an idle machine.
- **Nothing on the keystroke path may touch it.** The settings window lives on its own
  thread. `decisions/0008` applies to the hook callback and the settings window is the
  most inviting way to violate it.

If measurement in Phase 6 shows the on-demand window costs more at rest than this
claims — a background thread that will not sleep, memory that does not come back — that
is a finding and a decision record, not a silent revert.

### The honest indicator is not optional

`docs/decisions/0007` exists because a menu bar claiming "VI" over a dead tap is a
defect. Windows has two ways to be silently dead that macOS does not: the hook removed
by `LowLevelHooksTimeout`, and UIPI blocking injection into the focused window. Both
need a visible state.

The tray glyph carries four states, mirroring the macOS set settled in this repo's UI
pass: `VI`, dimmed `VI` (app excluded), `EN`, and `⚠`.

The `⚠` state has to distinguish its two causes in the menu text, because the user's
remedy differs completely. A hook removed by timeout is a GlowKey bug and the menu
should say so and offer to reinstall the hook. An elevated foreground window is a
permanent, correct limitation and the menu should name the window, not apologise.

## Related Code Files

- Create: `app/src/platform/windows/tray.rs`, `settings_ui.rs`, `startup.rs`,
  `clipboard.rs`, `paths.rs`, `indicator.rs`
- Modify: `app/Cargo.toml` — `winit` and `egui` under the Windows target table only.
  They must not become workspace-wide dependencies; the macOS shell does not use them
  and the engine must never see them.
- Modify: `app/src/settings_store.rs`, `app/src/log.rs` — path resolution per platform
  (the rotation logic itself is already portable)
- Modify: `app/src/strings.rs` — `system_prefers_vietnamese` per platform

## Implementation Steps

1. `paths.rs` first — everything else needs it. Known-folder lookup (`SHGetKnownFolderPath`),
   not `%APPDATA%` string interpolation, which is wrong on a redirected profile.
2. Tray icon with the four-state glyph and a menu mirroring the macOS one: per-app
   toggle, mode, auto-fix, settings, quick guide, about, quit.
3. `indicator.rs`: the four states and the two distinct `⚠` causes, driven by the
   hook-alive check and `elevation.rs` from Phase 4. Build this **before** the settings
   window — it is the part that is not cosmetic, and it is what makes Phase 6's
   elevated-window test observable at all.
4. Startup via the `Run` key, with the same "remove cleanly on disable" behaviour as
   `SMAppService`.
5. Clipboard tools: remove tones, upper, lower — the same three the macOS menu has.
6. Settings UI in `egui`, one pane at a time, in the order the macOS tabs are in.
   Create the window on demand; destroy it on close.
7. The elevation/UIPI notice wired into the menu text, naming the offending window.
8. Log path + rotation reuse; confirm the privacy posture is identical (local only,
   bounded, rotates at 5 MB keeping one generation).
9. Measure GlowKey at rest with the settings window closed — CPU at idle, working set —
   and record the numbers. This is the check on the `egui` decision, and it is worthless
   if taken only while the window is open.

## Success Criteria

- [ ] Tray icon appears, four states visible, menu functional
- [ ] The two `⚠` causes are distinguishable in the menu text
- [ ] Settings persist to `%APPDATA%\GlowKey\settings.json` and reload
- [ ] A settings file written by macOS still loads (the schema is shared; Phase 2 made
      `HotkeyPreset` carry both platforms' key identity)
- [ ] Startup toggle adds and removes the registry value cleanly
- [ ] The log rotates and lives under `%LOCALAPPDATA%`, no typed text leaves the box
- [ ] Focusing an elevated window changes the indicator rather than failing silently
- [ ] No console window appears at any point
- [ ] Idle cost recorded with the settings window closed, and it is not embarrassing
- [ ] `winit`/`egui` appear only under the Windows target table

## Risk Assessment

**The UI is ~1,900 lines on macOS and transfers nothing.** This phase is the single
largest chunk of new code and the least interesting. *Signal:* it starts absorbing the
schedule. *Response:* the step order above already answers this — tray, indicator,
startup and clipboard land before the settings panes, and the underlying settings are
hand-editable JSON that the engine tolerates. A shipped tray with two panes beats an
unshipped complete window.

**`egui` costs more at rest than expected.** A retained renderer in a background
process is a real risk and the reason raw Win32 was the original recommendation.
*Signal:* step 9's idle measurement shows non-trivial CPU or a working set that does
not shrink after the window closes. *Response:* the tray is already native, so the
fallback is bounded — rebuild the panes in Win32 dialogs, keeping everything else.
Decide on the measurement, not on taste, and record it either way.

**`egui` leaks onto the keystroke path.** Its event loop is inviting and the hook needs
somewhere to send `Effects`. *Signal:* any `egui` or `winit` type reachable from
`hook.rs`. *Response:* the settings window owns its thread and communicates by channel.
This is `decisions/0008` restated, and it is a review gate, not a guideline.

**Registry startup and elevation interact.** A `Run` entry does not start elevated,
which is correct — but it means the UIPI limitation is permanent for elevated apps
unless the user deliberately runs GlowKey elevated. *Response:* document it; do not
quietly request elevation. An input method asking for admin is a red flag to users and
rightly so.
