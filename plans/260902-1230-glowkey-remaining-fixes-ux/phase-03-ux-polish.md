---
phase: 3
title: "UX polish: VI label + Reveal Log"
status: completed
priority: P2
effort: "1h"
dependencies: []
---

# Phase 3: UX polish — VI label + Reveal Log

## Overview
Two small UX wins: (a) show `VI`/`EN` (user's wording) on the menu-bar glyph and
toggle HUD instead of `VN`; (b) a menu item to reveal the log file in Finder so
the user can grab it when reporting issues.

## Requirements
- Functional: glyph + HUD read `VI` when Vietnamese active, `EN` when off; a
  "Reveal Log in Finder" menu item opens the log's folder selected.
- Non-functional: no behavior change to typing.

## Related Code Files
- Modify: `app/src/menu_bar.rs` (glyph text VN→VI; add revealLog: item/action)
- Modify: `app/src/tap.rs` (HUD flash text VN→VI)
- Modify: `app/src/log.rs` (expose log path for reveal)

## Implementation Steps
1. Glyph: `is_active()` → "VI"/"EN".
2. HUD flashes "VI"/"EN".
3. `log::path()` public; menu action `revealLog:` →
   `NSWorkspace::activateFileViewerSelectingURLs` (or open the folder).
4. Add the menu item near "Reset input".

## Success Criteria
- [x] Glyph + HUD show VI/EN.
- [x] Reveal-log menu item present and wired.
- [x] Tests green, clippy clean, bundle builds.

## Risk Assessment
Low. Cosmetic + a file-reveal call. GUI is unverifiable headless; described for
the user's later visual check.
