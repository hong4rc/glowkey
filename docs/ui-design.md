# GlowKey UI design — menu bar & Settings window

> **Status (2026-09-04).** This was written as a specification before the UI was
> built, and the build diverged from it. It has been corrected against the code:
> the Settings window has **four tabs**, not the single pane specified below, and
> lives in `app/src/prefs/` rather than a single `prefs_window.rs`. The parts that
> were specified and never built — app icons and localized names in the excluded
> list, the dimmed glyph for an excluded app — were built on 2026-09-04 and are
> described here as they now are. Where this file and the code disagree, the code
> is right and this file is the bug.

GlowKey is a native macOS menu-bar utility (objc2 AppKit). The design follows
Apple's Human Interface Guidelines for a **menu bar extra** and a **Settings
window**, not mobile/web patterns. Universal principles applied from the UI/UX
review: no emoji as icons (use real app icons + SF Symbols), 4.5:1 contrast,
visible focus, an 8pt spacing rhythm, one primary purpose per section, and
light/dark parity — all of which macOS system controls give us for free.

Design values (from the product): **simple**, native, unobtrusive. No custom
chrome, no color theming — a menu-bar tool should look like it came with the
system. ("No tabs" was a value here too, and the Settings window outgrew it; see
§2.)

## 1. Menu bar extra

The status item shows the current state as text so it is legible at a glance and
needs no bitmap. **Four states**, and the distinction between the middle two is
the whole point — the app exists for the per-app ignore list, so "off" and "off
*here*" must not look the same:

| Glyph | Means |
|---|---|
| `VI` | Vietnamese, and this app is not excluded |
| `VI` dimmed (45% alpha) | Vietnamese is on, but the app in front is excluded |
| `EN` | The user has switched Vietnamese off globally |
| `⚠` | The Accessibility permission is gone; nothing is reaching the engine |

`VI`, not the `VN` this file first specified — the built app has always said `VI`.

The dimmed state was specified here from the start and was **not** built until
2026-09-04: the glyph read `EN` both for global English and for an excluded app,
which collapsed the one question the indicator exists to answer.

Menu layout (HIG: current state first, destructive/quit last, standard shortcuts):

```
┌─────────────────────────────────────┐
│  Vietnamese                         │   state header (disabled label style)
├─────────────────────────────────────┤
│  Disable for “Safari”               │   per-app toggle (verb + real app name)
├─────────────────────────────────────┤
│  ✓ Vietnamese input       ⌃⇧Space   │   mode toggle (check + the LIVE shortcut)
│  ✓ Auto-fix English words           │   auto-fix toggle (check)
├─────────────────────────────────────┤
│  Settings…                     ⌘,   │   opens the window (standard ⌘,)
├─────────────────────────────────────┤
│  Quit GlowKey                  ⌘Q   │
└─────────────────────────────────────┘
```

HIG refinements over the current build:
- Rename "Preferences…" → **"Settings…"** (macOS Ventura+ terminology) with the
  standard **⌘,** key equivalent.
- Give **Quit** the standard **⌘Q**.
- Show the toggle shortcut on the mode row so it is discoverable. It must be read
  from the session, not written out: the hotkey is configurable (four presets plus
  a recorded custom combo), and this row said `⌃⇧Space` regardless until
  2026-09-04. The Quick Guide had the same bug.
- The state header uses the disabled/secondary label style (it is informational,
  not clickable).

## 1b. The application main menu (invisible, and required)

GlowKey is `LSUIElement`, so it draws no menu bar. It installs one anyway
(`app/src/main_menu.rs`), because **Cocoa dispatches every ⌘-key equivalent
through `NSApp.mainMenu` before the responder chain**. Without it there is no Cut,
Copy, Paste, Select All or Undo in any text field the app owns, and ⌘W closes
nothing.

That was not hypothetical: the Macros window exists so a UniKey shortcut table can
be carried across, and its expansion field is where `Việt Nam` goes — a string
most people paste. The app that types Vietnamese could not paste Vietnamese into
itself.

Three submenus. Edit and Window send standard actions to `nil` so the responder
chain resolves and enables them; the App submenu targets the status-item
controller, so About, Settings and Quit have exactly one implementation each.
`hide:` is deliberately absent — hiding an app with no windows and no Dock icon is
a way to lose it.

## 2. Settings window

**Four tabs** (General / Typing / Corrections / Apps & macros), not the single
pane specified below. A single column had grown past 800 points, which is taller
than the content area of a small laptop; each tab now builds its own stack and the
tab title carries the grouping that section headers used to. The original
single-pane sketch is kept below as the record of what was intended when the scope
was two settings. Throughout: system font (SF Pro), standard control metrics, and
system colors so light/dark and contrast are automatic.

The three list windows — Excluded Apps, Macros, Personal Words — are **resizable
and scrolled**. They hold lists of unbounded length and were all built fixed-size
with no scroll view, so rows past the bottom edge were simply unreachable: the
fourteen shipped exclusions overflowed on a clean install, and an import reporting
"214 macros" showed about thirteen.

```
┌──────────────────────────────────────────────────────┐
│ ● ● ●            GlowKey Settings                      │  standard title bar
├──────────────────────────────────────────────────────┤
│                                                        │
│   Typing                                               │  section header (bold, secondary)
│   ┌──────────────────────────────────────────────┐    │
│   │ Tone marks      ⟨ Modern hoà │ Classic hòa ⟩ │    │  NSSegmentedControl (2)
│   │                                                │    │
│   │ ☑ Auto-fix words that aren’t Vietnamese        │    │  NSButton checkbox
│   │    Types “exit” instead of “eĩt”.              │    │  help text (secondary)
│   │                                                │    │
│   │ Toggle Vietnamese / English      ⌃⇧Space       │    │  read-only shortcut row
│   └──────────────────────────────────────────────┘    │
│                                                        │
│   Excluded apps                                        │  section header
│   GlowKey won’t type Vietnamese in these apps.         │  help text
│   ┌──────────────────────────────────────────────┐    │
│   │  Terminal                                    │    │  NSTableView rows:
│   │  iTerm                                       │    │   16pt app icon + name
│   │  Visual Studio Code                          │    │   (greyed if uninstalled)
│   │  Xcode                                       │    │
│   │  …                                           │    │
│   └──────────────────────────────────────────────┘    │
│   ⟨ + ⟩ ⟨ − ⟩                                          │  gradient +/− below list (HIG)
│                                                        │
└──────────────────────────────────────────────────────┘
```

### Controls (all native → free accessibility, focus, light/dark)

| Element | Control | Behaviour |
|---|---|---|
| Tone marks | `NSSegmentedControl`, 2 segments | Modern (`hoà`) / Classic (`hòa`); applies immediately, saves |
| Auto-fix | `NSButton` (checkbox) + help label | Toggles restore-invalid; help text names the `exit` example |
| Toggle shortcut | `NSSegmentedControl` + recorder | Four presets plus "Custom…", which arms a recorder; rendered everywhere by one `hotkey_display` |
| Excluded apps | `NSStackView` of rows in an `NSScrollView` | App icon (16pt) + the name Finder shows; uninstalled app greyed and labelled, never dropped |
| Add | gradient `+` button (or "Add App…") | `NSOpenPanel` scoped to `/Applications`; bundle id → list |
| Remove | gradient `−` button | Removes the selected row |

### HIG / review checklist applied
- **Icons:** real app icons in the list, SF Symbols (or none) elsewhere — no emoji.
- **Contrast & dark mode:** system label/control colors meet AA in both themes.
- **Focus & keyboard:** native controls are Tab-navigable with visible focus;
  the table supports arrow keys and `⌫` to remove.
- **Spacing:** 20pt window margins, 8pt intra-group rhythm (system metrics).
- **One purpose per section:** Typing vs Excluded apps, visually separated.
- **Labels:** every control carries an accessibility label (native default).
- **Immediate + persisted:** every change applies to the live session and writes
  `settings.json` at once — no Apply/OK button, matching macOS Settings behaviour.
- **Reduced motion / Dynamic Type:** no custom animation; system font scales.

## 3. What maps to what

- Menu bar → `app/src/menu_bar.rs`; the invisible main menu → `app/src/main_menu.rs`.
- Settings window → `app/src/prefs/` (`mod.rs` controller, `tabs.rs` panes,
  `widgets.rs` shared helpers, plus `excluded.rs`, `macros_window.rs` and
  `personal_words.rs` for the three list windows).
- App icon and name resolution for the excluded list → `app/src/app_info.rs`.
- The engine already exposes everything the UI drives (`toggle_mode`, `set_style`,
  `set_auto_fix`, `exclusions_mut`), so this is shell-only.

## Open questions
1. Settings window `+` action: an in-app app-picker table vs `NSOpenPanel` over
   `/Applications`. Spec assumes `NSOpenPanel` (simplest, standard). Confirm.
2. Menu bar glyph: text `VN`/`EN` (chosen, zero-asset) vs a template SF Symbol.
   Text is clearer for a language toggle; revisit if you want an icon.
