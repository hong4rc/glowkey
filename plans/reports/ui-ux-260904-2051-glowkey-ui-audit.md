# GlowKey UI audit — menu bar, Settings, auxiliary windows

Read-only audit of the whole AppKit surface (`menu_bar.rs`, `prefs/*`,
`about_window.rs`, `welcome.rs`, `hud.rs`, `tap/permission.rs`, `strings.rs`)
against macOS HIG, `docs/ui-design.md`, and the app's own stated value: *"a
menu-bar tool should look like it came with the system."*

Nothing was modified. 22 findings, most valuable first, then what genuinely
needs a human at a screen.

---

## 1. No application main menu — ⌘C / ⌘V / ⌘A / ⌘Z / ⌘W are dead in every window

`app/src/tap/mod.rs:440` (`NSApplication::sharedApplication(mtm)` … `app.run()`),
consequences at `app/src/prefs/macros_window.rs:170`,
`app/src/prefs/personal_words.rs:87`

`setMainMenu:` is never called anywhere in `app/src`. In Cocoa, ⌘-key
equivalents are dispatched through `NSApp.mainMenu.performKeyEquivalent:`
*before* the responder chain. With no main menu there is no Edit menu, so the
field editor never sees Cut/Copy/Paste/Select All/Undo, and there is no Window
menu, so ⌘W closes nothing.

Concretely: a user migrating from UniKey opens **Macros**, wants to add
`vn` → `Việt Nam`, and cannot paste `Việt Nam` into the expansion field. They
also cannot select-all to correct a typo, cannot undo, and cannot close the
window from the keyboard. Same in **Personal Words**' single field. This is the
single most user-visible defect in the audit because it hits the exact flow the
Macros window exists for. It is also why the manual-verification checklist has
never caught it — nobody typed ⌘V.

Fix: build a minimal `NSMenu` as `NSApp.setMainMenu:` at the same place the
status item is installed. Three submenus is enough and is what every agent app
ships: an app submenu (`Settings…` ⌘, / `Quit GlowKey` ⌘Q), an **Edit** submenu
wired to the standard selectors (`undo:` ⌘Z, `redo:` ⇧⌘Z, `cut:` ⌘X, `copy:`
⌘C, `paste:` ⌘V, `selectAll:` ⌘A — target `None` so they travel the responder
chain), and a **Window** submenu (`performClose:` ⌘W, `performMiniaturize:`
⌘M). Titles come from `t()`. Effort: **small**.

## 2. The menu says "⌃⇧Space" no matter which toggle key the user chose

`app/src/menu_bar.rs:273`, and the same string at `app/src/welcome.rs:59-60`

```rust
t("Vietnamese input (⌃⇧Space)", "Gõ tiếng Việt (⌃⇧Space)"),
```

The shortcut is baked into the menu label, but Settings → General → **Toggle
key** offers ⌃Space, ⌥Space, ⌃⇧Z and a recorded custom combo. The moment a user
changes it, the one place the app advertises the shortcut is wrong — and the
menu is rebuilt on every open (`menuNeedsUpdate:`), so this is a live lie, not a
stale cache. The Quick Guide has the same problem: it is reopenable from the
menu forever, and after any hotkey change it teaches the wrong keystroke.

Fix: `prefs::widgets::hotkey_display(self.state().toggle_hotkey())` already
renders exactly the string needed ("⌃⇧Z", "⌃⌥K", "⌥Space"). Make it
`pub(crate)`, then build the label as
`t("Vietnamese input ({})", "Gõ tiếng Việt ({})").replace("{}", &display)`. Do
the same substitution in `welcome.rs`. Effort: **trivial**.

## 3. No list scrolls and no window resizes — the Excluded Apps list clips on first run, and an imported macro table is unviewable

`app/src/prefs/excluded.rs:26` (420×380) and `:73-79`;
`app/src/prefs/macros_window.rs:25` (460×400) and `:109-116`;
`app/src/prefs/personal_words.rs:36` and `:109-116`

Every list is a bare `NSStackView` inside a fixed-size window. There is no
`NSScrollView` anywhere in `app/src`, and no window carries
`NSWindowStyleMask::Resizable`.

- **Excluded Apps** ships 14 default exclusions
  (`crates/glowkey-engine/src/exclusion.rs:185-200`). 14 rows at ~20pt plus 2pt
  spacing is ~306pt, on top of a two-line caption (~30pt), the "Add App…" button
  (~32pt), 24pt of stack spacing and 40pt of insets — roughly 430pt of content
  in a 380pt window. On a clean install the last rows are off the bottom, and
  the user cannot scroll, cannot resize, and cannot reach them. This is the
  window for the feature the whole app exists for.
- **Macros** advertises Import as the migration path for "a table curated in
  Unikey or EVKey" — those routinely run to hundreds of entries. After a
  successful import the alert says "Imported 214 macros." and the window shows
  the first ~13. The other 201 are unreachable and unremovable.

Fix: wrap each list stack in an `NSScrollView` (`setHasVerticalScroller(true)`,
`setDocumentView(&list)`, `setDrawsBackground(false)`), give the scroll view a
height constraint, and add `NSWindowStyleMask::Resizable` plus a
`setContentMinSize`. One shared helper covers all three windows, which also
fixes finding 12. Effort: **medium**.

## 4. Settings opens on every launch, by default

`crates/glowkey-engine/src/config.rs:104` (`open_settings_at_launch: true`),
consumed at `app/src/tap/mod.rs:437`

The default is on. With "Launch GlowKey at login" also on — which is the
intended steady state for an input method — every login puts a Settings window
in front of the user's face. That contradicts the stated design value
(*unobtrusive*) and no shipping menu-bar utility does it. On a genuine first
run the user gets three modal/front windows in a row: the Accessibility alert,
then the welcome alert, then Settings.

Fix: default `open_settings_at_launch` to `false`, and keep the checkbox
("Open this window at launch", `tabs.rs:101-104`) for people who want the
UniKey control-panel feel. The welcome panel already carries the first-run
orientation, so nothing is lost. Effort: **trivial**.

## 5. Excluded Apps shows machine-derived names, no icons, and sorts by bundle identifier

`app/src/prefs/widgets.rs:58-66` (`display_name`), used at
`app/src/prefs/excluded.rs:117`; ordering from
`app/src/tap/settings.rs:61-66` → `ExclusionList::ids()` over a `BTreeSet`

`display_name` takes the last dotted component and upper-cases the first letter.
On a clean install, sorted by bundle id, the user's first look at the app's
headline feature is literally:

> Hyper · Terminal · Xcode · Wezterm · Iterm2 · WebStorm · Intellij · Pycharm ·
> VSCode · Ghostty · Warp-Preview · Warp-Stable · Kitty · Alacritty

Wrong casing ("Iterm2", "Intellij", "Pycharm"), raw build-channel suffixes
("Warp-Stable"), no icons, no alphabetical order, and no way to tell which of
these the user actually has installed. `docs/ui-design.md:88` specified the
opposite — "App icon (16pt) + `localizedName`; uninstalled app greyed, never
dropped" — and that never got built.

Fix: for each bundle id call
`NSWorkspace::URLForApplicationWithBundleIdentifier`. When it resolves, use
`NSBundle::bundleWithURL` → `localizedInfoDictionary["CFBundleDisplayName"]` (or
the URL's last path component minus `.app`) for the name and
`NSWorkspace::iconForFile` sized to 16×16 in an `NSImageView` at the head of the
row. When it does not resolve, keep `display_name` but draw it in
`NSColor::secondaryLabelColor()` — the greyed "not installed" state the spec
asked for. Sort the rows by the resolved display name, not the bundle id.
Effort: **medium**.

## 6. The session-only terminal un-exclusion is invisible in the list, and ⚠ means two different things

`app/src/prefs/excluded.rs:56-62` (caption) and `:111-137` (rows);
`app/src/tap/decide.rs:104-108` (HUD "VI ⚠");
`app/src/menu_bar.rs:201-207` (glyph "⚠")

Three problems stacked on one glyph:

- `ExclusionList::ids()` returns `bundle_ids`, which still contains a terminal
  that ⌃⇧E has suspended for the session. So the user presses ⌃⇧E in Ghostty,
  types Vietnamese happily, opens Excluded Apps, and Ghostty is listed as
  excluded. The window contradicts what is happening on screen.
- Nothing in the UI says the suspension is session-only, or that this window is
  the *only* place a shipped terminal default can be dropped permanently
  (`docs/decisions/0004`). The user's next restart silently re-excludes their
  terminal and they have no way to know why.
- ⚠ is used for two unrelated states with no legend: "VI ⚠" in the HUD means
  *"Vietnamese is on in a terminal and will mangle text"*, and "⚠" in the menu
  bar means *"the Accessibility permission is gone"*. Only the second one
  explains itself, and only after opening the menu.

Fix: (a) mark suspended rows — append `t(" — on until restart", " — bật đến khi
khởi động lại")` in `secondaryLabelColor`, which needs a
`session_removed`-aware accessor next to `exclusion_ids`; (b) extend the
Excluded Apps caption with one sentence naming the session-only rule; (c) set a
tooltip on the status item button (`button.setToolTip`) that spells out the
current state in words, so ⚠ is never the only signal. Effort: **medium**
(mostly (a)).

## 7. The menu-bar glyph cannot distinguish "English mode" from "excluded app"

`app/src/menu_bar.rs:196-214`

`update_glyph` shows `VI` when `is_active()`, otherwise `EN`. But `is_active()`
is false both when the user switched to English globally and when the frontmost
app is on the ignore list. The single most common question this app has to
answer at a glance — *"why is Vietnamese off right here?"* — is exactly the
distinction the indicator collapses. `docs/ui-design.md:16-18` specified a third
state ("a dimmed VN, or `–`") for precisely this.

The menu header does distinguish them (`menu_bar.rs:249-253`), but that costs a
click.

Fix: `menu_state()` already returns `excluded` separately. Title stays `EN` for
mode-off; for the excluded case use the dimmed variant the spec chose —
`button.setTitle("EN")` plus
`button.setAppearsDisabled(true)` (native, no custom drawing, follows menu-bar
tinting), reset to `false` otherwise. Pair with the tooltip from finding 6.
Effort: **small**.

## 8. A language change leaves Personal Words in the old language, and unmanaged

`app/src/prefs/mod.rs:468-472` and `:479-484`

`rebuild_windows` retires `window`, `excluded_window` and `macros_window` — and
`about_window::invalidate()` — but never `words_window`. It also does not clear
`words_list` / `word_keys`. Switch Settings → General → Language to Tiếng Việt
and the Personal Words window keeps every English label forever (its labels are
baked at build time, exactly as the doc comment on `rebuild_windows` explains),
and if it was open it is not reopened with the others.

Fix: add `self.ivars().words_window.borrow_mut().take()` to the `windows` array,
add the matching `reopen_words` flag and `manage_personal_words` re-invocation,
and `replace(None)` on `words_list` and `word_keys` alongside the other four.
Effort: **trivial**.

## 9. Personal Words' caption and empty state are single-line and will truncate

`app/src/prefs/personal_words.rs:67-77` and `:139-147`

Every other caption in the app hard-wraps with explicit `\n`
(`tabs.rs:184`, `:207`, `:233`, `:259`, `:304`, `:381`; `excluded.rs:58`).
These two do not — they are one continuous ~180-character and ~100-character
string. `NSTextField::labelWithString` creates a **non-wrapping** label, so in a
460pt-wide window with 40pt of insets neither fits; they clip or force the
stack. The window whose entire purpose is explaining a subtle two-way choice is
the one where the explanation is cut off.

Fix: the durable version is `NSTextField::wrappingLabelWithString` in
`widgets.rs::caption` plus
`label.setPreferredMaxLayoutWidth(<content width − insets>)`, which then lets
every other caption drop its hand-placed `\n` and stop being tuned to English
line lengths (see finding 22 note on Vietnamese overflow). The one-line version
is to insert `\n` in these two strings, matching the neighbours. Effort:
**small** either way.

## 10. Adding a macro by hand silently overwrites; an empty shortcut silently does nothing; Return does not add

`app/src/prefs/mod.rs:373-399`, engine at
`crates/glowkey-engine/src/lib.rs:1513-1525`, button at
`app/src/prefs/macros_window.rs:69-79`

Three separate gaps in one row:

- `Session::add_macro` does `retain(|m| !m.shortcut.eq_ignore_ascii_case(...))`
  then pushes — add-**or-replace**. Typing an existing shortcut destroys the
  user's expansion with no warning and no undo. Import, in the same window, goes
  out of its way to never overwrite and reports "N skipped". The two halves of
  one window follow opposite rules.
- An empty shortcut returns `false`, so `add_macro` skips the field clear and
  nothing at all happens. The user clicks Add and the app appears frozen.
- `Add` has no key equivalent. Type shortcut, Tab, type expansion, press
  Return → nothing. Every macOS form adds on Return.

Fix: (a) before calling, check the shortcut against `state().macros()` and, on a
collision, put up a `notify()` with a Replace / Cancel choice — or at minimum
report "Replaced “vn”."; (b) when the shortcut is empty, `notify()` with
`t("Enter a shortcut.", "Nhập chữ viết tắt.")` and
`window.makeFirstResponder(&shortcut_field)`; (c)
`add.setKeyEquivalent(&NSString::from_str("\r"))`, which also renders it as the
blue default button. Effort: **small**.

## 11. "Add App…" picker: wrong default-button verb, no type filter, silent failure

`app/src/prefs/mod.rs:279-304`

The panel sets a message but no prompt, so its default button reads **Open** —
the user is not opening anything, they are excluding it. `setCanChooseFiles(true)`
with no allowed content types means any file can be selected; then
`NSBundle::bundleWithURL(...).bundleIdentifier()` returns `None`, the loop
skips it, `refresh_list()` shows no change, and the user gets no explanation.
The directory is pinned to `/Applications`, so apps in `~/Applications` and
`/System/Applications` (TextEdit, for one) take extra navigation.

Fix: `panel.setPrompt(&NSString::from_str(t("Exclude", "Loại trừ")))`;
`panel.setAllowedContentTypes(&NSArray::from_slice(&[UTType::applicationBundle()]))`
so only apps are selectable; and count the URLs that yielded no bundle
identifier, reporting them through the existing `notify()` helper. Effort:
**small**.

## 12. The three auxiliary windows do not match each other

`app/src/prefs/excluded.rs:26,73-79,111-137` ·
`app/src/prefs/macros_window.rs:25,63-116,138-160` ·
`app/src/prefs/personal_words.rs:36,84-116,150-183`

They were built at different times and it shows:

| | Excluded Apps | Macros | Personal Words |
|---|---|---|---|
| Content size | 420 × 380 | 460 × 400 | 460 × 400 |
| Row label width | 250 | 320 | 250 |
| List spacing | 2.0 | 2.0 | 4.0 |
| How you add | modal `NSOpenPanel` | inline fields + Add | inline field + 2 buttons |
| Import / Export | — | yes | — |
| Row actions | Remove | Remove | Flip, Remove |

`personal_words.rs:15-17` states the intent — *"Modelled closely on
`macros_window.rs` rather than improved upon … divergence between them is worse
than duplication among them"* — and the three still diverge on size, column
width and spacing.

Fix: one `list_window(title, mtm) -> (window, root_stack, list_stack)` helper
and one `list_row(label, buttons) -> NSStackView` helper in `widgets.rs`; adopt a
single content size (460 × 420 with `Resizable`, per finding 3) and a single
row-label width. Effort: **medium**, and it pays for finding 3.

## 13. No accessibility labels on the repeated buttons, or on the status item

`app/src/prefs/excluded.rs:121-133` · `app/src/prefs/personal_words.rs:164-181` ·
`app/src/prefs/macros_window.rs:145-156` · `app/src/menu_bar.rs:208-213`

VoiceOver in Excluded Apps reads "Remove, button" fourteen times with nothing to
tell them apart; in Personal Words it alternates "Flip, button" / "Remove,
button" the same way. The status item is worse: its accessible name is whatever
`setTitle` last wrote — "VI", "EN", or the bare character "⚠", which VoiceOver
announces as "warning sign" with no product context.

Fix: after each `setTag:`, add
`button.setAccessibilityLabel(&NSString::from_str(&t("Remove {}", "Xóa {}").replace("{}", name)))`
— same for Flip and for the macro rows using the shortcut. For the status item,
set an accessibility label on `item.button(mtm)` that states the app and the
state in words ("GlowKey — Vietnamese on", "GlowKey — Accessibility permission
revoked"). Effort: **small**.

## 14. The three clipboard tools are always enabled and never confirm anything

`app/src/menu_bar.rs:312-335`, behaviour at `:175-185`

`transform_clipboard` early-returns when the pasteboard holds no string — the
right safety choice, but the menu item stays enabled, so a user who copied an
image, picked "Clipboard: remove tones", and got no reaction learns nothing.
Even on success there is no feedback: the clipboard changed invisibly and
destructively, with no undo.

Fix: the menu is rebuilt on every open, so gate it there — read
`NSPasteboard::generalPasteboard().stringForType(NSPasteboardTypeString)` once
in `rebuild` and call `item.setEnabled(false)` on the three clipboard items when
it is `None` (this needs `add_item` to stop discarding its return value, or an
`enabled` parameter). On success, `hud::flash` a short confirmation — the HUD
already exists for exactly this "no menu is open to give feedback" case. Effort:
**small**.

## 15. Every window re-centres on the screen each time it is opened

`app/src/prefs/mod.rs:270,315,366,523` · `app/src/about_window.rs:139`

`window.center()` runs on every `show`, not only on first build, and no window
sets `setFrameAutosaveName`. Move Settings to your second display, close it,
reopen it — it jumps back to the middle of the main screen. macOS windows
remember where you put them.

Fix: move `center()` into the `build_*` functions (first creation only) and add
`window.setFrameAutosaveName(&NSString::from_str("GlowKeySettings"))` — and
distinct names for the other three. AppKit then restores position and, once
finding 3 lands, size. Effort: **trivial**.

## 16. Menu capitalisation is mixed, and "Quick Guide…" should not have an ellipsis

`app/src/menu_bar.rs:283,295,304,314,322,330,341,359` · `tabs.rs:447`

macOS menu commands use title-style capitalisation. The current menu mixes both
styles in one list: "Reveal Log in Finder", "Quick Guide…", "About GlowKey",
"Quit GlowKey" are title case; "Auto-fix English words", "Open at login",
"Reset input (if stuck)", "Clipboard: remove tones", "Clipboard: lowercase" are
sentence case. Read top to bottom it looks like two menus glued together.

Separately, HIG reserves the ellipsis for commands that need more input before
they can complete. "Quick Guide…" shows an alert with a single "Got it" button —
it completes immediately and should be **"Quick Guide"**. ("Settings…",
"Open System Settings…", "Add App…", "Import…", "Export…", "Manage Excluded
Apps…", "Manage Macros…", "Personal Words…" are all correct as-is.)

The Settings tab label `t("Apps & macros", "Ứng dụng & gõ tắt")`
(`tabs.rs:447`) is the only sentence-case tab among "General" / "Typing" /
"Corrections" — should be "Apps & Macros".

Fix: retitle to "Auto-Fix English Words", "Open at Login", "Reset Input (if
Stuck)", "Clipboard: Remove Tones", "Clipboard: UPPERCASE", "Clipboard:
lowercase" (that last pair is deliberately literal — leave the case as the
sample it is), drop the ellipsis from "Quick Guide", and capitalise the tab.
Vietnamese strings are unaffected; Vietnamese does not title-case.
Effort: **trivial**.

## 17. The three hotkeys are never shown together, and the one that is configurable sits on the wrong tab

`app/src/prefs/tabs.rs:330-352` (Toggle key, on **General**),
`:381` (⌃⇧E, buried in a caption on **Apps & macros**),
`:324` (⌃⇧W, buried in a caption on **Corrections**)

A user who forgets ⌃⇧E — the per-app on/off switch, the app's core feature —
has to guess which of four tabs mentions it, and it is a grey 11pt caption under
a button, not a row. ⌃⇧W is in the same position on another tab. Meanwhile the
one shortcut that *is* configurable is on **General**, away from both, and away
from the Typing tab where someone hunting for keyboard behaviour would look.

Fix (KISS, no new window): under the existing "Current: …" row on General, add
two read-only `form_row`s using the same two-column layout —
`t("Per-app on/off", "Bật/tắt theo ứng dụng")` → `⌃⇧E`, and
`t("Correct last word", "Sửa từ vừa gõ")` → `⌃⇧W`. Three shortcuts, one place,
no new controls. Effort: **small**.

## 18. When the tap is dead the menu contradicts itself and stays clickable

`app/src/menu_bar.rs:228-254` then `:257-335`

The dead-tap branch adds "⚠ Accessibility permission revoked — Vietnamese is
off", then falls through and adds the ordinary state header, which says
"Vietnamese". Two adjacent lines, opposite claims. Everything below stays
enabled too, although the code comment at `:226-227` says outright that "every
item below it is inert until the permission comes back" — clicking "Vietnamese
input" flips a mode that changes nothing, and its checkmark then disagrees with
the warning three rows up.

Fix: when `tap_is_dead()`, skip the state header (the warning line *is* the
state) and pass `enabled: false` for the per-app toggle, the mode toggle,
auto-fix and the clipboard items — leaving Open System Settings, Reveal Log,
Settings, Quick Guide, About and Quit live. `add_item` needs to return its item
(it already does) so `setEnabled(false)` can be applied. Effort: **small**.

## 19. The Settings window is an NSTabView with a fixed title, not a macOS settings window

`app/src/prefs/tabs.rs:30-47` and `:442-454`

Two divergences from what a system settings window looks like:

- Panes are an `NSTabView` with tabs across the top. macOS settings windows have
  used a toolbar of pane icons since 10.0, and every app the user compares
  GlowKey to (including System Settings itself) does. A boxed tab view inside a
  window reads as a 2010 utility dialog.
- The title is fixed at "GlowKey Settings" across all four panes. The convention
  for a multi-pane settings window is the **selected pane's name** as the window
  title.

Neither is bespoke styling; both are the system default the app opted out of.
Against that, `NSTabView` is honest, native, and free, and the toolbar swap is
the largest single change in this report.

Fix, if taken: `NSToolbar` in `NSToolbarDisplayMode::IconAndLabel` with four
`NSToolbarItem`s (SF Symbols `gearshape`, `keyboard`, `text.badge.checkmark`,
`app.badge`), swapping `window.setContentView` per pane and resizing the window
to the pane's fitting size; set `window.setTitle` to the pane label in the same
handler. Fix, if not: at minimum set the title from the selected tab via an
`NSTabViewDelegate`. Effort: **large** (toolbar) / **small** (title only).

## 20. The HUD is bare text over whatever is behind it, and a long correction clips

`app/src/hud.rs:61-87` and `:89-101`

The flash panel is a borderless `NSWindow` whose content view is a plain
`NSTextField` label — no background. The label uses the default label colour, so
on a dark desktop in light mode (or the reverse) the "VI" / "EN" / "ứa → was"
flash can land as near-invisible text over arbitrary app content. The system's
own transient overlays (volume, brightness, caps lock) use a HUD material
backdrop, which is what makes them readable anywhere.

Second, the window is a fixed 160×120 and the label does not wrap. The
correction flash is `format!("{was} → {becomes}")`; anything past ~8 characters
at the 20pt band (`hoongfa → hồng`, or any longer word pair) runs past 160pt and
is clipped. `docs/manual-verification.md` §8b asks a human to confirm it flashes
"legibly, without clipping" — the code cannot currently guarantee it.

Fix: set the window's content view to an `NSVisualEffectView` with
`NSVisualEffectMaterial::HUDWindow`, `BlendingMode::BehindWindow`, state
`Active`, `wantsLayer`, `cornerRadius` 12 — with the label as its subview. This
is the system HUD material, not custom chrome. For the clipping, either widen
the window to `label.fittingSize().width + padding` before showing, or use
`wrappingLabelWithString` with `setMaximumNumberOfLines(2)`. Effort: **small**.

## 21. Import/export results appear as free-floating app-modal alerts

`app/src/prefs/mod.rs:444-451`, callers throughout
`app/src/prefs/macros_window.rs:206-322`

`notify` runs `NSAlert::runModal()`, which puts an unattached app-modal panel in
the middle of the screen. The alert is always *about* the Macros window and is
always triggered from a button inside it — that is what a sheet is for. As it
stands a user with several GlowKey windows open has to work out which one the
alert belongs to.

Fix: change `notify` to take the owning window and call
`alert.beginSheetModalForWindow_completionHandler(&window, None)`. The
`notify(message, detail, mtm)` signature is already funnelled through one
helper, so this is a one-place change plus threading the window in. Effort:
**small**.

## 22. `docs/ui-design.md` no longer describes the app

`docs/ui-design.md`, throughout

Divergences to correct, in file order:

| Line | Spec says | Build does |
|---|---|---|
| 10 | "No tabs" | four-tab `NSTabView` (`tabs.rs:442`) |
| 16-18 | glyph is `VN` / `EN` / dimmed `VN` | `VI` / `EN` / `⚠`; no third state (finding 7) |
| 22-34 | six-item menu | ~16 items incl. clipboard tools, Reveal Log, Open at login, Quick Guide, About |
| 45-50 | one 480×440 pane | 460×540 with four tabs |
| 61 | "Auto-fix words that aren't Vietnamese" | "Auto-fix non-Vietnamese words" (`tabs.rs:217`) |
| 76 | gradient `+` / `−` below the list | "Add App…" button *above* the list, per-row "Remove" buttons (`excluded.rs:63-71`) |
| 88-90 | app icon + `localizedName`, uninstalled greyed | derived text name, no icon (finding 5) |
| 96 | "the table supports arrow keys and `⌫` to remove" | not a table; a stack of buttons, no keyboard removal at all |
| 106-108 | maps to `app/src/prefs_window.rs` | `app/src/prefs/` — six files |

Also stale in code comments: `hud.rs:1` and `menu_bar.rs:478` both still say
`"VN" / "EN"` where the glyph is `VI`; `prefs/mod.rs:1-12` still describes "a
single non-resizable pane … and the 'Excluded apps' list", which moved to its
own window; `prefs/mod.rs:528-534` has the `refresh_hotkey_ui` doc comment
stranded above `add_word` (split artefact).

One item from that table is a real functional gap and not just documentation:
**there is no keyboard way to remove an entry from any of the three lists.**
`⌫` on a selected row does nothing, because the rows are stack views, not a
table. If the three list windows are ever rebuilt on `NSTableView` (findings 3
and 12), that comes back for free. Effort to update the doc: **small**. Effort
for the table rebuild: **large**.

---

## What I could not judge from code

The app cannot be launched, screenshotted, or driven in this environment. These
need a human at a screen; everything above is settled from source.

**Genuinely needs eyes**

1. **Actual clipping, all of it.** I can compute that 14 rows exceed a 380pt
   window (finding 3) and that a 180-character non-wrapping label exceeds 420pt
   (finding 9), but AppKit's compression-resistance arbitration decides what you
   actually see — truncation, overflow, or a squeezed neighbour. Somebody has to
   look. Highest-risk spots: Excluded Apps on a clean install; the Personal
   Words caption; the five-segment hotkey row on General in **Tiếng Việt**
   ("⌃⇧Space ⌃Space ⌥Space ⌃⇧Z Tùy chọn…" plus a 92pt label in a ~410pt tab
   content width — my estimate is single-digit points of slack, either way).
2. **Vietnamese overflow in the hand-wrapped captions** (`tabs.rs:184`, `:207`,
   `:233`, `:259`, `:304`, `:381`; `excluded.rs:58`). Each has `\n` placed for
   English line lengths; the Vietnamese lines are 0-4 characters longer. Several
   land within a few points of the pane width. This is exactly what finding 9's
   `wrappingLabelWithString` fix removes as a class of problem, but until then
   it needs a look at both languages.
3. **What the menu header says while a GlowKey window is focused.**
   `show_window` calls `NSApplication::activate()`, so `app_info::frontmost()`
   may return GlowKey itself, making the header read "Vietnamese" for GlowKey
   and the toggle read `Disable for "GlowKey"` — which would let the user add
   GlowKey's own bundle id to the ignore list. There is no self-exclusion guard
   anywhere in `menu_bar.rs` or `tap/`. Whether the status-item menu opening
   itself changes the frontmost app is behaviour I will not assert from source.
   **Check: open Settings, then open the menu-bar menu and read the top two
   lines.** If it names GlowKey, the fix is a bundle-id guard in
   `rebuild`/`toggleCurrentApp:`; effort trivial.
4. **HUD legibility** over a bright photo desktop and over a dark one
   (finding 20), and whether the correction flash clips in practice.
5. **The permission gate at its real size** — `tap/permission.rs:49-63`'s
   informative text is ~430 characters of two-paragraph prose in an `NSAlert`
   laid out by hand with `layout()`. It reads well; whether it *fits* is a
   different question, and §6.5 of the handoff records that this exact panel has
   rendered wrong before.
6. **Whether the Settings window's 540pt height still fits all four panes** —
   the comment at `tabs.rs:28-30` says the size was tuned for the pre-tab single
   column and grew by hand; `NSTabView` sizes to its largest pane and stack
   views compress silently.

**Settled from code, not open questions**

Findings 1, 2, 4, 8, 10, 11, 13, 14, 15, 16, 17, 18, 21 and the divergence table
in 22 are all determinable from the source and need no screenshot: the absent
`setMainMenu:`, the literal `⌃⇧Space` in the label, `open_settings_at_launch:
true`, the missing `words_window` in the retire list, `add_macro`'s
`retain`-then-push, the missing `setPrompt`, the absent
`setAccessibilityLabel` calls, the always-enabled clipboard items, `center()` on
every show, the label capitalisation, and the doubled state line under `⚠`.

**Things that are already right and I am deliberately not touching**

Immediate-apply with no OK/Apply button, and one persist per change
(`prefs/mod.rs`, every `*_and_save`); `setReleasedWhenClosed(false)` on every
window; the `retired_windows` deferral in `rebuild_windows`; every empty state
having real copy (`excluded.rs:106`, `macros_window.rs:135`,
`personal_words.rs:139-147`) rather than a blank box; the import error ladder
(too large / unreadable / VIQR / no macros / could-not-import), each with a
distinct and honest message; `t()` at the call site instead of a key table, with
only proper nouns hardcoded (`Telex`, `VNI`, `English`, `Tiếng Việt`, and the
hotkey glyphs — all correctly left untranslated); and the welcome panel being an
`NSAlert` with a single "Got it", reopenable from the menu.

---

## Unresolved questions

1. Finding 19 (toolbar vs `NSTabView`) is the only large item and the only one
   that touches the window's whole structure. It is the difference between
   "native controls" and "looks like a macOS settings window", which is the
   stated design value — but it is also a day of work on a surface with no
   automated coverage. Worth doing, or accept the tab view and take the
   title-per-pane half?
2. Finding 3's fix (scroll views) and finding 12's (one shared list helper) and
   finding 22's last item (`NSTableView` for keyboard `⌫`) are three sizes of the
   same change. Rebuilding the three lists on `NSTableView` inside one shared
   window helper resolves all three at once and restores what
   `docs/ui-design.md:96` promised. Is that in scope, or is the scroll view
   alone the right increment now?
3. Finding 10(a): should a hand-typed duplicate shortcut ask before replacing,
   or just report "Replaced “vn”." after the fact? Asking matches Import's
   never-overwrite rule; reporting is one line of code and no modal.

---

Status: DONE
Summary: 22 findings against the AppKit surface, ranked. The three that cost a
user something today are the absent application main menu (⌘C/⌘V/⌘W do nothing
in the Macros and Personal Words fields), the menu label that hardcodes
`⌃⇧Space` regardless of the configured toggle key, and the fact that no list
scrolls and no window resizes — so the 14 shipped exclusions clip out of the
Excluded Apps window on a clean install and an imported UniKey macro table is
mostly unreachable.
Concerns: nothing here is confirmed on screen. Six items are flagged as needing
a human look, and one of them (whether the menu header names GlowKey itself
while a GlowKey window is focused) could turn into a real dead end — a user
excluding GlowKey from itself — or into nothing at all, depending on runtime
behaviour I will not guess at from source.
