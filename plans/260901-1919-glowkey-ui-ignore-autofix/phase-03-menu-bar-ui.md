---
phase: 3
title: "Menu bar UI"
status: pending
priority: P1
effort: "3-4d"
dependencies: [1, 2]
---

# Phase 3: Menu bar UI

## Overview

An `NSStatusItem` in the menu bar showing the current state and offering the
one-click controls people use constantly: toggle Vietnamese for this app, flip
auto-fix, open preferences, quit. objc2 AppKit — verifiable only by running.

## Requirements

**Functional**
- Menu bar icon/text reflecting state: **VN**, **EN**, or **excluded here**.
- Menu items:
  - `Disable Vietnamese for <AppName>` / `Enable Vietnamese for <AppName>` —
    toggles the frontmost app in the ignore list. Label updates to match state.
  - `Vietnamese` / `English` mode toggle (mirrors ⌃⇧Space), showing a checkmark.
  - `Auto-fix` checkbox item.
  - `Preferences…` (opens the Phase 4 window; may be a stub until then).
  - `Quit GlowKey`.
- Every action updates the live `Session` and saves settings immediately.
- The current-app name comes from the frontmost app's bundle id (Phase's
  `app_info` helper).

**Non-functional**
- Runs on the main thread with the run loop already used by the tap.
- Mutates `Session` via `try_borrow_mut` (never the panicking borrow) since the
  tap callback also holds it.
- `LSUIElement` is already set, so the status item shows with no Dock icon.

## Architecture

```
app/src/menu_bar.rs   NSStatusItem + NSMenu; targets/actions wired to a controller
app/src/app_info.rs   bundle id -> display name (+ icon later) via NSRunningApplication / NSWorkspace
```

The status item and its menu are built at startup, after the tap is running. Menu
actions need Objective-C targets: define a small objc2 class (like the tap
controller) whose methods are the menu actions, holding a pointer to the shared
`TapState` (or a shared `Rc<TapState>`). AppKit calls these on the main thread.

Because the tap callback and the menu actions both touch `TapState.session`, and
both are on the main thread, they never truly overlap — but use `try_borrow_mut`
defensively and skip on contention.

Updating labels: rebuild or update the menu when it is about to open
(`menuNeedsUpdate:`), reading the current frontmost app and session state so
"Disable for <App>" always names the right app and shows the right verb.

## Related Code Files

- Create: `app/src/menu_bar.rs`
- Create: `app/src/app_info.rs`
- Modify: `app/src/tap.rs` / `main.rs` — construct the status item after the tap;
  share `TapState` with the menu controller; add a `save_settings` call on actions
- Modify: `app/Cargo.toml` — ensure `NSStatusItem`, `NSMenu`, `NSMenuItem`,
  `NSStatusBar`, `NSRunningApplication` features are enabled (mostly already are)

## Implementation Steps

1. `app_info`: resolve a bundle id to a display name (frontmost app's
   `localizedName`); return the icon later if easy.
2. Define the menu controller objc2 class with action methods
   (`toggleCurrentApp:`, `toggleMode:`, `toggleAutoFix:`, `openPreferences:`,
   `quit:`), holding a shared handle to `TapState`.
3. Build the `NSStatusItem` (retain it for the process lifetime) and its `NSMenu`;
   set the controller as target.
4. Implement `menuNeedsUpdate:` to refresh labels/checkmarks from current state.
5. Each action mutates the session (`try_borrow_mut`), saves settings, and updates
   the status item title/icon.
6. `Preferences…` opens the Phase 4 window (stub: a no-op or a log line until
   Phase 4 lands).
7. Verify by running: the item appears, labels are correct, toggling actually
   changes typing behaviour in a test app, and the state persists after relaunch.

## Success Criteria

- [ ] Status item appears and shows VN / EN / excluded correctly
- [ ] "Disable/Enable for <App>" names the real frontmost app and toggles it; typing changes immediately
- [ ] Mode toggle and auto-fix checkbox work and show correct check state
- [ ] Actions persist (relaunch keeps them)
- [ ] Quit works cleanly
- [ ] No `borrow_mut` panic under normal use

## Risk Assessment

**objc2 AppKit menus are new territory** (as the tap was). Signal: slow
compile-iteration on class/target/action wiring. Response: compile-iterate against
the vendored objc2-app-kit sources; budget for it; reuse the `define_class!`
pattern already proven in the tap.

**Assumption at risk:** that the frontmost app at menu-open time is the app the
user means. With the menu itself focused, `NSWorkspace.frontmostApplication` may
report the menu-owning process. Signal: "Disable for GlowKey" appears. Response:
capture the frontmost app id in the tap on each keystroke (already tracked) and
use that cached value for the menu label, not a fresh query at menu-open.

**Shared `TapState` ownership across the tap callback and menu.** Signal: a borrow
panic or a dropped action. Response: single-threaded main run loop + `try_borrow_mut`
everywhere; never hold a borrow across an AppKit call.
