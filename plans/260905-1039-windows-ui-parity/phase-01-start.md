---
phase: 1
title: "Long-lived UI thread, Settings as a deferred viewport"
status: pending
priority: P1
effort: "6h"
dependencies: []
---

# Phase 1: Long-lived UI thread, Settings as a deferred viewport

## Overview
Run eframe once, on its own thread, for the process's life. Settings becomes a
deferred viewport that opens and closes on command, so it can reopen, and the
main thread never blocks on it.

## Requirements
- Functional: Settings opens from the tray any number of times; on close its
  result reaches the main thread and is merged and saved exactly as
  `shell::open_settings` does today; `open_settings_at_launch` opens it at start.
- Non-functional: nothing on the hook path waits on the UI thread; the UI thread
  never touches `hook::with_session` (thread-local to the main thread); idle cost
  at rest is one hidden window.

## Architecture
```
main thread (hook + tray loop)            glowkey-ui thread
  tray Settings ──► ui_thread::open_settings(snapshot)
                       │ mpsc UiCommand::OpenSettings(Settings)
                       │ ctx.request_repaint()  ───────────►  UiHost::update
                       │                                        ├ drain commands
                       │                                        ├ show_viewport_deferred("settings")
                       │                                        │    SettingsApp (Arc<Mutex>) draws tabs
                       │                                        │    on close_requested → finalize → result
  run_message_loop ◄── PostThreadMessageW(main_tid, WM_GLOWKEY_UI) ◄┘ shell::deliver_settings_result(baseline, updated)
   take_pending_ui_result → shell::apply_settings(baseline, updated)  (merge_settings, save, hook::set_state)
```
- New `app/src/platform/windows/ui_thread.rs`: `start()` spawns the thread and
  calls `eframe::run_native` once with `NativeOptions { event_loop_builder:
  Some(Box::new(|b| { use winit::platform::windows::EventLoopBuilderExtWindows;
  b.with_any_thread(true); })), viewport: ViewportBuilder::default().with_visible(false), .. }`.
  Root app `UiHost { rx, settings: Option<Arc<Mutex<SettingsApp>>>, about_open, ... }`.
  Root `update`: if `close_requested()` → `CancelClose` (the root never closes).
  `static CTX: OnceLock<egui::Context>` set from `CreationContext`; `static TX:
  OnceLock<Sender<UiCommand>>`. `pub fn open_settings(snapshot)`, `open_about()`
  send then `request_repaint`.
- `settings_ui.rs`: `show()` and its `run_native` go away. Keep `SettingsApp`,
  add `pub fn viewport_builder()` (title, size, min size, icon) and
  `pub fn draw(&mut self, ctx)`; the viewport closure locks the `Arc<Mutex>` and
  calls `draw`; when `close_requested` is seen inside the viewport, `finalize()`
  and hand `(baseline, result)` to `shell::deliver_settings_result`. Root then
  drops the app so the viewport disappears.
- `shell.rs`: split `open_settings` into `open_settings()` (snapshot →
  `ui_thread::open_settings`) and `apply_settings(baseline, updated)` (the
  existing merge/save/rebuild body). Add `deliver_settings_result` (stores in a
  `Mutex<Option<..>>`, posts `WM_GLOWKEY_UI` to the main thread id recorded at
  `run()`), and `take_pending_result` called from `hook::run_message_loop` beside
  `take_pending_save`.
- `mod.rs::run`: record main thread id; `ui_thread::start()` after `hook::set_state`;
  after the tray is up, `if settings.open_settings_at_launch { shell::open_settings() }`.
  After `run_message_loop` returns and cleanup: `std::process::exit(0)` so the UI
  thread's blocked `run_native` does not keep the process alive.
- winit is not a direct dependency: add `winit = "0.30"` to `app/Cargo.toml`
  under the Windows target (already in the lock via eframe) for the
  `EventLoopBuilderExtWindows` trait, or use `eframe::egui_winit::winit`
  re-export if present in 0.29 (check first; prefer the re-export).

## Related Code Files
- Create: `app/src/platform/windows/ui_thread.rs`, `docs/decisions/0011-windows-ui-thread.md`
- Modify: `app/src/platform/windows/settings_ui.rs`, `shell.rs`, `hook.rs`
  (expose a `wake_main(msg)` / pending-result hook), `mod.rs`, `tray.rs` (Quit
  path), `app/Cargo.toml` (winit only if needed)

## Implementation Steps
1. Check whether eframe 0.29 re-exports winit (`eframe::egui_winit::winit`); if
   not, add the target-scoped dependency.
2. Write `ui_thread.rs` with `UiCommand`, `UiHost`, `start`, `open_settings`,
   `open_about` (about wired in phase 2; command variant exists now).
3. Refactor `settings_ui.rs`: remove `show`, add `viewport_builder`, `draw`,
   `take_result`; keep every test; add a headless test that a `UiHost` given
   `OpenSettings` renders the settings viewport closure and that dropping it
   after close leaves no result unread.
4. Refactor `shell.rs` into `open_settings` / `deliver_settings_result` /
   `apply_settings`; keep `merge_settings` and its tests untouched.
5. `hook.rs`: generalise `wake()` into `post_to_main(msg)` with the main thread
   id captured at `set_state`; in `run_message_loop` handle the new pending slot.
6. `mod.rs`: start the thread, honor `open_settings_at_launch`, exit the process
   after cleanup.
7. Write decision 0011.
8. Gates: tests, both clippy targets, then build release, stop the running
   GlowKey, start the new one, open Settings twice from the tray.

## Success Criteria
Unticked items need a hand on the desktop (no synthetic input into the live
session); the code and headless tests are in place. `open_settings_at_launch`
was seen working in the log on 2026-09-05 10:45.
- [ ] Settings opens, closes, reopens three times in one process (log shows no
      `RecreationAttempt`).
- [ ] An edit made in the second open is saved and applied.
- [ ] Typing Vietnamese in Notepad works while Settings is open.
- [x] `open_settings_at_launch = true` opens Settings at startup.
- [ ] Tray Quit ends the process (no GlowKey.exe left).
- [x] Gates green on both targets.

## Risk Assessment
- egui #3655/#5229: never send `Visible` to the root. If a viewport fails to
  reappear on reopen, the cause is a stale `ViewportId` state; recreate the
  `SettingsApp` per open rather than reuse.
- Deadlock between `Arc<Mutex<SettingsApp>>` and the result delivery: deliver
  after the lock is released.
- `with_any_thread` + glow: all GL stays on the UI thread; nothing else touches it.
