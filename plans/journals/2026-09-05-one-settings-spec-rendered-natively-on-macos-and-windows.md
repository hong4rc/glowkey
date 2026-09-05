---
title: "One settings spec, rendered natively on macOS and Windows"
date: 2026-09-05
summary: The four settings tabs became data in settings_spec.rs with an AppKit and an egui renderer; review caught an orphaned hotkey status row the macOS cross-check could not.
---

# One settings spec, rendered natively on macOS and Windows

## What happened

The Windows port had copied the macOS settings window's four tabs by hand into
egui. Within three weeks the two windows disagreed on a checkbox label, a caption
opening, a shortcut spelling and whether a Done button exists. The user asked for
one UI for all platforms, based on the macOS layout, with macOS behaving as it
does today, and welcomed improvements.

Research (`plans/reports/research-260905-0944-one-ui-spec-native-backends.md`)
showed no toolkit keeps macOS native: egui, iced, Slint and a webview all
replace AppKit; Slint's own 2026 blog calls its Cupertino style an uncanny
valley; libui-ng is stale and too thin. So the sharing moved one level up:
`app/src/settings_spec.rs` defines tabs, sections, rows, both-language strings,
bindings and dependencies as data, and `prefs/tabs.rs` (AppKit) and
`platform/windows/settings_ui.rs` (egui) render it. Net 140 lines removed.

The UX review (`plans/reports/ux-review-260905-0944-shared-settings-layout.md`)
drove the layout changes: one-sentence captions that also set accessibility
help, "Fix as I type" indented under and disabled by "Auto-fix", section
headers, counts beside Manage… buttons, shortcut placeholders spelled per
platform, no Done button. Two of its findings were wrong and are recorded as
errata: the macOS list windows already had empty states, and the toggle-key
picker was already in General on macOS. The real gap was Windows, which had no
picker at all; it now has the presets, minus Alt+Space (the system-menu key).

## What the review caught

The macOS renderer could only be `cargo check`ed on this Windows machine
(`--target aarch64-apple-darwin` installs cleanly and takes 46s). The
code-reviewer found a regression compile checks cannot see: the hotkey status
line ("Current: ⌃⇧Space" and the recorder prompt) was built and stored in ivars
but never added to any superview, because the `ToggleHotkey` branch added one
view itself and the common path then skipped it. Fixed by having the branch add
the picker and hand the status row back to the common path. Also fixed from the
review: the read-only shortcut row lacked accessibility help; wrapping captions
got a pinned width, since `preferredMaxLayoutWidth` is only a hint; the
caption-length test now checks Vietnamese, which runs longer.

The tester found two headless gaps, now covered by one test: a tab rendered
with auto-fix off and with a Mac-recorded custom hotkey.

Earlier in the session the Windows theme defect from the last handoff was
closed by evidence without opening the window: the Rust registry read returns
light, egui 0.29 resolves `ThemePreference::Light` unconditionally, and the
running binary postdates the fix commit. `plans/reports/windows-handoff-260905.md`
records it. The branch was also rebased onto a rewritten `origin/main`
(re-authored email) and fast-forwarded into local `main`, unpushed.

## Decision

Decision 0010: one settings spec, a native renderer per platform. Section
headers stay in the current macOS style. The per-app toggle hotkey stays a
read-only row. No Done button anywhere.

## Next steps

- On a Mac: open Settings and check `docs/manual-verification.md` §6's new
  item (headers, wrapping, dependent checkbox, counts, VoiceOver help). This is
  the first real run of the AppKit renderer.
- On this machine: open Settings once from the tray to confirm the window is
  light, and to see the spec-rendered egui tabs.
- Decide whether to push `main` (18 commits ahead) and this branch
  (`feat/shared-settings-spec`, 2 commits).
- Alt+Space on Windows: verify whether the hook beats the system menu before
  ever offering it.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
