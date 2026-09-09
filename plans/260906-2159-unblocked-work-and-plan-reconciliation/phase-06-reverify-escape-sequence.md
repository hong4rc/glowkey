---
phase: 6
title: "Re-verify the hoongfa / ss sequence"
status: pending
priority: P2
effort: "1h, or 1-2d if it becomes a defect"
dependencies: []
---

# Phase 6: Re-verify the `hoongfa` / `ss` sequence

Executes [`260905-1643` §3.7](../260905-1643-glowkey-production-hardening/phase-3-correctness.md).
That section is the design of record.

## Overview

A user reported that `hoongfa` ␣ `ss` ⌫⌫⌫ did not restore `hồng`. The log showed
the feature working two keystrokes earlier, then all four backspaces coming back
`Passthrough` — meaning nothing was composing, so something flushed in between.

§3.7 says to re-run it once §3.5's flush logging exists. **§3.5 now exists**
(commit `b3940d8`), so this phase is unblocked and its whole purpose is to read
a log line that could not be read before.

This is a diagnostic phase with a fork at the end: it either explains the report
or reclassifies it as a real defect.

## Requirements

- Functional: run the exact reported sequence both without a click and with a
  click before the backspaces, and record what the flush log says.
- Functional: reach a verdict — flush-explained, or a re-composition defect.
- Non-functional: change no behaviour in this phase. If it is a defect, it gets
  its own phase with a proven cause first.

## Architecture

**The original evidence, from §3.7:**

```
#17610  a   Emit bs=3 ins=6u    hồng → hoongfa   (spell-check escape)
#17611  ⌫   Emit bs=7 ins=4u    hoongfa → hồng   (unescape) ✓
...
#17616-17619  ⌫  Passthrough ×4
```

`Passthrough` means nothing was composing. Something flushed between `#17615`
and `#17616`, and the leading hypothesis was a mouse click — which used to write
nothing at all to the log. That blind spot is what §3.5 fixed.

**What §3.5 gives this phase.** `flush_session_because` now logs
`FLUSH {cause} — composing word discarded` (`hook.rs:259`), gated on
`remembers_position()` rather than `is_composing` — deliberately, because a
flush also clears committed history, so a click *after* a word committed
silently removes the ability to restore it. §3.5's comment names this as "the
exact report this reporting was built for." `FlushCause::MouseButton` is the
variant to look for (`crates/glowkey-input/src/decision.rs:48`).

**The two runs, and what each outcome means:**

| Run | Expected | If it holds | If it does not |
|---|---|---|---|
| No click | Backspaces handled, delete within `ss` | The click was the cause; report explained | **The cause is not the click.** This becomes a real re-composition defect |
| With a click first | Log shows `FLUSH mouse-button`, then passthrough | Confirms the mechanism end to end | The flush is not being recorded, or not firing — a §3.5 gap |

**Where to look if it is a defect.** §3.7 names it: the committed-history stack
at `crates/glowkey-session/src/session.rs:775-788`, where a restored word clears
the stack deliberately (`self.committed.clear()` in the `restore.is_some()`
branch). That deliberate clear is the prime suspect, because the reported
sequence restores a word (`#17611`) *before* the failing backspaces.

§3.7 writes the location as bare `session.rs`, which resolves to nothing — there
is no `glowkey-engine/src/session.rs`, and three other `session*.rs` files exist.
Path corrected 2026-09-06 by verification; the line numbers were right.

## Related Code Files

- Read only: `app/src/platform/windows/hook.rs:259` — `flush_session_because`
- Read only: `crates/glowkey-input/src/decision.rs:40-60` — `FlushCause`
- Read only: `crates/glowkey-session/src/session.rs:775-788` — the deliberate
  stack clear, if the verdict is "defect"
- Create: `plans/reports/verification-260906-<time>-escape-sequence.md`
- Modify: `plans/260905-1643-glowkey-production-hardening/phase-3-correctness.md`
  — record the §3.7 verdict

## Implementation Steps

1. **Ask first; idle machine.** Same prerequisite as phases 4-5.
2. Enable whatever log verbosity shows per-key decisions, so `Emit` /
   `Passthrough` lines are visible as in the original capture.
3. **Run A, no click.** Type `hoongfa`, space, `ss`, then three backspaces —
   mouse untouched throughout. Record every log line from the space onward.
4. **Run B, with a click.** Same sequence, but click elsewhere in the text field
   before the backspaces. Expect `FLUSH mouse-button — composing word discarded`
   followed by passthrough.
5. Compare against the `#17610`-`#17619` capture in §3.7.
6. Verdict:
   - Run A passes, Run B shows the flush → **explained.** Record it, close §3.7,
     and note that the fix was the log line, not the behaviour.
   - Run A fails → **defect.** Do not fix it here. Capture the log, then read
     `crates/glowkey-session/src/session.rs:775-788` and write a new phase with a
     proven cause.
7. Report under `plans/reports/`; update §3.7's status either way.

## Success Criteria

- [ ] Both runs executed and logged, mouse state explicit for each
- [ ] Run B's log contains a `FLUSH` line naming `MouseButton`
- [ ] A verdict is recorded: explained, or defect with a captured log
- [ ] If defect: a new phase exists with a cause, and no behaviour was changed
      in this phase
- [ ] §3.7's status in `260905-1643` reflects the outcome

## Risk Assessment

| Risk | Signal it broke | Response |
|---|---|---|
| Run A is done with an accidental click, making a defect look explained | Log shows a `FLUSH` line during a run recorded as click-free | The flush log is now the check on the tester. Any `FLUSH` in Run A invalidates it — re-run |
| A defect gets "fixed" in this phase without a proven cause | A behaviour change appears in the diff for a diagnostic phase | Step 6 forbids it. §3.7's own instruction is to look at the committed-history stack *first* |
| Neither run reproduces anything, and the report is closed as unreproducible | Both runs pass, no flush, no failure | Not a pass. The original had a real log capture; an unreproducible result means the conditions differ — record what differed rather than closing it |
| The flush fires but is not logged, and that is read as "no flush" | Run B passes the backspaces through with no `FLUSH` line | That is a §3.5 gap, not a §3.7 result. Reopen §3.5 with the capture |
