---
phase: 8
title: "Lift the mid-word escape latch on forward keys"
status: pending
priority: P2
effort: "4-6h incl. a sweep"
dependencies: []
---

# Phase 8: Lift the mid-word escape latch on forward keys

Filed 2026-09-06 from the code review of `260903-1637` phase 1. **This reverses
a documented decision, so it is its own phase rather than a fix smuggled into
that one.**

## Overview

The mid-word spell check's escape is a latch: once a keystroke makes the render
unspellable, the word renders verbatim until the next word boundary. Typing
onward can never repair it — only Backspace can, via `can_unescape`.

That makes a word unreachable when an *intermediate* render is impossible but
the finished one is fine.

## The measured case

```
typed("tuanwa", strict = true)  -> "tuanwa"      // want "tuân"
typed("tuanwa", strict = false) -> "tuân"
```

`t u a n` renders `tuan`; `w` puts the horn on the `u`, giving `tưan`; `ưa`
closed by a coda is impossible, so `violates_open_diphthong_coda` refuses it and
`engine.rs` latches `escaped = true`; the final `a` — which would have produced
`tuân` — never applies.

Same shape, all measured: `xuanwa`→`xuân`, `tuanwaf`→`tuần`, `chwanar`→`chuẩn`,
`luanwaj`→`luận`, `huanwas`→`huấn`, `nhwanaj`→`nhuận`, `thwanaf`→`thuần`.

**What bounds it today**, and why this is P2 rather than P0:

- The strict mid-word check is **off by default** (`off_by_default` pins it).
- The ordinary spelling is unaffected — `tuaan`, `xuaan`, `chuaanr`, `luaanj`,
  `thuaanf` all give the right word with the check on. Only the order that puts
  the horn in before the vowel cluster finishes is hit.
- Auto-fix at the word boundary is unaffected: the committed render `tuân` is
  valid, so nothing is restored.
- The latch **predates** the open-diphthong rule. `vi` called `tưan` valid, so
  the rule widened the set of words reaching an existing behaviour rather than
  creating it. A sweep found the `uâ` family to be the only class the new rule
  adds; the `nh`/`ch` rule adds none, because no Telex modifier can turn a back
  vowel into a front one.

Pinned as known behaviour by
`the_escape_latch_still_swallows_a_horn_first_ua_word` in
`crates/glowkey-engine/tests/midword_spell_check.rs`, which this phase must
rewrite when it changes the behaviour.

## The decision being reversed

`engine.rs` `process_key` states the latch as intentional:

> Refuse the transformation for the rest of the word: the raw keys come back and
> stay literal until the next boundary. […] Escaping the whole word rather than
> the single key is deliberate: the engine re-derives everything from the raw log
> on every keystroke, so a key merely dropped here would be re-applied by the
> next one.

The second half is about *scope* (whole word, not one key) and stays true. What
this phase revisits is the *duration* — "until the next boundary" — which the
`uâ` case shows to be stricter than the promise the check actually makes ("show
you what you typed when the result is impossible"): `tuân` is not impossible.

## Requirements

- Functional: a word whose render becomes spellable again while typing forward
  transforms again, without a Backspace.
- Functional: a word that is *still* unspellable stays escaped — the exit must
  not admit anything the entry would refuse.
- Non-functional: no extra work on the keystroke path beyond the one render the
  escape check already performs. This runs on every key in strict mode.
- Non-functional: the whole-word scope of the escape is unchanged; only its
  lifetime changes.

## Architecture

`can_unescape` (`engine.rs`) already asks exactly the right question — it renders
the raw log and returns whether the result is valid — and is currently reached
only from the Backspace path. The change is to consult it on the forward path
too, so `escaped` becomes derived state rather than a latch.

The cheap shape: in `process_key`, when `self.escaped` is already set, re-ask
`can_unescape()` and clear the flag when it answers true, before `rerender()`.
Entry and exit then remain the same question, which is the property
`can_unescape`'s own doc comment says the design depends on.

Watch for: `render` is already called once by the escape check and once by
`rerender`; wiring this in naively adds a third call per keystroke. Reuse the
render rather than recomputing it.

## Related Code Files

- Modify: `crates/glowkey-engine/src/engine.rs` — `process_key`, and the doc
  comment stating the latch lasts until the next boundary
- Modify: `crates/glowkey-engine/tests/midword_spell_check.rs` — rewrite
  `the_escape_latch_still_swallows_a_horn_first_ua_word`, which pins the current
  behaviour and must invert
- Read only: `crates/glowkey-session/src/session.rs` — the boundary path, to
  confirm it is genuinely unaffected

## Implementation Steps

1. Write the failing test first: `typed("tuanwa", true)` must give `tuân`, and
   the seven sibling words with it.
2. Consult `can_unescape` on the forward path, reusing the existing render.
3. Re-assert the tests that pin the escape *staying* on when it should:
   `a_still_unspellable_word_stays_escaped`,
   `a_rejection_that_is_still_unspellable_shows_the_raw_keys`,
   `the_escape_does_not_outlive_the_word`,
   `a_restored_escaped_word_can_still_unescape`. These are the guard against
   over-correcting, and none may be weakened to make step 1 pass.
4. Sweep for behaviour changes rather than reasoning about them: enumerate Telex
   key sequences, render each with the check on, and diff against the current
   build. Every difference must be a word that became *more* correct.
5. Rewrite the pinning test from this phase's parent so it asserts the new
   behaviour, and update the note in
   `plans/260903-1637-unikey-phonotactics-and-restore/phase-01-more-phonotactic-rules.md`.
6. `cargo test --workspace`, `cargo clippy --workspace --all-targets`.

## Success Criteria

- [ ] `tuanwa` and its seven siblings transform with the strict check on
- [ ] Every existing escape test still passes, unweakened
- [ ] The sweep in step 4 shows no word made worse
- [ ] No added render call on the keystroke path
- [ ] The parent phase's pinning test and note are updated, not left contradicting

## Risk Assessment

| Risk | Signal it broke | Response |
|---|---|---|
| Over-correcting: the escape lifts for a word that is still unspellable | `a_still_unspellable_word_stays_escaped` or `a_rejection_that_is_still_unspellable_shows_the_raw_keys` fails | Entry and exit must stay the *same* question via `can_unescape`. Never weaken those tests to pass step 1 — they are the reason this is safe |
| A third `render` per keystroke on the strict path | `tests/latency.rs` regresses | Reuse the render the escape check already computed. Latency is why the whole-word escape was chosen originally |
| The flag oscillates within a word, producing visible flicker | A word flips between raw and transformed as the user types | That is what derived state means, and it matches what happens with the check off. If it reads badly on screen that is a product decision to take back to the user, not a bug to patch |
| The reversal turns out to be wrong under real use | A user reports a word they cannot type that used to work | The decision text above records what was reversed and why, so it can be reverted knowingly rather than re-litigated |
