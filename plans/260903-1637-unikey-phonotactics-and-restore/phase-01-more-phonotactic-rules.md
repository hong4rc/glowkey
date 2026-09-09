---
phase: 1
title: "More phonotactic rules"
status: completed
priority: P2
effort: "2-3h (ưa + nh/ch only; ia/ua/eng deferred)"
dependencies: []
---

# Phase 1: More phonotactic rules

## Overview

`vi::validation::is_valid_syllable` is lenient, and the stop-coda tone rule was
not the only thing it misses. Three more families are confirmed, by the same
method: state the rule, probe `vi`, watch it accept something impossible.

**Updated 2026-09-06 with a third family, from a user report.** Typing `wasm`
produced `ưám`. That is not a possible Vietnamese syllable, and it is the family
below that is now the highest-priority rule here — it has a real English-word
collision (`wasm`, `wast`, `wasp`) and, unlike the other two, no counterexample
risk at all.

## Requirements

- Functional: each rule added must be backed by a probe showing `vi` accepts an
  impossible syllable, and by a real Vietnamese word it must **not** reject.
- Non-functional: rules live beside `violates_stop_coda_tone` and feed the same
  `is_invalid_vietnamese` predicate, so auto-fix and the mid-word spell check
  both inherit them at once.

## Architecture

Measured — `vi` says `true` for all of these, and every one is impossible:

| syllable | rule broken | `vi` | ships here |
|---|---|---|---|
| `ưam`, `ưám`, `ưat`, `ưáp`, `ưac` | `ưa` is the **open** diphthong; closed by a coda it must be written `ươ` | accepts | **yes** |
| `ưnh`, `ônh`, `ơnh` | `nh` closes only a **front** vowel (a, ă, â, e, ê, i, y) | accepts | yes |
| `ơch`, `ôch`, `ưch` | `ch` closes only a front vowel | accepts | yes |
| `onh`, `unh`, `uch`, `och` | same rule, but **pure ASCII** | accepts | **no** — the verbatim guard answers first, see below |
| `iam`, `iat`, `uan`, `uat` | same open-diphthong rule as `ưa`, via `iê`/`uô` | accepts | **deferred** — needs qu-/gi- exclusion |
| `eng` | `e` + `ng` is not a Vietnamese rime (`êng` is) | accepts | **deferred** — one rime is not a rule |

Controls that must keep passing: `ưa`, `mưa`, `chứa`, `ươm`, `vườn`, `mượn`,
`iêm`, `tiên`, `uôn`, `muôn`, `anh`, `inh`, `ênh`, `ach`, `ich`, `êch`, `êng`,
`qua`, `quy`, `quán`, `tuấn`. All 21 measured valid today, so no rule here may
change them.

### 1a. The open diphthongs — `ưa`, and why only `ưa`

`ưa`, `ia` and `ua` are the **open** forms of three diphthongs. Closed by any
coda they must be written `ươ`, `iê`, `uô`: `ươm`, `iêm`, `uôn`, never `ưam`,
`iam`, `uan`. Nucleus + coda is impossible **regardless of tone**, which is why
`violates_stop_coda_tone` misses `ưát` and `ưáp` — sắc is legal on a stop coda,
so the existing rule passes them.

This is the family behind the `wasm` report. In Telex `w`→`ư`, `a`, `s`→sắc,
`m` — every key applied faithfully, producing a syllable that cannot exist, and
auto-fix then declines to rescue it because `vi` says it is fine:

```
wasm -> ưám   wast -> ưát   wasp -> ưáp
```

**Ship `ưa` alone, first.** The `ia`/`ua` siblings share the rule but not the
safety, and the difference is measured:

| Family | Counterexamples | Safe as a surface rule? |
|---|---|---|
| `ưa` + coda | **none** — no `qư-` or `gư-` initial exists in Vietnamese | Yes |
| `ua` + coda | `quan`, `quát`, `quăn` — `qu-` initial, the `u` belongs to the initial | No |
| `ia` + coda | `gian`, `giam`, `giát` — `gi-` initial, same shape | No |

All six counterexamples are real words and all are currently valid. A naive
surface match on `ua`/`ia` + coda rejects every one of them. So the `ia`/`ua`
rules need qu-/gi- initial exclusion before they are safe — and they are
**deferred out of this phase** (decision 2026-09-06, "The deferred siblings"
below). `ưa` ships now and does not wait for them.

Note also `oa` (`toàn`, `hoàn`) is a *different* nucleus and is unaffected, as
is `uâ` (`xuân`, `tuân`, `thuật`) — `uân` is not `uan`. Neither may be caught.

UniKey encodes all of this in `isValidCV` / `isValidVC` (`ukengine.cpp:396`),
two large tables. **Do not port the tables** — that is a phonotactics engine we
do not need when `vi` already covers everything but the edges. Port the edges.

## Related Code Files

**Paths corrected 2026-09-06.** This section named `src/lib.rs` and
`tests/auto_fix.rs`; the function moved during the engine split and that test
file does not exist.

- Modify: `crates/glowkey-engine/src/engine.rs` — extend the rule set next to
  `violates_stop_coda_tone` (~:670) and add the call to `is_invalid_vietnamese`
  (~:641). `lib.rs` only re-exports (`pub use engine::*`), so nothing there
  needs touching
- Modify: `crates/glowkey-engine/tests/midword_spell_check.rs` — rules and
  controls go here, beside the existing corpus that these rules must not break.
  There is no `auto_fix.rs`; do not create one for two rules when the corpus
  they must survive already lives in this file
- Read only: `crates/glowkey-engine/src/tones.rs` — `remove_tones`, which turned
  out to be **unusable here**; see "`remove_tones` cannot do this" below

## Implementation Steps

1. Write the probe first: a test listing each impossible syllable with the rule
   it breaks, asserting `is_invalid_vietnamese` now catches it, plus all 21
   controls above. It should fail on the impossible list and pass on the
   controls — the controls passing *before* any rule exists is what proves the
   test can detect over-reach.
2. **Add `violates_open_diphthong_coda` for `ưa` first, and ship it on its own.**
   It fixes a live user report, and it is the only rule here with no
   counterexample risk. Match the nucleus after stripping **tone marks only**
   (`ứa`, `ừa`, `ửa` are all the same nucleus), then reject if any coda follows.
3. Add `violates_front_vowel_coda` for the `nh`/`ch` family. Front vowels are
   a, ă, â, e, ê, i, y and their toned forms — match on the vowel next to the
   coda after stripping **tone marks only** (not `remove_tones`; see below).
4. Run the corpus test in `midword_spell_check.rs` with the mid-word check on
   and confirm no real-Vietnamese entry changed. That corpus is the over-reach
   guard that actually exists (see the note below).
5. **Do not add the `ia`/`ua` siblings in this phase.** Deferred by decision
   2026-09-06 — see "The deferred siblings" below.
6. **Do not add the `eng` rule in this phase.** It was conditional on the same
   missing measurement, and a single rime is not a rule.

### The dictionary sweep does not exist

**Corrected 2026-09-06 by verification.** Steps 4-5 previously read "re-run the
dictionary sweep and record how much of the 8,362-word residue this removes."
There is no sweep: no script under `scripts/`, no test, no committed wordlist,
and no source in the repo for the 8,362 figure. A grep for it returns nothing.
The step was unbuildable as written, and it was gating a scope decision.

What exists instead is `crates/glowkey-engine/tests/midword_spell_check.rs`'s
`no_false_rejection_across_real_vietnamese` (~:75) — a curated corpus, not a
dictionary sweep. It catches over-reach on the words it contains, which is
enough to ship a rule safely and not enough to *size* a rule's benefit.

### The deferred siblings

`ia` and `ua` are deferred, not rejected. The rule is real and the counterexample
handling is understood (qu-/gi- initial exclusion); what is missing is any way to
know what the added complexity buys. Reviving them needs a measurement
instrument first — a sourced Vietnamese wordlist and a sweep over it — which is
its own piece of work and does not belong inside a 3h rule phase.

`ưa` does not wait for that, because it needs no sizing: it fixes a reported bug
(`wasm`, `wast`, `wasp`) and has no counterexample in the language.

### `remove_tones` cannot do this

**Found during implementation 2026-09-06.** Steps 2-3 originally said to strip
tones "which `remove_tones` already does". It does not do the right thing:
`remove_tones` flattens all the way to plain ASCII (`ư`→`u`, `ơ`→`o`, `ă`→`a`)
because it exists for filenames and search boxes. That collapses exactly the
distinctions these rules turn on — `ưa` would become `ua`, so the rule would
judge the deferred sibling instead of its own family, and `ơch` would become
`och`.

So the implementation carries a private `strip_tone_marks` beside the rules: it
removes the five tone marks and **keeps** the vowel's modifier (horn on `ư`/`ơ`,
breve on `ă`, circumflex on `â`/`ê`/`ô`). `remove_tones` is left alone — it is
correct for what it is for.

### The ASCII guard bounds this phase

`is_invalid_vietnamese` opens with "a pure-ASCII word is what the user typed
verbatim — leave it alone" and returns `false`. So the all-ASCII spellings of the
`nh`/`ch` family — `onh`, `unh`, `uch`, `och` — are **not** reachable by this
phase's rules, and that is correct rather than a gap: those reach the screen only
by being typed literally, and there is nothing to restore them *to*. The rules
bite on the non-ASCII renders Telex actually produces.

This is what phase 2 (ASCII-render restore) lifts. Until then every rule here
sits behind that guard, and
`the_ascii_guard_still_short_circuits_every_rule` in `midword_spell_check.rs`
pins it so the next reader does not file it as a bug in the rule.

## Success Criteria

- [x] **`wasm`, `wast` and `wasp` survive as typed** — the reported bug, and the
      one criterion a user would notice
- [x] `ưam`, `ưám`, `ưat`, `ưáp`, `ưac` are all treated as non-Vietnamese
- [x] The non-ASCII `nh`/`ch` family (`ưnh`, `ônh`, `ơch`, `ôch`, `ưch`) is
      treated as non-Vietnamese; the all-ASCII spellings are out of reach behind
      the verbatim guard, pinned as such
- [x] `ưa`, `mưa`, `chứa`, `ươm`, `vườn`, `mượn` are untouched — the open form
      bare, and the closed form written correctly
- [x] `anh`, `inh`, `ênh`, `ach`, `ich`, `êch`, `mách`, `sách`, `tinh`, `xanh`
      are untouched
- [x] `quan`, `quát`, `gian`, `giam`, `xuân`, `tuân`, `toàn` are untouched
      whether or not the `ia`/`ua` rules ship
- [x] The real-Vietnamese corpus in `midword_spell_check.rs` still passes with
      the mid-word check on, and **gained seven `-nh`/`-ch`/`ưa` entries** — it
      had none of either shape, so it could not have caught over-reach in these
      rules before
- [x] The corpus is unchanged *in verdict* by the new rules (it was extended,
      not altered — see above)
- [x] The deferral of `ia`/`ua`/`eng` is recorded with the reason (no measurement
      instrument), not left looking like an oversight

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

**The `ia`/`ua` siblings break real words if matched on the surface.** `quan`,
`quát`, `quăn`, `gian`, `giam`, `giát` all carry a surface `ua`/`ia` + coda in
which the vowel belongs to the **initial** (`qu-`, `gi-`), not the nucleus. All
six are currently valid and were measured so.
*Signal:* any of the six is rejected, or the corpus loses `qu-`/`gi-` words.
*Response:* the sibling rules require qu-/gi- exclusion, and per step 5 they
ship only if measured residue justifies it. `ưa` is unaffected — no `qư-`/`gư-`
initial exists — which is why it ships first and alone.

**`uâ` and `oa` are different nuclei and must not be caught.** `xuân`, `tuân`,
`thuật` are `uâ`; `toàn`, `hoàn` are `oa`. Neither is the `ua` diphthong.
*Signal:* `xuân` or `toàn` rejected.
*Response:* match the nucleus after tone-stripping on the exact vowel pair, not
on "u followed by a vowel".

## Outcome — 2026-09-06

Shipped: `violates_open_diphthong_coda` (`ưa`) and `violates_front_vowel_coda`
(`nh`/`ch`), both feeding `is_invalid_vietnamese`, plus a private
`strip_tone_marks` the rules needed.

**The reported bug is fixed.** `wasm`, `wast` and `wasp` survive as typed,
asserted end to end through the auto-fix boundary in
`crates/glowkey-session/tests/auto_fix.rs` — not only against the predicate.

Verification: 356 tests pass across the workspace including the generative suite
in `glowkey-session/tests/properties.rs`; `cargo clippy --workspace
--all-targets` clean; the three touched files rustfmt-clean. A code review swept
~38M rendered Telex key sequences hunting for a real Vietnamese syllable either
rule wrongly rejects and **found none**.

Three things the phase did not anticipate, each recorded above where it belongs:

1. `remove_tones` could not be used — it flattens to ASCII and would have
   collapsed `ưa` into the deferred `ua` family.
2. The `is_ascii()` guard puts the all-ASCII spellings of the `nh`/`ch` family
   out of reach. Correct, not a gap; phase 2 is what lifts it.
3. `tests/auto_fix.rs` **does** exist, in `glowkey-session`. This file said it
   did not, which was true only of `glowkey-engine`.

One regression accepted knowingly, filed as
[`260906-2159` phase 8](../260906-2159-unblocked-work-and-plan-reconciliation/phase-08-lift-the-escape-latch.md):
with the **non-default** strict mid-word check on, a `uâ` word typed horn-first
(`tuanwa`) now stays raw instead of reaching `tuân`, because the escape is a
latch. Ordinary spellings (`tuaan`) and the boundary auto-fix path are
unaffected. Lifting the latch reverses a documented decision, so it was filed
rather than folded in here. Pinned meanwhile by
`the_escape_latch_still_swallows_a_horn_first_ua_word`.
