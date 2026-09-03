//! Behavioural tests for the Telex engine. These run on any platform and are the
//! headless proof that the core typing behaviour is correct — the parts that do
//! not need a Mac, a text field, or a human watching the screen.

use glowkey_engine::{Engine, PlacementStyle};

/// Types a whole string through a fresh engine and returns the final committed
/// text, reconstructed by applying each [`KeyResponse`] to a running buffer — the
/// same edits the shell would apply to a document. This exercises the real diff
/// path, not just the internal view.
fn type_word(input: &str) -> String {
    let mut engine = Engine::new(PlacementStyle::New);
    let mut screen = String::new();
    for ch in input.chars() {
        let resp = engine.process_key(ch);
        if resp.handled {
            apply(&mut screen, &resp.insert, resp.backspaces);
        } else {
            // The host would insert the raw char (a boundary like space).
            screen.push(ch);
        }
    }
    screen
}

/// Applies one edit to a UTF-16-agnostic screen buffer: delete `backspaces`
/// trailing UTF-16 code units, then append `insert`.
fn apply(screen: &mut String, insert: &str, backspaces: usize) {
    if backspaces > 0 {
        let units: Vec<u16> = screen.encode_utf16().collect();
        let keep = units.len().saturating_sub(backspaces);
        *screen = String::from_utf16(&units[..keep]).expect("valid utf16 prefix");
    }
    screen.push_str(insert);
}

#[test]
fn free_tone_placement_all_orders() {
    // The headline requirement: tone key anywhere in the sequence yields the same word.
    assert_eq!(type_word("hoongf"), "hồng"); // tone last
    assert_eq!(type_word("hofong"), "hồng"); // tone mid-cluster
    assert_eq!(type_word("hoonfg"), "hồng"); // tone before the final consonant
}

#[test]
fn immediate_circumflex() {
    // `oo` becomes `ô` without waiting for a tone key.
    assert_eq!(type_word("oo"), "ô");
    assert_eq!(type_word("caption"), "caption"); // no false trigger without doubling
}

#[test]
fn hard_nuclei_and_onsets() {
    assert_eq!(type_word("nguyeenx"), "nguyễn");
    assert_eq!(type_word("dduwowcj"), "được");
    // A single `w` after `uo` horns both vowels (uo → ươ), then the tone applies.
    assert_eq!(type_word("nguoiwf"), "người");
    assert_eq!(type_word("quar"), "quả");
    assert_eq!(type_word("khuyru"), "khuỷu");
    assert_eq!(type_word("uyr"), "uỷ");
}

#[test]
fn uppercase_and_mixed_case() {
    assert_eq!(type_word("Hoongf"), "Hồng");
    assert_eq!(type_word("NGUYEENX"), "NGUYỄN");
}

#[test]
fn interior_capitals_survive_when_not_transformed() {
    // Words with no Vietnamese transformation keep their exact original case —
    // they must not be flattened to lowercase or title-case.
    assert_eq!(type_word("iPhone"), "iPhone");
    assert_eq!(type_word("JavaScript"), "JavaScript");
    assert_eq!(type_word("macOS"), "macOS");
    assert_eq!(type_word("PhD"), "PhD");
    assert_eq!(type_word("GlowKey"), "GlowKey");
}

#[test]
fn edits_apply_onto_pre_existing_text() {
    // The diff edits must be correct even when the field already holds text before
    // the word — the empty-screen assumption is where desync bugs hide.
    let mut engine = Engine::new(PlacementStyle::New);
    let mut screen = String::from("Hello ");
    for ch in "hoongf".chars() {
        let r = engine.process_key(ch);
        apply(&mut screen, &r.insert, r.backspaces);
    }
    assert_eq!(screen, "Hello hồng");
}

#[test]
fn word_boundary_passes_through() {
    // A space ends the word and is inserted verbatim after the transformed syllable.
    assert_eq!(type_word("hoongf "), "hồng ");
    assert_eq!(type_word("xin chaof"), "xin chào");
}

#[test]
fn backspace_replays_raw_keys() {
    let mut engine = Engine::new(PlacementStyle::New);
    let mut screen = String::new();
    for ch in "hoongf".chars() {
        let r = engine.process_key(ch);
        apply(&mut screen, &r.insert, r.backspaces);
    }
    assert_eq!(screen, "hồng");

    // Deleting the tone key's effect: backspace rebuilds from h,o,o,n,g.
    let r = engine.backspace();
    assert!(r.handled);
    apply(&mut screen, &r.insert, r.backspaces);
    assert_eq!(screen, "hông");
}

#[test]
fn reset_prevents_cross_field_leak() {
    let mut engine = Engine::new(PlacementStyle::New);
    engine.process_key('h');
    engine.process_key('o');
    assert!(engine.is_composing());
    engine.reset(); // e.g. focus moved to another app
    assert!(!engine.is_composing());
    // A fresh word starts clean.
    assert_eq!(type_word("oo"), "ô");
}

#[test]
fn old_style_placement_differs() {
    let mut new_engine = Engine::new(PlacementStyle::New);
    let mut old_engine = Engine::new(PlacementStyle::Old);
    let render = |engine: &mut Engine, input: &str| {
        let mut screen = String::new();
        for ch in input.chars() {
            let r = engine.process_key(ch);
            apply(&mut screen, &r.insert, r.backspaces);
        }
        screen
    };
    // "hoaf" (f = huyền): new style puts the mark on the 2nd vowel (hoà),
    // old style on the 1st (hòa). "hoas" would be sắc (hoá) — a different tone.
    assert_eq!(render(&mut new_engine, "hoaf"), "hoà");
    assert_eq!(render(&mut old_engine, "hoaf"), "hòa");
}

#[test]
fn vni_input_method() {
    // VNI: digits carry tone/diacritic. viet65 → việt, a6 → â, o7 → ơ, d9 → đ.
    use glowkey_engine::InputMethod;
    fn type_vni(input: &str) -> String {
        let mut e = Engine::new(PlacementStyle::New);
        e.set_method(InputMethod::Vni);
        let mut screen = String::new();
        for ch in input.chars() {
            let r = e.process_key(ch);
            if r.handled {
                apply(&mut screen, &r.insert, r.backspaces);
            } else {
                screen.push(ch);
            }
        }
        screen
    }
    assert_eq!(type_vni("a6"), "â");
    assert_eq!(type_vni("o7"), "ơ");
    assert_eq!(type_vni("d9"), "đ");
    assert_eq!(type_vni("viet65"), "việt");
    // Telex still works unchanged on a default engine.
    assert_eq!(type_word("hoongf"), "hồng");
}

#[test]
fn mid_word_backspace_drops_a_visible_char_and_keeps_composing() {
    // The host deletes the character itself, so the engine must land on exactly
    // what the screen will show: hồng⌫ is hồn, keeping the tone. That means
    // dropping the raw `g`, not popping the last key — popping gives hông.
    let mut engine = Engine::new(PlacementStyle::New);
    for ch in "hoongf".chars() {
        engine.process_key(ch);
    }
    assert_eq!(engine.current_word(), "hồng");

    assert!(engine.backspace_visible_char());
    assert_eq!(engine.current_word(), "hồn");
    assert_eq!(engine.raw_string(), "hoonf");
    assert!(engine.is_composing());

    // Still composing, so z is the tone-removal key and not a literal.
    let r = engine.process_key('z');
    let mut screen = String::from("hồn");
    apply(&mut screen, &r.insert, r.backspaces);
    assert_eq!(screen, "hôn");
    assert_eq!(engine.current_word(), "hôn");
}

#[test]
fn mid_word_backspace_reports_failure_when_it_cannot_stay_in_step() {
    // `oo` renders as the single character ô. Deleting it leaves nothing to
    // compose and no single raw key removal reproduces an empty target, so the
    // engine says so and the caller flushes.
    let mut engine = Engine::new(PlacementStyle::New);
    for ch in "oo".chars() {
        engine.process_key(ch);
    }
    assert_eq!(engine.current_word(), "ô");
    assert!(!engine.backspace_visible_char());
}
