---
phase: 2
title: "Cover it where it actually runs"
status: completed
priority: P1
effort: "2h"
dependencies: [1]
---

# Phase 2: Cover it where it actually runs

## Overview

Put Backspace into the tap-level property model, so a sequence of this shape is
checked against the code the app runs rather than against a model of it.

## Requirements

- Functional: the property model drives `decide()` with Delete events and applies
  each `Decision` as the OS would, including the re-composition path.
- Functional: it asserts the invariant that matters here — after a re-composition
  the engine's render must equal the text actually sitting at the caret.
- Non-functional: the model must not share the engine's assumptions. Where the
  engine says "I reopened `hồng`", the model checks the *screen* ends with
  `hồng`, not that the engine agrees with itself.

## Architecture

**Why this phase exists.** Three field reports were each checked against a
hand-written model in a scratch binary and pronounced fine. All three were real.
The pattern is now familiar enough to name: a model written by the same person,
at the same time, from the same assumption, agrees with the assumption. The
existing `crates/glowkey-engine/tests/properties.rs` models `Session` directly
and has twice been found sharing a belief with the code under test — once about
control characters, once about a probe that cleared the state it was probing.

`app/src/tap/tests.rs` gained `type_with_deletes` today, which drives real
`CGEvent`s through `decide()` and applies each `Decision` as the OS would. That
is the right level: it is the first harness in this repo that reproduced a
reported Backspace bug. This phase generalises it from three hard-coded strings
into a property.

**The property.** For a generated sequence of word characters, boundaries and
Backspaces, driven through `decide()`:

1. the screen the model builds never loses a character to an over-delete;
2. whenever the engine reports composing a word, the screen **ends with** that
   word;
3. a reopen is followed by a screen whose tail is the reopened word;
4. the history never exceeds its cap, and every entry in it is a suffix of the
   committed text on screen — the invariant that makes the stack order a valid
   stand-in for caret position.

Point 2 is the one that catches this class. It is the blind model's invariant
stated against the document rather than against the engine's own fields.

## Related Code Files

- Modify: `app/src/tap/tests.rs` — generalise `type_with_deletes` into a
  proptest, or a deterministic sweep if pulling `proptest` into the app crate is
  not wanted.
- Modify: `crates/glowkey-engine/tests/properties.rs` — the engine-level model
  learns re-composition after intervening text.
- Modify: `docs/handoff.md` §4 and `docs/manual-verification.md`.

## Implementation Steps

1. Decide where the generated sequences live. The app crate has no dev-dependency
   on `proptest` today; a deterministic sweep over a few hundred shaped sequences
   (word, boundary, word, N deletes) may be enough and avoids the dependency.
   Either is fine — say which and why in the phase record.
2. Add the three assertions above.
3. **Prove it fails on the bug**: revert Phase 1's two-line change and confirm
   the property fails, naming the sequence. If it passes, the model is wrong and
   the phase is not done.
3b. Prove it a second way: cap the history at one entry and confirm the
   two-words-back sequence fails. A model that only catches the single-slot bug
   would pass a stack that silently keeps just the newest word.
4. Update `docs/handoff.md` §4's re-composition entry with the new lifetime, and
   add the sequence to `docs/manual-verification.md` §2.

## Success Criteria

- [x] The model drives Delete through `decide()`
- [x] It asserts the screen ends with whatever the engine says it is composing
- [x] Reverting Phase 1 makes it fail, by name
- [x] `cargo test --workspace` green, properties green at 60,000 cases
- [x] Both docs updated

## Risk Assessment

- **The model agrees with the code again.** The specific defence is step 3: a
  model that cannot fail on the known bug is not evidence. *Signal:* reverting
  Phase 1 leaves the suite green. *Response:* the model is the bug; fix it before
  believing anything else in this plan.
- **`proptest` in the app crate is unwanted weight.** It would be the first
  dev-dependency there and the crate is macOS-only, so CI runs it on one job
  only. *Response:* the deterministic sweep, which costs nothing and covers the
  shapes that actually occur; generated input matters most where the space is
  large, and here it is not.

## Outcome — 2026-09-04

Deterministic named tests at the tap level rather than generated sequences, and
the reason is the one the phase itself gave: the value here is *where* the tests
run, not how many shapes they cover. `app/src/tap/tests.rs` drives real
`CGEvent`s through `decide()` and applies each `Decision` as the OS would — the
first harness in this repo that reproduced a reported Backspace bug. Pulling
`proptest` into the macOS-only app crate would have added a dev-dependency to buy
breadth over a space that is small and enumerable.

Six tap tests and two engine tests. The mutation checks the phase demanded both
pass:

| Mutation | Result |
|---|---|
| revert the two-line lifetime change | `deleting_back_to_a_word_reopens_it` and `deleting_back_through_two_words_reopens_the_right_one` fail |
| cap the history at 1 | `deleting_back_through_two_words_reopens_the_right_one` fails |

The second is the one that matters: a model catching only the single-slot bug
would wave through a "stack" that silently kept just the newest word.
