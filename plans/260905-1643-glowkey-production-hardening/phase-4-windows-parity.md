# Phase 4 — Windows shell parity and UI polish

**Depends on:** phase 3 (both edit `hook.rs`; 3 lands first).
**Gated by:** decisions B4 (apply model), B8 (picker scope), B14, B22.

The Windows renderer draws **all seven** `settings_spec.rs` `Control` variants —
that was checked and is true. Every gap below is behavioural inside a rendered
row, or a piece of the shell that has no Windows counterpart at all.

Order matters: **B4 first**, because the apply model is the contract the other
items build on.

## 4.1 — Apply model [B4, decision]

`settings_ui.rs:1397-1399` finalises only on `close_requested`; only `Language`
applies live (`:888`). `ui-design.md:111` says every change applies live, and
macOS honours it.

Note this is not sloppiness: `shell.rs:203-245` is a designed three-way merge
that keeps ⌃⇧W-taught words safe while the hook keeps running. Either make
per-edit application go through that same merge, or keep the button and write the
exception into `ui-design.md`. What cannot stand is the doc and the renderer
disagreeing.

## 4.2 — The app picker [B8, decision]

`settings_ui.rs:1022-1026` is a `TextEdit::singleline` that wants an `.exe`
filename. macOS gets `NSOpenPanel` with real names and icons. Recommended: a
running-process picker plus the free-text escape hatch.

Also: `default_exclusions/windows.rs:8-11` is a Mac-authored table of guessed
executable names, never checked against real processes. Needs a machine.

## 4.3 — Feedback the user has no other way to get

- **No HUD** (`grep -i hud platform/windows/` → nothing). A hotkey toggle is
  silent except for a 16px tray glyph. This is the app's single most important
  signal.
- **Tray never states the current state**: `tray.rs:722`'s first item is the
  literal string "GlowKey", where macOS shows "Vietnamese" / "Excluded in X".
  `describe()` already exists and is used only for the broken case (`:738`) —
  the header is close to free.
- **No session-only-terminal warning state**: `indicator.rs:29-45` has no
  equivalent of macOS's "VI ⚠", so ⌃⇧E in a terminal gives no signal that the
  un-exclusion lasts only until restart.
- **`Notice::PersonalWordsChanged` is dropped** into a catch-all
  (`hook.rs:518`), so a ⌃⇧W decision is invisible.
- **Broken-state glyph** is `⚠` on macOS, `!` on Windows.

## 4.4 — Discoverability [B6]

No Windows welcome guide and no Quick Guide item — `welcome` appears in
`platform/windows/` only as a merged settings field (`shell.rs:343-346`). So the
three hotkeys that carry the product (⌃⇧Space, ⌃⇧E, ⌃⇧W) are undiscoverable on
Windows. ⌃⇧W appears in exactly one caption anywhere
(`settings_spec.rs:399-400`).

Also missing: "Reset input"; the mode item does not name the live hotkey.

## 4.5 — Hotkey recorder [B10]

Not implemented (`settings_ui.rs:935`, `hook.rs:416-431`); a Mac-recorded custom
hotkey is character-matched on Windows, and `Alt+Space` is dropped silently
(`settings_ui.rs:257-266`). Land **after** 3.2 — the recorder sits in the
keystroke path, and a watchdog that predates it is worth having.

Ship a caption saying there is no recorder in the meantime.

## 4.6 — Spec copy [B20]

One file (`settings_spec.rs`), all headless-testable, and it is the Vietnamese
copy of a Vietnamese product:

- `khôi phục` used for both "Auto-fix" and "Restore English words" — two
  different features, one word.
- Typing tab reads `Kiểu gõ / Kiểu gõ` — section header identical to the row
  label. Same collision for `Gõ tắt`.
- Auto-fix is named three different ways across the UI.
- Four vocabularies for the exclusion list across two languages.
- Mixed orthography.

Add a test asserting a row label never equals its section title.

## 4.7 — Accessibility [ui-polish]

Neither renderer gives a screen reader the row label. Tab/arrow navigation
through the Windows segmented controls is untested. macOS Settings tabs are
neither scrollable nor resizable — the same bug the list windows already fixed.

## Validation

`cargo test --workspace`, `cargo clippy -- -D warnings`, plus
`docs/manual-verification-windows.md` Tier 5 for anything visual. Most of this
phase needs eyes on a screen; the spec copy and the tray header do not.
