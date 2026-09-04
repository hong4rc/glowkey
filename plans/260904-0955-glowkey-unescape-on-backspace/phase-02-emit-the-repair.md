---
phase: 2
title: "Emit the repair from the tap"
status: completed
priority: P1
effort: "2h"
dependencies: [1]
---

# Phase 2: Emit the repair from the tap

## Overview

Apply the engine's repair on screen. This is the delicate half: it turns the
Delete key from a passthrough into a suppression, on the one code path where
getting it wrong deletes the user's text.

## Requirements

- Functional: when the engine answers `Repair`, the tap suppresses the Backspace
  and emits the edit — the user's delete is accounted for *inside* that edit.
- Functional: the other two answers behave exactly as today (pass through, and
  flush-then-pass-through).
- Non-functional: one edit, one ordered post from GlowKey's tagged source. No
  native keystroke and synthesized edit for the same action.
- Non-functional: the repair goes through `emit_edit`, so it inherits the circuit
  breaker and the Chromium omnibox guard like every other edit.

## Architecture

**Why suppression, not passthrough plus an edit.** `docs/handoff.md` §5 records
the race that forced the full-suppression model: a natively-typed character and a
synthesized backspace posted a moment later reach the document out of order in
multiprocess applications, which is how `aa` once became `aâ`. Letting the host
delete and *then* posting the repair reintroduces exactly that shape — worse
here, because the repair's backspace count is computed against a screen state the
host may not have reached yet.

So the Delete branch in `app/src/tap/decide.rs:275` gains one arm:

```rust
match session.backspace_visible_char() {
    BackspaceOutcome::Repair(edit) => Decision::Emit(edit),   // suppress the key
    BackspaceOutcome::InStep       => Decision::Passthrough,  // as today
    BackspaceOutcome::Flush        => { session.flush(); Decision::Passthrough }
}
```

The existing `recompose_after_boundary_backspace` check stays ahead of it,
unchanged and still first: deleting the boundary after a committed word is a
different thing from editing inside one.

**Exhaustive match, no catch-all.** A `_ =>` arm here would silently pass a
future fourth outcome through as a plain delete, which is the failure mode this
phase is most exposed to.

## Related Code Files

- Modify: `app/src/tap/decide.rs` — the `KEY_CODE_DELETE` branch.
- Modify: `app/src/tap/tests.rs` — a decision test with a real Delete `CGEvent`.
- Modify: `docs/handoff.md` — §4's mid-word spell check entry gains the exit, and
  the Backspace behaviour list gains this case.
- Modify: `docs/manual-verification.md` — the sequence, in §3's options section.

## Implementation Steps

1. Add the arm, exhaustively.
2. Add the decision test: type `hoongf`, then `a`, then a real Delete event, and
   assert `Decision::Emit` with `backspaces` equal to the on-screen word's UTF-16
   length — not `Passthrough`.
3. Add a second test asserting the unescaped path still returns `Passthrough`, so
   the ordinary mid-word Backspace is not quietly changed for everyone.
4. Update the docs.
5. Live check, which is the only way to see the thing the tests cannot: type the
   reported sequence in TextEdit **and** in Chrome's address bar — the repair
   emits backspaces, so it goes through the omnibox guard.

## Success Criteria

- [x] The reported sequence restores `hồng` on screen in a real app
- [x] The Backspace is suppressed exactly once — no doubled delete, no leftover
      character
- [x] An ordinary mid-word Backspace on an unescaped word is unchanged
- [x] The repair works in Chrome's address bar as well as a plain field
- [x] `cargo test --workspace` green, clippy silent
- [x] `docs/manual-verification.md` covers it

## Risk Assessment

- **A doubled delete.** If the key is both suppressed and somehow performed, or
  if `backspaces` is computed against the post-delete screen, the word loses a
  character. *Signal:* `hồn` instead of `hồng` in the live check. *Response:* the
  count comes from the engine's pre-delete `rendered`, and the key is suppressed
  by returning `Emit` — the two must be changed together or not at all.
- **The Chromium omnibox.** This adds a second path that emits backspaces, so the
  accessibility guard's known residual race (§6.1) now applies to it too. Nothing
  new in kind; it is a second place that guard matters, and the live check covers
  it explicitly.
- **A future fourth outcome falls through.** *Response:* no catch-all arm, so the
  compiler stops it.

## Outcome — 2026-09-04

Done. The `KEY_CODE_DELETE` branch matches exhaustively with no catch-all arm;
`Repair` returns `Decision::Emit`, which suppresses the keystroke.

End-to-end against the owner's real settings (`strict_spell_check` and
`quick_telex` on):

```
type hoongfa  ->  "hoongfa"
Backspace     ->  Repair: suppress the key, emit bs=7 ins="hồng"
                  screen "hồng"   engine "hồng"
then type s   ->  "hống"    <- still composing, tone key still applies
```

That last line is as important as the fix: the word comes back **live**, not as
dead literal text, so typing continues into it.

Two decision tests pin the mechanism rather than the result: the escaped case
must return `Emit` with a backspace count covering the whole on-screen word, and
the unescaped case must still return `Passthrough` — so the common path, which is
every user with the spell check off, is provably untouched.
