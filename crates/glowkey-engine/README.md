# glowkey-engine

The Vietnamese transformation at the core of [GlowKey](https://github.com/hong4rc/glowkey):
raw keystrokes in, a minimal edit out. Telex and VNI, with the tone key
accepted in any position (`hoongf`, `hofong` and `hoonfg` all give `hồng`).

This crate knows nothing about keyboards, applications, files or operating
systems. It depends on [`vi`](https://crates.io/crates/vi) and `phf` only, and
CI builds and tests it on Linux.

## What it does

`Engine` keeps the raw keystroke log for the word being typed and re-derives
the whole rendering on every key. Each key returns a `KeyResponse`: how many
UTF-16 code units to delete before the caret and what text to insert in their
place. That is the shape every shipping Vietnamese input method uses, and it
lets a caller render the change with marked text or with backspaces plus an
insert without caring which.

```rust
use glowkey_engine::{Engine, PlacementStyle};

let mut engine = Engine::new(PlacementStyle::New);
let mut screen = String::new();
for ch in "hoongf".chars() {
    let edit = engine.process_key(ch);
    let keep = screen.encode_utf16().count() - edit.backspaces;
    let units: Vec<u16> = screen.encode_utf16().take(keep).collect();
    screen = String::from_utf16(&units).unwrap() + &edit.insert;
}
assert_eq!(screen, "hồng");
```

Run the same thing with `cargo run -p glowkey-engine --example type_a_word`.

Also here: `InputMethod` (Telex, VNI, simple Telex), `PlacementStyle` (where a
tone mark goes on a diphthong), `remove_tones`, the mid-word spell check
(`is_invalid_vietnamese`, including the stop-coda tone rule `vi` lacks), and
`diff`, the minimal edit between two renderings.

## What it does not do

- No keyboard hook, no event synthesis, no window server.
- No VN/EN mode, no per-application ignore list, no auto-fix at a word
  boundary. Those are policy, and they live in
  [`glowkey-session`](../glowkey-session), which re-exports this crate.
- No settings file. `serde` derives for `InputMethod` and `PlacementStyle` are
  behind the optional `serde` feature.

## Licence

MIT. The `vi` crate is MIT as well; see the repository's
`THIRD-PARTY-NOTICES.md`.
