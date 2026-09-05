---
title: "Windows settings UX polish: every item, from screenshots"
description: "Item-by-item improvements to the Windows settings and About windows: list editors as real windows, one alignment axis, even rhythm, a popup for long hotkey names, keycaps for shortcuts, keyboard and screen-reader access for the custom controls."
status: pending
priority: P2
effort: "1-1.5 days"
tags: [glowkey, windows, ui, ux, egui, settings]
created: 2026-09-05
blockedBy: []
blocks: []
---

# Windows settings UX polish

Reviewed from real captures of the running build (11:41), taken with
`PrintWindow` and tab clicks posted to the window itself:
`screenshots/settings-{general,typing,corrections,apps,excluded}.png`,
`screenshots/about.png`. Rules applied: Apple HIG for settings forms and the
ak-ui-ux-pro-max Quick Reference (alignment, touch/target size, focus states,
keyboard nav, spacing rhythm, progressive disclosure).

## Findings, item by item

| # | Item | What the capture shows | Improvement |
|---|---|---|---|
| 1 | List editors (Excluded apps, Macros, Personal words) | `egui::Window` overlays inside Settings: cover the tabs, persist when the tab changes, swallow clicks behind them, list scroll capped at 200 pt inside a 320-pt box, no taskbar entry, no Esc | Real windows: deferred viewports on the UI thread, like About. Own title, icon, size; list fills the window; Esc closes. This is what macOS does. |
| 2 | Alignment | Two axes: form controls start at the label column (x≈150), checkboxes at the left margin (x≈30). Captions under checkboxes indent 22, captions under form rows indent to neither | One control axis. Checkboxes sit in the control column with no label text, as in the macOS form. Every caption aligns to the control column. |
| 3 | Vertical rhythm | A checkbox row without a caption gets the same trailing gap as one with, so "Launch at login" → "Open at launch" is a 20-pt hole while captioned rows are tight | Constants: 6 between control and its caption, 10 between rows, 18 before a section header. One `row_gap` applied after the caption or the control, never both. |
| 4 | Toggle Vietnamese hotkey | Three long segments ("Ctrl+Shift+Space") end 3 pt from the window edge; a fourth (a Mac-recorded custom) would overflow | A popup button (`egui::ComboBox` styled like `NSPopUpButton`: white, hairline, chevron) on Windows. Spec unchanged; the renderer picks the control. macOS keeps its segmented glyphs. HIG: segmented for ≤5 short labels, popup otherwise. |
| 5 | Shortcut display ("Ctrl+Shift+E", and inside captions) | Plain body text, same colour as the label; hard to scan | Keycaps: each key in a small rounded badge (white/hairline in light, grey in dark), monospace-free, 11.5 pt. In the read-only row and where a caption names a shortcut. |
| 6 | Count + Manage rows | "Personal words 0 Manage…", "Excluded apps 20 Manage…": a bare number between label and button | Secondary text with a unit, "20 apps", "0 words", "0 macros", then the button. Count text in the caption colour. |
| 7 | Segmented control (tabs and choices) | Track and raised segment read well. No keyboard focus, no arrow keys, no screen-reader name | Focusable: Tab reaches it, ←/→ move the selection, a focus ring on the raised segment; `WidgetInfo::selected` per segment for AccessKit. |
| 8 | Checkboxes | Hairline and fill fine. Caption colour fine (≈6.6:1) | Only the alignment change from item 2. |
| 9 | Section headers | Bold grey, right | Keep. Raise the gap above from 14 to 18 so sections separate from the row above. |
| 10 | Tab strip | Centred segmented control, good | Keep. 2 pt more top padding so it does not sit on the title bar. |
| 11 | About | Reads well; the elevated-windows note touches the bottom edge (window 250 pt, content ≈ 262) | Height 280. Version line gets a small "Copy" button beside it (the one string a user is asked to quote). |
| 12 | Window | 460×540, resizable, light. Dark theme unverified | Keep. Phase 4 captures dark by flipping `AppsUseLightTheme` for the capture only, then restoring it. |
| 13 | Language segmented | "System / Tiếng Việt / English" — fine | Keep. |
| 14 | Input method / Tone marks | Fine; "Modern hoà / Classic hòa" double-space labels come from the spec | Keep. |
| 15 | Startup captions | None; the two checkboxes are self-explanatory | Keep. |

## Phases

| # | Phase | Status | Depends on |
|---|---|---|---|
| 1 | [List editors as real windows](./phase-01-start.md) | pending | — |
| 2 | [Alignment, rhythm and controls](./phase-02-alignment-rhythm-and-controls.md) | pending | — |
| 3 | [Keyboard and accessibility](./phase-03-keyboard-and-accessibility.md) | pending | 2 |
| 4 | [Verify and document](./phase-04-verify-and-document.md) | pending | 1, 2, 3 |

## Acceptance criteria

1. Excluded apps, Macros and Personal words each open as their own window
   from Manage…, alongside Settings; Esc and X close; reopen works; the list
   fills the window and scrolls; edits still reach the draft and save on
   Settings close exactly as today.
2. Every control on every tab starts at one x (the control column); every
   caption starts there too. Row gaps are 10, control-to-caption 6, section
   gap 18, verified by capture.
3. The hotkey is a popup on Windows; all four presets and a saved custom or
   Alt+Space value are reachable; the row fits with ≥ 20 pt to spare.
4. Shortcuts render as keycaps in the shortcut row and in captions.
5. Count rows read "20 apps", "0 macros", "0 words" in the caption colour.
6. Tab reaches the segmented controls and the popup; ←/→ change a segmented
   selection; a focus ring is visible; AccessKit exposes each segment's name
   and selected state (headless test on `WidgetInfo`).
7. About is 280 pt tall with a Copy button beside the version.
8. `cargo test -p glowkey`, clippy (Windows and `aarch64-apple-darwin`) green.
   macOS renderer untouched except where the spec-neutral row layout helper
   changes require a compile fix.

## Non-goals

- No spec (`settings_spec.rs`) content change: same rows, same strings.
- No macOS visual change.
- No hotkey recorder on Windows.

## Risks

- Deferred list viewports share `SettingsApp` state behind the mutex with the
  Settings viewport; a list closure and the settings closure must not hold the
  lock across each other. Same pattern as About: lock, draw, release.
- egui `ComboBox` popup is an `Area` inside the viewport; fine.
- Focus ring and arrow keys in a hand-painted control: egui's `Response` has
  `has_focus`; keys come from `ui.input`. Test headlessly by sending
  `Event::Key` after `request_focus`.

## Rollback

Revert the branch. Phases are independent except 3 on 2.
