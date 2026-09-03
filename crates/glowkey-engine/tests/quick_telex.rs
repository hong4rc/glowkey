//! "Quick Telex": a doubled consonant at the start of a syllable stands for its
//! digraph (`cc`→`ch`, `nn`→`ng`). Opt-in, because it changes what a plain
//! consonant pair means. EVKey and later UniKey releases offer this; it is not
//! in the 2015 UniKey source.

use glowkey_engine::{Engine, InputMethod, PlacementStyle};

fn typed(input: &str, quick_telex: bool) -> String {
    let mut engine = Engine::new(PlacementStyle::New);
    engine.set_quick_telex(quick_telex);
    for ch in input.chars() {
        engine.process_key(ch);
    }
    engine.current_word().to_string()
}

#[test]
fn expands_doubled_consonants_at_the_syllable_start() {
    for (keys, expected) in [
        ("cc", "ch"),
        ("gg", "gi"),
        ("kk", "kh"),
        ("nn", "ng"),
        ("pp", "ph"),
        ("qq", "qu"),
        ("tt", "th"),
    ] {
        assert_eq!(typed(keys, true), expected, "quick telex for {keys}");
    }
    // uu expands to the Telex keys `uw`, so `vi` still produces the character.
    assert_eq!(typed("uu", true), "ư");
}

#[test]
fn expansion_composes_with_the_rest_of_the_word() {
    assert_eq!(typed("ccao", true), "chao");
    assert_eq!(typed("ccaof", true), "chào");
    assert_eq!(typed("nnuowif", true), "người");
}

#[test]
fn case_of_the_trigger_survives() {
    // One shifted key is the Title-case gesture.
    assert_eq!(typed("Ccao", true), "Chao");
}

#[test]
fn caps_lock_words_stay_all_caps() {
    // Uppercasing only the head of the digraph left a lowercase key in the
    // sequence, which defeated the all-caps detection and downgraded the whole
    // word: CCAO came out "ChAO".
    assert_eq!(typed("CCAO", true), "CHAO");
    assert_eq!(typed("NNAM", true), "NGAM");
    assert_eq!(typed("KKAC", true), "KHAC");
    assert_eq!(typed("NNUOWIF", true), "NGƯỜI");
    assert_eq!(typed("UUNG", true), "ƯNG");
}

#[test]
fn does_not_apply_under_vni() {
    // The expansions are Telex key sequences — `uu` stands for the keys `uw` — so
    // under VNI they would put a literal w on screen that the user never typed,
    // and auto-fix cannot repair it because the result is plain ASCII.
    let mut engine = Engine::new(PlacementStyle::New);
    engine.set_method(InputMethod::Vni);
    engine.set_quick_telex(true);
    for ch in "uu1".chars() {
        engine.process_key(ch);
    }
    assert_eq!(engine.current_word(), "úu");
}

#[test]
fn english_words_with_inner_doubles_are_untouched() {
    // The expansion is syllable-initial only, so a doubled consonant in the
    // middle of an English word never fires.
    for word in ["letter", "happy", "accept", "little", "sudden"] {
        assert_eq!(
            typed(word, true),
            typed(word, false),
            "{word} must render the same with quick telex on"
        );
    }
}

#[test]
fn off_by_default_and_byte_identical_when_off() {
    assert!(!Engine::new(PlacementStyle::New).quick_telex());
    for keys in ["cc", "nn", "tt", "uu", "hoongf", "vieejt", "ccao"] {
        assert_eq!(typed(keys, false), typed_without_the_option(keys));
    }
}

fn typed_without_the_option(input: &str) -> String {
    let mut engine = Engine::new(PlacementStyle::New);
    for ch in input.chars() {
        engine.process_key(ch);
    }
    engine.current_word().to_string()
}
