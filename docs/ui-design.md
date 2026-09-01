# GlowKey UI design — menu bar & Settings window

GlowKey is a native macOS menu-bar utility (objc2 AppKit). The design follows
Apple's Human Interface Guidelines for a **menu bar extra** and a **Settings
window**, not mobile/web patterns. Universal principles applied from the UI/UX
review: no emoji as icons (use real app icons + SF Symbols), 4.5:1 contrast,
visible focus, an 8pt spacing rhythm, one primary purpose per section, and
light/dark parity — all of which macOS system controls give us for free.

Design values (from the product): **simple**, native, unobtrusive. No tabs, no
custom chrome, no color theming — a menu-bar tool should look like it came with
the system.

## 1. Menu bar extra

The status item shows the current state as text so it is legible at a glance and
needs no bitmap: **`VN`** when Vietnamese is active, **`EN`** when off, and a
dimmed **`VN`** (or `–`) when the frontmost app is excluded.

Menu layout (HIG: current state first, destructive/quit last, standard shortcuts):

```
┌─────────────────────────────────────┐
│  Vietnamese                         │   state header (disabled label style)
├─────────────────────────────────────┤
│  Disable for “Safari”               │   per-app toggle (verb + real app name)
├─────────────────────────────────────┤
│  ✓ Vietnamese input       ⌃⇧Space   │   mode toggle (check + shortcut)
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
- Show **⌃⇧Space** on the mode row so the shortcut is discoverable.
- The state header uses the disabled/secondary label style (it is informational,
  not clickable).

## 2. Settings window

A single, non-resizable pane (~480×440pt) — no tabs; the scope is small enough
that grouping into two labelled sections is clearer than a toolbar. System font
(SF Pro), standard control metrics, system colors so light/dark and contrast are
automatic.

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
| Toggle shortcut | static text | Read-only ⌃⇧Space (not editable in v1) |
| Excluded apps | `NSTableView`, 1 column | App icon (16pt) + `localizedName`; uninstalled app greyed, never dropped |
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

- Menu bar → `app/src/menu_bar.rs` (built; needs the HIG label/shortcut tweaks).
- Settings window → `app/src/prefs_window.rs` (Phase 4, to build to this spec).
- The engine already exposes everything the UI drives (`toggle_mode`, `set_style`,
  `set_auto_fix`, `exclusions_mut`), so this is shell-only.

## Open questions
1. Settings window `+` action: an in-app app-picker table vs `NSOpenPanel` over
   `/Applications`. Spec assumes `NSOpenPanel` (simplest, standard). Confirm.
2. Menu bar glyph: text `VN`/`EN` (chosen, zero-asset) vs a template SF Symbol.
   Text is clearer for a language toggle; revisit if you want an icon.
