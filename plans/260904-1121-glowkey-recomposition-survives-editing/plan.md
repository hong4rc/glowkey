---
title: "GlowKey — deleting back to a word should reopen it"
description: "Re-composition only survives a Backspace landing immediately after the boundary; the first keystroke after a commit destroys it. Typing a word, deleting it, and deleting the space leaves you visually back at the previous word with no way to edit it."
status: completed
priority: P1
effort: "0.5 days"
tags: [glowkey, engine, recomposition, bug]
created: 2026-09-04
---

# GlowKey — deleting back to a word should reopen it

## The report, and what the log showed

> `hoongf s(del)(del)z` shows `hồngz`, should be `hông`.

Read off `~/Library/Logs/GlowKey/glowkey.log` rather than from the shorthand —
**there is a space between `hoongf` and `s`**, and that is the whole story:

```
'f'  Emit bs=3 ins="ồng"   raw="hoongf" rendered="hồng"
' '  Passthrough           raw=""       rendered=""      ← commits the word
's'  Emit bs=0 ins="s"     raw="s"      rendered="s"     ← a new word starts
⌫    Passthrough                                          ← deletes the s
⌫    Passthrough                                          ← deletes the space
'z'  Emit bs=0 ins="z"     raw="z"      rendered="z"     ← z is its own word
```

Screen: `hồngz`. The engine is behaving exactly as designed, and the design is
wrong for this.

**Cause.** `Session::last_committed` — the memory that makes `hồng`␣⌫`z` → `hông`
work — is destroyed by the *first keystroke after the boundary*
(`lib.rs:1251`, `process_key`), and again by a Backspace taken while composing
(`lib.rs:1814`). So re-composition only survives if the Backspace is
**immediate**. Type anything and delete it, and the memory is gone even though
the caret has returned to exactly where it was.

The user's phrase for this, from the first report of the day, was *"should
remember before text"*. Three reports — `hoongf a(del)(del)z`, `hoongf
s(del)(del)z`, and the original — have all been this one thing.

## What it should do

Deleting back to a boundary should reopen the word behind it, however you got
there. The caret is provably in the same place; the only reason it does not work
is that the engine threw the memory away in the meantime.

```
hoongf ␣ s ⌫ ⌫ z   →  hông      (today: hồngz)
hoongf ␣ ⌫ z       →  hông      (today: hông — must not regress)
```

## Goals

| # | Goal | Priority |
|---|------|----------|
| 1 | Deleting back to a word reopens it, across one or several intervening words | P1 |
| 2 | It still cannot fire when the caret may have moved behind the engine's back | P1 |
| 3 | The property model covers it, at the tap level, with a Backspace in the sequence | P1 |

## Design

**A bounded history, not one slot.** `Session::last_committed` holds exactly one
word. Extending *its* lifetime would fix the reported case and leave the next one
— delete back past two words and the second is gone. Instead keep a short stack
of recently committed words, capped at five.

```rust
/// A word that was committed and is still sitting behind the caret.
struct CommittedWord {
    /// The raw keystrokes, so the word can be re-opened for editing.
    raw: Vec<char>,
    /// What it renders to — the text that must be at the caret when it reopens.
    rendered: String,
}

/// Most recent last. Capped: five is past the point anyone deletes back to, and
/// the cap is really about bounding how far a wrong assumption could reach.
committed: VecDeque<CommittedWord>,
```

**Position is the stack order, not a stored number.** The document the engine
believes in is `[entry₁][entry₂]…[composing]`, and the caret is at the end of it.
Each entry accounts for exactly one boundary character on screen, plus the word
before it if there was one. So whatever the caret is standing behind is always the
top of the stack — no offsets to maintain, and no second source of truth that can
drift out of step with the first. The rules are four lines:

| Event | Effect |
|---|---|
| a word commits at a boundary | **push** `Behind::Word`; drop the oldest past five |
| a boundary commits nothing (a second boundary in a row) | **push** `Behind::Boundary` |
| Backspace arrives with nothing composing | **pop** the top: reopen a word, or just consume a boundary |
| the caret may have moved unseen | **clear the whole stack** |

The `Behind::Boundary` entry was added on 2026-09-04 after review — see the
amendment at the end. The original design had no way to represent a boundary with
no word before it and threw the stack away instead, which left the reported bug
reachable one comma later.

Deleting back through several words falls out of that without a special case:
the composing word empties, the next Backspace pops and reopens the word before
it, that one empties in turn, and so on until the stack runs out — at which point
the engine flushes and Backspace is an ordinary delete again.

**Invalidation is where the safety lives.** Twelve of the fourteen sites that
currently forget the committed word keep doing so, and now clear the whole stack:
flush, caret keys, mouse-down, app switch, mode and exclusion toggles, input
method, placement style, and the three render-shaping options. Every one is a
case where the engine cannot know where the caret is, and reopening a word on a
guess is how a blind editor corrupts a document.

**The two memories stop sharing a lifetime.** A single `forget_last_word()`
currently clears both the committed word and `correctable` (the ⌃⇧W hotkey) at
all fourteen sites. They need different rules, and conflating them is why this
bug is awkward to fix without breaking the other:

| | `correctable` | the committed stack |
|---|---|---|
| Purpose | fix the word just typed | reopen a word behind the caret |
| Lifetime | one-shot: any key ends it | survives keys that are later deleted |
| Cleared by | everything | only what can move the caret unseen |

## Non-goals

| Out | Why |
|---|---|
| Changing what re-composition *does* once it fires | `hồng`␣⌫`z` → `hông` is right and documented; only its lifetime is wrong. |
| Keystroke-undo for Backspace | Asked twice, declined twice (`docs/handoff.md` §4). Unrelated: this is about which word is being edited, not how much one Backspace removes. |
| Remembering across a flush | A flush means the engine lost track of the caret. Reopening a word on a guess is how you corrupt a document. |
| An unbounded history | Five is well past what anyone deletes back through, and the cap is the thing that bounds how far a wrong assumption can reach. Unbounded would also mean the stack outliving any reasonable claim that the caret is still where the engine thinks. |
| Explicit caret offsets per entry | The stack order *is* the position. Storing a number alongside it would be a second source of truth, and the two would drift. |

## Phases

| # | Phase | Status | Depends on |
|---|-------|--------|------------|
| 1 | [Phase 1: The committed-word history](./phase-01-committed-word-history.md) | Done | — |
| 2 | [Phase 2: Cover it where it actually runs](./phase-02-cover-it-at-the-tap.md) | Done | 1 |
| 3 | Review findings, in the amendment below | Done | 2 |

Phase 2 is not optional polish. Every one of the three reports was checked
against a hand-written model in a scratch binary and pronounced fine; the tap
test that finally reproduced it was written today, after the third report. The
model kept agreeing with the assumption instead of the app.

## Conflict with the typing-accuracy plan

`plans/260903-1637-unikey-phonotactics-and-restore/` Phase 2 rewrites the restore
decision inside `Session::commit`, which is also where `last_committed` is set.
Different lines, same function — **do not run them concurrently.** Neither blocks
the other.

## Success Criteria

- [x] `hoongf` ␣ `s` ⌫ ⌫ `z` gives `hông`
- [x] `hoongf` ␣ ⌫ `z` still gives `hông` — the immediate case does not regress
- [x] Deleting back through two and three intervening words reopens the right one
- [x] Past the cap, Backspace is an ordinary delete rather than reopening a word
      the engine can no longer vouch for
- [x] After a click, an arrow key or an app switch the key does nothing
- [x] ⌃⇧W stays one-shot — its memory is unchanged by this
- [x] The tap-level property model drives Backspace and holds
- [x] `cargo test --workspace` green, clippy silent, properties green at 60,000 cases
- [x] `docs/handoff.md` §4 states the new lifetime
- [x] A second boundary in a row does not break the chain (`hoongf, ` ⌫⌫ `z`)
- [x] The cap test actually fails at four and at six

## Open questions

1. **Answered by the owner during planning:** how far back should it reach?
   A bounded history of the most recent few words, restoring whichever one the
   caret has returned to — not a single slot. Capped at five. The remaining
   detail is whether five is right; it is one constant and the tests will say if
   it ever matters.

## Amendment (2026-09-04): what the review found after "done"

The work above shipped and passed. An adversarial review of it then ran 180,000
randomised sequences across nine configurations without desynchronising the stack
from the document — and still found four things. Three were small. One was the
original bug, still reachable.

### The reported bug was one comma away

A boundary that commits nothing — the space in `hồng, ` — used to clear the whole
history, because the design had no entry that could represent a boundary with no
word before it. So:

| | |
|---|---|
| `hoongf ␣ s ⌫⌫ z` | `hông` ✓ |
| `hoongf , ␣ ⌫⌫ z` | **`hồngz`** ✗ |
| `hoongf . ␣ ⌫⌫ z` | **`hồngz`** ✗ |

`, ` and `. ` are the two commonest pairs in prose, so this is not a corner. It
was an incompleteness rather than a defect: everything the model *could* express
was correct, and the model could not express this.

**Fix:** a stack entry is now `Behind::Word` **or** `Behind::Boundary` — one
boundary character, plus the word before it if there was one. The reviewer
proposed a single trailing-boundary counter instead, which is smaller and does
fix the reported case; it was rejected because a count kept only for the tail
loses those boundaries as soon as another word commits in front of them:

```
hoongf, man ⌫⌫⌫⌫⌫⌫z   →  hông
```

By the time you delete back to them, the two boundaries behind `hồng` are no
longer the trailing ones. A tail-only count would have reopened `hồng` with the
comma still sitting at the caret — the exact corruption the whole feature is
built to avoid. Per-entry costs nothing and makes depth ordinary rather than
special. Pinned by `deleting_back_through_a_bare_boundary_reopens_the_word_before_it`.

This does not reopen the "no offsets per entry" non-goal. A `Behind::Boundary` is
not an offset; it is one of the two things that can be behind the caret, and it
occupies real space on screen exactly as a word does.

### The cap test did not test the cap

`beyond_the_history_cap_nothing_reopens` deleted the whole document away and
asserted the screen was empty, which is true for any cap of two or more. Replaced
by `the_history_cap_is_five_entries`, which puts a re-composable word on each side
of the boundary. Verified by mutation: at four the within-cap half fails, at six
the past-cap half fails, and only five passes both.

### `Flush` safety had moved to the caller

`recompose_after_boundary_backspace` returned `false` on an empty stack and
cleared nothing, leaving the caller to flush. After `work `⌫ that left ⌃⇧W holding
an edit that would put back a boundary character the Backspace had just removed —
over-deleting by one. The engine now calls `forget_position()` itself on that
path. Also mutation-checked.

### Three doc comments

The header orphaned above `COMMITTED_HISTORY`, the `committed` field still
described as a one-shot slot "consumed by the very next event" (the behaviour this
change *removed*), the stale `last_committed` name in four places, and a comment
asserting "all nine sites" where there are thirteen — replaced with a claim that
does not rot.

### Shape of the answer

`recompose_after_boundary_backspace` returns `BoundaryBackspace` — `Reopened`,
`BoundaryRemoved`, `NotApplicable` — instead of a `bool`. Collapsing the first two
into "true" is what hid the bug: `BoundaryRemoved` was indistinguishable from
"nothing remembered", which the caller answers by flushing. The same reasoning
`BackspaceOutcome` already carries, for the same reason.

187 tests, clippy silent.

<!-- slug: glowkey-recomposition-survives-editing -->
