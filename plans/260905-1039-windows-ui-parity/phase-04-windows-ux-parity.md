---
phase: 4
title: "Windows UX parity and docs"
status: in-progress
priority: P2
effort: "2h"
dependencies: [1, 2, 3]
---

# Phase 4: Windows UX parity and docs

## Overview
Close the loop: verify on the running build that every reported symptom is
gone, then update the documents that describe the Windows shell.

## Requirements
- Functional: the five acceptance criteria in `plan.md` hold on the real build.
- Non-functional: docs describe the new shape; the handoff's defect 2 is closed.

## Architecture
No new code beyond fixes found in verification.

## Related Code Files
- Modify: `docs/handoff.md` §5b (Windows shell shape, defect list),
  `docs/ui-design.md` (§1 About mapping for Windows, §2 Windows renderer note),
  `docs/manual-verification-windows.md` (new checks), `plans/reports/windows-handoff-260905.md`
  (defect 2 closed, open-at-launch honored), `plans/260904-2127-glowkey-cross-platform-port/plan.md`
  (Phase 5 note: shell now on a UI thread; unblock).

## Implementation Steps
1. Build release; stop the running GlowKey; start the new one (the startup
   entry points at `target/release/GlowKey.exe`).
2. Walk the checklist below by hand or, where it needs no keystrokes, by log:
   open Settings ×3, About, both together, Ctrl+Shift+Space with About open,
   Esc on About, Quit.
3. Fix anything found; rerun gates.
4. Update the four documents; add the checks to `manual-verification-windows.md`.
5. Journal.

## Success Criteria
- [ ] All five `plan.md` acceptance criteria confirmed on the running build,
      with the log lines that show it quoted in the phase report.
- [ ] Docs updated; `plans/reports/windows-handoff-260905.md` defect 2 marked closed.
- [ ] Gates green on both targets.

## Risk Assessment
- Manual steps need the user's hands for tray clicks (no synthetic input into the
  live session). Do what the log can show; list the rest for the user.
