---
title: "GlowKey — remaining correctness fixes + UX polish"
status: in-progress
created: 2026-09-02
branch: main
---

# GlowKey — remaining fixes + UX polish

All outstanding issues found this session that are safe to land without a live
human at the screen. Each phase is test-covered (engine/tap) or a low-risk,
clearly-described GUI change. The one genuinely risky item — the Chrome
address-bar (omnibox) delivery bug — is scoped but **deferred**, because every
viable fix risks the working normal-field case and cannot be verified headless.

## Outcome
Typing and app-state behave correctly and predictably; the app launches ready to
use; small UX rough edges removed.

## Acceptance criteria
- Arrow / Home / End / PageUp / PageDown keys re-sync the engine (no stale
  baseline, no spurious auto-fix restore at the moved caret).
- The app always launches in Vietnamese; ⌃⇧Space is a session-only toggle; the
  ignore list / auto-fix / tone style still persist.
- Menu-bar glyph and toggle HUD read `VI` / `EN` (per user preference).
- A menu item reveals the log file in Finder.
- All existing tests stay green; clippy clean; the release bundle builds.

## Non-goals
- Chrome omnibox delivery fix (see Phase 4 — deferred, needs live verification).
- English-word ambiguity (`was → ứa`): inherent to Telex without a dictionary.

## Phases
| # | Phase | Layer | Status |
|---|-------|-------|--------|
| 1 | [Caret-navigation flush](./phase-01-caret-navigation-flush.md) | engine + tap, tested | ✅ Done |
| 2 | [Launch always in Vietnamese](./phase-02-launch-vietnamese.md) | config + session, tested | ✅ Done |
| 3 | [UX polish: VI label + Reveal Log](./phase-03-ux-polish.md) | shell (objc2), GUI | ✅ Done |
| 4 | [Omnibox delivery (deferred)](./phase-04-omnibox-deferred.md) | investigation | ⬜ Deferred |
