---
title: "GlowKey — deleting the mistake should undo the fallback"
description: "The mid-word spell check escapes a word to its raw keys and never un-escapes it, so deleting the offending key leaves `hoongf` on screen instead of restoring `hồng`. Re-evaluate the escape on Backspace and repair the word in one suppressed edit."
status: completed
priority: P1
effort: "0.5 days"
tags: [glowkey, engine, spell-check, bug]
created: 2026-09-04
---

# GlowKey — deleting the mistake should undo the fallback

## The report, and the mechanism behind it

> Type `hoongf`, get `hồng`. Mistype `a` and it becomes `hoongfa`. Delete the
> `a` and I want `hồng` back — but I get `hoongf`.

Reproduced against the owner's own settings (`strict_spell_check: true`, which
is what makes it happen; with the option off the same sequence already works):

```
h o o n g f   →  "hồng"
a             →  "hoongfa"   (bs=3 ins="oongfa")  ← the word escapes
⌫             →  "hoongf"                          ← wanted: "hồng"
```

**Cause.** `Engine::escaped` is a one-way latch. The mid-word spell check sets it
when a keystroke makes the render unspellable (`lib.rs:601-612`) and it is only
ever cleared when the word becomes *empty* (`lib.rs:678`, `:718`) or at a word
boundary via `reset` (`:543`). Deleting the key that caused the escape does not
re-evaluate anything, so the word stays verbatim for the rest of its life.

`backspace_visible_char` then does its job perfectly and makes it worse: while
escaped, `render_keys` returns the raw keys, so dropping `a` re-renders to
`hoongf`, which *is* the screen minus its last character. The engine reports
success, stays escaped, and the word is stuck.

The escape itself is right — the deliberate design from
`plans/260903-1531-unikey-telex-brackets-spellcheck/phase-02`, which escapes the
whole word rather than one key because the engine re-derives from the raw log
every keystroke. What is missing is the way back out.

## The behaviour to restore

Deleting the key that caused a fallback should undo the fallback. That is what
the user expects, and it is what makes the spell check safe to leave on: at the
moment the check fires, the user's next instinct is to hit Backspace, and today
that lands them somewhere worse than where they started.

Un-escaping is well-defined at every depth — this is the same word re-rendered
with the escape lifted:

| raw | un-escaped render | spellable? |
|---|---|---|
| `hoongfa` | `hồnga` | **no** — which is why the escape fired |
| `hoongf` | `hồng` | yes |
| `hoong` | `hông` | yes |
| `hoon` | `hôn` | yes |
| `hoo` | `hô` | yes |

So the rule falls out of the data: **on Backspace, lift the escape when the
shortened word is spellable again.** No new judgement, just the check that set it.

## Goals

| # | Goal | Priority |
|---|------|----------|
| 1 | Deleting the offending key restores the transformation | P1 |
| 2 | The repair lands as one suppressed edit, not a passthrough plus a race | P1 |
| 3 | The correction is covered by the property model, not only by examples | P1 |

## Phases

| # | Phase | Status | Depends on |
|---|-------|--------|------------|
| 1 | [Phase 1: Lift the escape in the engine](./phase-01-lift-the-escape.md) | Done | — |
| 2 | [Phase 2: Emit the repair from the tap](./phase-02-emit-the-repair.md) | Done | 1 |

Phase 1 is independently testable and changes no on-screen behaviour by itself;
Phase 2 is the delicate half, because it turns the Delete key from a passthrough
into a suppression.

## The design constraint that shapes Phase 2

The obvious implementation is wrong. Today the tap lets Backspace through and the
**host** performs the delete; to also repair the word we would post an edit
afterwards — which is exactly the mix of native passthrough and synthesized edit
that `docs/handoff.md` §5 records as the race the full-suppression model exists
to remove. A natively-typed keystroke and a synthesized backspace posted a moment
later reach the document out of order in multiprocess apps.

So when the engine says it can repair the word, the tap must **suppress** the
Backspace and emit the whole thing itself:

```
screen  "hoongfa"  (7 UTF-16 units)   →   edit { backspaces: 7, insert: "hồng" }
```

One edit, one ordered post, no race. The user's Backspace is accounted for inside
that edit rather than performed separately.

## Non-goals

| Out | Why |
|---|---|
| Re-deriving the escape on every keystroke instead of latching | The latch is right on the way *in*: once the user has been shown the raw keys, having the word silently re-transform because a later key happened to make it spellable again would be worse than the bug being fixed. Backspace is a deliberate undo; a forward keystroke is not. |
| Un-escaping the always-macro verbatim path (`process_key_verbatim`) | A different escape with a different cause — the user asked for verbatim there, and nothing has gone wrong to undo. |
| Making a restored auto-fix word re-composable at a boundary | Deliberately not re-composable (`docs/handoff.md` §4), and a separate decision from this one. |
| Changing when the escape *fires* | The check is correct; only the exit is missing. |

## Success Criteria

- [x] `hoongf` `a` ⌫ leaves `hồng` on screen with the engine composing it, with
      `strict_spell_check` on
- [x] The same sequence with the option **off** is unchanged
- [x] The repair is a single emitted edit; the Backspace is suppressed, not passed
      through alongside it
- [x] Deleting further (`hoong`, `hoon`, `hoo`) keeps transforming rather than
      re-escaping
- [x] A word that is *still* unspellable after the delete stays escaped
- [x] The repair is exercised by `crates/glowkey-engine/tests/properties.rs`, and
      re-introducing the bug fails the suite by name
- [x] `cargo test --workspace` green, clippy silent
- [x] `docs/handoff.md` §4's mid-word spell check entry states the exit

## Outcome — 2026-09-04

Fixed and installed. The reported sequence now works, and the word comes back
composing rather than as literal text:

```
type hoongfa  ->  "hoongfa"
Backspace     ->  suppress the key, emit bs=7 ins="hồng"   ->  "hồng"
then type s   ->  "hống"
```

172 tests (from 166), clippy silent, properties green at 60,000 cases.

The open question below was answered **the general way** — the repair fires at
any backspace depth, not only on the offending key — which is what changed
`the_escape_does_not_outlive_the_word`. Left for the live check to confirm it
feels right; `docs/manual-verification.md` §3 asks for it by name.

## Questioned in live use, and reaffirmed — 2026-09-04

Reported after the fix landed: `hoongf` `a` ⌫ ⌫ `z` gave `hồngz`, and `hông` was
expected. Two separate things were tangled in that report.

**`hồngz` was the old build.** The installed binary and the freshly built one
have identical hashes, but GlowKey had never run it — the log ends
`STARTUP quit at the Accessibility gate`, twice. With the fix running the same
sequence gives `hồng` → `hồn` → `hôn`; the stuck-literal `z` is gone.

**The remaining gap was a real design fork, and the owner chose to keep current
behaviour.** After the repair restores `hồng`, the next Backspace deletes one
**visible character** (`hồn`), not one keystroke (`hông`). The two diverge
exactly at a tone key: `hồng` is four characters and six keystrokes, and `f`
produced no character of its own. Visible-character deletion is the documented
contract (`docs/handoff.md` §4, pinned by `hoongf`⌫`z` → `hôn`), it is how
Backspace behaves in every other word, and the alternative would have meant
either a second Backspace mode that only exists after a repair, or reversing a
deliberate decision for every user. Asked and answered: keep it.

## Open questions

1. Should the repair also fire for a *mid*-word Backspace that is not adjacent to
   the offending key — deleting the `n` of `hoongfa`, say? Phase 1 answers it the
   general way (re-evaluate whatever remains), which costs nothing extra and
   avoids a special case, but it means a delete far from the mistake can also
   re-transform the word. Worth an explicit look during the live check.

<!-- slug: glowkey-unescape-on-backspace -->
