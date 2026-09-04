---
phase: 1
title: "Lift the escape in the engine"
status: completed
priority: P1
effort: "3h"
dependencies: []
---

# Phase 1: Lift the escape in the engine

## Overview

Teach the engine that a Backspace can undo an escape: when the shortened word is
spellable again, drop the verbatim rendering and report the edit that turns what
is on screen into the restored word. On its own this changes nothing the user
sees — Phase 2 applies the edit — which is what makes it independently testable.

## Requirements

- Functional: after a Backspace, if the word was escaped and the remaining raw
  keys render to something the spell check accepts, the escape lifts and the
  engine composes the transformed word again.
- Functional: a word that is still unspellable stays escaped. So does a word that
  was never escaped — this must be inert for everyone with the option off.
- Functional: the engine reports the edit needed to reconcile the screen, since
  the screen currently holds the verbatim keys and will hold the render.
- Non-functional: the repaired state must satisfy the blind model's invariant —
  `current_word()` equals what the emitted edit leaves at the caret
  (`docs/handoff.md` §5).
- Non-functional: no change to the escape's *entry* condition.

## Architecture

**Where it goes.** `Engine::backspace_visible_char` (`lib.rs:~712`) is the
mid-word Backspace path the tap uses. It returns `bool` today: `true` means "I
stayed in step with the screen, pass the key through", `false` means "flush". It
gains a third answer — "I can put this right, here is the edit" — so the return
type becomes something like:

```rust
pub enum BackspaceOutcome {
    /// Nothing composed, or no single removal reproduces the screen: flush.
    Flush,
    /// The engine is in step with what the host's delete will leave.
    InStep,
    /// The escape lifted: the host must NOT delete separately — apply this
    /// instead. Backspaces cover the whole on-screen word including the
    /// character the user asked to delete.
    Repair(KeyResponse),
}
```

Three named outcomes rather than a `bool` plus an out-parameter, because the
caller must treat `Repair` differently in kind — it suppresses the key — and a
boolean that sometimes means "also apply this" is exactly the sort of contract
that gets misread once and corrupts a document.

**The order of operations.** The current search picks the single raw-key removal
that re-renders to *the screen minus its last character*. While escaped that is
trivially "drop the last key", because the render is the raw keys. So:

1. Note the on-screen text before anything changes (`self.rendered`).
2. Do the existing search, which lands the raw log on the shortened word.
3. **If escaped**, re-run the spell check's own question against the shortened
   word's *un-escaped* render. If it passes, clear `escaped`, re-render, and
   return `Repair` with `backspaces` = the noted on-screen text's UTF-16 length
   and `insert` = the new render.
4. Otherwise behave exactly as now.

Reusing `last_key_made_it_impossible`'s judgement rather than inventing a second
one keeps the entry and exit conditions from drifting apart — two rules that must
agree, written once.

**Why `backspaces` covers the whole word.** The user's Backspace is suppressed by
the caller, so the edit is responsible for removing everything on screen for this
word — `hoongfa`, all seven units — not just the six that would remain after a
delete the host never performs.

## Related Code Files

- Modify: `crates/glowkey-engine/src/lib.rs` — `BackspaceOutcome`,
  `Engine::backspace_visible_char`, `Session::backspace_visible_char`.
- Modify: `crates/glowkey-engine/tests/midword_spell_check.rs` — the exit is part
  of that feature's contract and belongs with its tests.
- Modify: `crates/glowkey-engine/tests/properties.rs` — `backspace_step` must
  learn the third outcome.

## Implementation Steps

1. Add the enum and change `Engine::backspace_visible_char`; keep both existing
   answers behaving identically so the only new path is `Repair`.
2. Thread it through `Session::backspace_visible_char`, whose `is_active()` guard
   stays.
3. Tests, from the report outwards:
   - `hoongf` `a` ⌫ → `Repair { backspaces: 7, insert: "hồng" }`, engine composing
     `hồng`
   - continuing to delete gives `hông`, `hôn`, `hô` — still transforming
   - with `strict_spell_check` **off**, the same sequence returns `InStep` and
     nothing changes
   - a word still unspellable after the delete stays escaped and returns `InStep`
   - a never-escaped word is untouched (the 51-word corpus in
     `midword_spell_check.rs` must be unmoved)
4. Extend `backspace_step` in the property model to apply a `Repair` to the
   screen and assert the invariant afterwards.
5. **Prove the test has teeth**: make `Repair` under-report its backspaces by one
   and confirm the property suite fails, then revert. The suite has now twice
   passed over a real corruption because a path was modelled but never exercised;
   assume nothing.

## Success Criteria

- [x] The reported sequence produces `Repair` with the right edit
- [x] Deleting further keeps transforming
- [x] With the option off, behaviour is byte-identical to today
- [x] A still-unspellable word stays escaped
- [x] The property model applies `Repair` and holds
- [x] The deliberate mutation fails the suite
- [x] `cargo test --workspace` green, clippy silent

## Risk Assessment

- **The three-way return is misread at the call site.** `Repair` means "do not
  also pass the key through"; treating it like `InStep` would delete a character
  twice. *Signal:* the live check in Phase 2 shows one character too few.
  *Response:* the enum is what prevents this — a `bool` would not — and Phase 2's
  match must be exhaustive with no catch-all arm.
- **Lifting the escape re-transforms a word the user had accepted as literal.**
  If someone escaped deliberately and then edits earlier in the word, it may
  transform again. That is the open question in `plan.md`; the general rule is
  chosen because a special case ("only the key adjacent to the escape") is a rule
  nobody can predict either. *Signal:* it feels wrong in the live check.
  *Response:* narrow to "only when the removed key is the one that caused the
  escape", which needs the escape to remember its trigger index.
- **The 51-word corpus in `midword_spell_check.rs` shifts.** That corpus asserts
  identical output with the option on and off, and is the guard that this feature
  never touches ordinary typing. *Signal:* any diff in it. *Response:* the change
  is wrong — the exit must not alter what typing forward produces.

## Outcome — 2026-09-04

Done. `BackspaceOutcome` (Flush / InStep / Repair) replaces the `bool`, and
`Engine::can_unescape` re-asks the spell check's own question against the
shortened word.

The three-way return earned itself immediately: the caller has to decide what to
do with `Repair`, and the compiler now forces that decision at the one call site
where getting it wrong deletes a character of the user's text.

**A behaviour change worth naming.** Un-escaping fires at every backspace depth,
not only on the key that caused the escape — the general rule from the plan's
open question. It changed `the_escape_does_not_outlive_the_word`, which
backspaced three times over an escaped word and asserted the engine was empty.
With the escape lifting, the second delete lands on `â` — a single transformed
character — and correctly answers `Flush`, which the test never modelled. The
test now models the shell's three-case ladder. Its actual property, that the
escape does not leak into the next word, still holds and is still asserted.

Two of my own test expectations were wrong and the engine was right: `z` removes
a tone rather than adding one (`hồng` + `z` → `hông`), and after the repair the
ordinary mid-word rule resumes, so the next delete gives `hồn`, not `hông`.
