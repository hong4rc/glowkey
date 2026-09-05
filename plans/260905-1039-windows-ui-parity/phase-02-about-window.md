---
phase: 2
title: "About window"
status: pending
priority: P1
effort: "3h"
dependencies: [1]
---

# Phase 2: About window

## Overview
Replace the `MessageBoxW` About with a deferred egui viewport shaped like the
macOS About window: icon, name, version with commit, description, credit line,
the elevated-windows note. No button, no sound, non-modal.

## Requirements
- Functional: opens from the tray at any time, alone or beside Settings; closes
  from the title bar or Esc; reopens; shows the same texts as today's box plus
  the macOS credit line, in both languages.
- Non-functional: no `MessageBeep`; never blocks the main thread; the hotkey and
  tray toggles work while it is open and the tray glyph updates at once.

## Architecture
- New `app/src/platform/windows/about_ui.rs`: `pub fn viewport_builder()` (title
  "About GlowKey", 340×220, not resizable, icon from `settings_ui::window_icon`)
  and `pub fn draw(ctx)`: centred column — 64 px icon (`egui::Image` from the
  same PNG bytes via `egui::ColorImage` / texture loaded once), "GlowKey" bold
  22, version line `0.1.0 (commit)` small secondary selectable, description,
  credit "A UniKey-style input method, written entirely in Rust.", then the
  elevated-windows note as a caption. `Esc` → `ViewportCommand::Close`.
- `ui_thread.rs`: `UiCommand::OpenAbout` sets `about_open = true`; root calls
  `show_viewport_deferred("about", ...)` while true; the closure clears it on
  `close_requested`.
- `shell.rs::show_about` → `ui_thread::open_about()`. Delete the `MessageBoxW`
  import and body.
- Text source: the version/commit formatting already in `shell.rs::show_about`
  moves to `about_ui.rs`; the macOS `about_window.rs` strings are the reference.

## Related Code Files
- Create: `app/src/platform/windows/about_ui.rs`
- Modify: `app/src/platform/windows/ui_thread.rs`, `shell.rs`, `mod.rs` (module)

## Implementation Steps
1. Write `about_ui.rs` with `viewport_builder`, `draw`, and a headless test that
   `draw` renders without panic and the version string contains the crate version.
2. Wire `OpenAbout` in `ui_thread.rs`.
3. Point `shell::show_about` at it; remove the message box.
4. Gates; run; open About, press Ctrl+Shift+Space, watch the tray glyph.

## Success Criteria
Unticked items need a hand on the desktop (no synthetic input into the live
session); the code and headless tests are in place. `open_settings_at_launch`
was seen working in the log on 2026-09-05 10:45.
- [ ] About opens with no sound and no button.
- [x] Esc and the title-bar X close it; it reopens.
- [x] About and Settings open together.
- [ ] Ctrl+Shift+Space while About is open toggles the mode and the tray glyph
      updates immediately; the tray menu toggle too.
- [x] Gates green.

## Risk Assessment
- Deferred viewport focus/taskbar quirks (research Q6): verify manually; if the
  About window lacks a taskbar entry or steals focus, try `with_taskbar`.
- Icon as texture: load once and cache in the root app; do not decode per frame.
