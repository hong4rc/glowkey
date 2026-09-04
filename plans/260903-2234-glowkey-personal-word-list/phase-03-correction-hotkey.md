---
phase: 3
title: "The correction hotkey"
status: completed
priority: P1
effort: "1d"
dependencies: [1, 2]
---

# Phase 3: The correction hotkey

## Overview

One keystroke, pressed just after a word, that swaps it to the other reading
**and records the choice**. This is the "learns" half, and it learns from an
explicit act rather than a guess.

Type `was`, get `ứa`, press ⌃⇧W: the screen becomes `was` and `was` is pinned to
its raw keys forever. The next time, no keystroke is needed.

## Requirements

- Functional: the hotkey swaps the last committed word between its raw keys and
  its Vietnamese render, in whichever direction it is not currently in.
- Functional: it writes the resulting preference to the Phase 1 list, so the
  correction is permanent.
- Functional: it is inert — no edit, no write — when there is no remembered word,
  which includes after any flush, caret move, mouse click, app switch, or a
  second press.
- Functional: it never fires in an excluded application, and never while a
  hotkey recording is armed.
- Non-functional: the edit obeys the full-suppression invariant. Backspaces and
  the replacement come from GlowKey's own tagged source in one ordered post, like
  every other edit (`docs/handoff.md` §5).
- Non-functional: nothing is written to the list except by this keystroke or the
  window in Phase 2.

## Architecture

**The memory this needs does not exist yet.** `Session::last_committed`
(`lib.rs:1420`) is set **only when no restore happened** — it exists for
re-composition (`hồng`␣⌫`z`), and a word that auto-fix already restored is
deliberately not re-composable. The correction hotkey needs the opposite: the
word that *was* restored is exactly the one the user most often wants to correct.

So this phase adds a separate, wider memory, set on **every** commit:

```rust
/// The word just committed, remembered for the correction hotkey.
struct CorrectableWord {
    /// The raw keys as typed.
    raw: String,
    /// The Vietnamese rendering.
    rendered: String,
    /// Which of the two is on screen right now.
    on_screen: WordPreference,
    /// UTF-16 length of the boundary character that followed it, so the edit can
    /// step back over the space before replacing the word.
    boundary_units: usize,
}
```

Cleared by everything that already clears `last_committed` — flush, caret move,
mouse-down, app switch, a new word starting — because all of those mean the
caret may no longer be just after that word, and the blind model has no way to
check. It is also cleared by the correction itself: pressing the key twice must
not toggle back and forth, because the second press would be recorded as a new
preference and the list would learn whichever direction the user stopped on by
accident.

**The edit.** The word is behind a boundary character the user typed after it, so
the swap is: delete the boundary and the on-screen word, insert the other form,
re-insert the boundary. As a single `KeyResponse`-shaped edit, that is
`backspaces = on_screen.len_utf16 + boundary_units`, `insert = other_form +
boundary_char`. One edit, one post, ordered — no separate replay step, which is
what the auto-fix boundary path needed `EmitThenReplayKey` for.

**Why a fixed ⌃⇧W and not a configurable one.** ⌃⇧E, the per-app toggle, is
fixed; only the VN/EN toggle is configurable, because that is the one people press
constantly and hold opinions about. Adding a second recorder for a key pressed a
few times a day is machinery for its own sake. The recorder already refuses ⌃⇧E
as reserved (`tap/decide.rs`); ⌃⇧W joins it in that list, or a user could record
the correction key as their VN/EN toggle and lose both.

## Related Code Files

- Modify: `crates/glowkey-engine/src/lib.rs` — `CorrectableWord`, set it in
  `commit`, clear it everywhere `last_committed` is cleared, and a
  `correct_last_word() -> Option<KeyResponse>` that performs the swap and records
  the preference.
- Modify: `app/src/tap/keys.rs` — `KEY_CODE_W`, and `is_correction_hotkey`.
- Modify: `app/src/tap/decide.rs` — the branch, ahead of the shortcut filter like
  the other ⌃⇧ hotkeys; and add ⌃⇧W to the recorder's reserved list.
- Modify: `app/src/hud.rs` — a brief flash naming what was learned, so the write
  is not silent.
- Modify: `crates/glowkey-engine/tests/word_overrides.rs`, and the tap decision
  tests in `app/src/tap/tests.rs` (a real `CGEvent` with ⌃⇧W).
- Modify: `docs/handoff.md` §4, §6.3; `docs/manual-verification.md`.

## Implementation Steps

1. Add `CorrectableWord` and set it on every commit. Find every site that clears
   `last_committed` (there are nine) and decide for each whether it must clear
   this too — the default answer is yes, and any exception needs a written reason.
2. Add `correct_last_word`, returning the edit and writing the preference.
3. Unit-test the engine half first, without any tap involved: commit a word,
   correct it, assert the edit shape and that the override now exists; correct
   twice and assert the second is inert.
4. Add the tap branch and a decision test with a real ⌃⇧W event.
5. Add the HUD flash and add ⌃⇧W to the recorder's reserved keys.
6. Run the property suite. The correction is a new kind of edit, and the suite's
   exact-backspace assertion applies to it as much as to a restore.
7. Verify by hand: the four cases in Success Criteria, then the two that must do
   nothing — pressing it after clicking elsewhere, and pressing it twice.

## Success Criteria

- [x] `was`␣ then ⌃⇧W gives `was ` on screen, and `was` is in the list as Raw
- [x] Typing `was`␣ again gives `was` with no keystroke
- [x] `cát`␣ then ⌃⇧W gives `cats ` and pins Raw; pressing it on `cats`␣ pins
      Vietnamese — the direction follows what is on screen, not a fixed side
- [x] A second press does nothing
- [x] After a mouse click, a caret key, or an app switch, the key does nothing
- [x] It does nothing in an excluded app
- [x] The HUD says what was learned
- [x] `tests/properties.rs` still green, clippy silent

## Risk Assessment

- **The word may not be where the engine thinks it is.** This edit reaches back
  over a boundary character into already-committed text, which is further than
  anything else in GlowKey goes. The blind model cannot verify it. *Signal:* a
  correction that eats a character of the preceding word, or lands inside the
  wrong word. *Response:* the memory is cleared by everything that could move the
  caret, and the key is inert without it — the failure mode is "does nothing",
  which is the right one. If a case is found where the caret moved without
  clearing, that clearing is the bug, not the edit.
- **The Chromium omnibox.** This emits backspaces, so it goes through the same AX
  trailing-selection guard as every other edit, with the same known residual race
  (§6.1). Nothing new, but it is a second place that guard now matters.
- **Learning the wrong direction from a double press.** Addressed by clearing the
  memory after a correction, so the second press is inert rather than a second
  vote. This is the specific reason the memory is one-shot.
- **The user cannot find what it learned.** Phase 2 exists first for this reason,
  and the HUD flash means the write is announced rather than silent. If the flash
  proves annoying it can go; the window cannot.

## Review — 2026-09-04

`code-reviewer` found **three reachable document-corruption paths** in this
phase, all fixed. Phases 1 and 2 came back clean.

**The word stayed re-composable after being corrected.** The correction cleared
its own memory but left `last_committed` describing a word no longer on screen,
so the next Backspace reopened the *old* rendering and the letter after it was
diffed against a baseline that no longer matched: `was `⌃⇧W⌫`f` produced `wừa`.
Three ordinary keystrokes, and it always corrupted. Fixed by routing the
correction through `forget_last_word()` — the tenth clear-site, which I had added
and not routed through the helper I wrote for exactly this reason.

**Keys that insert nothing were charged as boundaries.** Escape, the function
keys, keypad Enter, Help and forward-delete all reach the boundary branch as
control characters while putting nothing at the caret, so the edit deleted one
unit too many — eating the space belonging to the *previous* word and typing a
control code into the document (`xin chào ứa` + Escape + ⌃⇧W → `xin chàowas␛`).
Pressing Escape to dismiss a popover is routine, and every function key does it.

**Tab and Return moved the caret entirely.** Both are control characters too, and
both are worse than a miscount: after Tab the edit posts into the next field
(deleting up to three characters GlowKey never wrote), and after Return in a
send-on-enter application into a message already sent. One rule covers all three
findings — a word is only correctable when the boundary key actually inserted
something — and it is the conservative reading of the blind model.

Also fixed: the decision was **never written to disk** (`decide` is deliberately
free of disk side effects, so it now flags `handle_key_down` to save), which
falsified this phase's headline claim; an app that activates itself did not clear
the memory although `forget_last_word`'s own comment said it did; the `on_screen`
enum could not express a third restored string and would have broken silently
when the ASCII-render restore lands on the same function, so it stores the text
itself; the `'\0'` sentinel became `Option<char>`, which removes the call-order
requirement rather than documenting it; one mistyped verdict in a hand-edited
settings file discarded every other setting; and the HUD now names both readings.

### The harness lesson, which is the real one

None of it was caught, and the reason was structural rather than luck:
`correct_last_word` had **no property coverage at all**. Adding it exposed two
further layers of self-deception, both mine:

1. The model pushed control characters onto the screen — sharing the engine's
   wrong belief, so there was nothing to disagree about. A model that makes the
   same assumption as the code under test cannot falsify it.
2. Once that was fixed the mutation still passed, because the model called
   `commit()` a second time to assert double-commit was inert — and that call
   clears the correction memory. Every correction check downstream silently
   became "nothing to correct" and returned OK. **A probe that destroys the state
   it is probing is worse than no probe, because it looks like coverage.** The
   double-commit assertion now lives in its own test.

With both fixed, re-introducing either critical bug fails the suite by name.
