---
phase: 3
title: "Segmented control and chrome polish"
status: pending
priority: P2
effort: "2h"
dependencies: []
---

# Phase 3: Segmented control and chrome polish

## Overview
Make the segmented controls and the tab strip read like macOS: a soft track, a
raised selected segment, no hairlines, normal text colour everywhere.

## Requirements
- Functional: same behaviour; only paint changes.
- Non-functional: legible in light and dark; captions and labels keep ≥4.5:1.

## Architecture
Reference (Apple HIG, segmented controls; Big Sur+ switcher style):
- Track: rounded (radius 6), fill a shade darker than the window
  (light: gray 220 on the 236 window; dark: gray 58 on the 40 window), **no
  stroke**.
- Selected segment: raised — light: white with a soft shadow
  (`epaint::Shadow { offset: (0,1), blur: 2, spread: 0, color: rgba(0,0,0,0.18) }`)
  and a hairline of rgba(0,0,0,0.04); dark: gray 105, shadow rgba(0,0,0,0.4).
  Text colour: the normal text colour, not white.
- Unselected segment: transparent on the track; hover: a faint lift
  (light: gray 228; dark: gray 66).
- Segment padding 10×3, height 22, one-point inset inside the track.
- Tab strip: the same control, centred, 8 pt below the title bar.

egui mechanics (`settings_ui.rs::segmented`): inside a `ui.scope`, set
`visuals.selection.bg_fill` to the raised colour and `visuals.selection.stroke =
Stroke { width: 0.0, color: text_color }` — `SelectableLabel` takes both its border
and its text colour from `selection.stroke`, so width 0 removes the border and the
colour keeps the text dark. Set `widgets.hovered.weak_bg_fill` to the hover
lift, `widgets.inactive.weak_bg_fill` transparent, all `bg_stroke` widths 0.
Paint the shadow yourself: after `selectable_label`, if selected, `ui.painter()
.rect(...)` behind it is not possible post-hoc; instead paint the track first,
then for the selected index paint a raised rect with `painter.rect_filled` +
`Shadow::as_shape` at the segment's rect using a two-pass layout (measure
labels, allocate the whole control, paint, then place labels). Keep it in one
function; add a headless test that the control allocates one rect per option and
that the selected index changes on click via `egui::RawInput` events.

Also remove the 1-pt stroke `chrome` left in `caption`/`intro` if any, and check
the checkbox tick colour matches the raised-segment accent (leave egui's).

## Related Code Files
- Modify: `app/src/platform/windows/settings_ui.rs` (`segmented`, `apply_style`)

## Implementation Steps
1. Rewrite `segmented` as described; keep its signature.
2. Adjust `apply_style` per-theme visuals for the track/hover colours.
3. Headless test for allocation and click selection.
4. Gates; run; compare against the macOS window screenshot in
   `docs/ui-design.md` intent: no hairlines, raised selection.

## Success Criteria
- [ ] No stroke around the track or the selected segment in either theme.
- [ ] Selected segment raised (white light / gray 105 dark) with a shadow; text
      colour normal.
- [ ] Tab strip uses the same control.
- [ ] Gates green.

## Risk Assessment
- egui `SelectableLabel` internals may change per version; the two-pass paint
  avoids depending on them. If `selection.stroke` width 0 still draws, paint
  labels with `ui.painter().text` instead of `selectable_label`.
