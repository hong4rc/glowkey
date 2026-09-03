---
phase: 1
title: "More phonotactic rules"
status: pending
priority: P2
effort: "3h"
dependencies: []
---

# Phase 1: More phonotactic rules

## Overview

`vi::validation::is_valid_syllable` is lenient, and the stop-coda tone rule was
not the only thing it misses. Two more families are confirmed, by the same
method: state the rule, probe `vi`, watch it accept something impossible.

## Requirements

- Functional: each rule added must be backed by a probe showing `vi` accepts an
  impossible syllable, and by a real Vietnamese word it must **not** reject.
- Non-functional: rules live beside `violates_stop_coda_tone` and feed the same
  `is_invalid_vietnamese` predicate, so auto-fix and the mid-word spell check
  both inherit them at once.

## Architecture

Measured — `vi` says `true` for all of these, and every one is impossible:

| syllable | rule broken | `vi` |
|---|---|---|
| `onh`, `unh`, `ưnh` | `nh` closes only a **front** vowel (a, e, ê, i, y) | accepts |
| `uch`, `och`, `ơch` | `ch` closes only a front vowel | accepts |
| `eng` | `e` + `ng` is not a Vietnamese rime (`êng` is) | accepts |

Controls that must keep passing: `anh`, `inh`, `ênh`, `ach`, `ich`, `êch`,
`êng`, `qua`, `quy`.

UniKey encodes all of this in `isValidCV` / `isValidVC` (`ukengine.cpp:396`),
two large tables. **Do not port the tables** — that is a phonotactics engine we
do not need when `vi` already covers everything but the edges. Port the edges.

## Related Code Files

- Modify: `crates/glowkey-engine/src/lib.rs` — extend the rule set next to
  `violates_stop_coda_tone`
- Modify: `crates/glowkey-engine/tests/auto_fix.rs` — rules and controls

## Implementation Steps

1. Write the probe first: a test listing each impossible syllable with the rule
   it breaks, asserting `is_invalid_vietnamese` now catches it. It should fail.
2. Add `violates_front_vowel_coda` for the `nh`/`ch` family. Front vowels are
   a, ă, â, e, ê, i, y and their toned forms — match on the base letter after
   stripping tones, which `remove_tones` already does.
3. Re-run the dictionary sweep and record how much of the 8,362-word residue
   this removes. If it is under ~100 words, say so in the outcome rather than
   implying the phase was more than a tidy-up.
4. Only add the `eng` rule if step 3 shows the vowel-rime family is worth a
   second pass; a single rime is not a rule.

## Success Criteria

- [ ] `onh`, `unh`, `ưnh`, `uch`, `och`, `ơch` are all treated as non-Vietnamese
- [ ] `anh`, `inh`, `ênh`, `ach`, `ich`, `êch`, `mách`, `sách`, `tinh`, `xanh`
      are untouched
- [ ] The real-Vietnamese corpus in `midword_spell_check.rs` still passes with
      the mid-word check on — these rules reach it too
- [ ] The residue measurement is recorded, whatever it says

## Risk Assessment

**The rules are cheap; the risk is over-reach.** Each rule rejects syllables, so
a wrong rule silently corrupts correct Vietnamese.
*Signal:* any corpus word changes.
*Response:* drop that rule. One bad rule is worse than five missing ones.

**Front-vowel classification must survive tones.** `ánh`, `ảnh`, `ãnh` are all
front-vowel + `nh` and all legal; matching raw characters instead of stripped
bases would reject them.
*Signal:* `ánh`/`ảnh` rejected.
*Response:* strip tones before classifying, and test the toned forms explicitly.
