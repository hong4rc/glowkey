---
phase: 4
title: "Omnibox delivery (deferred)"
status: pending
priority: P2
effort: "unknown"
dependencies: []
---

# Phase 4: Chrome address-bar (omnibox) delivery — DEFERRED

## Overview
`hoongf → hoồng` / `work → ưwork` in Chrome's address bar. Root cause
(investigated): the omnibox's inline-autocomplete adds a trailing SELECTION;
GlowKey's synthetic Backspace deletes that selection instead of the character,
so transforms corrupt. Works in every normal field.

## Why deferred
Every viable fix (post Left-arrow to collapse the selection before deleting;
select-back-N then replace) also changes behavior in normal fields that have NO
selection — Left would move the caret and corrupt normal typing. GlowKey is a
blind model (can't read caret/selection), so no safe *universal* key exists. The
correct-but-large fix is the InputMethodKit composition path, which contradicts
the CGEventTap "wrap" architecture. None of this can be verified headless.

## Decision
Do NOT ship a blind heuristic while unverified — it risks the working
normal-field case (the primary feature). Revisit WITH the user present:
1. Confirm the autocomplete-selection mechanism (type a no-suggestion prefix).
2. Prototype the Left-collapse-before-delete guarded to the omnibox, verify it
   doesn't regress normal fields, keep only if clean.

## Success Criteria
- [ ] Live-verified fix that leaves normal fields intact, OR a documented
      accepted limitation (as EVKey has).

## Risk Assessment
High blast radius on the core feature; unverifiable headless. Deferral is the
safe choice.
