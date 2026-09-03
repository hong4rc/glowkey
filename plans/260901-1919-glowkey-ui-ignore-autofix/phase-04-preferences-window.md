---
phase: 4
title: "Preferences window"
status: completed
priority: P1
effort: "3-5d"
dependencies: [3]
---

# Phase 4: Preferences window

## Overview

A small preferences window to manage the whole ignore list and the global
options. The menu bar handles the quick current-app toggle; this window is where
you see and edit the full set of excluded apps and flip the settings. objc2
AppKit — verifiable only by running.

## Requirements

**Functional**
- A window (opened from the menu's `Preferences…`) containing:
  - **Excluded apps list** — the apps where Vietnamese is off, each shown with its
    name (and icon if easy). Add via a standard open-panel/app-picker over
    `/Applications`; remove the selected row. An app whose bundle is no longer
    installed still shows (greyed), not silently dropped.
  - **Auto-fix** toggle.
  - **Placement style** — `hoà` (new) vs `hòa` (old), a two-option control.
  - **Toggle hotkey** — shown read-only as ⌃⇧Space (not editable in v1).
- Every change applies to the live `Session` immediately and saves settings.
- The window is created lazily on first open (not at launch — keeps startup fast).

**Non-functional**
- Main thread; same `try_borrow_mut` discipline as the menu.
- English strings, but via a `.strings` table so Vietnamese localization is cheap
  later.

## Architecture

```
app/src/prefs_window.rs   NSWindow + NSTableView (or a simple stack of rows) for
                          the excluded-app list; controls for auto-fix, style
```

The window controller is an objc2 class holding the shared `TapState`. The list is
an `NSTableView` backed by the current `ExclusionList` ids resolved to names via
`app_info`. Add uses `NSOpenPanel` restricted to applications; the chosen app's
bundle id goes into the list. Remove deletes the selected id.

Keep it minimal: a single pane, a table plus a few controls. Resist a tabbed,
multi-pane preferences design — "simple UI" is a stated product value.

## Related Code Files

- Create: `app/src/prefs_window.rs`
- Modify: `app/src/menu_bar.rs` — `openPreferences:` builds (once) and shows the window
- Modify: `app/src/app_info.rs` — add icon resolution if not done in Phase 3
- Create: `app/Resources/en.lproj/Localizable.strings` (English strings)

## Implementation Steps

1. Build the window lazily: `openPreferences:` creates it on first call, then just
   shows/focuses it.
2. Excluded-apps table: data source reads `ExclusionList` ids → names/icons.
3. Add button → `NSOpenPanel` (applications only) → insert bundle id → refresh
   table → apply to session → save.
4. Remove button → delete selected id → refresh → apply → save.
5. Greyed row for a missing app (name unresolved) rather than dropping it.
6. Auto-fix toggle and placement-style control, both applying immediately + saving.
7. Show the hotkey read-only.
8. Verify by running: open prefs, add/remove an app and watch typing behaviour
   change in that app, flip options, relaunch and confirm persistence.

## Success Criteria

- [x] Preferences opens from the menu and shows the excluded apps with names
- [x] Add (via picker) and remove work; changes affect typing in those apps immediately
- [x] A no-longer-installed excluded app still appears (greyed), not dropped
- [x] Auto-fix and placement style controls work and persist
- [x] Hotkey shown read-only
- [x] Window instantiated lazily (startup cost unchanged)

## Risk Assessment

**`NSTableView` wiring in objc2 is the heaviest AppKit surface in this plan.**
Signal: slow going on data source/delegate. Response: if the table proves costly,
fall back to a simple vertical stack of rows (name + remove button) — the list is
short; a full table is not essential for v1.

**Assumption at risk:** that `NSOpenPanel` restricted to applications returns a
usable bundle id. Signal: the picked item has no bundle id. Response: read the
bundle id from the selected `.app` via `NSBundle(url:)`; if nil, reject with a
message rather than adding a blank entry.

**Icon/name resolution for an uninstalled app fails.** Signal: blank row.
Response: show the raw bundle id as the label and grey it; never drop the entry,
so the user does not silently lose an exclusion.
