---
phase: 1
title: "List editors as real windows"
status: completed
priority: P1
effort: "4h"
dependencies: []
---

# Phase 1: List editors as real windows

## Overview
Excluded apps, Macros and Personal words become deferred viewports on the UI
thread, opened from their Manage… rows, instead of `egui::Window` overlays
drawn inside the Settings viewport.

## Requirements
- Functional: each opens beside Settings with its own title bar, icon and
  taskbar entry; Esc and X close it; reopen works; the list fills the window and
  scrolls; edits land in `SettingsApp.draft`/`exclusion_list` as today and are
  saved when Settings closes. Closing Settings closes the three with it.
- Non-functional: no new lock ordering. Same `Arc<Mutex<SettingsApp>>`; each
  closure locks, draws, releases.

## Architecture
- `ui_thread.rs::UiHost::frame`: after asking for the settings viewport, for
  each of `ListId::ALL`, if `lock(app).list_open(id)` then
  `show_viewport_deferred(list_id(id), settings_ui::list_viewport_builder(id),
  closure)`; the closure locks the app, calls `app.draw_list(id, ctx)`, and on
  `close_requested` sets `app.set_list_open(id, false)` and repaints ROOT.
- `settings_ui.rs`: the three `*_open: bool` fields become
  `list_open(ListId)`/`set_list_open`; `excluded_body`, `macros_body`,
  `words_body` become the bodies of `draw_list`, drawn into a `CentralPanel`
  with the window fill and 16-pt margins; `ScrollArea` takes the remaining
  height (`auto_shrink([false,false])`, no `max_height`). `aux_window` and the
  `EXCLUDED_SIZE`… consts go; `list_viewport_builder(id)` gives title
  (existing `t()` pairs), size 380×420 (excluded) / 420×440 (macros, words),
  min 320×300, icon. Esc → `ViewportCommand::Close` inside the body.
- When the settings viewport closes (result decided), the root drops the app,
  so the list viewports are no longer asked for and close with it.

## Related Code Files
- Modify: `app/src/platform/windows/ui_thread.rs`, `settings_ui.rs`

## Implementation Steps
1. Add `list_open`/`set_list_open`/`draw_list`/`list_viewport_builder` to
   `SettingsApp`; move the three bodies under `draw_list`.
2. Delete `aux_window`, `show_aux_windows`, the three size consts.
3. Wire the three viewports in `UiHost::frame`.
4. Tests: `every_tab_and_window_builds` renders each list body via
   `draw_list` headlessly with all three open; a `UiHost` test opens Settings,
   sets a list open, runs a frame, and asserts the root would ask for it
   (inspect `ctx` viewport output for the list `ViewportId`).
5. Gates; build; capture Settings + Excluded apps side by side with
   `PrintWindow`.

## Success Criteria
- [ ] Excluded apps, Macros, Personal words open as separate windows with
      taskbar entries; Esc and X close; reopen works.
- [ ] Adding an excluded app in its window and closing Settings saves it
      (log `SETTINGS applied`, settings.json contains it).
- [ ] No overlay code remains (`aux_window` gone).
- [ ] Gates green.

## Risk Assessment
- Three more deferred viewports per frame cost nothing while closed (not
  asked for). Open, each repaints independently.
- A list closure and the settings closure run sequentially on the UI thread;
  no deadlock as long as neither calls back into the root.
