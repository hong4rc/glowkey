---
phase: 4
title: "About window"
status: completed
priority: P3
effort: "1h"
dependencies: []
---

# Phase 4: About window

## Overview
Unikey/EVKey have an About/version dialog. GlowKey has none. Add a small About
window (name, version, one-line credit, link to the repo) — expected polish for a
menu-bar utility.

## Requirements
- Functional: menu "About GlowKey" opens a small window showing the app name,
  version (from `CFBundleShortVersionString`), and a short credit line.
- Non-functional: native, non-resizable; no network.

## Architecture
Reuse the `prefs_window` window pattern: a tiny controller (or a second window on
the existing controller) with a vertical stack of labels. Version read from the
main bundle's Info.plist.

## Related Code Files
- Modify: `app/src/menu_bar.rs` (About item + action)
- Create: `app/src/about_window.rs` (or extend `prefs_window`)

## Implementation Steps
1. Read version from the bundle (`NSBundle::mainBundle().objectForInfoDictionaryKey`).
2. Build a small window: title, "GlowKey <version>", credit line.
3. Menu item "About GlowKey" near Settings.

## Success Criteria
- [ ] About window opens from the menu and shows the correct version.
- [ ] Clippy clean, bundle builds.

## Risk Assessment
Low. Pure UI. Unverifiable headless; described for the user's visual check.
