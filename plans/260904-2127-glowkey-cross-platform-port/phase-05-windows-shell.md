---
phase: 5
title: "Windows application shell"
status: pending
priority: P2
effort: "4d"
dependencies: [4]
---

# Phase 5: Windows application shell

## Overview

Everything around the input core that makes it an application: tray icon, settings
UI, startup, clipboard tools, paths, and — the part that is not cosmetic — an
indicator that tells the truth about whether GlowKey is actually working.

## Requirements

- Functional: feature parity with the macOS menu for the things that transfer.
- Functional: the indicator distinguishes active / English / excluded / broken.
- Functional: no console window; a background tray process.
- Non-functional: no web runtime, no cross-platform UI framework.

## Architecture

| macOS | Windows |
|---|---|
| `NSStatusItem` + `NSMenu` | `Shell_NotifyIcon` + `TrackPopupMenu` |
| AppKit Settings window (~1,900 lines) | Native Win32 dialogs, or a minimal `winit`+`egui` window |
| `SMAppService` | `HKCU\...\Run` registry value |
| `NSPasteboard` | `OpenClipboard` / `CF_UNICODETEXT` |
| `NSLocale::preferredLanguages` | `GetUserPreferredUILanguages` |
| `~/Library/Application Support`, `~/Library/Logs` | `%APPDATA%\GlowKey`, `%LOCALAPPDATA%\GlowKey\Logs` |
| `LSUIElement` | `/SUBSYSTEM:WINDOWS`, no console |
| Accessibility gate + health poll | Hook-alive check + **UIPI/elevation notice** |

**UI toolkit recommendation:** raw Win32 dialogs for the settings window. It is more
tedious than `egui` but keeps the dependency surface near zero for a background
process that must never be the reason a machine feels slow, and it matches the
"looks like it came with the system" value in `docs/ui-design.md`. If that proves
too slow to build, `winit` + `egui` is the fallback — decide after one pane is
prototyped, not before.

**The honest indicator is not optional.** `docs/decisions/0007` exists because a
menu bar claiming "VI" over a dead tap is a defect. Windows has two ways to be
silently dead that macOS does not: the hook removed by `LowLevelHooksTimeout`, and
UIPI blocking injection into the focused window. Both need a visible state.

The tray glyph carries four states, mirroring the macOS set settled in this repo's
UI pass: `VI`, dimmed `VI` (app excluded), `EN`, and `⚠`.

## Related Code Files

- Create: `app/src/platform/windows/tray.rs`, `settings_ui.rs`, `startup.rs`,
  `clipboard.rs`, `paths.rs`, `indicator.rs`
- Modify: `app/src/settings_store.rs`, `app/src/log.rs` — path resolution per
  platform (the rotation logic itself is already portable)
- Modify: `app/src/strings.rs` — `system_prefers_vietnamese` per platform

## Implementation Steps

1. `paths.rs` first — everything else needs it. Known-folder lookup, not `%APPDATA%`
   string interpolation.
2. Tray icon with the four-state glyph and a menu mirroring the macOS one: per-app
   toggle, mode, auto-fix, settings, quick guide, about, quit.
3. Startup via the `Run` key, with the same "remove cleanly on disable" behaviour as
   `SMAppService`.
4. Clipboard tools: remove tones, upper, lower — same three the macOS menu has.
5. Settings UI, one pane at a time, in the order the macOS tabs are in.
6. The elevation/UIPI notice: when the foreground window is at a higher integrity
   level, the glyph shows `⚠` and the menu names the reason.
7. Log path + rotation reuse; confirm the privacy posture is identical (local only,
   bounded, rotates at 5 MB keeping one generation).

## Success Criteria

- [ ] Tray icon appears, four states visible, menu functional
- [ ] Settings persist to `%APPDATA%\GlowKey\settings.json` and reload
- [ ] Startup toggle adds and removes the registry value cleanly
- [ ] The log rotates and lives under `%LOCALAPPDATA%`, no typed text leaves the box
- [ ] Focusing an elevated window changes the indicator rather than failing silently
- [ ] No console window appears at any point

## Risk Assessment

**The UI is ~1,900 lines on macOS and transfers nothing.** This phase is the single
largest chunk of new code and the least interesting. *Signal:* it starts absorbing
the schedule. *Response:* ship the tray plus a minimal settings pane first; the
Excluded Apps and Macros windows can follow, since the underlying settings are
editable by hand and the engine tolerates it.

**Raw Win32 dialogs may prove slower to build than budgeted.** *Signal:* one pane
takes more than a day. *Response:* switch to `winit` + `egui` for the settings
window only, keeping the tray native. Decide once, record it, do not oscillate.

**Registry startup and elevation interact.** A `Run` entry does not start elevated,
which is correct — but it means the UIPI limitation is permanent for elevated apps
unless the user deliberately runs GlowKey elevated. *Response:* document it; do not
quietly request elevation. An input method asking for admin is a red flag to users
and rightly so.
