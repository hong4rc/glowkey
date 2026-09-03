---
phase: 1
title: "Telex bracket shortcuts"
status: pending
priority: P2
effort: "4h"
dependencies: []
---

# Phase 1: Telex bracket shortcuts

## Overview

UniKey's full Telex maps four keys straight to vowels: `[`→ơ, `]`→ư, `{`→Ơ,
`}`→Ư (`inputproc.cpp:99`, `TelexMethodMapping`). GlowKey has no equivalent —
verified: the engine renders nothing for `[`, `]`, `t[` or `d]`, and the tap
would not deliver them anyway because `is_word_char` accepts only ASCII letters
plus digits in VNI, so a bracket is a word boundary that flushes the word.

Ships behind an opt-in checkbox, off by default.

## Requirements

- Functional: with the option on and the method Telex, `[`/`]`/`{`/`}` produce
  ơ/ư/Ơ/Ư anywhere in a word, and a tone key applied afterwards still works
  (`[f` → ờ).
- Functional: with the option off, or under VNI, behaviour is exactly today's —
  brackets are word boundaries and type themselves.
- Non-functional: no new per-keystroke cost beyond the existing raw-log
  re-derivation.

## Architecture

The engine already pre-translates keys before handing them to the `vi` crate —
that is how Quick Telex works. Brackets use the same seam:

```
'[' → keys "ow"    ']' → keys "uw"
'{' → keys "OW"    '}' → keys "UW"
```

Substituting Telex *keys* rather than inserting the character `ơ` is the point:
it keeps everything inside the Telex alphabet, so a later tone key still lands.
Verified by probe: `ow`→ơ, `OW`→Ơ, `uw`→ư, `UW`→Ư, `tow`→tơ, `Tow`→Tơ.

Ordering against Quick Telex, which inspects the first two raw keys: **Quick
Telex expands first, brackets second.** Quick Telex is about the literal doubled
keystroke the user made; running bracket substitution first would change the
pair it is looking at.

The tap must also stop treating brackets as boundaries — but only when the
option is on and the method is Telex, or `[` stops typing a bracket for
everyone, which is exactly what the opt-in exists to avoid.

## Related Code Files

- Modify: `crates/glowkey-engine/src/lib.rs` — `Engine` field + setter mirroring
  `quick_telex`; the substitution inside `render()`; `Session` pass-through
- Modify: `crates/glowkey-engine/src/config.rs` — `telex_brackets: bool`,
  `#[serde(default)]`, plus the two existing `Settings` initializers
- Modify: `app/src/tap.rs` — `is_word_char` and `TapState` accessors
- Modify: `app/src/prefs_window.rs` — checkbox and caption under Typing
- Create: `crates/glowkey-engine/tests/telex_brackets.rs`

## Implementation Steps

1. Add `telex_brackets` to `Settings` (defaulted) and to both existing `Settings`
   literals, including the round-trip test in `config.rs`.
2. Add the engine field, `set_telex_brackets`/`telex_brackets`, and `Session`
   pass-throughs, mirroring `quick_telex` exactly.
3. Add `expand_telex_brackets(raw) -> Vec<char>` and call it in `render()` after
   `expand_quick_telex`, gated on the flag **and** `method == InputMethod::Telex`.
4. Widen the tap's `is_word_char` to accept the four brackets when Telex and the
   option is on; add `TapState::telex_brackets` / `set_telex_brackets_and_save`.
5. Add the Settings checkbox with a caption naming the cost ("`[` and `]` stop
   typing brackets").
6. Write the tests, then run the app and type `[`, `]`, `{`, `}`, `t[`, `[f`.

## Success Criteria

- [x] `[`→ơ, `]`→ư, `{`→Ơ, `}`→Ư with the option on under Telex
- [x] `[f`→ờ — a tone key still applies to a bracket-produced vowel
- [x] `t[`→tơ — works mid-word, not only at the start
- [x] Option off: every bracket input renders exactly as it does today
- [x] VNI with the option on: brackets unchanged
- [x] Quick Telex and brackets together behave per the stated order
- [x] Tap: with the option off `[` still flushes the word; with it on it does not
- [x] Clippy silent, whole suite green

## Risk Assessment

**The substitution is an approximation of a character insertion.** `[` means
"the vowel ơ"; we spell it as the keys `o`+`w`. Where an `o` would be legal that
is identical, but after an existing vowel it inserts an extra `o` — probe shows
`touw` renders as literal `touw`, so the failure mode is a word that does not
transform rather than a wrong word.
*Signal it broke:* any test in the corpus where a bracket after a vowel produces
something other than the character alone.
*Response:* fall back to inserting the precomposed character into the raw log and
accept that a following tone key may not apply, or drop the phase — do not ship a
half-working substitution.

**Widening `is_word_char` changes commit behaviour.** With the option on, `[` no
longer ends a word, so `hello[` keeps composing. That is correct per UniKey but
it is a real behaviour change, and it is why the whole phase is opt-in.

**Low blast radius otherwise:** off by default, and the flag is checked before
any substitution runs, so the default path is byte-identical.
