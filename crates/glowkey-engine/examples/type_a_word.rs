//! Types `hoongf` through the engine and prints what lands on screen.
//!
//! ```text
//! cargo run -p glowkey-engine --example type_a_word
//! ```

use glowkey_engine::{Engine, PlacementStyle};

fn main() {
    let mut engine = Engine::new(PlacementStyle::New);
    let mut screen = String::new();
    for ch in "hoongf".chars() {
        let edit = engine.process_key(ch);
        // `backspaces` counts UTF-16 code units, the unit every host text API
        // deletes in; the screen is kept in that unit too.
        let units: Vec<u16> = screen.encode_utf16().collect();
        let keep = units.len() - edit.backspaces;
        screen = String::from_utf16(&units[..keep]).expect("valid UTF-16") + &edit.insert;
        println!(
            "{ch} -> delete {} insert {:?} => {screen}",
            edit.backspaces, edit.insert
        );
    }
    assert_eq!(screen, "hồng");
}
