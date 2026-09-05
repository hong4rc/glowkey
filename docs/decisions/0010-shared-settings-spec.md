# 0010 — The settings window is one spec with a native renderer per platform

## Status

Accepted (2026-09-05).

## Context

The Windows port added a second settings window, in egui, copying the macOS
window's four tabs by hand. Within three weeks the two had drifted in wording
("Launch GlowKey at login" versus "Start at login"), in captions (one opened
"The blunt version:", the other "Off by default:"), in shortcut spelling (`⌃⇧E`
written literally, meaningless on Windows, so the Windows copy dropped the
sentence), and in shape (a Done button on Windows that macOS never had). Linux
in Phase 8 would have been a third copy.

The user asked for one UI for all platforms, based on the macOS layout, with
macOS continuing to work as it does: native AppKit controls, live apply with no
OK button, VoiceOver, keyboard focus, separate resizable list windows.

No cross-platform toolkit satisfies "macOS unchanged". egui, iced, Slint and a
webview all replace AppKit; Slint's own 2026 blog calls its Cupertino style an
uncanny valley. libui-ng wraps real native widgets but is stale and a
least-common-subset API that cannot express the macro table or hotkey recorder.
Research: `plans/reports/research-260905-0944-one-ui-spec-native-backends.md`.

## Decision

Share the description, not the renderer.

`app/src/settings_spec.rs` defines the window as data: four tabs, each a list of
titled sections, each a list of rows. A row is a control (a typed choice, a
checkbox bound to a `Toggle`, the hotkey picker, a read-only shortcut, or a
list's count-and-Manage… button), its label and one-line caption in both
languages, and optionally the toggle it depends on. Shortcuts inside captions are
placeholders (`{toggle_app}`, `{fix_word}`) the renderer spells for its
platform. The spec has no AppKit, egui or Win32 types and compiles on every
target.

`app/src/prefs/tabs.rs` (AppKit) and `app/src/platform/windows/settings_ui.rs`
(egui) walk the spec. Each owns colours, fonts, metrics, wrapping and window
lifetime. The three list windows stay per-platform; only the row that opens them
is in the spec.

Layout decisions taken with the spec, from the UX review
(`plans/reports/ux-review-260905-0944-shared-settings-layout.md`): captions are
one sentence; "Fix as I type" is indented under and disabled by "Auto-fix";
section headers are bold secondary text; list rows show their count; no Done
button anywhere, since both platforms apply live; the toggle-hotkey preset
picker appears on Windows, which previously had no way to change it.

## Consequences

- One row list. A wording change is one edit and lands on every platform.
- The spec is tested: four tabs, every toggle and list placed exactly once, both
  languages present, no hard line breaks, every placeholder expands, dependent
  rows follow their parent.
- macOS keeps every native behaviour. Captions now also set each control's
  accessibility help, which they did not before.
- The macOS renderer was written on Windows and verified only by
  `cargo check --target aarch64-apple-darwin`. Layout on a real Mac is the
  first thing to look at there.
- Alt+Space is the Windows system-menu key, so the Windows picker does not
  offer `OptionSpace`; a settings file that already holds it still shows it as
  the current choice. Whether the hook wins that race is unverified.
- `hotkey_display` now uppercases a recorded key (`⌃⌥K`, not `⌃⌥k`). The menu
  bar and Quick Guide show it the same way, being the same function.

## Alternatives rejected

- **egui everywhere.** One codebase, but macOS loses native controls and
  reopen-after-close. Fails the stated requirement.
- **Slint / iced / Tauri.** Own renderers or a webview; none native; new
  dependencies and, for Slint, a licence decision.
- **libui-ng.** Native widgets, stale bindings, too thin for the list windows.
- **Two hand-written windows.** Zero cost today, drift forever.
