---
phase: 2
title: "The Personal Words window"
status: pending
priority: P1
effort: "0.5d"
dependencies: [1]
---

# Phase 2: The Personal Words window

## Overview

A window listing every override, with its verdict, and a way to remove it. This
comes **before** the hotkey that writes to the list, on purpose: the writer is
only trustworthy once the user can see and undo what it wrote.

## Requirements

- Functional: every override is visible with both its keys and its verdict, in
  one place.
- Functional: an override can be removed, and one can be added by hand.
- Functional: a row's verdict can be flipped without deleting and re-adding it —
  changing your mind about a word is the common case, not an edge case.
- Non-functional: both languages, via `strings::t`. The Vietnamese interface
  option means an English-only string is a regression.
- Non-functional: the window reopens after being closed —
  `setReleasedWhenClosed(false)`, whose absence is invisible until someone closes
  a window twice (`docs/handoff.md` §9).

## Architecture

**Reuse the Macros window wholesale.** `app/src/prefs/macros_window.rs` already
solves exactly this shape: a scrolling stack of rows built from a `Vec`, an add
row with text fields, per-row Remove buttons tagged by index, and a `refresh_*`
that rebuilds after every mutation. The differences are two:

- the second column is a two-segment control (Typed / Vietnamese) rather than a
  free-text expansion field;
- there is no import or export (see the plan's non-goals).

Opened from Settings → Corrections, next to the English-restore checkbox it
supersedes — that is where a user goes looking when `was` comes out as `ứa`.

**Row identity.** The Macros window maps a Remove button back to its entry by
integer tag, and `prefs/mod.rs` holds the ordered list to resolve it. Copy that,
including the reason it exists: the button cannot carry a string, and rebuilding
the stack invalidates any index the button captured earlier.

**The caption matters as much as the control.** The English-restore checkbox's
caption currently has to explain a global trade-off. Once this window exists, the
honest caption is that the checkbox is the blunt default and this list is the
per-word answer.

## Related Code Files

- Create: `app/src/prefs/personal_words.rs` — the window, modelled on
  `macros_window.rs`.
- Modify: `app/src/prefs/mod.rs` — the controller ivar for the ordered list, the
  action methods (add, remove, flip), and the module declaration.
- Modify: `app/src/prefs/tabs.rs` — the button in the Corrections tab, and the
  reworded English-restore caption.
- Modify: `app/src/strings.rs` — nothing, unless a shared string appears; strings
  are picked at the call site by `t(english, vietnamese)`.
- Modify: `docs/handoff.md` §3 (the `prefs/` file map) and §6.3.

## Implementation Steps

1. Copy `macros_window.rs`'s structure; replace the expansion text field with the
   two-segment verdict control.
2. Wire add / remove / flip through the Phase 1 accessors, each saving as it
   changes (the house pattern — every `*_and_save` in `tap/settings.rs`).
3. Add the Corrections-tab button and reword the English-restore caption.
4. Both languages for every new string.
5. Verify by eye, then add a section to `docs/manual-verification.md`: add a
   word, restart, confirm it persisted; flip it, confirm the typing behaviour
   changed; remove it, confirm the behaviour reverted; close and reopen the
   window twice.

## Success Criteria

- [ ] Overrides are listed with their verdicts, and the list survives a restart
- [ ] Add, remove and flip all work and all persist
- [ ] Every string exists in English and Vietnamese
- [ ] The window reopens after being closed twice
- [ ] `docs/manual-verification.md` covers it
- [ ] Clippy silent, `cargo test --workspace` green

## Risk Assessment

- **All GUI here is unverifiable headless** (`docs/handoff.md` §6.4), which is
  why step 5 ends in the checklist rather than in a test. Nothing in this phase
  can be proved by CI; the engine half was proved in Phase 1.
- **A fourth window is one more thing to keep in step.** Settings, Excluded Apps,
  Macros and now Personal Words all need the same close/reopen fix and the same
  bilingual treatment. That is an argument for copying `macros_window.rs` closely
  rather than inventing a better structure here — divergence between the four is
  worse than duplication among them.
- **The Corrections tab could become the place options go to hide.** If the
  reworded caption does not make the relationship between the global checkbox and
  the list obvious, the user will find one and not the other. *Response:* the
  button belongs directly under the checkbox, not in a separate group.
