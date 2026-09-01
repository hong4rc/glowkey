---
title: "GlowKey — menu bar UI, per-app ignore control, and auto-fix"
description: "Give GlowKey a menu bar, a per-application enable/disable model (quick current-app toggle + a managed list), persisted settings, and an optional auto-fix that restores invalid Vietnamese to the raw English word (exit, not eĩt)."
status: pending
priority: P1
effort: "1.5-2 weeks"
tags: [glowkey, macos, objc2, ui, ignore-list, auto-fix, telex]
created: 2026-09-01
---

# GlowKey — menu bar UI, per-app ignore control, and auto-fix

## Overview

GlowKey now types Vietnamese via a CGEventTap. This plan adds the day-to-day
controls a real user needs: a **menu bar item**, an easy **per-application
enable/disable** model (toggle the current app in one click, plus a managed list),
**persisted settings** so choices survive a restart, and an optional **auto-fix**
that restores a word to its raw English when the Telex result is not valid
Vietnamese (`exit`, not `eĩt`).

The Vietnamese engine and the ignore-list data model already exist
(`crates/glowkey-engine`): `Session` has `toggle_mode`, `set_style`,
`exclusions_mut`, and `ExclusionList` has `add`/`remove`/`toggle`/`is_excluded`/
`ids`. This plan is mostly the **macOS shell** (objc2 AppKit: `NSStatusItem`,
menus, a preferences window), plus **persistence** and one **engine addition**
(commit-time auto-fix).

## The per-application model, made precise

The user asked for two things that are two views of one idea:

- **A disable list** — the apps where Vietnamese is off (terminals, editors).
- **A quick current-app switch** — "I'm in this app, turn it on/off" without
  opening anything.

Both operate on the **same set**: the ignore list (`ExclusionList`). The menu bar
toggles the frontmost app in that set; the preferences window edits the whole set.
This is a **deny list**: Vietnamese is on everywhere by default, off for listed
apps — which matches the examples (Terminal off, Chrome on). The default set seeds
terminals and IDEs, exactly as today.

An **allow list** (off by default, on only for listed apps) is a plausible
alternative some users want. It is **out of scope** for v1 unless testing shows
the deny-list default is wrong; noted as an open question.

## Auto-fix, made precise

When Telex produces something that is not a valid Vietnamese syllable, restore the
raw keys. Example: typing `exit` in Telex mangles into `eĩt` (the `x` is the ngã
tone key); at the word boundary GlowKey deletes the mangled text and re-types
`exit`.

- It runs **at the word boundary** (space, punctuation, Enter), the moment the
  syllable is final — the standard UniKey/EVKey "auto-restore" behaviour.
- Validity is judged by the engine's syllable check (`vi`'s validator plus the
  tone×coda rule). If invalid → emit the raw keystrokes in place of the rendering.
- It is an **option** (default on), toggleable from the menu and preferences.
- It does **not** need a dictionary for the `eĩt` case — that string is
  structurally invalid Vietnamese, so structural validation catches it. Words that
  are *both* valid Vietnamese and English (rare) are a known limit, not this
  feature's job.

## Goals

| # | Goal | Priority |
|---|------|----------|
| 1 | Menu bar item showing VN / EN / excluded, with a quick current-app enable/disable toggle | P1 |
| 2 | Preferences window to manage the full ignore list (add/remove apps by picker) | P1 |
| 3 | Settings persist across restarts (ignore list, auto-fix, placement style, default mode) | P1 |
| 4 | Auto-fix: at a word boundary, restore raw keys when the result is invalid Vietnamese (`exit`, not `eĩt`) | P1 |
| 5 | Auto-fix and per-app changes take effect immediately, no restart | P1 |
| 6 | Placement style (`hoà` vs `hòa`) selectable in preferences | P2 |

## Non-goals (v1)

- Allow-list mode (off-by-default). Deny list only unless testing forces the change.
- Dictionary/spell-check auto-restore for words that are valid in both languages.
- Custom hotkey remapping UI (the ⌃⇧Space toggle stays fixed; show it, don't edit it).
- Macros / abbreviation expansion, VNI/VIQR, per-field control inside one app.
- Localizing the UI (English strings via `.strings`, Vietnamese later).

## Architecture

```
crates/glowkey-engine/
  src/config.rs      NEW  Settings struct (exclusions, auto_fix, style, default mode)
                          + (de)serialization for persistence
  src/lib.rs         MOD  Session gains auto-fix at commit; expose settings snapshot
app/src/
  tap.rs             MOD  boundary path consults auto-fix; menu/prefs mutate Session
  settings_store.rs  NEW  load/save Settings to a JSON file in Application Support
  menu_bar.rs        NEW  NSStatusItem + menu (VN/EN, toggle current app, auto-fix, prefs, quit)
  prefs_window.rs    NEW  NSWindow with the ignore-list editor + toggles
  app_info.rs        NEW  resolve a bundle id to a display name + icon (for the list/menu)
```

The engine stays platform-free and testable. The macOS shell owns all objc2/AppKit
code, which — like the tap — can only be verified by running and looking, not by
unit tests. Every phase says which half it is.

### State ownership and threading

`TapState` owns the `Session` behind a `RefCell` on the main run loop thread. The
menu bar and preferences window also run on the main thread (AppKit requirement),
so they mutate the same `Session` through the same `RefCell` without locks. A
settings change (toggle app, flip auto-fix, pick style) updates the `Session` and
writes the settings file. The tap reads the `Session` on the next keystroke, so
changes are immediate.

## Phases

| # | Phase | Half | Status |
|---|-------|------|--------|
| 1 | [Settings & persistence](./phase-01-settings-persistence.md) | engine + shell, testable | ✅ Done |
| 2 | [Auto-fix restore](./phase-02-auto-fix-restore.md) | engine, testable | ✅ Done |
| 3 | [Menu bar UI](./phase-03-menu-bar-ui.md) | shell (objc2), GUI-verified | ✅ Done (code) |
| 4 | [Preferences window](./phase-04-preferences-window.md) | shell (objc2), GUI-verified | ✅ Done (code) |
| 5 | [Integration & live test](./phase-05-integration-and-live-test.md) | live | ⬜ Needs your run |

```
1 (settings) ──> 2 (auto-fix) ──┐
1 ──────────────────────────────┼──> 3 (menu bar) ──> 4 (prefs window) ──> 5 (live)
                                 └──> 3 also needs 2 (auto-fix toggle in menu)
```

Phases 1–2 are pure Rust and fully unit-testable. Phases 3–4 are objc2 AppKit and
verifiable only by running. Phase 5 ties it together on real apps.

## Success criteria

- [ ] Menu bar shows the current state (VN / EN / excluded for this app)
- [ ] One click in the menu toggles Vietnamese for the frontmost app; the effect is immediate
- [ ] Preferences window lists excluded apps with name + icon; add via picker, remove works
- [ ] Quitting and relaunching restores the ignore list, auto-fix, style, and mode
- [ ] With auto-fix on, typing `exit` yields `exit` (not `eĩt`); `hoongf` still yields `hồng`
- [ ] With auto-fix off, `exit` stays as the Telex result
- [ ] Placement style toggle changes `hoà` ⇄ `hòa`
- [ ] No new networking framework linked (privacy guard still passes)
- [ ] Engine crate still compiles and tests on Linux (no macOS leak into the engine)

## Risk register

| Risk | Signal | Response |
|---|---|---|
| objc2 AppKit menu/window is unfamiliar territory (like the tap was) | Phase 3/4 compile-iterate is slow | Compile-iterate against the vendored crate sources, as done for the tap; budget for it |
| Auto-fix restore races/mis-deletes at the boundary (same class as the Chrome bug) | Live test shows leftover or missing chars after restore | Reuse the single-channel session-posting path; the restore is just a bigger `(backspaces, insert)` diff the shell already knows how to emit |
| Menu/prefs mutate `Session` while the tap callback holds the borrow | A `borrow_mut` panic, or a UI action dropped | All on the main thread; use `try_borrow_mut` and retry/skip, never the panicking borrow |
| `exit`-class validity check also rejects real Vietnamese | A valid word gets restored to raw | The check is the engine's existing syllable validator; add the exact `exit`/`eĩt` case and a batch of real words as tests before shipping |
| Settings file corrupt or missing | Load fails on startup | Fall back to defaults, never crash; write atomically (temp + rename) |

## Open questions

1. **Deny list vs allow list.** v1 ships deny-list (on by default). Do you ever
   want allow-list (off by default, on only for chosen apps)? → revisit after use.
2. **Auto-fix timing.** Restore at the word boundary (chosen). Do you also want a
   manual "undo transform" key (like `z`) for mid-word fixes? → v2 if wanted.
3. **Chrome delivery bug (parallel).** The session-posting fix for Chrome's
   `hoồng` is still unverified. It is independent of this plan, but Phase 5's live
   test will exercise the same emit path — if Chrome is still wrong, that surfaces
   there too and is fixed separately.

<!-- slug: glowkey-ui-ignore-autofix -->
