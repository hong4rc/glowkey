---
title: Review caught two real bugs in the tap health monitor
date: 2026-09-03
summary: "Phases 1-6 delivered; review found a possible double tap and a broken blind invariant, both fixed. New plan: a personal word list that learns."
---

# Review caught two real bugs in the tap health monitor

## What happened

Delivered the remaining five phases of the hardening plan (signing, release,
module split, onboarding, tap death), then a review of the lot found three
high-severity defects — two of them mine in the same phase.

## The one that mattered

**Nothing flushed the engine when the tap came back.** I added a health monitor
that rebuilds the tap after the Accessibility permission returns, and never
considered what the engine believed in the meantime. A dead tap means keys reach
the document **natively, unsuppressed**, while `Session` keeps the composing raw
log and render from before the gap — which is the blind model's one invariant
(`rendered == the text tail at the caret`) broken by construction.

Concretely: type `hoo` (render `hô`), lose the permission mid-word, type `ngf`
which lands literally so the document reads `hôngf`, re-grant. The next letter is
diffed against the stale `hô` and the emitted backspaces delete characters the
user typed themselves.

The reviewer also pointed out why the usual safety net could not save it:
everything else that moves the caret behind GlowKey's back is caught by a flush
on mouse-down or a caret key — but those arrive *through the tap*, and the tap is
exactly what was dead. Nothing else was ever going to notice.

**Generalisable:** when adding a recovery path, ask what stale state survived the
outage, not just whether the mechanism comes back up. I was thinking about the
tap's liveness and not at all about the engine's memory of a word.

## The second one

`create_tap` used `if let (Ok(..), Ok(..))` for its teardown. That reads like a
guard and is not one — it skips the cleanup **and carries on**, so a failed
borrow would attach a second tap to the same run loop (every keystroke processed
twice, every edit applied twice) and drop the only handles able to remove the old
source. Now a `let … else` that logs and refuses; the next tick retries two
seconds later. Refusing beats a double tap.

`if let` on a tuple of `Result`s is worth distrusting on sight when the body is
cleanup: the failure branch is invisible and the fall-through is the dangerous
path.

## Also fixed

- The welcome alert never called `activate()` — the only window in the app that
  did not — so it could open behind whatever the user was looking at, and on the
  already-trusted path it runs before `app.run()` with no `finishLaunching()`,
  which the permission gate's own comments record as "runs but draws nothing".
  A modal that draws nothing ahead of the run loop reads as a hang.
- The re-enable branch logged on every tick: ~43,000 lines a day, and the log's
  size cap is evaluated once per process, so a long-running agent would grow the
  file without bound. My own success criterion said "once per transition".
- A trusted-but-unrecoverable tap looped forever with the glyph still claiming
  VI — the exact lie the phase exists to end, in the one branch it did not cover.
- `security find-identity | grep -qF` under `pipefail`: `grep -q` exits at the
  first match without draining, so SIGPIPE can make the pipeline report failure
  **when the identity matched**, silently falling back to ad-hoc signing. That is
  the precise problem the signing phase exists to remove.

## New plan

`plans/260903-2234-glowkey-personal-word-list/` — three phases for the one
limitation the handoff calls inherent: English/Telex ambiguity. Today it is a
global on/off switch whose trade-off list runs to a paragraph. The plan makes it
per-word: an override list, a window to inspect it, and a ⌃⇧W that fixes the word
just typed and remembers the choice.

Deliberate non-goal: inferring intent from undo patterns. Every write comes from
an explicit act. And the UI ships **before** the learning, so the user can see
and undo what the hotkey wrote.

Found while designing it: `last_committed` is set only when no restore happened,
so the correction hotkey needs its own wider memory — the word that *was*
auto-fixed is exactly the one the user most wants to correct.

## Next

Fix nothing else here; run `manual-verification.md` §1 and §9, which now test
both fixed defects.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
