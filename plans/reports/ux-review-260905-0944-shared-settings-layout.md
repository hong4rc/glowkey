# UX review: the settings layout that becomes the shared spec

2026-09-05 10:05. Companion to `research-260905-0944-one-ui-spec-native-backends.md`.
User direction: one UI for all platforms, based on the macOS layout, and
improvements over the current macOS window are welcome.

Source of truth reviewed: `app/src/prefs/tabs.rs` (macOS) and
`app/src/platform/windows/settings_ui.rs` (Windows), every user-visible string
extracted and compared. The ak-ui-ux-pro-max database is web/mobile oriented and
returned a landing-page pattern; its Quick Reference rules were applied instead,
alongside Apple HIG for settings windows.

## Errata (2026-09-05, during implementation)

- Finding 4 was wrong: the toggle-key picker was already in the macOS General
  tab. The real gap was Windows, which had no picker at all. The spec now gives
  Windows the preset picker.
- Finding 9 was wrong: the macOS list windows already had empty-state text.
  Nothing to do there.

Everything else was implemented in `plans/260905-1002-shared-settings-spec/`.

## Verdict

The bones are right: four tabs, label column plus control, segmented controls
for two-way choices, captions under checkboxes, live apply with no OK button,
lists in their own windows. Keep all of that. Eleven concrete things are worth
fixing when the layout moves into one spec, and the two platforms have already
drifted in five places, which is the argument for the spec.

## Drift found today (Windows vs macOS)

| Item | macOS | Windows |
|---|---|---|
| Startup checkbox label | "Launch GlowKey at login" | "Start at login" |
| Startup captions | none | two captions, one per checkbox |
| "Restore common English words" caption | opens "The blunt version:" | opens "Off by default:" |
| Personal Words caption | mentions ⌃⇧W repair | drops it |
| Closing | title bar only, live apply | a Done button |

Same product, two wordings, three weeks in. The spec ends this.

## Findings, ranked

### 1. Captions are paragraphs (HIG: one line, example optional)

"Restore common English words" carries a 3-line mechanism dump with five
arrow examples. "Telex bracket shortcuts" explains key interception. Rule
`input-helper-text` and `progressive-disclosure`: a caption states the
outcome in one sentence with at most one example. Mechanism goes to a
tooltip or is cut.

Proposed captions (English, Vietnamese to follow):

- Quick Telex: `Double a consonant to type its pair: cc→ch, nn→ng, uu→ư.`
- Bracket shortcuts: `[ ] { } type ơ ư Ơ Ư and never reach the app.`
- Auto-fix: `Types "exit", not "eĩt".`
- Fix as I type: `Repairs at the first impossible letter instead of at the space.`
- Restore common English words: `"was" stays "was". Off by default; costs some Vietnamese key orders.`
- Personal words: `Words you have decided about, kept across sessions.`
- Excluded apps: `GlowKey types plain keys here. Terminals and editors by default.`
- Macros: `Type a shortcut then a space to expand it: vn → Việt Nam.`

### 2. Hard line breaks inside caption text

macOS captions contain `\n`. Wrapping belongs to the renderer, not the string.
The spec carries unbroken text; each renderer wraps to its own width. Windows
already does this.

### 3. Dependent setting shown as a peer

"Fix as I type, not at the space" only means anything when "Auto-fix
non-Vietnamese words" is on, but it sits as an equal checkbox. Rules
`progressive-disclosure`, `disabled-states`: indent it under its parent and
disable it when the parent is off. The spec gets `enabled_when: Option<Field>`.

### 4. Toggle key lives in the wrong tab

The Vietnamese/English hotkey picker is in "Apps & macros". Users look for it
in General (Windows and macOS both put keyboard shortcuts with general or
keyboard settings). Move it to General under a "Keyboard" section. Apps &
macros then holds exactly what its name says.

### 5. Hardcoded shortcut glyphs in prose

Captions say ⌃⇧E and ⌃⇧W literally. On Windows those are Ctrl+Shift+E and
Ctrl+Shift+W, and the Windows strings simply dropped them. The spec carries a
`Shortcut(HotkeyId)` token inside text; each renderer formats it through the
existing `hotkey_display` for its platform. One sentence, correct on both.

### 6. "Manage…" buttons hide state

"Manage Excluded Apps…" and "Manage Macros…" and "Personal Words…" tell the
user nothing until clicked. Show the count on the row: `Excluded apps  14  Manage…`.
Rule `state-clarity`. The spec's `ListButton` already has `count: fn(&Settings) -> usize`.

### 7. Windows-only Done button

Both platforms apply live and persist at once. A Done button implies there is
something to confirm. Drop it; close via the title bar on both, matching macOS
System Settings and Windows 11 Settings. If the Windows session wants a visible
exit, an "X" is the title bar.

### 8. Section headers missing on tabs with mixed content

General mixes language and startup; Corrections mixes auto-fix, capitalization,
English-word handling and personal words. Rule `field-grouping`: light section
headers (macOS style, bold secondary text) in the spec as `Section.title`.
Renderers may render them or fold them into spacing, but the grouping is data.

### 9. Empty states exist on Windows only

"No apps excluded", "No macros yet", "No words yet" are Windows strings. The
macOS lists show blank. Rule `empty-states`: put the text in the list spec.

### 10. Caption contrast

Windows fixed caption colour to about 6.6:1 light and 7.4:1 dark. macOS uses
`secondaryLabelColor`, which meets AA. Fine on both; the spec does not carry
colour. Renderer concern only.

### 11. Segmented control labels carry the example

"Modern  hoà / Classic  hòa" and "Simple Telex" are good: the example is the
label. Keep. Do the same for Language: `System / English / Tiếng Việt`.

## Proposed shared layout

Four tabs, sections named, one caption line each. Counts on list rows.

```
General
  Interface
    Language            [ System | English | Tiếng Việt ]
  Startup
    ☑ Launch GlowKey at login
    ☑ Open this window at launch
  Keyboard
    Toggle Vietnamese   [ ⌃⇧Space | ⌃Space | ⌥Z | ⇧⇧ | Custom… ]
    Toggle current app  ⌃⇧E                       (read-only row)

Typing
  Method
    Input method        [ Telex | Simple Telex | VNI ]
    Tone marks          [ Modern  hoà | Classic  hòa ]
  Telex extras
    ☑ Quick Telex                 Double a consonant to type its pair: cc→ch, nn→ng, uu→ư.
    ☑ Telex bracket shortcuts     [ ] { } type ơ ư Ơ Ư and never reach the app.

Corrections
  Auto-fix
    ☑ Auto-fix non-Vietnamese words          Types "exit", not "eĩt".
       ☑ Fix as I type                        Repairs at the first impossible letter.   (disabled when parent off)
    ☑ Auto-capitalize sentences
  English words
    ☑ Restore common English words           "was" stays "was". Off by default.
    Personal words        3    Manage…        Words you have decided about. Undo one with ⌃⇧W right after typing it.

Apps & macros
  Apps
    Excluded apps        14    Manage…        GlowKey types plain keys here. Terminals and editors by default.
  Macros
    Macros              214    Manage…        Type a shortcut then a space: vn → Việt Nam.
    ☑ Expand macros even when Vietnamese is off     Never in an excluded app.
```

Window: 460×540 points, resizable down to 420×420, tabs on top. Same on all
platforms. List windows: separate, resizable, scrolled, with the empty-state
text above; add/remove below the list on macOS, inline buttons on Windows.

## What the spec needs beyond the research sketch

- `Text` may contain `Shortcut(HotkeyId)` tokens, not just a string pair.
- `Row.enabled_when: Option<Field>` for the dependent checkbox.
- `Section.title` is required, not optional, so grouping is never lost.
- `ListSpec.empty_text`.
- No `Done`, no colour, no wrapping, no metrics. Those stay in renderers.

## Accessibility check against the Quick Reference

- Contrast: met on both after the Windows caption fix. Unchanged.
- Focus and keyboard: native on macOS; egui provides Tab focus. Windows list
  rows need ⌫ to remove, as macOS has. Add to the Windows renderer.
- Labels: every caption must be the control's accessibility help on macOS
  (`setAccessibilityHelp`) and the widget's label on egui. The spec pairs them,
  so the renderer cannot forget one.
- Reduced motion: no animation anywhere. Nothing to do.
- Text scaling: system font on both; egui uses points. Fine.

## Next step

Fold these into the two-phase plan proposed in the research report: phase 1
tabs (this layout), phase 2 list windows. Vietnamese strings for the new
captions written alongside the English in the spec, reviewed by the user.

## Unresolved questions

1. Section headers on macOS: bold secondary text like System Settings, or
   grouped boxes like the old single-pane sketch in `docs/ui-design.md`?
2. "Toggle current app" row in General: read-only display only, or should
   that hotkey become configurable too while we are here?
3. Drop the Windows Done button, or keep it because Windows users expect a
   button to leave a dialog? Recommendation is drop.
