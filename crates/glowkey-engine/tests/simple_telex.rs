//! UniKey's Simple Telex (`UkSimpleTelex`): Telex, except that `w` only ever
//! modifies a vowel already typed — it never stands alone as `ư`.

use glowkey_engine::{Engine, InputMethod, PlacementStyle};

fn typed(input: &str, method: InputMethod) -> String {
    let mut engine = Engine::new(PlacementStyle::New);
    engine.set_method(method);
    for ch in input.chars() {
        engine.process_key(ch);
    }
    engine.current_word().to_string()
}

#[test]
fn w_no_longer_stands_alone_for_u_horn() {
    // The one difference from full Telex, and the whole reason UniKey ships both.
    assert_eq!(typed("w", InputMethod::Telex), "ư");
    assert_eq!(typed("w", InputMethod::SimpleTelex), "w");
}

#[test]
fn w_still_adds_the_horn_and_the_breve() {
    for (keys, expected) in [("uw", "ư"), ("ow", "ơ"), ("aw", "ă")] {
        assert_eq!(typed(keys, InputMethod::SimpleTelex), expected, "{keys}");
    }
}

#[test]
fn everything_else_matches_full_telex() {
    for keys in [
        "hoongf", "vieejt", "nguyeenx", "ddaij", "cas", "cass", "aaa",
    ] {
        assert_eq!(
            typed(keys, InputMethod::SimpleTelex),
            typed(keys, InputMethod::Telex),
            "{keys} should be identical in both Telex variants"
        );
    }
}

#[test]
fn quick_telex_applies_to_both_telex_variants() {
    // Its digraphs are plain letters, so nothing about them is Telex-specific.
    for method in [InputMethod::Telex, InputMethod::SimpleTelex] {
        let mut engine = Engine::new(PlacementStyle::New);
        engine.set_method(method);
        engine.set_quick_telex(true);
        for ch in "ccao".chars() {
            engine.process_key(ch);
        }
        assert_eq!(engine.current_word(), "chao", "{method:?}");
    }
}

#[test]
fn brackets_stay_telex_only() {
    // UniKey's Simple Telex mapping drops the bracket entries, so we do too.
    let mut engine = Engine::new(PlacementStyle::New);
    engine.set_method(InputMethod::SimpleTelex);
    engine.set_telex_brackets(true);
    engine.process_key('[');
    assert_ne!(engine.current_word(), "ơ");
}
