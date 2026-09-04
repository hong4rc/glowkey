---
phase: 10
title: "Linux shell and packaging"
status: pending
priority: P3
effort: "3d"
dependencies: [9]
---

# Phase 10: Linux shell and packaging

## Overview

Tray, settings, autostart and distribution for Linux — the smallest of the three
shells, because Linux desktop integration is the least uniform and over-building it
is waste.

## Requirements

- Functional: a tray indicator where the desktop supports one, and a working path
  where it does not.
- Functional: settings editable, autostart toggleable, distribution installable.
- Non-functional: no assumption that every desktop has a system tray.

## Architecture

| Concern | Linux approach |
|---|---|
| Tray | StatusNotifierItem via D-Bus (`ksni`-style), which GNOME needs an extension for — hence the fallback |
| Fallback when no tray | The settings window itself, plus the hotkeys, which are the primary control anyway |
| Settings UI | Same toolkit decision as Windows; reuse whatever Phase 5 settled on |
| Autostart | XDG autostart `.desktop` file in `~/.config/autostart/` |
| Clipboard | X11 selections via the same X connection |
| Paths | XDG: `$XDG_CONFIG_HOME/glowkey`, `$XDG_STATE_HOME/glowkey/logs` |
| Packaging | Portable tarball first; AppImage next; distro packages only on demand |
| UI language | `$LANG` / `$LC_ALL` |

Flatpak is deliberately excluded: its sandbox forbids the global input interception
this application is built on, so it would ship something that cannot work.

## Related Code Files

- Create: `app/src/platform/linux/{tray,settings_ui,startup,clipboard,paths}.rs`
- Create: `scripts/package-linux.sh`, `packaging/glowkey.desktop`
- Modify: `.github/workflows/release.yml`, `README.md`, `docs/handoff.md` §8

## Implementation Steps

1. XDG paths, reusing the portable settings and log logic.
2. StatusNotifierItem tray with the four-state glyph, and detection for its absence.
3. XDG autostart file written and removed by the settings toggle.
4. Settings UI, reusing the Phase 5 toolkit decision.
5. Tarball packaging plus a `.desktop` entry; AppImage if it is cheap.
6. Document the supported-session matrix, prominently — a Wayland user must learn
   this before installing, not after.

## Success Criteria

- [ ] Tray works on KDE and XFCE; GNOME's requirement documented
- [ ] Autostart toggles cleanly both ways
- [ ] Settings persist under XDG paths
- [ ] A tarball a user can extract and run
- [ ] The supported-session matrix is in the README, not buried

## Risk Assessment

**Linux desktop integration has no single right answer**, and chasing every desktop
is unbounded. *Signal:* per-desktop special cases accumulate. *Response:* support
the common path well, document the rest, and let the hotkeys be the fallback control
surface. GlowKey is usable with no tray at all.

**GNOME needs an extension for a tray.** *Signal:* GNOME users see no indicator.
*Response:* documented, with the settings window and hotkeys as the answer. Do not
ship a GNOME extension; that is a separate product.
