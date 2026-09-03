---
title: "GlowKey — Telex bracket shortcuts and mid-word spell check"
description: "The two UniKey ideas left after the parity work: opt-in [ ] { } shortcuts for ơ/ư, and UniKey's second spell-check option that refuses an impossible diacritic as it is typed rather than only restoring at the word boundary."
status: completed
priority: P2
effort: "1-2 days"
tags: [glowkey, engine, telex, unikey, spell-check]
created: 2026-09-03
---

# GlowKey — Telex bracket shortcuts and mid-word spell check

## Overview

Everything else from the UniKey source reading has shipped. Two ideas remain,
and they are independent of each other.

Source of the analysis: `plans/reports/xia-260903-1447-unikey-source-comparison.md`,
read against `hochanh/unikey-source` at commit `e3b8f3b`. Both items below were
verified absent from GlowKey by probing the engine, not assumed.

## Goals

| # | Goal | Priority |
|---|------|----------|
| 1 | `[`→ơ, `]`→ư, `{`→Ơ, `}`→Ư in Telex, behind an opt-in checkbox | P2 |
| 2 | UniKey's second spell-check option: refuse an impossible diacritic at the keystroke, not only at the boundary | P2 |

## Phases

| # | Phase | Status |
|---|-------|--------|
| 1 | [Phase 1: Telex bracket shortcuts](./phase-01-telex-brackets.md) | Pending |
| 2 | [Phase 2: Mid-word spell check](./phase-02-midword-spell-check.md) | Pending |

Independent — either can ship without the other. Phase 2 is the riskier one and
should go second.

## Decisions already taken

| Decision | Choice | Why |
|---|---|---|
| Bracket default | **Opt-in checkbox, off by default** | UniKey has it always on in full Telex, which stops `[` typing a bracket. That would silently break `[hello]` in markdown and chat for every existing user. |
| Mid-word spell check | **Build it, as a second option** | UniKey splits `spellCheckEnabled` from `autoNonVnRestore`; GlowKey only has the second. |
| Simple Telex | **Dropped** | Needs `W` remapped to Hook-All, which the `vi` crate does not allow without pre-translating every `w`. One mapping difference is not worth that. |
| User-defined key map | **Deferred** | Largest change in the source; nothing is asking for it. |
| Licensing | **Ideas only, never code** | Owner's call, recorded: UniKey is LGPL v2, GlowKey MIT. Nothing is copied — the C does not map onto an engine that delegates to the `vi` crate anyway. |

## Success Criteria

- [ ] With both options off, engine output is byte-identical to today
- [ ] Brackets: `[`→ơ, `]`→ư, `{`→Ơ, `}`→Ư, in Telex only, and a tone key still applies afterwards (`[f`→ờ)
- [ ] Brackets: with the option off, `[` remains a word boundary and types a bracket
- [ ] Spell check: no false rejection across a corpus of real Vietnamese words typed key by key
- [ ] `cargo test --workspace` green, `cargo clippy --workspace --all-targets` silent
- [ ] Both options documented in `docs/handoff.md` §4 with their trade-off

## Open questions

None blocking. One to settle inside Phase 2 step 1: whether the rejection rule
fires on every transforming key or only on tone keys — decided by what the
corpus shows, not up front.

<!-- slug: unikey-telex-brackets-spellcheck -->

## Outcome (2026-09-03)

Both phases implemented and verified in the running app. 108 tests, clippy clean.

**Phase 1** landed as designed. The probe-first approach paid off: `ow`/`OW`/`uw`/`UW`
were confirmed to produce ơ/Ơ/ư/Ư before any code was written, so the substitution
approach was known-good rather than hoped-for. `[f`→ờ works, which was the whole
reason for substituting keys instead of the character.

**Phase 2's spike gate did its job, twice.**

First, it passed: 51 real Vietnamese words, zero intermediate rejections — after
five corpus entries turned out to be *my own Telex typos* (`tuoiir`, `caamr`,
`noiis`, `giuwax`, and `khoer` which renders `khoẻ` under New-style placement,
not `khỏe`). Worth noting for anyone extending the corpus: a failing entry is far
more likely to be a mistyped expectation than an engine bug.

Second, and more usefully, the first implementation of the refusal was a **no-op**
— it popped the offending key, restored the previous render, then re-pushed the
same key and re-rendered, arriving back at the identical invalid string. Every
test passed because the feature did nothing. It was caught by probing whether the
option changed any output at all (`exit` gave `eĩt` with the option both on and
off), not by the test suite. The lesson is in the tests now: `no_false_rejection`
alone cannot distinguish "no false rejections" from "no behaviour".

The `escaped`-flag design that replaced it is forced by the architecture, and the
plan's risk section named the interaction that then bit: the repeat-key rejection
gesture (`hoongff`→`hôngf`) leaves a trailing literal that makes the word
"impossible", so the check fired and undid a rejection the user asked for. The
pre-decided response — exclude the repeat-key case — is what shipped.
