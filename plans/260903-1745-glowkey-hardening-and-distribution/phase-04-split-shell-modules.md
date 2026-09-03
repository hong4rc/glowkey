---
phase: 4
title: "Split the two oversized shell files"
status: in-progress
priority: P2
effort: "1d"
dependencies: [3]
---

# Phase 4: Split the two oversized shell files

## Overview

`app/src/tap.rs` is 1992 lines and `app/src/prefs_window.rs` is 1423 — together
half the project's code, in two files. Everything else in `app/src/` is under
450. Both grew honestly, one feature at a time, and both are now past the point
where a reader can hold them in their head; `tap.rs` in particular carries the
project's most delicate logic (the full-suppression invariant) buried among
keycode constants and NSString plumbing.

This is a **pure move**. No behavior changes, no signatures change, no tests are
rewritten. The existing test set passing untouched is the entire proof, which is
why Phase 3 comes first.

## Requirements

- Functional: identical behavior. The same tests, unmodified, pass before and
  after.
- Non-functional: no file in `app/src/` over roughly 700 lines.
- Non-functional: every moved item keeps its doc comment. The comments in these
  two files carry the project's hard-won "why" (the race that forced full
  suppression, the `NSAlert::layout()` ordering, the omnibox trailing selection)
  and are worth more than the code.
- Constraint: objc2's `define_class!` cannot be split across files. The
  controller class and its action methods stay in one module; only helpers,
  window builders and free functions move.

## Architecture

**`app/src/tap.rs` → `app/src/tap/`**

| File | Contents | Approx. lines |
|---|---|---|
| `mod.rs` | `TapState`, `run`, the C callback + `catch_unwind` wrapper, circuit breaker, re-exports | ~600 |
| `decide.rs` | `Decision` (`tap.rs:913`), the `decide` method (`tap.rs:701`) in its own `impl TapState` block, and the `#[cfg(test)] mod tests` at `tap.rs:1469-1992` that builds real `CGEvent`s | ~600 |
| `hotkey.rs` | `is_ctrl_shift`, `is_toggle_hotkey`, `is_app_toggle_hotkey`, the recorder state machine, keycode constants | ~250 |
| `emit.rs` | `emit`, `post_key`, `post_key_with_flags`, `post_string`, the tagged `CGEventSource`, the omnibox-guard call site | ~300 |
| `event.rs` | `integer_field`, `unicode_char`, `modifier_names`, `is_shortcut`, `is_caret_move`, `is_own_event`, `frontmost_bundle_id`, `own_bundle_id`, `is_chromium_browser` | ~250 |

Two Rust facts make this legal and are worth stating, because they are the
usual reason a split like this gets abandoned halfway: an **inherent `impl`
block may live in any module of the defining crate**, so `decide` can move to
`tap/decide.rs` while `TapState` stays in `tap/mod.rs`; and **a private field is
visible to descendant modules**, so `tap::decide` reaches `TapState`'s private
fields without any of them becoming `pub`.

The split follows a real boundary, not a line count: `decide.rs` is the pure
function of (event, session) that the tests already treat as the unit under
test, and `emit.rs` is everything that touches the outside world. That is the
project's actual architecture — a pure decision and an impure emit — and the
files should say so.

**`app/src/prefs_window.rs` → `app/src/prefs/`**

| File | Contents | Approx. lines |
|---|---|---|
| `mod.rs` | `define_class!` block with the ivars and all action methods, `show`, `hotkey_recording_done` | ~550 |
| `tabs.rs` | `build_window` and the four tab builders it inlines today (lines 597–994) | ~400 |
| `excluded.rs` | `build_excluded_window`, `refresh_list`, `add_app`, `remove_app` support | ~150 |
| `macros_window.rs` | `build_macros_window`, `refresh_macros`, import/export helpers | ~250 |
| `widgets.rs` | `tab_stack`, `make_label`, `caption`, `form_row`, `input_field`, `hotkey_display`, `display_name`, the label-column width constant | ~200 |

The action methods must stay inside `define_class!`; the *bodies* that are long
(macro import at 100 lines, export at 40) move to free functions in
`macros_window.rs` that the action calls, which is where the size actually is.

## Related Code Files

- Modify: `app/src/main.rs` — module declarations only.
- Delete: `app/src/tap.rs`, `app/src/prefs_window.rs` (replaced by directories).
- Create: the ten files above.
- Modify: `docs/handoff.md` §3 — the file map is part of the handoff's value and
  goes stale the moment this lands.

## Implementation Steps

1. Record the baseline: `cargo test --workspace 2>&1 | tail -3` — the exact test
   count and the green line. This is what "unchanged" is measured against.
2. Split `tap.rs` one file at a time, compiling between each. `event.rs` first
   (leaf, no dependencies), then `hotkey.rs`, `emit.rs`, `decide.rs`, leaving
   `mod.rs` as the remainder.
3. Keep every item's visibility as narrow as it can be — `pub(crate)` or
   `pub(super)`, not `pub`. A move that widens the API is not a pure move.
4. Run the test set after each file. Any test that needs editing means the move
   was not pure — stop and reconsider rather than editing the test.
5. Repeat for `prefs_window.rs`, moving the long action bodies to free functions
   as described.
6. `cargo clippy --workspace --all-targets` must stay silent. Watch for the
   unnecessary-`unsafe` warnings noted in `docs/handoff.md` §9 reappearing on
   moved objc2 calls.
7. Update the handoff file map.

## Success Criteria

- [ ] Same test count, all green, with **zero** edits to any test file
- [ ] Clippy silent
- [ ] No `app/src/` file over ~700 lines
- [ ] `git diff --stat` shows moves, not rewrites (line totals roughly conserved)
- [ ] The app still types Vietnamese, verified by hand once after the split
- [ ] `docs/handoff.md` §3 file map matches reality

## Risk Assessment

- **A "pure move" that quietly changes behavior.** The classic way this happens
  is a lazily-initialised global getting duplicated per module. There is exactly
  one file-scope static to protect — `DISABLED: AtomicBool` at `tap.rs:102`, the
  latching circuit breaker. The others (`DEBUG` at `:90`, `OWN` at `:1099`,
  `ax.rs`'s `SYSTEM` at `:54`) are function-local `OnceLock`s and move with their
  function safely.
  *Signal:* tests pass but typing produces doubled characters or the feedback
  guard stops recognising GlowKey's own events. *Response:* `DISABLED` stays in
  exactly one module and is imported, never re-declared. Grep for `static` after
  the split and confirm the count is unchanged.
- **Refactoring the delicate part.** `decide()` and the emit path are where the
  project's correctness lives. The temptation to improve them while moving them
  must be refused — improvement is a separate commit with its own tests.
- **objc2 `define_class!` resists splitting.** Already accounted for above; if a
  method body cannot be moved out cleanly, leave it. The line budget is a target,
  not a rule to satisfy by damaging the code.
- **Merge conflict with concurrent work.** This phase rewrites the two largest
  files wholesale. It must not run at the same time as any other change to
  `app/src/` — including Phases 5 and 6, which both touch `tap.rs`. Run those
  first, or land this in a single sitting.

## Outcome — 2026-09-03

Both files split. Same 135 tests, all green, with **zero edits to any test
body** — the only change to `tests.rs` is its `use` block, since `use super::*`
reached everything while it all lived in one file and now the siblings have to be
named. Clippy silent.

| Before | After | Largest |
|---|---|---|
| `tap.rs` 2255 lines | `tap/{mod,decide,keys,emit,settings,health,permission,tests}.rs` | 531 (`tests.rs`) |
| `prefs_window.rs` 1423 lines | `prefs/{mod,tabs,excluded,macros_window,widgets}.rs` | 522 (`mod.rs`) |

Nothing in `app/src/` now exceeds 531 lines; the target was ~700.

**The split deviates from the plan in one way, because measurement disagreed with
the plan.** The plan proposed five files for `tap.rs` and did not know about two
things: the `impl TapState` block was ~820 lines and *mostly a wall of forty
`*_and_save` settings accessors* — four lines each, none of it on the keystroke
path — and Phase 6 had just added a health monitor. So `settings.rs`,
`health.rs` and `permission.rs` are additional, and the boundary the plan cared
about most is intact: `decide.rs` holds the pure decision, `emit.rs` holds
everything that writes to the outside world. That is the project's real
architecture, and the files now say so.

Two Rust facts made it legal and are recorded in the phase file above: an
inherent `impl` block may live in any module of the defining crate, and a private
field is visible to descendant modules. Nothing became `pub`; cross-module items
are `pub(super)`.

The `import_macros` (99 lines) and `export_macros` (36 lines) action bodies moved
out of `define_class!` into free functions, as the plan asked — 141 lines of file
dialog and table parsing had been sitting among forty four-line toggles.

### Evidence it is a move, not a rewrite

- 135 tests before, 135 after, no test body touched.
- `tap.rs` 2255 lines → 2380 across eight files; the difference is eight module
  headers and their import blocks.
- Exactly two file-scope statics survive, each in one place: `DISABLED` in
  `tap/mod.rs`, `TAP_DEAD` in `tap/health.rs`. The rest are function-local
  `OnceLock`s that moved with their functions.
- New files are rustfmt-clean (the pre-existing drift elsewhere was left alone by
  the owner's decision).
