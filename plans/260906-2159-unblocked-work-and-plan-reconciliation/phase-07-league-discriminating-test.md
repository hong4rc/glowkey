---
phase: 7
title: "The League discriminating test"
status: pending
priority: P1
effort: "30m test; design only after it runs"
dependencies: []
---

# Phase 7: The League discriminating test

Executes [`260905-1643` §3.4](../260905-1643-glowkey-production-hardening/phase-3-correctness.md)
(B3, P1). That section is the design of record.

## Overview

GlowKey is unusable in League of Legends while EVKey is fine. The log has
already ruled out elevation and refused injection. Two candidates remain, and
they need opposite fixes:

1. A lone `w` becoming `ư` — an engine/policy problem, fixable by us.
2. Vanguard dropping injected input — an anti-cheat problem, likely not fixable
   at all.

**§3.4's instruction is explicit: run the discriminating test before designing
anything. Do not build the `w` option before running it.** This phase is that
test and nothing else.

Last of the seven because it is the only one needing something this environment
cannot provide: a Practice Tool game, and a person to play it.

## Requirements

- Functional: determine which of the two candidates is true, by one measurement.
- Non-functional: **no design work, no code, until the test has run.** The `w`
  option is the tempting one because it is fixable; that is exactly why §3.4
  forbids building it first.

## Architecture

**Currently worked around by exclusion**, and confirmed live 2026-09-06
(`#17213`, `#17216`): both `league of legends.exe` and `leagueclientux.exe` are
on the ignore list. So the product is not broken today — the user loses
Vietnamese in the game, which for a game is close to no loss. That is why this
is P1-diagnostic and not P0.

**The test, from §3.4.** In a Practice Tool game, with League **not** excluded,
Vietnamese **on**, and EVKey confirmed stopped: press **B** (then Y, M). Then a
Notepad control run immediately after.

`B`, `Y` and `M` are chosen because they are ordinary letters that open shop and
menu panels — they exercise injected input on keys with no Telex meaning, which
separates the two candidates:

| Observation | Verdict |
|---|---|
| `B`/`Y`/`M` work in game, only `w`-involving input misbehaves | Candidate 1 — the lone `w`. Ours to fix; then design the option |
| `B`/`Y`/`M` also fail in game, but the Notepad run immediately after is fine | Candidate 2 — Vanguard drops injected input. Not fixable by us; exclusion is the answer and becomes documented, not a workaround |

The Notepad control run is the part that makes this discriminating rather than
merely suggestive: it proves GlowKey was alive and injecting correctly seconds
either side of the game, so a failure inside the game is about the game.

**EVKey must be confirmed stopped**, not merely closed — two IMEs injecting into
the same window produce results that cannot be attributed to either.

## Related Code Files

- Read only: `app/src/default_exclusions/` — where League currently sits
- Create: `plans/reports/verification-260906-<time>-league.md`
- Modify: `plans/260905-1643-glowkey-production-hardening/phase-3-correctness.md`
  — record the §3.4 verdict
- **Modify nothing in `app/` or `crates/` in this phase**

## Implementation Steps

1. **Human only.** Needs a League client, a Practice Tool game, and someone to
   play it. An agent cannot supply any of the three.
2. Confirm EVKey is stopped — process gone, not just window closed.
3. Remove `league of legends.exe` and `leagueclientux.exe` from the ignore list
   for the duration of the test. **Restore them afterwards** regardless of
   outcome; the exclusion is the current working state.
4. Vietnamese on. Enter a Practice Tool game.
5. Press `B`, then `Y`, then `M`. Record for each whether the panel opened.
6. Alt-tab to Notepad immediately and type `hoongf`. Expect `hồng`. This is the
   control and it must run in the same sitting.
7. Capture the log for the whole window, including any `REACH` lines.
8. Record the verdict against the table above. Restore the exclusions.
9. Only now, if the verdict is candidate 1, write the design phase for the `w`
   option — with the measurement attached.

## Success Criteria

- [ ] The test ran with EVKey confirmed stopped and League not excluded
- [ ] `B`, `Y`, `M` outcomes recorded individually
- [ ] The Notepad control ran in the same sitting and its result is recorded
- [ ] A verdict names candidate 1 or candidate 2, with the log
- [ ] The exclusions are back in place
- [ ] No code changed in this phase
- [ ] §3.4's status in `260905-1643` reflects the outcome

## Risk Assessment

| Risk | Signal it broke | Response |
|---|---|---|
| The `w` option gets built because it is the fixable-looking candidate | A design or code change appears before the test result | §3.4 forbids it in writing, and this phase's success criteria require no code. Candidate 2 would make that work worthless |
| Exclusions left off after the test | Vietnamese misbehaves in the user's games afterwards | Step 8 restores them; it runs on every outcome, including an abandoned test |
| EVKey still running, results unattributable | Both IMEs' effects in one window; behaviour inconsistent between keypresses | Step 2 checks the process, not the window |
| The control run is skipped or deferred to another day | A game failure looks like proof of candidate 2 without evidence GlowKey was injecting at all | The control is what makes the test discriminating; same sitting, immediately after |
| Anti-cheat reacts badly to injection during the test | Warning, disconnect, or worse | Practice Tool, not a live match — as §3.4 specifies. If anything about the account is at risk, stop: exclusion already works and this is a diagnosis, not a fix |
