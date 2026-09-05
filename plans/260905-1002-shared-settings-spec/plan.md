# Shared settings spec — one layout, two native renderers

Status: implemented and reviewed (2026-09-05); macOS runtime check pending on a Mac. Branch: `main` at `9aa0e80`, work on
`feat/shared-settings-spec`.

Inputs: `plans/reports/research-260905-0944-one-ui-spec-native-backends.md`,
`plans/reports/ux-review-260905-0944-shared-settings-layout.md`.

## Outcome

The four settings tabs are defined once, as data, in
`app/src/settings_spec.rs`. macOS (AppKit, `app/src/prefs/tabs.rs`) and Windows
(egui, `app/src/platform/windows/settings_ui.rs`) each walk that spec. The
layout is the macOS one with the review's improvements. macOS keeps native
controls, live apply, VoiceOver, separate list windows.

## Constraints

- No new dependencies. No AppKit, egui or windows-sys types in the spec.
- Nothing on the hook or tap path changes (`docs/decisions/0008`).
- macOS can only be `cargo check --target aarch64-apple-darwin` on this
  machine. Runtime verification of the macOS window is deferred to a Mac.
- Decisions taken (user, 2026-09-05): no Done button on Windows; section
  headers in the current macOS style (bold secondary text); the per-app toggle
  hotkey stays a read-only row.

## Non-goals

- List windows (Excluded apps, Macros, Personal words) keep their current
  per-platform implementation. Only the row that opens them changes.
- No hotkey recorder on Windows. The preset picker only.
- No change to the tray or menu bar.

## Phases

| # | Phase | Files | Status |
|---|---|---|---|
| 1 | Spec + tests | `app/src/settings_spec.rs`, `app/src/main.rs`, `app/src/prefs/widgets.rs` (hotkey_display moves) | done |
| 2 | Windows renderer | `app/src/platform/windows/settings_ui.rs` | done |
| 3 | macOS renderer | `app/src/prefs/tabs.rs`, `app/src/prefs/mod.rs`, `app/src/prefs/excluded.rs`, `macros_window.rs`, `personal_words.rs` | done; `cargo check`/clippy on `aarch64-apple-darwin` only, not run |
| 4 | Docs | `docs/ui-design.md`, `docs/decisions/0010-shared-settings-spec.md`, `docs/handoff.md`, `docs/manual-verification.md` | done |
| 5 | Review + test | code-reviewer and tester subagents; findings fixed (hotkey status row was orphaned; Shortcut row lacked accessibility help; caption width pinned; Alt+Space not offered on Windows; two tests added) | done |

## Acceptance criteria

1. `settings_spec::TABS` has four tabs; every `Toggle` and `ListId` appears
   exactly once; every text has both languages, no hard line breaks; every
   caption placeholder expands. Tested.
2. Windows: no Done button; hotkey preset picker in General; "Toggle current
   app" read-only row; list rows show a count and a Manage… button; "Fix as I
   type" is indented and disabled when auto-fix is off; section headers shown.
   `cargo test -p glowkey` green, clippy `-D warnings` silent.
3. macOS: same rows in the same order from the same spec; captions wrap
   instead of carrying `\n`; every control's caption is its accessibility
   help; "Fix as I type" enabled state follows auto-fix live; list counts
   refresh when a list window changes them. `cargo check --target
   aarch64-apple-darwin` green.
4. Both platforms' strings come from the spec, so the five drifts named in the
   UX review cannot recur.

## Risks

- Alt+Space is the Windows system-menu key. The Windows picker does not offer
  `OptionSpace`; a saved one is still shown. Whether the hook wins against the
  system menu is unverified.
- macOS renderer is unrun. Layout regressions (label widths, wrapping) are
  possible and can only be seen on a Mac.

## Rollback

Revert the branch. The spec is additive; the renderers are the only behaviour
change.
