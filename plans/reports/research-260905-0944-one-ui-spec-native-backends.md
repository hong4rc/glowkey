# Research Report: One settings UI for every platform, with macOS unchanged

Conducted 2026-09-05 09:50 (Asia/Saigon). Branch `main` at `9aa0e80`.

## Executive Summary

Requirement as stated: one UI definition for all platforms, modelled on the
current macOS layout, and macOS must keep working exactly as it does now
(native AppKit controls, live-apply with no OK button, VoiceOver, Tab focus,
three separate resizable list windows).

No cross-platform toolkit satisfies "macOS unchanged". Every candidate that
renders its own widgets (egui, Slint, iced, Tauri webview) replaces AppKit and
therefore changes macOS. Slint's own March 2026 blog admits its Cupertino style
is an "uncanny valley" and deprecated native styling. The only toolkit wrapping
real native widgets on all three desktops, libui-ng via `libui-rs`, is stale and
too thin for the macro table, import buttons and hotkey recorder.

Recommendation: **share the description, not the renderer.** Lift the four tabs,
sections, rows and their bindings into one platform-neutral Rust spec
(`app/src/settings_spec.rs`), and make the existing AppKit code and the existing
egui code two thin renderers of that spec. macOS behaviour is untouched because
AppKit still draws it. Windows and later Linux get the macOS layout for free
because they read the same spec. Roughly 60% of `prefs/tabs.rs` and 40% of
`settings_ui.rs` become one file; the rest is glue.

## Research Methodology

- Sources: 5 web searches, plus repo inspection (`docs/ui-design.md`,
  `docs/decisions/0001`, `app/src/prefs/*`, `app/src/platform/windows/settings_ui.rs`).
- Date range: 2022 to March 2026.
- Terms: Slint cupertino fluent native styles; Rust native widgets libui-ng
  xilem; egui AccessKit VoiceOver; iced slint egui gpui comparison 2026; Tauri v2
  tray settings webview.

## Key Findings

### 1. What "macOS works like currently" actually pins

From `docs/ui-design.md` §2 and `prefs/`:

| Property | Provided by | Survives a non-native toolkit? |
|---|---|---|
| System font, system colours, light/dark | AppKit | Partly (egui needs manual font + theme, as Windows showed) |
| Free accessibility labels, VoiceOver | AppKit | Degraded. egui has AccessKit, but labels are manual and the tree is flatter |
| Tab focus, arrow keys, ⌫ removes row | AppKit | Must be re-implemented |
| `NSOpenPanel` scoped to /Applications | AppKit | Must be bridged |
| Windows reopen after close (fix `0f442bf`) | AppKit | Lost: eframe/winit allow one event loop per process (open Windows defect) |
| Live-apply, persisted at once | Controller code | Yes, toolkit-independent |

The last row is the tell: the *behaviour* is already in the controller and
model, not in AppKit. The *layout* is what is duplicated today.

### 2. Toolkit landscape, 2026

- **egui 0.29 / eframe.** Already in the repo for Windows, 1,625 lines. Fastest
  to build, AccessKit on Windows and macOS, but immediate-mode, own rendering,
  one event loop per process. Adopting it on macOS violates the requirement.
- **Slint.** Declarative DSL, Rust, C++, JS. Ships `fluent` and `cupertino`
  styles implemented in pure Slint. Slint's 2026 blog says Cupertino "has fallen
  into an uncanny valley, close to native but not close enough" and changed
  default-style policy. Not native, adds a DSL and a license decision. Reject.
- **iced.** Elm-style, own renderer, no native look. Reject for this need.
- **GPUI.** Reported abandoned as an open-source project. Reject.
- **libui-ng (`libui-rs`, `iui`).** Real Cocoa, Win32, GTK widgets from one API,
  "least common subset". Last meaningful Rust activity years old; no table with
  icons, no scroll-view control, no hotkey recorder, GTK on Linux. Would force
  rewriting macOS *down* to the subset, so macOS would change. Reject.
- **Tauri v2.10.** WKWebView / WebView2 / WebKitGTK. Sub-5 MB, but first-launch
  WebView2 init is slow, UI is HTML/JS, and decision 0001 is all-Rust. Reject.
- **Xilem / Masonry, Ribir, Alchemy.** Own renderers or pre-alpha. Reject.

Conclusion: no toolkit swap keeps macOS native. The sharing has to happen one
level up.

### 3. The shared-spec pattern

Define the window once as data:

```rust
// app/src/settings_spec.rs — no AppKit, no egui, no windows-sys
pub enum Control {
    Segmented { field: Field, options: &'static [(&'static str, &'static str)] }, // (en, vi)
    Checkbox  { field: Field, caption: Option<Text> },
    HotkeyPicker { field: Field },
    ListButton { window: ListWindow, count: fn(&Settings) -> usize },
    Shortcut  { label: Text, display: fn(&Settings) -> String },
}
pub struct Row { pub label: Text, pub control: Control }
pub struct Section { pub title: Option<Text>, pub rows: Vec<Row> }
pub struct TabSpec { pub title: Text, pub sections: Vec<Section> }
pub fn tabs() -> [TabSpec; 4]   // General, Typing, Corrections, Apps & macros
```

`Field` is an enum naming each `Settings` member with `get`/`set` closures, so
the renderer never knows what a setting means. `Text` is the existing `t(en, vi)`
pair. `ListWindow` names Excluded / Macros / Personal words.

Each platform then has one function `render(tab: &TabSpec, ...)`:

- **macOS:** walks the spec and emits `NSSegmentedControl`, `NSButton`,
  `NSTextField` through the helpers already in `prefs/widgets.rs`
  (`form_row`, `caption`, `tab_stack`). Same controls, same metrics, same
  behaviour. Nothing the user sees changes.
- **Windows / Linux:** walks the same spec with egui widgets, replacing the
  hand-written tab bodies in `settings_ui.rs`.

The three list windows keep their own spec (`ListSpec { columns, add, remove,
import }`) and their own two renderers. Their macOS windows stay separate
`NSWindow`s; on Windows they stay whatever the Windows session decides
(overlay today, child viewport later). That decision is unaffected.

### 4. What stays per platform, on purpose

- Tray / menu bar. `NSStatusItem` versus `Shell_NotifyIcon`, small, deeply native.
- File pickers, font loading, theme read, launch-at-login.
- Window lifetime. AppKit reopens; eframe's one-loop-per-process is a Windows
  defect to fix separately.

### 5. Security and performance

No new dependencies, no new event loop, no webview. Nothing on the hook or tap
path is touched (`docs/decisions/0008`). Startup cost unchanged.

## Comparative Analysis

| Option | macOS unchanged | One layout definition | New deps | Effort | Verdict |
|---|---|---|---|---|---|
| Shared spec + AppKit / egui renderers | Yes | Yes | None | Medium | **Recommend** |
| egui everywhere | No | Yes | None | Medium, plus macOS rewrite | Fails requirement |
| Slint cupertino / fluent | No (uncanny valley per Slint) | Yes | Slint + license | High | Reject |
| libui-ng | No (subset) | Yes | Stale bindings | High | Reject |
| Tauri webview | No | Yes | Tauri + JS | High | Reject |
| Keep two hand-written UIs | Yes | No | None | Zero now, drift forever | Status quo |

## Implementation Recommendations

### Quick start

1. Read `prefs/tabs.rs` and the tab bodies in `settings_ui.rs` side by side;
   list every row. Expect about 25 rows across four tabs.
2. Write `app/src/settings_spec.rs` with the enums above and `tabs()`. Unit
   test: every `Field` round-trips get/set on a `Settings`, every `Text` has
   both languages, tab count is 4.
3. macOS: replace the body of each `Tab::*` builder in `tabs.rs` with a spec
   walk using `widgets.rs` helpers. Verify by running: window must be pixel-
   identical apart from nothing. Keep `hotkey_display` shared as it is.
4. Windows: replace the tab bodies in `settings_ui.rs` with the egui walk.
   Existing headless tests (font, caption colour, theme) keep passing.
5. Delete the duplicated row definitions. Update `docs/ui-design.md` §3
   mapping to name `settings_spec.rs` as the owner of layout.
6. Record it as `docs/decisions/0010-shared-settings-spec.md`.

### Common pitfalls

- Putting AppKit or egui types in the spec. The spec must compile on all three
  targets with no cfg; CI's Linux job should compile it.
- Letting the spec grow a layout engine. Rows, sections, tabs only. Pixel
  metrics stay in each renderer (20 pt margins on macOS, whatever egui uses).
- Migrating the list windows in the same change. Do tabs first, lists second.

## Resources & References

- [Slint: Changing the Default Style, deprecating native styles (Mar 2026)](https://slint.dev/blog/default-native-style-change)
- [Slint 1.3 native styles announcement](https://slint.dev/blog/slint-1.3-released)
- [Slint issue #3431, default style on Windows and macOS](https://github.com/slint-ui/slint/issues/3431)
- [libui-rs, Rust bindings to libui](https://github.com/rust-native-ui/libui-rs)
- [Are we GUI yet?](https://areweguiyet.com/)
- [egui AccessKit integration PR #2294](https://github.com/emilk/egui/pull/2294)
- [egui accessibility issue #167](https://github.com/emilk/egui/issues/167)
- [Windows GUI in Rust, egui / WinUI / iced / Slint guide 2026](https://rust-pc.github.io/rust-windows-gui.html)
- [Tritium: Rust GUI observations](https://tritium.legal/blog/desktop)
- [Tauri v2 config reference](https://v2.tauri.app/reference/config/)
- [Tauri webview versions](https://v2.tauri.app/reference/webview-versions/)
- Repo: `docs/ui-design.md`, `docs/decisions/0001-all-rust-objc2-shell.md`,
  `app/src/prefs/tabs.rs`, `app/src/prefs/widgets.rs`,
  `app/src/platform/windows/settings_ui.rs`.

## Next steps

1. User confirms the direction: shared spec, two renderers, macOS untouched.
2. Open a plan `plans/260905-xxxx-shared-settings-spec/` with two phases:
   tabs, then list windows.
3. Do it on a branch off `main`, after the Windows theme confirmation.

## Unresolved questions

1. Should the spec also carry the tray menu items, or is that over-reach for a
   menu that is a dozen lines per platform?
2. Linux (Phase 8) renderer: egui, or GTK for a native look there too? The
   spec makes either possible; not deciding now.
3. Does the user accept that the Windows egui window keeps egui's look, only
   with the macOS *layout*, rather than mimicking macOS chrome?
