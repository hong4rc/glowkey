# Fix known issues + remaining features (from docs/handoff.md §6/§11)

Status: completed (code + tests + docs; GUI/omnibox live verification pending user)
Branch: main · Date: 2026-09-02

## Scope

1. Chromium omnibox mangling (`hoongf`→`hoồng`) — guarded fix: AX-detected trailing
   selection cleared with one forward-delete, Chromium bundle ids only, before any
   backspace emit. Normal fields (no selection) untouched by construction.
2. Terminal exclusion robustness — tombstone model so shipped defaults merge into
   existing settings files; ⌃⇧E un-exclusion of a known terminal is session-only
   (restart re-excludes) with a warning HUD. Permanent removal only via the
   Excluded Apps window.
3. English/Telex ambiguity (`was`→`ứa`) — opt-in "Restore common English words"
   with an embedded common-word list, applied at commit even when the render is
   valid Vietnamese. Default OFF (it inverts the ambiguity for `cát`, `cả`, `hải`…).
4. Toggle-hotkey recorder — `HotkeyPreset::Custom` + "Record…" in Settings; the
   tap captures the next ⌃/⌥ combo; Esc cancels.

## Acceptance

- `cargo test --workspace` green, `cargo clippy --workspace --all-targets` clean.
- New engine behavior unit-tested (tombstones, session-only terminal toggle,
  English restore, custom hotkey decide-path).
- `scripts/build-app.sh release` builds; GUI additions verified by the user by eye
  (headless-unverifiable, as before).

## Non-goals

- InputMethodKit composition path (contradicts the CGEventTap design decision).
- Full English dictionary; the wordlist is curated best-effort.
- Legacy encodings/VIQR (intentionally omitted, see handoff §2).
