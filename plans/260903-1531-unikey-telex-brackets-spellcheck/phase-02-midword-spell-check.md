---
phase: 2
title: "Mid-word spell check"
status: pending
priority: P2
effort: "1d"
dependencies: []
---

# Phase 2: Mid-word spell check

## Overview

UniKey carries two independent spell-check options, not one:
`spellCheckEnabled` gates the check itself (`ukengine.cpp:2280`), and
`autoNonVnRestore` additionally restores the raw keys when the finished word is
not Vietnamese (`ukengine.cpp:2292`). GlowKey's single "Auto-fix" checkbox is
the second only. The first — refusing a diacritic that cannot occur in
Vietnamese *at the moment it is typed* — GlowKey does not do at all.

This is the most behaviourally interesting gap found in the source, and the
riskiest to build: it runs on every transforming keystroke, in the same path
that produced today's `đddc` and `ưwork` defects.

## Requirements

- Functional: with the option on, a keystroke whose transformation would produce
  an impossible Vietnamese form is refused — the diacritic is not applied and the
  key types itself instead.
- Functional: **zero false rejections** across a corpus of real Vietnamese words
  typed key by key. A single wrongly-refused legitimate word makes the option
  worse than useless.
- Functional: independent of auto-fix. Either, both, or neither may be on.
- Non-functional: the check is a string test on an already-computed render; no
  extra allocation per keystroke beyond what `rerender` already does.

## Architecture

The check runs on the **render**, never on the raw keys. That distinction is the
whole design, and a probe already showed why: the raw prefix `nguow` fails
`vi::validation::is_valid_syllable`, while what it renders to — `ngươ`, an
ordinary intermediate state of typing `người` — passes.

Probe results on rendered intermediate states, all `is_valid_syllable == true`:
`ng`, `ngh`, `ngu`, `nguo`, `ngươ`, `hoa`, `hoan`, `th`, `kh`. The validator is
lenient about incomplete syllables, which is what makes prefix checking viable
at all.

Rule, to be confirmed by step 1: after a transforming keystroke, if the new
render is invalid Vietnamese, discard the transformation, keep the previous
render, and append the key literally. `is_invalid_vietnamese` already exists and
already skips pure-ASCII words, so it is the natural predicate.

## Related Code Files

- Modify: `crates/glowkey-engine/src/lib.rs` — the check inside the key path;
  `Engine`/`Session` flag
- Modify: `crates/glowkey-engine/src/config.rs` — `strict_spell_check: bool`
- Modify: `app/src/tap.rs`, `app/src/prefs_window.rs` — accessors and checkbox
- Create: `crates/glowkey-engine/tests/midword_spell_check.rs`
- Create: a word corpus for the false-rejection test

## Implementation Steps

1. **Spike first, and be willing to stop.** Build the corpus (a few hundred real
   Vietnamese words in Telex and VNI), drive each key by key through today's
   engine, and record every intermediate render that fails
   `is_valid_syllable`. This measures the false-rejection rate *before* any
   behaviour changes. Also settle here whether the rule fires on every
   transforming key or only on tone keys.
2. If the rate is not zero, stop and report. Either the predicate needs
   narrowing (tone keys only) or the phase is not buildable on this validator —
   both are acceptable outcomes, shipping false rejections is not.
3. Add `strict_spell_check` to `Settings` and both `Settings` literals.
4. Implement the refusal in the engine behind the flag.
5. Wire the accessors and a Settings checkbox, captioned to distinguish it from
   auto-fix: this one acts *while typing*, auto-fix acts *at the space*.
6. Run the corpus as a test, plus the existing suite, plus a live pass.

## Success Criteria

- [x] Corpus of real Vietnamese words: zero false rejections with the option on
- [x] An impossible diacritic is refused and the key types itself
- [x] Option off: engine output byte-identical to today
- [x] Auto-fix and this option are independent in both directions
- [x] The existing 97 tests still pass; clippy silent
- [x] `docs/handoff.md` states plainly which option acts when

## Risk Assessment

**False rejections are the whole risk.** A wrongly refused keystroke corrupts a
word the user typed correctly, which is far worse than the sloppy-but-recoverable
behaviour it replaces. Step 1 exists to measure this before committing, and step
2 is an explicit licence to abandon the phase.
*Signal it broke:* any corpus word whose typed-out result differs with the option
on.
*Response:* narrow the predicate to tone keys only; if that still rejects, drop
the phase and record why.

**It runs in the hot path.** The same per-keystroke path produced `đddc` and
`ưwork`. Mitigation: the check only ever *discards* a transformation and falls
back to a literal append — it never computes a new edit shape, so the UTF-16
backspace arithmetic is untouched.

**Interaction with the repeat-key escape hatch.** Pressing a tone key twice
already rejects the mark (`cass`→`cas`). The new refusal must not fire on that
path and turn a deliberate rejection into a double one.
*Signal:* `cass`, `aaa`, `ddd`, `hoongff` change behaviour with the option on.
*Response:* exclude the repeat-key case explicitly; it is already tested by
`repeating_the_diacritic_key_rejects_it`.
