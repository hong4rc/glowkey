---
phase: 1
title: "The committed-word history"
status: completed
priority: P1
effort: "4h"
dependencies: []
---

# Phase 1: The committed-word history

## Overview

Replace the single `last_committed` slot with a capped stack of recently
committed words, so deleting back through one or several words reopens the one
the caret has actually returned to.

## Requirements

- Functional: committing a word pushes it; the stack holds at most five.
- Functional: a Backspace arriving with nothing composing pops the top entry and
  reopens it for editing, exactly as `recompose_after_boundary_backspace` does
  today for the single slot.
- Functional: deleting back through several words reopens each in turn.
- Functional: past the cap — or with an empty stack — Backspace is an ordinary
  delete and the engine flushes, as now.
- Functional: `correctable` (⌃⇧W) stays one-shot and is unaffected.
- Non-functional: every event that can move the caret unseen clears the **whole**
  stack. This is the safety property; the feature is worthless without it.
- Non-functional: the blind model's invariant holds after a reopen — the engine's
  render must equal the text at the caret, or the next keystroke eats real text.

## Architecture

**The type.** A small struct in place of the current
`Option<(Vec<char>, String)>` tuple, because with a stack the fields need names:

```rust
struct CommittedWord {
    raw: Vec<char>,      // re-opened for editing
    rendered: String,    // must equal the text at the caret when it reopens
}

committed: VecDeque<CommittedWord>,   // most recent last, capped at COMMITTED_HISTORY
const COMMITTED_HISTORY: usize = 5;
```

**Push, pop, clear — and nothing else.** `commit` pushes and trims to the cap
where it currently assigns `last_committed`, under the same condition (a word
that was *not* restored by auto-fix; a restored word is deliberately not
re-composable). `recompose_after_boundary_backspace` pops instead of `take`s.
Everything else that touches the memory today clears the stack.

**Position is the stack order.** The engine's picture of the document is
`[word₁][b₁]…[composing]` with the caret at the end, so the word behind the caret
is the top of the stack. Storing an offset per entry would be a second source of
truth able to disagree with the first; there is nothing for it to add.

**Splitting the two lifetimes.** `forget_last_word()` currently clears both the
committed word and `correctable` at fourteen sites. It becomes two:

```rust
/// The caret may be somewhere this engine cannot see. Nothing behind it is
/// trustworthy, so drop the whole history and the correction memory with it.
fn forget_position(&mut self)

/// A new word has started. The words behind the caret have not moved; only the
/// one-shot correction memory ends.
fn start_new_word(&mut self)
```

Twelve sites take `forget_position`; two take `start_new_word` — `process_key`
and `recompose_after_boundary_backspace`'s composing branch. `forget_position`
is named for *why* rather than *what*, because that is the test for whether a
future call site belongs on the list.

## Related Code Files

- Modify: `crates/glowkey-engine/src/lib.rs` — the type, the deque, the three
  operations, the lifetime split, and the fourteen call sites.
- Modify: `crates/glowkey-engine/tests/session.rs` — re-composition's home.
- Modify: `app/src/tap/tests.rs` — the reported sequence, end to end.

## Implementation Steps

1. Move all fourteen sites to `forget_position` as a pure rename, still clearing
   both memories. Tree stays behaviour-identical and green — a reviewable no-op.
2. Introduce `CommittedWord` and the deque, still capped at one entry. Behaviour
   unchanged; this isolates the data-structure change from the behaviour change.
3. Raise the cap to five and switch the two named sites to `start_new_word`.
   **This is the two-line behaviour change**, and steps 1 and 2 exist so it can
   be read as such.
4. Tests — the point of the phase, and each one names the sequence it pins:

   | Test | Sequence | Expect |
   |---|---|---|
   | the reported bug | `hoongf` ␣ `s` ⌫⌫ `z` | `hông` |
   | immediate case unchanged | `hoongf` ␣ ⌫ `z` | `hông` |
   | a longer intervening word | `hoongf` ␣ `abc` ⌫⌫⌫⌫ `z` | `hông` |
   | two words back | `hoongf` ␣ `vieet` ␣ `s` ⌫⌫⌫⌫⌫⌫⌫ `z` | reopens `hồng` |
   | at the cap | six commits, then delete back through all six | the sixth is gone; Backspace is an ordinary delete |
   | a click intervenes | commit, `flush()`, ⌫ `z` | `z` is literal — nothing reopens |
   | an app switch intervenes | commit, `set_frontmost_app(other)`, ⌫ `z` | nothing reopens |
   | ⌃⇧W is untouched | commit, type a second word, ⌃⇧W | corrects nothing |
   | restored words stay out | `work` ␣ (auto-fix restored) ⌫ `z` | nothing reopens, as today |

5. Run the existing suites unchanged. `tests/session.rs` owns re-composition and
   `tests/telex.rs` owns the mid-word Backspace contract; if either moves, the
   change is wrong.

## Success Criteria

- [x] Every row of the table above passes
- [x] No existing test needed editing
- [x] `cargo test --workspace` green, clippy silent
- [x] The stack never exceeds five entries (asserted, not assumed)
- [x] `correctable` behaviour is byte-identical to before

## Risk Assessment

- **A stale reopen rewrites text the engine does not own.** The memory now
  outlives keystrokes, so the failure is reopening a word the caret is no longer
  beside — and a reopen installs a render that the next keystroke is diffed
  against, so the damage is immediate and silent. *Signal:* Phase 2's model finds
  the screen no longer ending with the reopened word. *Response:* the twelve
  `forget_position` sites are the whole defence; a miss there is the bug, not the
  history.
- **The split is applied to a wrong site.** Twelve of fourteen must not change
  and a mistake is silent. *Response:* step 1 makes them all identical first, so
  step 3's diff is two lines.
- **The cap hides a real case.** If someone routinely deletes back through six
  words, five is wrong. *Signal:* a report that it "sometimes" works. *Response:*
  it is one constant; raise it. It is capped for safety, not for memory.
- **`correctable` leaks into the longer lifetime**, letting ⌃⇧W correct a word
  two words back. Pinned by a test rather than by reading the code.

## Outcome — 2026-09-04

Done, staged as planned: a pure rename to `forget_position` (18 sites, tests
unmoved), then the deque still capped at one, then the cap to five and the two
call sites to `start_new_word`. The behaviour change really is two lines.

**One design correction the plan did not have.** A word auto-fix restored must
**clear the stack**, not merely stay out of it. It still occupies space on
screen, so leaving it unrepresented would break the invariant the whole design
rests on — that the entries are an unbroken run of words immediately behind the
caret. Without it, `hồng`␣`work`␣ (where `work` was restored) would leave `hồng`
on top of the stack with `work ` between it and the caret, and deleting back
would re-open a word five characters from where the engine thought it was.
Pinned by `a_restored_word_breaks_the_chain`.

**A limit found while testing, kept deliberately.** Deleting back *through* a
word the engine cannot track character-by-character ends the chain. `viêt` → `vi`
has no single raw-key removal (`viee` minus an `e` renders `viê`), so
`backspace_visible_char` flushes — and after a flush the engine no longer knows
how much of that word is on screen, so it cannot find the boundary either.
Clearing there is correct, and `losing_track_mid_word_ends_the_chain` pins it so
it is a known limit rather than a surprise.
