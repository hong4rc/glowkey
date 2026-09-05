---
phase: 3
title: "Keyboard and accessibility"
status: completed
priority: P2
effort: "3h"
dependencies: [2]
---

# Phase 3: Keyboard and accessibility

## Overview
The hand-painted segmented control gains keyboard focus, arrow-key selection,
a focus ring and screen-reader names. Tab order follows the visual order.

## Requirements
- Functional: Tab reaches each segmented control (tabs strip included) and the
  popup; ←/→ (and Home/End) move the selection; Space/Enter is a no-op (the
  selection is the action); a focus ring shows on the raised segment.
- Non-functional: AccessKit exposes each segment as a selectable item with its
  label and selected state; nothing else about paint changes.

## Architecture
`segmented()` in `settings_ui.rs`:
- One focusable id for the whole control: `ui.memory_mut(|m|
  m.interested_in_focus(base_id))`; the allocated track `Response` is
  `interact(track, base_id, Sense::click())` so Tab lands on it.
- When `response.has_focus()`: read `ui.input(|i| i.key_pressed(Key::ArrowRight
  | ArrowLeft | Home | End))`, move the index, set `*value`.
- Paint: if focused, a 2-pt ring in `visuals.selection.stroke.color` around the
  raised segment (`painter.rect_stroke(inner.expand(1.5), rounding, stroke)`).
- Each segment: `response.widget_info(|| WidgetInfo::selected(WidgetType::
  SelectableLabel, selected, label))` so AccessKit names it.
- ComboBox and checkboxes are egui-native and already focusable; verify order.

## Related Code Files
- Modify: `app/src/platform/windows/settings_ui.rs`

## Implementation Steps
1. Focus registration and key handling in `segmented`.
2. Focus ring paint.
3. `widget_info` per segment.
4. Tests: headless — request focus on the control via `ctx.memory_mut(|m|
   m.request_focus(id))`, send `Event::Key { ArrowRight, pressed }`, assert
   the value moved; assert `WidgetInfo` via `egui::accesskit` is out of reach
   headlessly, so assert instead that `response.widget_info` was set by
   enabling `ctx.options_mut(|o| o.screen_reader = true)` and checking
   `output.platform_output.events` contains a `ValueChanged` or
   `Clicked`-style `OutputEvent` with the label.
5. Gates; capture the ring.

## Success Criteria
- [ ] Tab cycles through tab strip, every segmented control, popup, checkboxes,
      buttons in visual order.
- [ ] ←/→ move a focused segmented selection; Home/End jump.
- [ ] Focus ring visible in capture.
- [ ] Headless test proves arrow selection and the screen-reader event.
- [ ] Gates green.

## Risk Assessment
- egui's Tab focus order is allocation order, which is the visual order here.
- `interested_in_focus` on a custom id is the documented way; if Tab skips it,
  the fallback is `ui.add(egui::Label::new("").sense(Sense::focusable_noninteractive()))`
  as a focus anchor.
