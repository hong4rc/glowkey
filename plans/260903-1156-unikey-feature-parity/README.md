# Unikey feature parity — what is worth copying

Status: **all phases implemented and reviewed** 2026-09-03. 94 tests green,
clippy clean, verified running. An adversarial review returned BLOCKED with
three confirmed shipping defects; all are fixed and pinned by regression tests.
See the "Outcome" note at the end of each phase, and "Review" below.

## Outcome

GlowKey already covers most of what a Unikey or EVKey user relies on daily. This
plan names the gaps that are actually worth closing, in priority order, and says
explicitly which Unikey features are being left out and why. Success is a
switcher being able to move from EVKey to GlowKey without giving anything up
they used.

## Non-goals (unchanged from the project's standing decision)

Legacy character sets (TCVN3, VNI for Windows), VIQR, the Microsoft layout, and
clipboard encoding conversion. Every modern macOS application is Unicode in
composed normal form, so these add no value. The Windows control panel devotes
half its surface to them; that surface should stay empty here.

## Audit — Unikey and EVKey feature by feature

Verified against the code and by probing the engine, not from memory.

| Feature | Unikey / EVKey | GlowKey |
|---|---|---|
| Telex, VNI input methods | yes | yes |
| Tone placement, modern and classic | yes | yes (`PlacementStyle`) |
| Free tone marking, any key order | yes | yes (`hofong`, `hoonfg`) |
| Spell check and restore invalid words | yes | yes (auto-fix) |
| **Repeat the tone key to reject it** | yes | **yes — already inherited** |
| Capitalize first letter of a sentence | EVKey only | yes |
| Text expansion macros | yes | yes |
| Per-application enable and disable | yes | yes, with tombstones |
| Global toggle hotkey | yes | yes, plus a recorder |
| Start at login, start hidden, panel at start | yes | yes |
| Menu bar indicator | yes | yes, plus a heads-up display flash |
| **Vietnamese user interface** | yes | **no** |
| **Macro table import and export** | yes | **no** |
| **Quick Telex consonant shortcuts** | yes | **no** |
| Sound on mode switch | EVKey only | no, has a visual flash instead |
| Telex and VNI accepted at the same time | EVKey only | no |
| Legacy character sets, VIQR, converter | yes | deliberately omitted |

### Correction to the handoff

Section 6.3 states "No per-word escape hatch exists yet (a
press-the-tone-key-again to reject would be the Unikey-style fix)." That is
wrong — the `vi` crate already implements it, confirmed by probe:

```
cas -> cá      cass -> cas     casss -> cass
aa  -> â       aaa  -> aa
dd  -> đ       ddd  -> dd
hoongf -> hồng     hoongff -> hôngf
```

Repeating the diacritic key removes the mark and emits the literal key, exactly
as Unikey does. So the English and Vietnamese ambiguity documented there already
has its standard escape hatch, and the opt-in English word list is a convenience
on top rather than the only remedy. Fix the section and add a test pinning the
behavior, so nobody re-implements it.

## Phase 1 — Vietnamese user interface

The largest gap, and the one a Vietnamese user notices first. Every competitor
ships a Vietnamese interface; GlowKey's menu, Settings window, heads-up display
and permission alert are English only. Unikey exposes this as a single
"Vietnamese interface" checkbox.

Design: a string table in the shell keyed by an enum, with a language picker in
Settings under General (System, Tiếng Việt, English) persisted in `Settings`.
Default to following the system language, which is what a native application
does and what the checkbox cannot express. The engine crate stays untouched — it
has no user-facing strings.

Acceptance: every string the user can see has both forms; switching the picker
updates the open window without a restart; the permission alert, which runs
before the main loop, also honors it.

Risk: the Settings window is built with fixed widths in places (the macro row
pins a label to 320 points). Vietnamese runs longer than English, so the layout
needs checking at the longest strings rather than assuming.


**Outcome: done.** `Language` enum in the engine, persisted; `app/src/strings.rs`
picks strings at the call site with `t(english, vietnamese)`; picker in Settings →
General. Resolved before the permission gate so that alert is translated too.
Verified live: switching to Tiếng Việt retitled the window to "Cài đặt GlowKey"
and relabelled every header with no restart. The predicted layout risk did not
bite, but the translation pass did surface a real defect — "Typing" and "Input
method" both rendered as "Kiểu gõ"; the section header is now "Gõ phím".

## Phase 2 — Macro table import and export

A switcher arrives with a macro table they have curated for years. Right now the
only way in is retyping it. Unikey and EVKey both read and write a table file.

Design: two buttons in the Macros window. Export writes the current macros;
import merges, reporting how many were added and how many shortcuts collided.
`NSSavePanel` is already declared in the crate features and unused, and
`NSOpenPanel` is already wired for "Add App", so both panels are available.

Format: accept EVKey's plain `shortcut:expansion` lines per row for import,
since that is what people have, and write the same on export. Read our own JSON
too if a file starts with a brace — cheap to support and lossless.

Acceptance: a table exported and re-imported round-trips exactly; a malformed
line is skipped with a count rather than aborting the import; importing never
silently overwrites an existing shortcut without saying so.


**Outcome: done.** `Macro::parse_table` / `Macro::format_table` in the engine
with six tests, Import/Export buttons in the Macros window, merge reporting
`(added, skipped)` in a modal. Junk lines are skipped with a count rather than
aborting, and an existing shortcut is never silently overwritten.

## Phase 3 — Quick Telex

"Quick Telex" turns doubled consonants into digraphs:
`cc`→`ch`, `gg`→`gi`, `kk`→`kh`, `nn`→`ng`, `pp`→`ph`, `qq`→`qu`, `tt`→`th`,
`uu`→`ư`. Probed and confirmed absent — the engine passes all of them through
unchanged.

Design: opt-in checkbox under Typing, off by default, applied in the engine
before the `vi` crate sees the keys. Restrict expansion to the syllable-initial
position, which is where Unikey applies it and where the digraphs are legal
Vietnamese onsets.

Acceptance: with the option on, `cc`→`ch` and `nn`→`ng` at a word start; with it
off, behavior is byte-identical to today; English words with doubled consonants
in the middle (`letter`, `happy`, `accept`) are untouched because the expansion
never fires mid-syllable.

Risk, and the reason it is last: this is the feature most likely to make English
typing worse, and it interacts with auto-fix, which is exactly the pairing that
produced today's `đddc` and `ưwork` bugs. Do it only after Phases 1 and 2 are
settled, and pin the interaction with tests that type English words with the
option on.


**Attribution corrected 2026-09-03** after reading the actual UniKey source:
`quickTelex` appears nowhere in it, so this is an EVKey / later-UniKey idea, not
a 2015 UniKey option. See `plans/reports/xia-260903-1447-unikey-source-comparison.md`.

**Outcome: done.** `expand_quick_telex` runs inside `render()` before `vi` sees
the keys, syllable-initial only. Five tests, including one asserting `letter`,
`happy`, `accept`, `little` and `sudden` render identically with the option on
and off, and one asserting byte-identical output when off.

## Settings window — where this lands

Three phases add one picker, two buttons and one checkbox. The window is a
single scrolling stack today with "General" and "Typing" headers, and it is
already the longest surface in the application.

- The language picker joins General, at the top: it changes everything below it.
- Quick Telex joins Typing next to the other typing toggles, with a caption
  naming the trade-off, matching how auto-fix and English restore already
  explain themselves.
- Import and export belong in the Macros window, not Settings — they act on that
  window's content.

Two rules worth honoring as the window grows: keep each option's caption next to
the control rather than collecting explanations elsewhere, and keep exactly one
primary action per window. If Typing grows past roughly eight controls, split it
before it becomes a wall.

## Sequencing

Phase 1 and Phase 2 are independent and can be done in either order. Phase 3
depends on nothing technically but should follow both, for the risk reason
above. The handoff correction is a few minutes and should go first, so the next
session does not build an escape hatch that already exists.

## Review

An adversarial review of the finished diff returned **BLOCKED**. It was right on
every count that mattered, and each defect is now fixed with a regression test.

1. **Macro import silently overwrote the user's macros.** `Session::add_macro`
   is add-*or-replace* and answers `true` either way; the merge treated that as
   "added", so an imported `vn` destroyed the user's own `vn` and the interface
   reported it as a clean import. The `skipped` counter was unreachable. The
   merge rule now lives in `Session::import_macros`, checks for the collision
   before calling `add_macro`, and is covered by three tests.
2. **Quick Telex ran under VNI**, where its Telex key sequences put a literal `w`
   on screen that auto-fix cannot repair. Now gated to Telex.
3. **Quick Telex destroyed ALL-CAPS.** Uppercasing only the head of the digraph
   left a lowercase key in the sequence, which defeated the all-caps detection:
   `CCAO`→`ChAO`, `NNUOWIF`→`Người`. Both trigger keys shifted now uppercases the
   whole digraph.
4. **Export/import did not round-trip.** A `#`-leading shortcut, a trailing space
   in an expansion (ordinary in gõ tắt) and an empty expansion were all silently
   lost. Those tables now take the JSON path.
5. **`rebuild_windows` freed the window from inside its own control's action**,
   and held three `RefMut` guards across `close()`. Replaced windows are now
   retired to a list and released on the next rebuild, and the guards are taken
   in a `let` binding first.
6. **The About window kept its first language** — now invalidated on change, and
   the Excluded/Macros windows are reopened if they were open.
7. **Seven strings were still English.** Translated.

Also caught by the live pass the review could not perform: the Vietnamese
auto-fix caption overflowed the window (x=464 in a 465pt window) — exactly the
risk Phase 1 flagged — and is now wrapped. Verified after fixing: no control
overflows, the language switch survives the teardown-from-its-own-action path,
and the Macros window opens as "Gõ tắt" with "Thêm / Nhập… / Xuất…".

Deliberately not changed: macro matching runs on unexpanded raw keys, so with
Quick Telex on a macro whose shortcut is `ch` is unreachable while one whose
shortcut is `cc` fires against a screen showing `ch`. There is no desync — the
backspace count comes from `current_word()` — but it is surprising, and which
form should match is a product decision, not a bug fix.
