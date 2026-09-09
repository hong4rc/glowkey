---
title: "Unblocked work, and the plans that lie about being done"
description: "Every actionable phase left across the repo's plans, plus the status reconciliation that made the list trustworthy"
status: pending
priority: P1
effort: "3-5d agent, incl. live runs needing an idle machine; League needs a person"
tags: [reconciliation, correctness, windows, privacy]
created: 2026-09-06
branch: hardening/phase-1-safety-fixes
blockedBy: []
blocks: []
related:
  - 260903-1637-unikey-phonotactics-and-restore
  - 260905-1643-glowkey-production-hardening
---

# Unblocked work, and the plans that lie about being done

**Origin:** a user report that typing `wasm` yields `ưám`, plus a request to
"fix all current not completed". The second half turned out to be mostly a
bookkeeping problem, and finding that out is the reason this plan exists.

## What this plan is for

A sweep of `plans/` found ~40 phase entries not marked `completed`. Only a third
of them are work. The rest split into two piles that must be handled
differently, and conflating them is how a repo ends up planning work it already
shipped:

| Category | Count | Reality |
|---|---|---|
| **Status drift** | 7 plans | `plan.md` says `completed`, phase files still say `pending`/`in-progress`. Six files in `260904-2127` use `status: complete`, which is not a value the schema accepts. Telex brackets and mid-word spell-check are shipped *with tests* and still marked pending. |
| **Real and unblocked** | 7 phases | This plan, phases 2-8. Phase 8 was filed by the code review of `1637` phase 1. |
| **Blocked or deliberately skipped** | 5 phases | Documented in "Not in scope" below with what unblocks each. Not silently dropped. |

The single most expensive discovery: **phase 3.5 of `260905-1643` is already
done.** `plan.md` there says it "is next and needs no hardware"; commit
`b3940d8` ("a flush says why, on every path that discards a word") shipped it on
both platforms — `hook.rs:259` on Windows, `macos/dispatch.rs:121` and
`macos/mod.rs:199` on macOS. It is an ancestor of `HEAD`. A phase file was
almost written for work that exists. Phase 1 exists so that stops happening.

## Run the ưám fix before any of this

**Decided 2026-09-06 in validation.** `260903-1637` phase 1 (the `ưa` rule) goes
first, ahead of phase 1 here. It is the reported bug, ~2-3h, needs no hardware,
and touches only `crates/glowkey-engine/` — disjoint from this plan's phase 1,
which touches only `plans/`.

Order: **`1637` phase 1 → this plan's phases 1-7.**

## The ưám bug does not live here

It belongs to `260903-1637` phase 1 ("More phonotactic rules") — same rule
family, same function, same test file. That phase is being **extended in place**
rather than duplicated here; see
[`260903-1637-unikey-phonotactics-and-restore/phase-01-more-phonotactic-rules.md`](../260903-1637-unikey-phonotactics-and-restore/phase-01-more-phonotactic-rules.md).

Summary of what was added there, so this plan is readable alone:

- `vi::validation::is_valid_syllable` accepts `ưam`, `ưám`, `ưat`, `ưáp`, `ưac`
  — measured, all `invalid=false`. So `wasm`→`ưám`, `wast`→`ưát`, `wasp`→`ưáp`
  transform and auto-fix never rescues them.
- The rule: `ưa`, `ia`, `ua` are the **open** diphthongs. Closed by any coda they
  must be written `ươ`, `iê`, `uô` (`ươm`, `iêm`, `uôn`). Nucleus + coda is
  impossible regardless of tone, so the existing `violates_stop_coda_tone` never
  catches the sắc cases.
- **The trap, measured:** a naive surface rule breaks real words. `quan`,
  `quát`, `quăn` (qu-) and `gian`, `giam`, `giát` (gi-) all carry a surface
  `ua`/`ia` + coda where the vowel belongs to the *initial*, not the nucleus.
  `ưa` has no such counterexample — no `qư-` or `gư-` initial exists.
- **`ia`/`ua` and `eng` are deferred out of that phase** (validation,
  2026-09-06). They were gated on a dictionary sweep and an "8,362-word residue"
  figure that **do not exist** in this repo — no script, no wordlist, no source
  for the number. Reviving them needs a measurement instrument first. `ưa` ships
  without one because it fixes a reported bug and cannot over-reach.
- That phase's `## Related Code Files` was stale: it names `src/lib.rs` and
  `tests/auto_fix.rs`. The function moved to `src/engine.rs:641` in the engine
  split, and `auto_fix.rs` does not exist. Corrected there.

## Phases

| # | Phase | Source | Who can execute | Depends on |
|---|---|---|---|---|
| 1 | [Reconcile plan status drift](./phase-01-reconcile-plan-status-drift.md) | this plan | agent | — |
| 2 | [Pin the digit/letter asymmetry](./phase-02-digit-asymmetry-test.md) | 1643 §3.6 | agent | — |
| 3 | [File permissions on the log and settings](./phase-03-file-permissions.md) | 1643 §2.3 | agent (macOS half unverifiable here) | — |
| 4 | [Windows hook liveness watchdog](./phase-04-windows-hook-liveness.md) | 1643 §3.2 | agent, **idle machine** | — |
| 5 | [Verify the Chromium omnibox guard](./phase-05-chromium-guard-real-fix.md) | 1643 §3.1 | agent, **idle machine** | — (code already landed) |
| 6 | [Re-verify the `hoongfa`/`ss` sequence](./phase-06-reverify-escape-sequence.md) | 1643 §3.7 | agent, **idle machine** | — |
| 7 | [The League discriminating test](./phase-07-league-discriminating-test.md) | 1643 §3.4 | **human only** (needs a Practice Tool game) | — |
| 8 | [Lift the mid-word escape latch](./phase-08-lift-the-escape-latch.md) | code review of 1637 ph1 | agent | — |

Phases 2-7 are **execution wrappers, not redesigns.** `260905-1643`'s phase
files remain the design of record; each phase here cites the section it executes
and adds only ordering, acceptance criteria, and what was verified since. This
is deliberate — duplicating those designs is exactly the "duplicate embedded
contracts" failure the consistency gate looks for.

### Ordering, and why it deviates from 1643's work order

`260905-1643` orders phase 3 as 3.5 → 3.7 → 3.6 → 3.2 → 3.1. Three things
changed since it was written: **3.5 is done**, **3.1's code is done**, and the
rule about who can run live verification turned out to be wrong.

**An agent *can* drive live Windows verification — corrected 2026-09-06.** An
earlier report recorded `SendInput` as impossible from an agent-spawned process,
inferred from a single `ERROR_ACCESS_DENIED`. That inference was wrong: an
elevated Windows Terminal was the foreground window, and UIPI blocks injection
into a higher-integrity foreground from *any* ordinary process. With an ordinary
window in front the same call returns `sent=2 err=0`.
`scripts/probe-sendinput.ps1` reports the result next to the foreground window
and its elevation so nobody repeats the inference.

Three real limits replace it, and they shape phases 4-7:

- Taking the foreground needs `AttachThreadInput`; `SetForegroundWindow` alone
  is refused for a background process.
- Focusing the Chromium address bar is unreliable while a page holds the
  keyboard — a YouTube player swallowed `Ctrl+L`, `Alt+D` and `F6` repeatedly.
  Open a plain tab first.
- **Not technical, and the one that actually bit:** these harnesses take over
  the screen and type. They need an idle machine, and asking first is a real
  requirement — ignoring it interrupted the user mid-session while this was
  being researched.

So phases 4-7 are agent-runnable *with the machine to itself, after asking*, and
they are grouped last because they share that one prerequisite — not because a
person must press every key.

## Not in scope, and what unblocks each

Recorded rather than dropped, per the scope decision:

| Item | Why not now | What unblocks it |
|---|---|---|
| 1643 §3.3 macOS tap callback blocks | Needs a Mac | Hardware. Code is writable now; sign-off waits on phase 5 of 1643 |
| 1643 phase 5 macOS runtime | Needs a Mac | Hardware. Gates every macOS-side change |
| 1643 phase 4 Windows shell parity | Open decisions **B4** (live-apply vs Apply button) and **B8** (how a user picks an app to exclude) | User answers, `260905-1643/decisions.md` §5-6 |
| 1643 phase 6 release and signing | Open decisions **A4** ($99/yr Apple Developer Program) and **A5** (Windows code signing) | User answers + money, `decisions.md` §3-4 |
| 1643 phase 8 Linux (IBus) | **SKIPPED by explicit decision**, `decisions.md` §9 | A decision reversal, which needs new evidence — not an audit's opinion |
| 1637 phase 2 ASCII-render restore | Unblocked, but P1 and 1-2d, and it changes the `is_ascii()` early return that every rule in phase 1 sits behind | Stays in 1637; do it after 1637 phase 1 lands |
| 1637 phase 3 user-defined input method | Unblocked, but 1637 itself says "listed for completeness, **not recommended**", P3, 3-5d | A decision that UniKey keymap parity is worth 3-5 days |
| 260903-1745 hardening-and-distribution | All 6 phases `in-progress`; signing/release/latency overlap `260905-1643` phases 6 and 3 | Phase 1 determines whether it is superseded or has residue |
| 1637 phase 1's `ia`/`ua` and `eng` rules | **Deferred in validation 2026-09-06.** Gated on a dictionary sweep and an "8,362-word residue" figure with no script, wordlist, or source in the repo | A measurement instrument: a sourced Vietnamese wordlist plus a sweep over it. Its own piece of work, not a step inside a 3h rule phase |

## Success Criteria

- [ ] Every phase file in `plans/` carries a status that matches the code, and
      `ak plan validate` passes on each plan directory
- [ ] No phase file uses `status: complete`
- [ ] `wasm`, `wast`, `wasp` survive as typed (delivered by 1637 phase 1)
- [ ] The log and settings files are unreadable by other users on both platforms
- [ ] Windows reports a dead hook instead of silently stopping
- [ ] A Chromium forward-delete fires only when a selection actually exists, and
      `OMNIBOX_CLASS` is confirmed against what Edge and Chrome really report
- [ ] The `hoongfa`/`ss` report is either explained by a flush or reclassified as
      a re-composition defect with a phase of its own

## Open questions

1. **Is `260903-1745` superseded or does it have residue?** Its 6 `in-progress`
   phases overlap `260905-1643` phases 3 and 6. Phase 1 answers this; if it has
   residue, that residue needs a home and this plan grows a phase.
2. ~~**Do the `ia`/`ua` sibling rules ship at all?**~~ **Resolved in validation
   2026-09-06: deferred.** The gate was a dictionary sweep that does not exist.
   They return only once a measurement instrument does. `ưa` ships alone.
3. **Does phase 5 need UIA at all on the address bar specifically?** The live
   session showed only the *first* word in a freshly cleared omnibox failing —
   the inline-autocomplete signature. A cheaper fix keyed to that condition may
   exist, and it should be compared against the UIA worker thread before the
   larger design is built.

## Validation Log

### Session 1 — 2026-09-06

#### Verification Results

- **Tier:** Full (7 phases → all 4 roles)
- **Claims checked:** 34
- **Verified:** 30 | **Failed:** 4 | **Unverified:** 0

Verified highlights: `shell.rs:272 reinstall_hook`; `indicator.rs` `HookGone`
(`:56`, `:75`, `:135`); `elevation.rs`; `hook_log.rs`; **no `SetTimer`/`WM_TIMER`
anywhere in `platform/windows/`**, confirming §3.2's claim that no liveness check
exists; `omnibox.rs:69 OMNIBOX_CLASS = "OmniboxViewViews"` and every cited line
in `omnibox.rs`/`foreground.rs`/`inject.rs`; `log.rs:95`; `settings_store.rs:123`;
`paths.rs` `settings_dir`/`log_dir`; `default_exclusions/mod.rs:55 is_chromium_app`;
`tones.rs:10 remove_tones`; `engine.rs:641`/`:673`; `midword_spell_check.rs:75`
corpus; `b3940d8` is an ancestor of `HEAD`; `Win32_Security` already enabled with
**zero** existing DACL calls in `app/` or `crates/`.

#### Failures

1. **[Fact Checker]** `engine.rs:293-295` cited for the digit/syllable rule —
   `is_syllable_char` is at **`engine.rs:310`**; `:288-298` is the repeat-key
   rejection comment. Origin: `1643` §3.6. **Fixed** here and upstream.
2. **[Fact Checker]** `session.rs:775-788` — no `glowkey-engine/src/session.rs`
   exists; three other `session*.rs` files do. Real file
   **`crates/glowkey-session/src/session.rs`**, line numbers correct and
   containing `self.committed.clear()` on the restore branch. Origin: `1643`
   §3.7. **Fixed** here and upstream.
3. **[Fact Checker]** **No dictionary-sweep tooling exists** — no script, no
   wordlist, no source for the "8,362-word residue". `1637` phase 1 steps 4-5
   were unbuildable *and* were gating a scope decision. **Fixed:** `ia`/`ua` and
   `eng` deferred; the corpus test became the over-reach guard.
4. **[Contract Verifier]** Phase 5 wrote manual instructions for work that
   `scripts/probe-omnibox-identity.ps1` (written explicitly for §3.1) and
   `scripts/verify-windows-omnibox.ps1` already do. **Fixed:** both cited, with
   the `verify-windows-tier*.ps1` suite preferred over new scripts.

#### Decisions

| # | Question | Decision |
|---|---|---|
| 1 | `ia`/`ua` gate with no sweep tooling | **Ship `ưa` only; defer `ia`/`ua` and `eng`** until a measurement instrument exists |
| 2 | Apply F1/F2/F4 corrections | **Yes, including upstream** `260905-1643` §3.6/§3.7, where two of them originated |
| 3 | Windows DACL approach | **Explicit per-file `SetNamedSecurityInfoW` DACL**; directory inheritance rejected as the mechanism that silently re-widens |
| 4 | Execution order | **`1637` phase 1 (the ưám fix) first**, then this plan's phases 1-7 |

#### Whole-Plan Consistency Sweep

Re-read `plan.md` and all 7 `phase-*.md` files, plus the two upstream files
edited. Contradictions found and reconciled:

- `1637` phase 1 still said the siblings "ship only if the residue measurement
  justifies" after that measurement was deleted → rewritten to point at the
  deferral.
- `1637` phase 1's rule table listed `eng` with no indication it was deferred →
  gained a "ships here" column marking `ia`/`ua`/`eng` deferred.
- Phase 6's risk table still cited bare `session.rs:775-788` after the path was
  corrected in its Architecture section → now refers to the stack by name.
- Phase 3 still said "check the existing feature list first" after the check had
  been done, and hedged the crate name as "or the app crate's name" → both
  resolved (`Win32_Security` enabled; crate is `glowkey`).
- `plan.md` open question 2 asked what validation had just decided → struck
  through with the resolution.

**Unresolved contradictions: none.**
