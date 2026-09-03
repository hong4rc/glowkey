---
title: "GlowKey — what is left to take from UniKey"
description: "Three things remain after two source passes: more phonotactic rules the vi crate misses, UniKey's structural notion of a non-Vietnamese word (which would rescue thousands of English words auto-fix currently cannot see), and the user-defined input method."
status: pending
priority: P2
effort: "2-4 days"
tags: [glowkey, engine, unikey, phonotactics, auto-fix]
blockedBy: [260903-1745-glowkey-hardening-and-distribution, 260903-2234-glowkey-personal-word-list]
created: 2026-09-03
---

# GlowKey — what is left to take from UniKey

## Overview

Two source passes have already landed everything cheap: the options struct, the
input methods (Telex, VNI, Simple Telex), macros with UniKey-compatible import,
always-macro, the clipboard tools, and the stop-coda tone rule. What remains is
three items, and the second is worth more than the other two combined.

Evidence: `plans/reports/xia-260903-1447-unikey-source-comparison.md` and
`xia-260903-1618-unikey-engine-behaviours.md`, plus the measurements below,
taken against `/usr/share/dict/words` (104,930 lowercase words, 3-9 letters).

## The measurement that shapes this plan

Typing every dictionary word through the engine as a Vietnamese user would:

| | count | share |
|---|---|---|
| Words whose render differs from what was typed | 31,178 | 30% |
| … of those, render is **non-ASCII** (auto-fix can see it) | 24,666 | 79% |
| … of those, render is **ASCII but different** (auto-fix is blind to it) | 6,512 | 21% |
| Still wrong after auto-fix runs | **8,362** | 8% of all words |

Auto-fix already rescues most of the non-ASCII cases. The residue is dominated
by the ASCII-but-different bucket, which `is_invalid_vietnamese` short-circuits
past on its first line. That is Phase 2, and it is the largest remaining defect
in everyday typing.

## Goals

| # | Goal | Priority |
|---|------|----------|
| 1 | Add the phonotactic rules `vi` still misses, as the stop-coda rule was added | P2 |
| 2 | Rescue words whose Telex render is ASCII but not what was typed | P1 |
| 3 | User-defined input method (`UkUsrIM`) — the last unported UniKey feature | P3 |

## Phases

| # | Phase | Status |
|---|-------|--------|
| 1 | [Phase 1: More phonotactic rules](./phase-01-more-phonotactic-rules.md) | Pending |
| 2 | [Phase 2: ASCII-render restore](./phase-02-ascii-render-restore.md) | Unblocked 2026-09-03 — the gating property tests are green |
| 3 | [Phase 3: User-defined input method](./phase-03-user-defined-input-method.md) | Pending |

Independent. Phase 2 carries the most value and the most risk; Phase 1 is the
cheap continuation of a vein that has already paid out once. Phase 3 is listed
for completeness and is not recommended yet.

## Relationship to the hardening plan

`plans/260903-1745-glowkey-hardening-and-distribution/` (created 2026-09-03)
covers signing, release, testing, refactor and UX. It does not touch
`crates/glowkey-engine/src/`, so the two plans can proceed in parallel without
file conflict.

**One hard ordering, decided by the owner at that plan's validation
(2026-09-03):** its **Phase 3 (property tests over the diff invariant)** lands
**before Phase 2 here (ASCII-render restore)**. Restoring a word whose render is
ASCII-but-different is the change most likely to violate "rendered == the text
tail at the caret", and a generated keystroke suite catches that class of
failure where a word-list corpus does not. Phase 2 here does not start until
those property tests are green — hence the `blockedBy` entry in this plan's
frontmatter. Phases 1 and 3 here are unaffected and may start any time.

**Gate satisfied 2026-09-03.** `crates/glowkey-engine/tests/properties.rs` is in
place and green, including the assertion that matters most to Phase 2 here: a
restore edit's backspace count must equal the rendered word's UTF-16 length, so
an ASCII-render restore that deletes the wrong amount fails immediately rather
than silently stranding characters on screen. A review found that assertion
missing on the first pass and it was added; the mutation that exposed the gap now
fails two tests. Phase 2 here may start.

## Conflict with the personal word list

`plans/260903-2234-glowkey-personal-word-list/` hooks into the restore decision
inside `Session::commit` — **the same function and the same decision** Phase 2
here rewrites. They compose logically (a per-word override beats a rule) but they
will collide textually.

- **Do not run them concurrently.**
- **That plan is recommended first.** It is smaller, and it gives the user a
  per-word escape hatch *before* this plan changes auto-fix's behaviour across
  thousands of words at once. Going in with an override available is a better
  order than going in without one.

## Standing decisions carried forward

| Decision | Choice |
|---|---|
| VIQR input method | **Out.** Its keys are `'`, `.`, `?`, `~` — enabling it stops ordinary punctuation working. Owner's standing decision. |
| Legacy charsets, `uvconvert`, clipboard encoding conversion | **Out.** Every modern macOS app is Unicode. |
| `UkMsVi` (Microsoft layout) | **Out.** Obscure, no demand. |
| Porting UniKey's `isValidCV`/`isValidVC` tables wholesale | **Out.** Take rules, not tables — the reason Phase 1 is a short rule list rather than a port. |
| Licensing | Ideas only, never code (owner's call, recorded). |

## Success Criteria

- [ ] Phase 1: every rule added is backed by a probe showing `vi` accepts something impossible
- [ ] Phase 2: the 8,362-word residue measurably shrinks, with zero regressions on the real-Vietnamese corpus
- [ ] The repeat-key escape hatch (`cass`→`cas`) keeps working in every phase
- [ ] `cargo test --workspace` green, `cargo clippy --workspace --all-targets` silent
- [ ] `docs/handoff.md` states each new rule and what it costs

## Open questions

1. Phase 2's carve-out list is the whole design and cannot be settled on paper —
   step 1 measures it. See that phase.
2. Is Phase 3 wanted at all? Nothing has asked for it.

<!-- slug: unikey-phonotactics-and-restore -->
