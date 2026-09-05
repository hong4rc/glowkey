---
title: "macOS renderer parity, and the Windows desktop checks nobody has run"
description: "Port the three Windows settings-polish items to the AppKit renderer with the shared spec still the single source, then run the Windows Tier 5 desktop checks on this machine and fix what they find."
status: completed
priority: P2
effort: "0.5-1 day"
tags: [glowkey, macos, windows, ui, settings, verification]
created: 2026-09-05
blockedBy: []
blocks: [260904-2127-glowkey-cross-platform-port]
---

# macOS renderer parity, and the Windows desktop checks

## Where this starts

The engine split (`decisions/0012`, plan `260905-1333`) landed on `main` today,
on top of the shared settings spec (`0010`) and the Windows UI thread (`0011`).
Both shells compile, the headless suites pass, and the whole macOS side of it is
compile-checked only.

The session runs on **Windows**. The user's own instruction covers this case:
the macOS runtime pass (handoff §11 item 1) is skipped, and the Windows desktop
checks in `docs/manual-verification-windows.md` **Tier 5** are the equivalent
list. Everything the macOS pass would have caught stays owed; §11 item 1 is not
struck off by this plan and phase 3 says so in the handoff.

What is *not* blocked by the platform is handoff §11 item 3, which the user has
now answered: the three Windows settings-polish items from plan
`260905-1145` should be ported to the AppKit renderer. That code is written and
gated on Windows and Linux hosts through the `aarch64-apple-darwin` clippy
target; only the pixels need a Mac, and the pixels are what phase 1 hands back
to the macOS runtime pass whenever it happens.

## The three parity items, as they stand today

| # | Item | Windows (after `260905-1145`) | macOS (`app/src/prefs/tabs.rs`) today |
|---|---|---|---|
| 1 | Checkbox alignment | `checkbox_row` indents by `column + COLUMN_GAP`, so a checkbox is a control in the control column; its caption sits at `column + CHECK_GLYPH` | `Control::Checkbox` is added bare (`control_view(checkbox)`), so it starts at the pane's left margin — two axes, the exact fault item 2 of `260905-1145` named |
| 2 | Count units | `"{count} {unit}"` with the unit from an inline `t("apps", "ứng dụng")` match in `settings_ui.rs` | `refresh_list_counts` writes `count.to_string()` — a bare number between label and button |
| 3 | Vertical rhythm | `CAPTION_GAP` 6, `ROW_GAP` 10, `GROUP_GAP` 18, applied edge to edge once per row in `finish_row` | one uniform `setSpacing(6.0)` on the tab stack, plus `SECTION_GAP` 22 before a header. A captioned row and a bare row get the same trailing gap |

Item 2 has a spec consequence the user's instruction settles: *the shared spec
stays the source*. The unit strings are content, and content belongs in
`settings_spec.rs`. They move to `ListId::unit() -> Text`, the Windows inline
match is deleted in favour of it, and macOS reads the same method. Nothing about
the rows, their order, or their wording changes — the spec grows one accessor
for a string that already exists in the product.

## Phases

| # | Phase | Status | Depends on |
|---|-------|--------|------------|
| 1 | [macOS renderer parity](./phase-01-start.md) | completed | — |
| 2 | [Windows Tier 5 desktop checks](./phase-02-windows-tier5-desktop-checks.md) | completed | — |
| 3 | [Gates, docs, branch and merge](./phase-03-gates-docs-and-merge.md) | completed | 1, 2 |

Phases 1 and 2 are independent: phase 1 touches `app/src/prefs/tabs.rs`,
`app/src/settings_spec.rs` and `app/src/platform/windows/settings_ui.rs`; phase
2 touches nothing until it finds a fault, and any fix it does make is reported
before it is written.

## Acceptance criteria

1. On macOS every control on every tab starts at one x — the control column —
   checkboxes included; a checkbox's caption starts under the checkbox's text,
   a form row's caption under the control column. Verified statically by
   reading the constants each view is built with, and by the headless spec
   tests; the pixels wait for the Mac.
2. The three count rows read `20 apps`, `0 macros`, `0 words` (and their
   Vietnamese forms) on **both** shells, from one definition in
   `settings_spec.rs`. No inline unit strings remain in either renderer.
3. macOS row rhythm is 6 between a control and its caption, 10 between rows and
   18 before a section header, applied once per row (after the caption when
   there is one, after the control when there is not) — the same rule
   `finish_row` follows on Windows.
4. No spec row, order, or wording changes. `settings_spec.rs`'s existing tests
   still pass unchanged, and a test covers `ListId::unit` for all three lists in
   both languages.
5. Every Tier 5 box in `docs/manual-verification-windows.md` is either ticked or
   carries a written reason it could not be, and the result is recorded in
   `plans/reports/windows-verification-260905.md` as that file's "Recording the
   results" section requires.
6. The full §11 gate list is green on every change:
   `cargo test --workspace`; the three library crates with and without
   `--features serde`; `cargo clippy --workspace --all-targets -- -D warnings`;
   `cargo clippy --target aarch64-apple-darwin -p glowkey --all-targets -- -D
   warnings`; `cargo check --target x86_64-unknown-linux-gnu` for the three
   library crates; `cargo doc` with `RUSTDOCFLAGS=-D warnings`.
7. The work lands on a feature branch, fast-forwards to `main`, and is pushed; a
   journal entry is written.

## Non-goals

- **No macOS runtime pass.** No `just dev`, no Settings-window walkthrough, no
  ⌃⇧Space / ⌃⇧E / ⌃⇧W. Handoff §11 item 1 stays open and phase 3 restates it
  with what phase 1 added to its list.
- No new spec rows, no rewording, no reordering.
- No macOS hotkey-recorder change; the `Custom…` segment stays a macOS-only
  detail of that renderer.
- No Windows visual change beyond deleting the inline unit strings in favour of
  the spec accessor — the rendered text is identical.
- No Tier 6 exclusion-table sweep.

## Risks

- **A checkbox title pushed to the control column can overflow the window.**
  `NSButton` checkbox titles do not wrap. The window is 460 pt with an 18-pt
  pane inset each side; a column at the `LABEL_COLUMN_WIDTH` floor of 92 plus
  the 8-pt gap leaves ~324 pt, and the column is measured from the widest
  *form* label, so a long Vietnamese label widens it further and takes that room
  away. Phase 1 measures the longest checkbox title in both languages against
  the room left and, if it does not fit, keeps the change and lets the title
  wrap — `NSButton` can be given a wrapping cell — rather than abandoning the
  one-axis rule. **Signal:** the measurement in step 1 comes out over budget.
  **Response:** wrap, do not retreat to the left margin.
- **`setCustomSpacing:afterView:` needs the right view.** The stack's spacing is
  uniform; the per-row gap has to be attached to whichever view was added last
  for that row, which for a captioned row is the caption and for the hotkey row
  is its status line. `add_row` already returns that view; the tab builder has
  to start using the return value for every row, not only before a header.
- **The `aarch64-apple-darwin` clippy target is the only gate phase 1 has.** It
  is what caught a stale test field in the previous phase, so it is not nothing,
  but it proves compilation and lints, not layout. Everything visual in phase 1
  is a claim until the Mac runs it.
- **Tier 5 is a desktop-interaction list on the user's live machine.** No
  synthetic keystrokes, ever. Window messages go only to GlowKey's own windows,
  the way plan `260905-1145` took its captures. Anything that cannot be done
  under that rule is left unchecked with the reason written down, not faked.

## Rollback

Revert the feature branch. Phase 1 is three edits in three files with no data or
settings-file consequence; phase 2 writes a report and, at most, small fixes
that are separately revertable.

<!-- slug: macos-renderer-parity-and-windows-tier5 -->
