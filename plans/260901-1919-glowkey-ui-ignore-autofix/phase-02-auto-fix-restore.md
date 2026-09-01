---
phase: 2
title: "Auto-fix restore"
status: completed
priority: P1
effort: "2-3d"
dependencies: [1]
---

# Phase 2: Auto-fix restore

## Overview

When a finished word is not valid Vietnamese, put back what the user actually
typed. `exit` typed in Telex mangles to `eĩt`; with auto-fix on, at the space
GlowKey deletes `eĩt` and types `exit`. Pure engine work, fully testable.

## Requirements

**Functional**
- At a word boundary (space, punctuation, Enter — anything that ends a syllable),
  if the composed word is **not** a valid Vietnamese syllable, produce a restore
  edit: delete the rendered word, insert the raw keystrokes.
- Gated by a `auto_fix` flag on the engine (from `Settings`).
- A valid word (`hồng`) is left untouched; only invalid results restore.
- The raw keystrokes are exactly what the user pressed (the engine already keeps
  the raw key log for backspace replay).

**Non-functional**
- No allocation on the common (valid) path beyond what exists.
- Validity uses the engine's existing check — `vi`'s syllable validator plus the
  tone×coda rule already discussed — not a new dictionary.

## Architecture

Today the engine treats a boundary as: reset + passthrough (the shell lets the
space through). Auto-fix inserts a check *before* the reset:

```
on boundary char, if auto_fix and !is_valid(current_word):
    emit a restore edit: KeyResponse { backspaces = current_word (utf16 len),
                                       insert = raw_keys as a string }
then reset, then let the boundary char through
```

The engine exposes this as a method the shell calls when it sees a boundary key,
returning an optional restore `KeyResponse`. The shell emits it (same
`(backspaces, insert)` path it already uses), then passes the boundary key
through.

Validity: the `vi` crate validates syllables; combine with the tone×coda rule.
Expose `is_current_word_valid()` on the engine, or fold it into a
`commit() -> Option<KeyResponse>` that returns `Some(restore)` when invalid and
auto-fix is on, else `None`.

The word `eĩt`: `ĩ` (tilde) before the stop coda `t` violates the tone×coda rule,
and `eĩt`'s structure is not a listed nucleus — so `is_valid` is false and it
restores. Confirm this exact case in a test.

## Related Code Files

- Modify: `crates/glowkey-engine/src/lib.rs` — `auto_fix` on `Session`/`Engine`;
  `commit() -> Option<KeyResponse>` (restore edit or none); raw-keys-to-string
- Modify: `crates/glowkey-engine/src/validate.rs` or wherever validity lives —
  ensure a public `is_valid_syllable`-style check the commit path can call
- Modify: `app/src/tap.rs` — on a boundary key, call `commit()`; if it returns a
  restore edit, emit it before letting the boundary key through
- Create/extend tests in `crates/glowkey-engine/tests/`

## Implementation Steps

1. Add `auto_fix: bool` to the engine session, set from `Settings`.
2. Expose a validity check for the current composed word.
3. Implement `commit() -> Option<KeyResponse>`: if composing and auto-fix and the
   word is invalid, return a restore edit (delete rendered, insert raw); always
   reset after. Return `None` when nothing to restore.
4. In `tap.rs`, change the boundary branch: call `commit()`; if `Some(edit)`, run
   it through the existing emit path, then return passthrough for the boundary key.
5. Tests:
   - `exit` (+ boundary) with auto-fix on → document shows `exit`
   - `eĩt`-producing sequence → restored to raw
   - `hoongf` (+ boundary) → `hồng`, unchanged (valid, no restore)
   - a batch of real Vietnamese words → none wrongly restored
   - auto-fix off → `exit` stays as the Telex result
   - drive it through the real-`CGEvent` tap harness too (like the existing tap tests)

## Success Criteria

- [ ] `exit` restores to `exit` at the boundary with auto-fix on
- [ ] `hồng` and a batch of real words are never restored
- [ ] auto-fix off leaves the Telex result in place
- [ ] The restore edit reconstructs correctly when applied (unit + tap-harness test)
- [ ] Engine still compiles and tests on Linux

## Risk Assessment

**The validity check rejects real Vietnamese**, restoring good words to raw.
Signal: a corpus word gets restored. Response: the check is the engine's existing
validator; add a real-word batch test and the `exit`/`eĩt` case before shipping,
and prefer under-restoring (leave a questionable word transformed) to
over-restoring (break a real word).

**Assumption at risk:** that the boundary is the right moment. Mid-word the user
still sees the mangled `eĩt` until they hit space. Signal: it feels laggy or
surprising. Response: that is the standard UniKey/EVKey behaviour and expected; a
mid-word manual undo key is a v2 option (open question 2), not this phase.

**Restore edit races like the Chrome bug.** Signal: leftover characters after a
restore in Chrome. Response: it uses the same single-channel session-posting emit
path; if that path has a residual race it is fixed once for all edits, not here.
