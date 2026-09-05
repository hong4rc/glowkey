---
phase: 2
title: "Alignment, rhythm and controls"
status: pending
priority: P1
effort: "4h"
dependencies: []
---

# Phase 2: Alignment, rhythm and controls

## Overview
One control axis, even spacing, a popup for the long hotkey names, keycaps for
shortcuts, counts with units, and a taller About.

## Requirements
- Functional: same rows, same strings, same bindings.
- Non-functional: captures show every control and caption starting at the
  control column; measured gaps 10 / 6 / 18.

## Architecture
`settings_ui.rs` helpers:
- `control_row(ui, label, caption, add)` — unchanged shape; caption inset =
  `label_column_width + 8` (was `INDENT` 22).
- `checkbox_row(ui, value, label, caption)` — renders in the control column:
  an empty label cell of `label_column_width`, then the checkbox with its text;
  the caption inset = column + 8 + checkbox glyph (18). Dependent rows
  (`enabled_when`) indent a further 20 inside the control column.
- Rhythm constants: `ROW_GAP = 10`, `CAPTION_GAP = 6`, `GROUP_GAP = 18`. A row
  adds `CAPTION_GAP` before its caption and `ROW_GAP` after whichever is last.
  Remove the `ui.add_space(4.0)` sprinkled in each helper.
- `Control::ToggleHotkey` on Windows: `egui::ComboBox::from_id_salt("hotkey")
  .selected_text(hotkey_display(current)).width(200)`; items = the three
  offered presets plus the current value when it is not among them (custom or
  Alt+Space). Style via `raise_controls` (already white + hairline); chevron is
  egui's.
- `keycaps(ui, text)`: split on `+`, draw each key as a small `Frame` (rounding
  4, hairline, fill white/grey 92, padding 4×1, text 11.5) with 2 pt gaps.
  Used by `Control::Shortcut` and by `caption()` when the text contains a
  known shortcut spelling: split the caption at `shortcut_display(s)`
  occurrences and paint keycaps inline with `ui.horizontal_wrapped`.
- `Control::List`: count text `format!("{n} {unit}")` with unit from
  `t("apps","ứng dụng")`, `t("macros","gõ tắt")`, `t("words","từ")`, in
  `secondary_color`, then the Manage… button.
- Section header gap: `GROUP_GAP` 18 (from 14). Tab strip top margin 12 (from 10).
- `about_ui.rs`: height 280; a small "Copy" (`t("Copy","Chép")`) button beside
  the version that calls `ctx.copy_text(build_string())`.

## Related Code Files
- Modify: `app/src/platform/windows/settings_ui.rs`, `about_ui.rs`

## Implementation Steps
1. Constants and the two row helpers; remove ad-hoc `add_space` calls.
2. Checkbox rows into the control column; dependent indent inside it.
3. Hotkey popup.
4. `keycaps` and its use in the shortcut row and captions.
5. Count units.
6. About height and Copy.
7. Tests: headless render of all tabs still passes; a test that `keycaps`
   splits "Ctrl+Shift+E" into three; a test that the hotkey popup lists a
   custom value when the draft holds one.
8. Gates; build; capture all four tabs and About.

## Success Criteria
- [ ] All controls and captions on one x per capture.
- [ ] Row gap 10, caption gap 6, section gap 18 by pixel measurement.
- [ ] Hotkey popup lists presets and a foreign value; row has ≥ 20 pt spare.
- [ ] Keycaps in the shortcut row and in the two captions that name a shortcut.
- [ ] "20 apps", "0 macros", "0 words".
- [ ] About 280 tall, Copy copies the version.
- [ ] Gates green.

## Risk Assessment
- `horizontal_wrapped` with mixed labels and frames can misalign baselines;
  set the caption row height to the keycap height.
- The macOS renderer shares no helper here; no compile impact expected beyond
  `settings_spec` untouched.
