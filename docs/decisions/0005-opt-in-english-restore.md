# 0005 — English/Telex ambiguity: opt-in wordlist restore, default off

## Status

Accepted (2026-09-02).

## Context

Auto-fix restores a committed word when its rendering is *invalid* Vietnamese
(`eĩt`→`exit`). It cannot help when the rendering is a **valid** syllable:
`was`→`ứa`, `how`→`hơ`, `cats`→`cát`. No blind rule can resolve this — the same
key sequence is legitimate Vietnamese and legitimate English.

## Decision

Add `restore_english_words` (Settings → Typing, **off by default**): at a word
boundary, if the raw keys match a curated ~370-word common-English list
(`crates/glowkey-engine/src/english.rs`), restore the raw keys even though the
rendering is valid Vietnamese. Independent of auto-fix; macros still take
precedence; a restored word is not re-composable.

Default off because the option inverts the ambiguity: syllables typed with a
trailing tone key that collide with listed words become untypeable in that key
order — `á`→`as`, `í`→`is`, `ú`→`us`, `ò`→`of`, `ỏ`→`or`, `mã`→`max`,
`sĩ`→`six`, `thú`→`thus`, `cả`→`car`, `hải`→`hair`, `cát`→`cats`, `sét`→`sets`.
A Vietnamese-first typist keeps it off; an English-first typist turns it on.

## Consequences

- The user's reported `was`→`ứa` is fixable, but only by an explicit choice —
  the Settings caption states the real cost, listing examples.
- The list is curated, not a dictionary: coverage is deliberately biased to
  words containing Telex trigger keys; misses are expected and harmless (the
  Vietnamese reading stands).
- Possible future refinement (not built): Unikey-style per-word escape — typing
  the tone key twice to reject the transform — which would resolve the
  ambiguity per word instead of globally.
