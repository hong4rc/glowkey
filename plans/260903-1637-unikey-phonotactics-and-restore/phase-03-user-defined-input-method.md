---
phase: 3
title: "User-defined input method"
status: pending
priority: P3
effort: "3-5d"
dependencies: []
---

# Phase 3: User-defined input method

## Overview

`UkUsrIM` plus `x-unikey/doc/keymap-syntax`: a plain text file of
`<key> = <action>` lines, `;` for comments, against a fixed table of about
twenty-five named actions (`Roof-A`, `Hook-UO`, `Bowl`, `Tone1`…`Tone5`,
`D-mark`, `Telex-W`, and literal-character actions). Telex and VNI are two
instances of it.

The last unported UniKey feature. **Listed for completeness, not recommended** —
nothing has asked for it, and it is the largest change on the board.

## Requirements

- Functional: load a keymap file, validate it, and use it as a third input
  method alongside the built-in ones.
- Functional: a broken line is reported, not silently dropped — unlike the macro
  table, a wrong keymap makes the keyboard unusable rather than losing one entry.
- Non-functional: no cost to the built-in methods.

## Architecture

The blocker is concrete and already known: `vi::methods::Definition` is
`phf::Map<char, &'static [Action]>`, and **`phf` maps are built at compile
time**. Simple Telex was cheap precisely because it could be a `static`. A
user-supplied map cannot.

Options, none free:

1. **Own the dispatch.** Stop handing `vi` a `Definition` and interpret the
   action list ourselves, using `vi`'s `processor` primitives directly. Most
   control, largest change, and it puts us in the business of Vietnamese
   transformation that delegating to `vi` was meant to avoid.
2. **Translate to an existing method.** Express the user's map as a key
   rewriting into Telex keys, the way the bracket shortcuts already do. Cheap,
   but it inherits exactly the failure the brackets have — a substitution that
   is wrong when the target position already holds a vowel.
3. **Ask upstream.** `vi` taking a runtime map would make this trivial. Out of
   our hands and on someone else's schedule.

## Related Code Files

- Create: keymap parsing in `crates/glowkey-engine/`
- Modify: `InputMethod`, `render`'s definition selection, Settings
- Modify: `app/src/prefs_window.rs` — a file picker and validation feedback

## Implementation Steps

1. Decide between options 1 and 2 **before** writing anything; they share almost
   no code. Option 1 is the honest one if this is wanted at all.
2. Parse and validate the keymap format, with the action table as an enum.
3. Wire the chosen dispatch.
4. Settings: pick a file, report what failed, fall back to Telex when a map is
   unusable.

## Success Criteria

- [ ] A keymap reproducing Telex behaves identically to built-in Telex
- [ ] A malformed keymap is refused with a message naming the line
- [ ] Built-in methods are byte-identical to today

## Risk Assessment

**The real risk is building it at all.** It is days of work on the engine's
hottest path for a feature with no demand, and option 1 dissolves the boundary
that has kept the Vietnamese logic in `vi` and out of this codebase.
*Signal:* nobody asks for it before the other two phases are done.
*Response:* leave it unbuilt. This phase exists so the option is written down,
not so it gets picked up by default.
