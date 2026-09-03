//! UniKey's Telex bracket shortcuts: `[`→ơ, `]`→ư, `{`→Ơ, `}`→Ư. Opt-in,
//! because turning them on stops `[` and `]` typing brackets.

use glowkey_engine::{Engine, InputMethod, PlacementStyle};

fn typed(input: &str, brackets: bool) -> String {
    let mut engine = Engine::new(PlacementStyle::New);
    engine.set_telex_brackets(brackets);
    for ch in input.chars() {
        engine.process_key(ch);
    }
    engine.current_word().to_string()
}

#[test]
fn brackets_produce_the_horned_vowels() {
    assert_eq!(typed("[", true), "ơ");
    assert_eq!(typed("]", true), "ư");
    assert_eq!(typed("{", true), "Ơ");
    assert_eq!(typed("}", true), "Ư");
}

#[test]
fn a_tone_key_still_applies_afterwards() {
    // The reason the substitution uses Telex keys rather than the character:
    // `vi` can still modify what it produced.
    assert_eq!(typed("[f", true), "ờ");
    assert_eq!(typed("[s", true), "ớ");
    assert_eq!(typed("]j", true), "ự");
}

#[test]
fn works_mid_word_not_only_at_the_start() {
    assert_eq!(typed("t[", true), "tơ");
    assert_eq!(typed("T[", true), "Tơ");
    assert_eq!(typed("th[", true), "thơ");
    assert_eq!(typed("ng[i", true), "ngơi");
}

#[test]
fn off_by_default_and_unchanged_when_off() {
    assert!(!Engine::new(PlacementStyle::New).telex_brackets());
    for keys in ["[", "]", "{", "}", "t[", "[f", "hoongf", "vieejt"] {
        let mut plain = Engine::new(PlacementStyle::New);
        for ch in keys.chars() {
            plain.process_key(ch);
        }
        assert_eq!(
            typed(keys, false),
            plain.current_word(),
            "{keys} must be untouched with the option off"
        );
    }
}

#[test]
fn does_not_apply_under_vni() {
    // The substitution emits Telex keys, which mean nothing under VNI.
    let mut engine = Engine::new(PlacementStyle::New);
    engine.set_method(InputMethod::Vni);
    engine.set_telex_brackets(true);
    for ch in "[".chars() {
        engine.process_key(ch);
    }
    assert_ne!(engine.current_word(), "ơ");
}

#[test]
fn brackets_extend_the_word_only_when_the_option_is_on() {
    let mut on = Engine::new(PlacementStyle::New);
    on.set_telex_brackets(true);
    assert!(on.is_syllable_char('['));
    assert!(on.is_syllable_char('}'));

    let off = Engine::new(PlacementStyle::New);
    assert!(!off.is_syllable_char('['));
}

#[test]
fn composes_with_quick_telex() {
    // Quick Telex expands the doubled pair first, brackets second.
    let mut engine = Engine::new(PlacementStyle::New);
    engine.set_quick_telex(true);
    engine.set_telex_brackets(true);
    for ch in "nn[".chars() {
        engine.process_key(ch);
    }
    assert_eq!(engine.current_word(), "ngơ");
}

#[test]
fn caps_lock_words_keep_their_case() {
    // Caps Lock does not shift `[`, so a caps-lock user types `[`. Injecting a
    // lowercase o/w left a lowercase key in the slice and defeated the all-caps
    // detection, downgrading the whole word.
    assert_eq!(typed("TH[", true), "THƠ");
    assert_eq!(typed("NG]", true), "NGƯ");
    assert_eq!(typed("HO[NG", true), "HƠNG");
    // One capital is Title case, not Caps Lock.
    assert_eq!(typed("T[", true), "Tơ");
    assert_eq!(typed("Th[", true), "Thơ");
}

#[test]
fn real_vietnamese_words_round_trip() {
    // The domain the feature exists for: ơ and ư after a consonant or at the
    // start of a syllable.
    for (keys, expected) in [
        ("c[m", "cơm"),
        ("th[", "thơ"),
        ("ng[i", "ngơi"),
        ("t[f", "tờ"),
        ("ng]", "ngư"),
        ("nh]ng", "nhưng"),
        ("ng]owif", "người"),
        ("ch[i", "chơi"),
    ] {
        assert_eq!(typed(keys, true), expected, "{keys}");
    }
}
