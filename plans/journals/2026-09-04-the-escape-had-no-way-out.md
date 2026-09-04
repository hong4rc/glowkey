---
title: The escape had no way out
date: 2026-09-04
summary: "Mid-word spell-check escape was a one-way latch; deleting the offending key now undoes it. Two live reports on the same path, one already fixed."
---

# The escape had no way out

## What happened

Reported from live use: type `hoongf` → `hồng`, mistype `a` → `hoongfa`, press
Backspace → `hoongf`, stuck as literal keys.

`Engine::escaped` was a one-way latch. The mid-word spell check set it and only
the word emptying or a boundary cleared it. Deleting the key that caused it
re-evaluated nothing.

The part worth remembering: `backspace_visible_char` made it worse **by working
correctly**. While escaped the render *is* the raw keys, so dropping the `a`
reproduced the screen exactly, the function reported success, and the engine
stayed escaped. A correct function, an incorrect system — the bug was in the
state machine around it, and reading that function alone would never have found
it.

## The design constraint that shaped the fix

The obvious implementation was wrong. The host performs the Backspace, so
repairing meant posting an edit afterwards — which is exactly the mix of native
passthrough and synthesized edit that forced the full-suppression model. So the
tap suppresses the key and emits one edit covering the whole on-screen word, with
the user's delete accounted for inside it.

That forced the return type from `bool` to a three-way enum: `Repair` has to be
different *in kind*, because the caller must not also let the key through. A
boolean that sometimes meant "and apply this" is the sort of contract that gets
misread once and eats a character.

## Two process failures worth keeping

**I planned and then reported as if I had implemented.** The user came back with
"still not work" — correctly, because I had written a plan and no code. The plan
was right about the cause; I just stopped at the boundary and did not say so
clearly enough.

**`rustfmt` on `lib.rs` reformats the entire crate**, because it is the crate
root and rustfmt follows `mod` declarations. That silently reformatted
`english.rs` and `exclusion.rs` twice, reversing an explicit decision to leave
pre-existing drift alone — including exploding a hand-packed 51-word corpus one
word per line. `--skip-children`, or format leaf files only.

## The second report, which was not a bug

`hoongf a ⌫ ⌫ z` gave `hồngz`, and `hông` was wanted. Two things were tangled:

- `hồngz` was the **old build**. Installed and freshly built binaries had
  identical hashes, but the log ended `STARTUP quit at the Accessibility gate`,
  twice — the fix had never run. Worth checking before planning against a live
  observation.
- The rest was a real design fork: after the repair, should Backspace delete a
  visible character (`hồn`) or undo a keystroke (`hông`)? They diverge only at a
  tone key. Asked rather than guessed; the owner chose to keep visible-character
  deletion, so the answer was **no code**. Recorded in the handoff so it is not
  relitigated.

## Next

Review of this fix is still running. 172 tests, clippy silent, properties green
at 60k. The app is installed but has never been launched past the permission
gate — that grant is the only thing between the user and the fix.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
