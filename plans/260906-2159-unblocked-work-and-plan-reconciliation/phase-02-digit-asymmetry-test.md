---
phase: 2
title: "Pin the digit/letter asymmetry"
status: pending
priority: P2
effort: "30m"
dependencies: []
---

# Phase 2: Pin the digit/letter asymmetry

Executes [`260905-1643` §3.6](../260905-1643-glowkey-production-hardening/phase-3-correctness.md).
That section is the decision of record; this phase only carries it out.

## Overview

`hoongf4` → `hồng4` was reported as a bug. It is not: it is the documented rule,
already decided (2026-09-06) and being kept. The action is a test, so the next
reader does not re-open it.

Cheapest real code in the plan, and it needs no hardware — hence second, right
after reconciliation.

## Requirements

- Functional: a test pins the `hoongfc` / `hoongf4` contrast as intended
  behaviour, with the reason in the test's own comment.
- Non-functional: no engine behaviour changes. If a test written to the rule
  fails, the rule and the code disagree and that is a finding, not a test to
  adjust.

## Architecture

From §3.6: **in Telex a digit is not a syllable character**, so it terminates the
word exactly like a space.

Confirmed against `engine.rs:310` (`is_syllable_char`). Note §3.6 cites
`:293-295`, which is wrong — those lines are the repeat-key rejection comment.
Corrected 2026-09-06 by verification, here and upstream.

- `hoongf4` — by the time `4` arrives, `hồng` has committed as valid Vietnamese.
  Auto-fix has nothing to restore. → `hồng4`
- `hoongfc` — a letter *extends* the syllable, so the whole word is re-judged,
  fails validation, and escapes to raw. → `hoongfc`

Both are correct. They differ because the digit is a boundary and the letter is
not.

§3.6 explicitly **rejects** treating a digit glued to a syllable as evidence the
token is not Vietnamese: it would break `tầng2`, `quận1`, `phường7` — forms
Vietnamese people type without a space. That rejection is the thing the test
protects; a future "fix" for `hoongf4` breaks real Vietnamese text.

## Related Code Files

- Modify: `crates/glowkey-engine/tests/telex.rs` — add the contrast beside the
  existing auto-fix/escape coverage, using the file's own `type_word` helper
- Read only: `crates/glowkey-engine/src/engine.rs:310` — `is_syllable_char`, the
  rule the test pins. Its body is `ch.is_ascii_alphabetic() || (method == Vni &&
  ch.is_ascii_digit()) || bracket shortcuts`, so in Telex a digit is excluded and
  ends the word

## Implementation Steps

1. Add one test to `tests/telex.rs` asserting both halves together, so the
   asymmetry is visible in a single place:
   - `type_word("hoongfc")` == `"hoongfc"`
   - `type_word("hoongf4")` == `"hồng4"`
2. Comment it with *why* they differ (digit is a boundary, letter extends the
   syllable) and with the rejected alternative and its cost (`tầng2`, `quận1`,
   `phường7`). A reader who only sees the assertions will file the bug again.
3. Add `tầng2`-shaped coverage — `taawng2` → `tầng2` — so the rejected
   alternative would actually fail a test rather than merely being argued
   against in prose.
4. `cargo test -p glowkey-engine --test telex`.

## Success Criteria

- [ ] `cargo test -p glowkey-engine --test telex` passes
- [ ] Both `hoongfc` and `hoongf4` are asserted in one test
- [ ] At least one real digit-suffixed Vietnamese word is asserted, so the
      rejected design breaks a test
- [ ] No change to any file under `crates/glowkey-engine/src/`

## Risk Assessment

| Risk | Signal it broke | Response |
|---|---|---|
| The test as written fails, meaning the documented rule and the code disagree | Either assertion fails on first run | Stop. This is a real finding about `engine.rs`, not a test to tune. Report it and re-open §3.6 with the measurement — the decision was made on a stated rule, and the rule would be wrong |
| The digit-suffix control does not actually exercise the rejected design | The test passes even with a naive "digit means non-Vietnamese" rule bolted in | Verify by construction: the control must be a word that such a rule would wrongly escape to raw |
