---
phase: 4
title: "Verify and document"
status: pending
priority: P2
effort: "1h"
dependencies: [1, 2, 3]
---

# Phase 4: Verify and document

## Overview
Capture every tab, every list window and About in light and dark, compare
against the findings table, and update the docs.

## Requirements
- Functional: each row of the findings table in `plan.md` has an after-capture
  showing the improvement.
- Non-functional: no input into the user's session; captures use `PrintWindow`
  and clicks posted to GlowKey's own windows (`scratchpad/shoot-tabs.ps1`
  pattern, not committed).

## Architecture
Dark capture: flip `HKCU\...\Personalize\AppsUseLightTheme` to 0, wait one
frame (the theme is re-read every frame), capture, flip back to 1. The user's
setting is restored within a second; note it in the report.

## Related Code Files
- Modify: `docs/ui-design.md` (Windows renderer notes), `docs/manual-verification-windows.md`
  (Tier 5: list windows, keyboard), `plans/reports/` (after-captures report)

## Implementation Steps
1. Build release; stop and start GlowKey (the user has asked for new builds to
   be started).
2. Capture: four tabs, three list windows, About; light then dark.
3. Write `plans/reports/verification-<ts>-windows-settings-ux-polish.md` with
   before/after per finding.
4. Docs.
5. Journal.

## Success Criteria
- [ ] After-captures for all 15 findings, light and dark.
- [ ] Docs updated.
- [ ] Gates green on both targets.

## Risk Assessment
- Flipping the theme key is a change to the user's machine, even for a second;
  restore in a `finally`. If the user objects, capture light only and list
  dark as a user check.
