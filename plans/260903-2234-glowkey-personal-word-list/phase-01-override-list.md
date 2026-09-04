---
phase: 1
title: "The override list"
status: completed
priority: P1
effort: "0.5d"
dependencies: []
---

# Phase 1: The override list

## Overview

A per-word verdict, persisted, consulted at the word boundary, that beats every
rule. Useful on its own: editing `settings.json` by hand already fixes `was`
without making `cát` untypeable.

## Requirements

- Functional: a word can be pinned to **its raw keys** (`cats` stays `cats`) or
  to **its Vietnamese render** (`cats` stays `cát`), independently of `auto_fix`,
  `restore_english_words`, and the curated list in `english.rs`.
- Functional: a macro still wins. `commit` checks macros first today and that
  order is right — a shortcut the user defined explicitly is a stronger
  statement than a word preference.
- Functional: an override that would change nothing emits nothing, exactly as
  the existing restore does (`raw != rendered`).
- Non-functional: unknown and missing keys in `settings.json` stay tolerated
  (`config.rs` already guarantees this), so an old settings file gains an empty
  list rather than failing to load.
- Non-functional: lookup is on the per-word commit path, not the keystroke path,
  but it must not be linear over a list that could grow to thousands.

## Architecture

**The type.** Modelled on `Macro`, which is the closest existing thing — a small
serde struct in the engine, persisted in `Settings`, edited through `Session`:

```rust
/// What the user decided for one set of typed keys.
pub enum WordPreference {
    /// Keep what was typed: `cats` stays `cats`.
    Raw,
    /// Keep the Vietnamese rendering: `cats` becomes `cát`.
    Vietnamese,
}

pub struct WordOverride {
    /// The raw keys, lowercased — the same normalisation `is_common_english` uses.
    pub keys: String,
    pub prefer: WordPreference,
}
```

Keyed on the **raw keys**, not the render, because the raw keys are what the
ambiguity is about: one key sequence, two readings. `cats` is the question;
`cats` and `cát` are the two answers.

**Where it plugs in.** `Session::commit` (`lib.rs`, the `let restore = …` block).
The decision becomes, in order:

1. a macro matches → expansion (unchanged, already first);
2. an override for these keys → obey it, full stop;
3. otherwise the existing rule: `auto_fix && is_invalid_vietnamese(rendered)`, or
   `restore_english_words && is_common_english(raw)`.

Step 2 must produce exactly the same **shape** of edit as step 3 — backspaces
equal to the rendered word's UTF-16 length, insert equal to the replacement —
because `crates/glowkey-engine/tests/properties.rs` now asserts that shape
exactly. An override that got the count wrong would fail the property suite
rather than silently stranding characters, which is the point of having it.

**Storage.** `Vec<WordOverride>` in `Settings` for the file (stable, diffable,
matches `macros`), and a `HashMap<String, WordPreference>` built once in
`Session::from_settings` for the lookup. The vector is the format; the map is the
index. Same relationship as the plan files and `plans.db`.

## Related Code Files

- Modify: `crates/glowkey-engine/src/lib.rs` — `WordPreference`, `WordOverride`,
  the map on `Session`, the branch in `commit`, and the accessors the UI will
  need (`word_overrides`, `set_word_override`, `remove_word_override`).
- Modify: `crates/glowkey-engine/src/config.rs` — the `word_overrides` field,
  `#[serde(default)]`, and its place in `Default`.
- Create: `crates/glowkey-engine/tests/word_overrides.rs`.
- Modify: `app/src/tap/settings.rs` — `*_and_save` accessors, matching the
  existing macro ones.

## Implementation Steps

1. Add the two types and the `Settings` field. Add the round-trip test and the
   old-file test (a settings JSON with no `word_overrides` key must load).
2. Build the map in `from_settings`; make `snapshot` write the vector back.
3. Add the branch to `commit`, ahead of the auto-fix/English decision.
4. Tests, and the pairs that matter are the ones that currently cannot both work:
   - `was` pinned `Raw` → `was`, with the global toggle **off**
   - `cats` pinned `Vietnamese` → `cát`, with the global toggle **on**
   - `exit` pinned `Vietnamese` → `eĩt`, proving an override beats auto-fix too
     (perverse, but it is the user's call and the code must not second-guess it)
   - a macro for the same keys still wins
   - an override whose two forms are identical emits nothing
5. Run the property suite. It must stay green: the override path goes through the
   same edit shape as every other restore.

## Success Criteria

- [x] All five test pairs above pass
- [x] `cargo test --workspace` green including `tests/properties.rs`
- [x] An old `settings.json` loads and gains an empty list
- [x] Lookup is a map, not a scan
- [x] Clippy silent

## Risk Assessment

- **The override is keyed on the wrong thing.** If a user expects `Cats` to obey
  the `cats` override and it does not, the feature feels broken. *Signal:* the
  first hand-test with a capitalised word at a sentence start. *Response:*
  lowercase on both write and read (planned), and if that turns out to be wrong
  in the other direction — someone wanting `US` and `us` to differ — the list
  gains a case column rather than the lookup gaining a heuristic.
- **An override that fights auto-fix produces something invalid.** Pinning `exit`
  to Vietnamese means the user gets `eĩt`. That is not a bug; it is an explicit
  instruction, and the code must not quietly refuse it. The UI in Phase 2 is
  where it becomes visible and removable.
- **The list grows without bound.** Phase 3 writes to it one word at a time.
  A map lookup does not care, and the file stays diffable. If it ever needs
  pruning, that is a real feature request and not a guess to make now.
