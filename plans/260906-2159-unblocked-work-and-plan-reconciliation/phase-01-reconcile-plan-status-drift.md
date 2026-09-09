---
phase: 1
title: "Reconcile plan status drift"
status: pending
priority: P1
effort: "2-3h"
dependencies: []
---

# Phase 1: Reconcile plan status drift

## Overview

Make `plans/` tell the truth, so the next person planning work does not plan
something that shipped three commits ago. This phase writes no product code.

It is first because it is the phase that stopped this plan from containing a
fabricated one: a `phase-02-flush-logging.md` was scaffolded here for
`260905-1643` §3.5 before a `git log -S` showed §3.5 already shipped in
`b3940d8`. That file was deleted. Without this phase, the same mistake is
waiting in seven more plans.

## Requirements

- Functional: every `phase-*.md` status matches what the code actually does,
  verified against source or tests — never against a sibling document's claim.
- Functional: no file uses `status: complete`; the schema takes `completed`.
- Non-functional: `ak plan validate` exits 0 for every plan directory under
  `plans/`.
- Non-functional: the index agrees with the files (`ak plan reindex` after the
  edits, since files are source of truth and the store is a rebuildable view).

## Architecture

Three distinct defects, and they need different treatment:

**1. Invalid status value.** Six files in `260904-2127-glowkey-cross-platform-port`
say `status: complete`:

```
phase-00-engine-tests-on-windows.md   phase-03-reseat-macos.md
phase-01-neutral-input-policy.md      phase-04-windows-input-core.md
phase-02-hotkeys-and-app-identity.md  phase-05-windows-shell.md
```

Not a schema value, so tooling counts them as unfinished and they inflated the
original ~40 count. Straight substitution to `completed`.

**2. Plan says done, phases say pending.** In each case the *plan* status is
authoritative because it was updated on completion and the phase files were not:

| Plan | Phase files to correct | Evidence the work shipped |
|---|---|---|
| `260901-1919-glowkey-ui-ignore-autofix` | `phase-05` pending | plan.md `status: completed` |
| `260902-1230-glowkey-remaining-fixes-ux` | `phase-04-omnibox-deferred` pending | plan.md `status: completed`; name says deferred — confirm whether deferred means done or dropped |
| `260903-1531-unikey-telex-brackets-spellcheck` | `phase-01`, `phase-02` pending | `tests/telex_brackets.rs` and `tests/midword_spell_check.rs` both exist and pass; `set_telex_brackets` is a real API |
| `260905-1002-shared-settings-spec` | `phase-01-start` pending | plan.md "implemented and reviewed (2026-09-05)" |
| `260905-1039-windows-ui-parity` | `phase-01`, `phase-02`, `phase-03` pending | plan.md `status: completed` |
| `260902-1515-fix-known-issues` | check all | plan.md "completed (code + tests + docs; GUI/omnibox live verification pending user)" — the *code* is done, so a phase blocked only on live verification is not `pending` |

`260902-1515` is the shape to be careful with: "done but unverified" is a real
state and flattening it to `completed` loses the fact that a human still owes a
check. Prefer `completed` on the phase plus an explicit verification note, over
either extreme.

**3. A plan whose own status is wrong.** `260905-1643/plan.md` claims §3.5 "is
next and needs no hardware". It shipped in `b3940d8` (ancestor of `HEAD`;
`hook.rs:259` Windows, `macos/dispatch.rs:121` + `macos/mod.rs:199` macOS). Mark
§3.5 done in `phase-3-correctness.md`, correct the plan.md status line, and
re-cut the phase-3 work order so it does not open with completed work.

**Then the two genuinely-pending plans:**

- `260903-1637` — phases stay `pending`; only phase 1 changes, and it is being
  extended for the ưám fix, not closed.
- `260903-1745` — all 6 phases `in-progress` with nothing obviously in flight.
  Its signing/release/latency scope overlaps `260905-1643` phases 6 and 3.
  **Determine supersession, do not assume it.** Diff its 6 phases against 1643's
  and classify each as superseded (mark `cancelled` with a pointer) or residual
  (keep `pending`, and it needs a home).

## Related Code Files

- Modify: `plans/260904-2127-glowkey-cross-platform-port/phase-0[0-5]-*.md`
- Modify: `plans/260901-1919-glowkey-ui-ignore-autofix/phase-05-*.md`
- Modify: `plans/260902-1230-glowkey-remaining-fixes-ux/phase-04-*.md`
- Modify: `plans/260903-1531-unikey-telex-brackets-spellcheck/phase-0[12]-*.md`
- Modify: `plans/260905-1002-shared-settings-spec/phase-01-start.md`
- Modify: `plans/260905-1039-windows-ui-parity/phase-0[123]-*.md`
- Modify: `plans/260902-1515-fix-known-issues/` (audit, then correct)
- Modify: `plans/260905-1643-glowkey-production-hardening/plan.md`, `phase-3-correctness.md`
- Modify: `plans/260903-1745-glowkey-hardening-and-distribution/` (classify all 6)
- Create: nothing

## Implementation Steps

1. Fix the six `status: complete` → `completed` in `260904-2127`.
2. For each plan in defect group 2, verify the claim against code or tests
   before editing — the `260903-1531` entries were confirmed by opening
   `telex_brackets.rs`, not by trusting plan.md. Then set the phase status.
3. Audit `260902-1515` phase by phase; where a phase is code-complete but owes a
   human check, mark `completed` and record the outstanding verification in the
   phase's own text so it is not lost.
4. Correct `260905-1643`: §3.5 done, plan.md status line, phase-3 work order.
5. Diff `260903-1745`'s 6 phases against `260905-1643` phases 3 and 6. Classify
   each superseded (→ `cancelled`, with a pointer to the phase that replaced it)
   or residual (→ stays `pending`). Record the verdict in its plan.md.
6. Run `ak plan validate` on every plan directory; fix format failures.
7. Run `ak plan reindex` so the store matches the files.
8. Re-run the repo-wide sweep and confirm the only non-`completed` phases left
   are this plan's, `260903-1637`'s, `260905-1643`'s genuinely-open ones, and
   whatever `260903-1745` residue step 5 identified.

## Success Criteria

- [ ] `grep -rl "^status: complete$" plans/*/phase-*.md` returns nothing
- [ ] `ak plan validate` exits 0 for every directory in `plans/`
- [ ] Every status change is backed by a source or test reference recorded in
      the phase file, not by another document's assertion
- [ ] `260903-1745`'s 6 phases are each classified superseded or residual, with
      the verdict written down
- [ ] `260905-1643` no longer claims §3.5 is next
- [ ] The final sweep count matches the phase table in this plan's `plan.md`

## Risk Assessment

| Risk | Signal it broke | Response |
|---|---|---|
| Marking a phase `completed` that only *looks* done, hiding real work | A later phase depends on it and the code is not there | Every status change cites a file, symbol or test. A phase with no citable evidence stays as it is and gets listed in this plan's open questions instead |
| Flattening "code done, human check owed" to `completed`, losing the check | A release ships something never verified on a desktop | Keep the verification obligation in the phase text; `260902-1515` and `260905-1002` are both this shape |
| Declaring `260903-1745` superseded when it has residue | Signing or latency work silently disappears from the roadmap | Step 5 is a phase-by-phase diff, not a judgement on the plan as a whole. Residue keeps `pending` and surfaces as an open question |
| `ak plan reindex` overwrites a hand-edit | Statuses revert after reindex | Files are source of truth by design; reindex reads them. Run it last and re-check the sweep afterwards |
