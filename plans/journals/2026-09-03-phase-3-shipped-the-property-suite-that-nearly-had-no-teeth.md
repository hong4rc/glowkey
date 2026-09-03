---
title: "Phase 3 shipped: the property suite that nearly had no teeth"
date: 2026-09-03
summary: Property suite and latency budget landed; a review proved the model was blind to the exact edit the next plan rewrites.
---

# Phase 3 shipped: the property suite that nearly had no teeth

## What shipped

Phase 3 of the hardening plan: a generated-input property suite over the
engine's one invariant ("rendered == the text tail at the caret"), a measured
latency budget, and per-emit timing in the log. Two commits — `51471cf` (engine
+ suite) and `5c4cb6c` (app shell + timing). 134 tests green, clippy silent.

Numbers, measured for the first time in this project: **2 µs** per keystroke in
release, 9 µs in the test profile, worst single keystroke 2.54 µs. The engine's
re-derive-the-whole-word-every-key design costs almost nothing. A fast typist
leaves 100,000 µs between keys.

## The lesson worth keeping

The suite passed on its first run. The plan had said, in advance, "if it passes
on the first run with a wide generator, suspect the harness before believing the
engine" — so I mutated `diff()` to under-report backspaces by one. It failed and
shrank to `['o', 'o']`: screen `oô` versus engine `ô`. That is the exact `aa`→`aâ`
race that forced the full-suppression design years of sessions ago, rediscovered
from scratch by a shrinker in under a second.

I took that as proof the suite had teeth. It was proof that **one** path had
teeth.

The review then mutated `commit()` instead of `diff()` — under-deleting by one
on the auto-fix restore edit — and **all four properties stayed green**. My
model applied the restore edit and checked only that it did not delete *too
much*. An edit deleting too little stranded characters on screen, and because
`commit` clears the re-composition memory whenever it restores, nothing
downstream could ever have noticed.

That was the worst possible blind spot, because the whole point of the phase was
to gate `plans/260903-1637-.../phase-02-ascii-render-restore.md`, which rewrites
that exact function. The suite would have waved it through.

Fixed by determining the count rather than bounding it: a restore replaces the
*whole* rendered word, so its backspace count must equal that word's UTF-16
length, exactly. The reviewer's mutation now fails two tests with a precise
message.

**Generalisable:** proving a property suite has teeth on one mutation says
nothing about the others. Mutate every edit path the model applies, not the one
that first comes to mind. "I verified the harness works" was true and
insufficient, and the gap was in the half I did not test.

## Also caught by review

- Proptest persistence was silently off (an integration test cannot find
  `lib.rs`), so the plan's own flake mitigation was a comment describing
  something that never happened. Then my fix used a workspace-relative path,
  which quietly created `crates/glowkey-engine/crates/glowkey-engine/…` —
  `FileFailurePersistence::Direct` resolves from the crate root.
- Moving the log write after the decision inverted causality in the log and lost
  the KEY line if the emit path panicked. Reverted; timing moved into
  `emit_edit`, which also removed a false claim in the handoff that a slow
  keystroke means the AX guard (it could equally have meant "wrote a settings
  file").
- `note_boundary` missing from the model left ~9,000 auto-capitalize paths
  reported as covered and not covered.

## Left open, deliberately

- The live `EMIT took=` figures in Chromium versus a plain field need a granted
  build and a human typing. §6.1's "typ. sub-ms" is still an estimate — now
  marked as one rather than quietly presented as measured.
- "CI runs the property tests" is unverifiable: **the repo has no git remote**,
  so `ci.yml` has never run, and the engine job's fmt step fails on pre-existing
  drift in ~20 files anyway. Owner chose to leave both recorded and untouched.

## Next

Phase 6 (tap death) is the P1 half-day that fixes something a user hits today.
The typing-accuracy plan is now unblocked — its gate was these tests being
green.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
