---
phase: 2
title: "ASCII-render restore"
status: pending
priority: P1
effort: "1-2d"
dependencies: []
---

# Phase 2: ASCII-render restore

## Overview

`is_invalid_vietnamese` opens with:

```rust
// A pure-ASCII word is what the user typed verbatim — leave it alone.
if word.is_ascii() {
    return false;
}
```

That comment is wrong, and the assumption behind it is the largest remaining
defect in everyday typing. A Telex render can be pure ASCII and still be nothing
like what the user typed, because a modifier key is *consumed* rather than
inserted: `aardvark` renders `advark`, `academial` renders `acdemial`. Auto-fix
never even looks.

**6,512 dictionary words** render as ASCII-but-different, and they are the bulk
of the 8,362 that survive auto-fix today.

## Requirements

- Functional: a word whose render is ASCII and differs from the typed keys is
  restored to those keys.
- Functional: the repeat-key escape hatch is untouched — `cass` must stay `cas`,
  never become `cass`.
- Functional: deliberate tone removal is untouched — `z` exists to strip a mark.
- Non-functional: zero regressions on the real-Vietnamese corpus.

## Architecture

The naive rule — "ASCII render ≠ raw keys, therefore restore" — is wrong, and
the counter-example is already in the test suite:

| typed | renders | restoring would give | correct? |
|---|---|---|---|
| `aardvark` | `advark` | `aardvark` | **yes** |
| `academial` | `acdemial` | `academial` | **yes** |
| `cass` | `cas` | `cass` | **no** — the user rejected the mark on purpose |
| `hoongff` | `hôngf` | — | non-ASCII, not this path |

So the phase is really about the carve-out list, not the rule. The repeat-key
case is already detectable — `last_key_made_it_impossible` uses exactly that
test — so the first candidate is "restore unless the last two keys repeat".

UniKey reaches the same place from the other side: it does not re-validate a
string at all. It tracks a **form** per word (`vnw_nonVn`, `vnw_cv`, `vnw_cvc` …)
as the word is built, and `lastWordIsNonVn` (`ukengine.cpp:2322`) answers from
that form. A word that never parsed as Vietnamese is non-Vietnamese by
construction, whatever its characters look like. Adopting the *idea* — decide
from how the word was built, not from how it ended up spelled — is the durable
fix; the carve-out list is the cheap approximation of it.

## Related Code Files

- Modify: `crates/glowkey-engine/src/lib.rs` — `is_invalid_vietnamese`, and the
  `commit` path that consumes it
- Modify: `crates/glowkey-engine/tests/auto_fix.rs`
- Create: a dictionary-sweep test, marked `#[ignore]` so it does not slow the
  normal run

## Implementation Steps

1. **Measure the carve-outs before writing the rule.** Sweep the dictionary and
   classify every ASCII-different render by *why* it differs: a consumed
   modifier (`aa`→`â` then stripped), a repeat-key rejection, a `z` removal,
   something else. The proportions decide the design.
2. Enumerate the carve-outs from step 1 as explicit predicates, each with a
   named example. No catch-alls.
3. Implement, keeping the ASCII short-circuit for the case it was right about:
   a render **identical** to the raw keys is genuinely untouched text.
4. Re-run the full dictionary sweep and the real-Vietnamese corpus. Record the
   before/after residue.
5. If the carve-out list cannot be closed — if step 1 finds a class that is
   indistinguishable from a deliberate rejection — stop and report. A wrong
   restore corrupts text the user typed correctly, which is worse than the
   status quo of leaving it mangled.

## Success Criteria

- [ ] `aardvark`, `academial`, `acalephan` and their class are restored
- [ ] `cass`→`cas`, `aaa`→`aa`, `ddd`→`dd` unchanged
- [ ] `az` unchanged (raw and render already match, so the guard excludes it)
- [ ] Real-Vietnamese corpus: zero changes
- [ ] The 8,362 residue is re-measured and the number recorded, not estimated
- [ ] Every carve-out has a test naming the word it protects

## Risk Assessment

**This is the highest-risk change in the plan** — it makes auto-fix act on a
class of words it has never touched, and auto-fix rewrites text at a boundary.
*Signal:* any real-Vietnamese corpus word changes, or a carve-out example
regresses.
*Response:* narrow the rule to the single largest measured class rather than the
general case, or stop (step 5).

**The `is_ascii` short-circuit also guards the English path.** With
`restore_english_words` off, an ordinary untransformed English word never
reaches the predicate at all. Removing the short-circuit wholesale would put
every English word through it.
*Signal:* plain words like `the`, `code`, `print` start being rewritten.
*Response:* keep the guard for render-equals-raw, which is what actually
identifies untouched text; that is why step 3 splits it rather than deleting it.

**Interaction with the mid-word spell check.** Both features share this
predicate, so any rule here also starts refusing keystrokes mid-word.
*Signal:* the strict-check corpus test fails.
*Response:* gate the new rule to the boundary path if mid-word proves too eager.
