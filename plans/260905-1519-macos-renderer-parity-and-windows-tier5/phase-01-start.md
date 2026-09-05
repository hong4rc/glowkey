---
phase: 1
title: "macOS renderer parity"
status: completed
priority: P2
effort: "3-4h"
dependencies: []
---

# Phase 1: macOS renderer parity

## Overview

Give the AppKit renderer the three things the egui renderer got in plan
`260905-1145`: checkboxes in the control column, count rows with a unit, and one
vertical rhythm. The unit strings move into `settings_spec.rs` so both shells
read one definition.

## Requirements

- Functional: a checkbox is a control in the control column, not at the pane
  margin; its caption starts under the checkbox's text. Count rows read
  `20 apps` / `20 ứng dụng`, `0 macros`, `0 words`. Row gaps are 6 (control to
  caption), 10 (row to row), 18 (before a section header).
- Non-functional: no spec row, order or wording changes; the Windows rendered
  text is byte-identical after the unit strings move; both renderers read the
  units from one place.

## Architecture

`settings_spec.rs` is the source. It already owns every string a row shows —
labels, captions, `MANAGE`, the hotkey preset names — and the count unit is the
one row string that leaked into a renderer. It comes back as a method on the
list identifier, next to `ListId::ALL`:

```rust
impl ListId {
    /// The unit a count is spoken in: "20 apps", "0 macros", "0 words".
    /// Both renderers show a count beside "Manage…"; the noun is content, so
    /// it lives with the rest of the row's words rather than in either shell.
    pub const fn unit(self) -> Text {
        match self {
            Self::ExcludedApps => Text::new("apps", "ứng dụng"),
            Self::Macros => Text::new("macros", "gõ tắt"),
            Self::PersonalWords => Text::new("words", "từ"),
        }
    }
}
```

The Windows renderer's inline `match list { ... t("apps", "ứng dụng") ... }` in
`settings_ui.rs` becomes `list.unit().get()`. The rendered string is unchanged;
`Text::get` calls the same `crate::strings::t`.

On macOS the two remaining items are geometry inside `tabs.rs`:

**Checkbox in the control column.** `add_row`'s `Control::Checkbox` arm builds
the `NSButton` and adds it bare unless the row is dependent, in which case it
indents by `DEPENDENT_INDENT`. Both cases become one: indent by
`label_width + COLUMN_GAP` (the control column, the same figure the form rows
already use as `label_width + 8.0`), plus `DEPENDENT_INDENT` when the row has an
`enabled_when`. The caption inset for a checkbox row becomes
`label_width + COLUMN_GAP + CHECK_GLYPH` — under the title, not under the box —
which is what the current bare `DEPENDENT_INDENT` was standing in for. Introduce
`COLUMN_GAP: f64 = 8.0` and `CHECK_GLYPH: f64 = 18.0` as named constants rather
than repeating `8.0` at five call sites, matching the Windows names so the two
renderers read alike.

`label_column_width` already filters `Control::Checkbox` out of the measurement,
so a long checkbox title still does not widen the column. Keep that: the column
is set by the form labels, and the checkbox lives in it.

**Rhythm.** The tab stack's uniform `setSpacing(6.0)` becomes the
control-to-caption gap and nothing else. `build_tab` already receives the last
view of each row from `add_row`; it uses it only before a section header. Change
it to set `ROW_GAP` after every row's last view, and `GROUP_GAP` before a header
(replacing `SECTION_GAP`'s 22 with 18). One gap per row, applied after the
caption when there is one and after the control when there is not — the rule
`finish_row` follows on Windows.

The hotkey row is the one row whose last view is neither its control nor a
`caption`: it is the status line, which `add_row` already returns. It needs no
special case as long as the caller uses the return value.

## Related Code Files

- Modify: `app/src/settings_spec.rs` — add `ListId::unit`; add its test.
- Modify: `app/src/platform/windows/settings_ui.rs` — the `Control::List` arm
  reads `list.unit().get()`; delete the inline match.
- Modify: `app/src/prefs/tabs.rs` — `COLUMN_GAP`, `CHECK_GLYPH`, `ROW_GAP`,
  `GROUP_GAP` constants; `SECTION_GAP` retired; the `Control::Checkbox` arm's
  indent and caption inset; `build_tab`'s per-row spacing;
  `refresh_list_counts` formats `{count} {unit}`.

## Implementation Steps

1. **Measure first.** Before changing the checkbox indent, compute the room:
   `WINDOW_SIZE.0 - 2*PANE_INSET - (label_width + COLUMN_GAP) - CHECK_GLYPH`
   against the longest checkbox title in `TABS` in each language (a short
   `#[test]` over the spec with a character-count proxy is enough to size the
   risk; the true metric needs AppKit). If it does not fit, give the checkbox a
   wrapping cell (`setLineBreakMode` word-wrap and a `preferredMaxLayoutWidth`,
   as `wrapping_caption` does) rather than leaving it at the margin.
2. Add `ListId::unit` to the spec with the doc comment above.
3. Point the Windows `Control::List` arm at it; delete the inline `t(..)` match.
4. In `tabs.rs`, add the four constants and retire `SECTION_GAP`; replace the
   bare `8.0` in the five `caption_inset` expressions with `COLUMN_GAP`.
5. Indent the checkbox: one `self.indented(&checkbox, column + dependent, mtm)`
   call for both the plain and the dependent case, with the caption inset
   `column + CHECK_GLYPH + dependent`.
6. Rewrite `build_tab`'s loop: after each `add_row`, set `ROW_GAP` after the
   returned view; before each section header after the first, set `GROUP_GAP`
   after the previous row's view (the later call wins, so set the header gap
   after the row gap).
7. `refresh_list_counts`: `format!("{count} {}", list.unit().get())`.
8. Run the gate list (phase 3 records it). The
   `aarch64-apple-darwin --all-targets` clippy run is the one that sees this
   file at all.

## Success Criteria

- [x] `ListId::unit` exists in `settings_spec.rs` and is the only place the
      three nouns appear in the tree (`grep` for `ứng dụng` finds one hit).
- [x] A spec test asserts all three units in both languages.
- [x] Windows Settings still shows `20 apps` / `0 macros` / `0 words`,
      unchanged, and macOS now shows the same shape.
- [x] No `Control::Checkbox` view is added to the stack without the control
      column indent.
- [x] `SECTION_GAP` is gone; `ROW_GAP` 10 and `GROUP_GAP` 18 are applied once
      per row and once per header.
- [x] `settings_spec.rs`'s existing tests pass unchanged.
- [x] `cargo clippy --target aarch64-apple-darwin -p glowkey --all-targets --
      -D warnings` is clean.

## Outcome

Done, with two deviations from the steps above.

**Step 1 became a runtime measurement, not a proxy test.** The character-count
proxy put the longest Vietnamese title (42 characters,
"Tự động khôi phục từ không phải tiếng Việt") right at the edge of the room a
measured column leaves — too close to settle from a Windows host without AppKit's
metrics. So the renderer asks AppKit instead: `fit_checkbox` compares the
button's `intrinsicContentSize().width` against the room at its inset and turns
on the cell's `setWraps:` plus a pinned width only when it overflows. A title
that fits keeps its single line, in either language, and no figure is guessed.

**One extra fix, committed separately.** The `cargo doc` gate in handoff §11 was
red on `main` before this change and on both hosts: `main.rs` linked
`platform::macos` and `platform::windows` as intra-doc links, and exactly one of
those modules exists in any build, so each host failed on the other's link.
They are code spans now, with the reason written next to them. Verified
pre-existing by stashing the phase's work and re-running the gate on `HEAD`.

The `aarch64-apple-darwin --all-targets` clippy gate was itself checked before
being trusted: a deliberate type error appended to `tabs.rs` made it fail, and
removing it made it pass, so the gate demonstrably compiles the one file this
phase changed.

## Risk Assessment

- **Checkbox titles overflow at the control column.** Step 1 measures before
  committing to the layout. *Signal:* the measurement is over budget in either
  language. *Response:* wrap the title; do not return the checkbox to the
  margin, which is the fault being fixed.
- **A missed `setCustomSpacing:` leaves one row on the old uniform 6.** Every
  row must go through the returned view. *Signal:* a row in `build_tab` whose
  `add_row` result is discarded. *Response:* the loop assigns unconditionally;
  no `if let` on the last row.
- **`GROUP_GAP` and `ROW_GAP` both target the same view before a header.**
  AppKit keeps the last `setCustomSpacing:` for a view, so order matters.
  *Signal:* section gaps measure 10 rather than 18 on the Mac. *Response:*
  the header gap is set after the row gap, which step 6 states explicitly.
- **This phase cannot be seen.** Nothing here is verified visually until the
  macOS runtime pass. Phase 3 adds these three items to the §11 item 1 list so
  the next Mac session watches for them.
