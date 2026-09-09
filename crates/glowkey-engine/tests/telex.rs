//! Behavioural tests for the Telex engine. These run on any platform and are the
//! headless proof that the core typing behaviour is correct — the parts that do
//! not need a Mac, a text field, or a human watching the screen.

use glowkey_engine::{BackspaceOutcome, Engine, PlacementStyle};

/// Types a whole string through a fresh engine and returns the final committed
/// text, reconstructed by applying each [`KeyResponse`] to a running buffer — the
/// same edits the shell would apply to a document. This exercises the real diff
/// path, not just the internal view.
fn type_word(input: &str) -> String {
    type_word_in(input, PlacementStyle::New)
}

/// The same, in the traditional placement style (`hòa`, not `hoà`).
fn type_word_old_style(input: &str) -> String {
    type_word_in(input, PlacementStyle::Old)
}

/// Types `input` under VNI, where the marks are digits.
fn type_vni(input: &str) -> String {
    let mut engine = Engine::new(PlacementStyle::New);
    engine.set_method(glowkey_engine::InputMethod::Vni);
    let mut screen = String::new();
    for ch in input.chars() {
        let resp = engine.process_key(ch);
        if resp.handled {
            apply(&mut screen, &resp.insert, resp.backspaces);
        } else {
            screen.push(ch);
        }
    }
    screen
}

fn type_word_in(input: &str, style: PlacementStyle) -> String {
    let mut engine = Engine::new(style);
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

/// **An unshifted tone key does not demote an all-caps word.**
///
/// Reported 2026-09-09 as `O` `A` `f` giving `Òa`: the case pattern was read off
/// every key typed, so the lowercase `f` — a tone key, not a letter of the word
/// — failed the all-caps test and the word came out Title case with its `A`
/// demoted. Releasing Shift for the mark is how people type these.
#[test]
fn a_lowercase_mark_key_keeps_an_all_caps_word_in_caps() {
    // The report, in both placement styles: the tone sits on a different vowel,
    // the case is the same question.
    assert_eq!(type_word("OAf"), "OÀ");
    assert_eq!(type_word_old_style("OAf"), "ÒA");

    // Held Shift for the letters, released it for the mark.
    assert_eq!(type_word("HOONGf"), "HỒNG");
    assert_eq!(type_word("CAs"), "CÁ");
    assert_eq!(type_word("NGUYEENx"), "NGUYỄN");
    assert_eq!(type_word("DDUWOWCj"), "ĐƯỢC");

    // Shift held throughout, which already worked.
    assert_eq!(type_word("HOONGF"), "HỒNG");
    assert_eq!(type_word("CAS"), "CÁ");

    // And the ordinary patterns, unchanged: a capitalized word is still Title
    // case, and a lowercase word is still lowercase.
    assert_eq!(type_word("Hoongf"), "Hồng");
    assert_eq!(type_word("hoongf"), "hồng");
    assert_eq!(type_word("Cas"), "Cá");
}

/// The same bug under **VNI**, where it hit every all-caps word: the marks are
/// digits, and a digit is never uppercase, so `VIET65` came out `Việt`.
#[test]
fn an_all_caps_vni_word_stays_in_caps() {
    assert_eq!(type_vni("VIET65"), "VIỆT");
    assert_eq!(type_vni("TOAN2"), "TOÀN");
    assert_eq!(type_vni("A6"), "Â");
    // And the ordinary patterns.
    assert_eq!(type_vni("Viet65"), "Việt");
    assert_eq!(type_vni("viet65"), "việt");
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

    assert_eq!(
        engine.backspace_visible_char(),
        BackspaceOutcome::InStep,
        "an unescaped word stays in step; the host performs the delete"
    );
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
    // compose: no single raw key removal reproduces an empty target, and undoing
    // the second `o` gives `o` — one character still, so the delete would appear
    // to do nothing. The engine says so and the caller flushes.
    let mut engine = Engine::new(PlacementStyle::New);
    for ch in "oo".chars() {
        engine.process_key(ch);
    }
    assert_eq!(engine.current_word(), "ô");
    assert_eq!(engine.backspace_visible_char(), BackspaceOutcome::Flush);
}

#[test]
fn mid_word_backspace_after_a_rejected_diacritic_restores_the_diacritic() {
    // Reported 2026-09-09: `oo` is ô and `ooo` rejects the circumflex back to a
    // literal `oo`, so three keys show two characters. Deleting one used to flush
    // and leave a bare `o` — the second one stranded as a literal, the engine no
    // longer composing. Undoing the keystroke that rejected the diacritic puts
    // the word back where the first two keys had it.
    let mut engine = Engine::new(PlacementStyle::New);
    for ch in "ooo".chars() {
        engine.process_key(ch);
    }
    assert_eq!(engine.current_word(), "oo");

    match engine.backspace_visible_char() {
        BackspaceOutcome::Repair(edit) => {
            assert_eq!(edit.backspaces, 2, "both on-screen o's are replaced");
            assert_eq!(edit.insert, "ô");
        }
        other => panic!("expected a repair, got {other:?}"),
    }
    assert_eq!(engine.current_word(), "ô");
    assert_eq!(engine.raw_string(), "oo");

    // Still composing, so the next key is a Telex key rather than a literal.
    let r = engine.process_key('f');
    let mut screen = String::from("ô");
    apply(&mut screen, &r.insert, r.backspaces);
    assert_eq!(screen, "ồ");
}

#[test]
fn repeating_the_diacritic_key_rejects_it() {
    // Unikey's escape hatch, inherited from the `vi` crate: press the tone or
    // modifier key again and the mark comes off, leaving the literal key. This is
    // what lets a Vietnamese speaker type an English word that collides with a
    // Telex sequence, without touching any setting.
    assert_eq!(type_word("cas"), "cá");
    assert_eq!(type_word("cass"), "cas");
    assert_eq!(type_word("aa"), "â");
    assert_eq!(type_word("aaa"), "aa");
    assert_eq!(type_word("dd"), "đ");
    assert_eq!(type_word("ddd"), "dd");
    assert_eq!(type_word("hoongf"), "hồng");
    assert_eq!(type_word("hoongff"), "hôngf");
}

/// **A cancelled repeat stays cancelled**, however many more keys arrive.
///
/// Reported 2026-09-09: `ooo` gives the literal `oo`, so a fourth `o` should
/// give `ooo` — instead the circumflex came back as `ôo`, two characters for
/// four keys, and composing carried on from there (`oooof` → `ồo`).
///
/// `vi` re-applies the modification on the fourth press and keeps the result if
/// its syllable validator accepts it. `âa` and `êe` are refused, so `a` and `e`
/// fall through to a literal; **`ôo` is accepted**, though no Vietnamese
/// syllable has `ô` before a bare `o`. Hence the one-letter difference.
#[test]
fn a_cancelled_repeat_stays_cancelled() {
    // The run: two letters for the first three keys, then one each.
    assert_eq!(type_word("oo"), "ô");
    assert_eq!(type_word("ooo"), "oo");
    assert_eq!(type_word("oooo"), "ooo");
    assert_eq!(type_word("ooooo"), "oooo");
    assert_eq!(type_word("oooooo"), "ooooo");

    // Mid-word, and with letters after the run.
    assert_eq!(type_word("hoooo"), "hooo");
    assert_eq!(type_word("cooool"), "coool");
    assert_eq!(type_word("oooong"), "ooong");

    // Case survives a cancelled run.
    assert_eq!(type_word("OOOO"), "OOO");
    assert_eq!(type_word("Oooo"), "Ooo");

    // The other three doubling keys, which `vi` already got right and which this
    // must not change.
    assert_eq!(type_word("aaaa"), "aaa");
    assert_eq!(type_word("aaaaa"), "aaaa");
    assert_eq!(type_word("eeee"), "eee");
    assert_eq!(type_word("dddd"), "ddd");

    // Not doubling keys in Telex, so a run of them is only ever letters.
    assert_eq!(type_word("uuuu"), "uuuu");
    assert_eq!(type_word("iii"), "iii");
}

/// **What the fix above had to leave alone**, pinned so it cannot be traded away
/// for the shorter rule a second time.
///
/// The first attempt at the fix ended the syllable at the cancel and started the
/// next key fresh — which reads as the tidier rule, and breaks real words. `oo`
/// is a Vietnamese sequence, and it takes tones: `moóc` (xe moóc, a trailer) and
/// `soóc` (quần soóc, shorts) are typed *through* the cancel, so the tone key
/// after it must still reach the vowel. Withholding only the fourth press keeps
/// that, because `vi` still sees the whole syllable.
#[test]
fn a_tone_after_a_cancelled_repeat_still_lands() {
    // The words that caught it. `s` is sắc, typed after the cancelling third `o`.
    assert_eq!(type_word("mooosc"), "moóc");
    assert_eq!(type_word("sooocs"), "soóc");
    assert_eq!(type_word("xooongf"), "xoòng");

    // And the toneless `oo` words, which only need the cancel itself.
    assert_eq!(type_word("xooong"), "xoong");
    assert_eq!(type_word("booong"), "boong");
    assert_eq!(type_word("looong"), "loong");
    assert_eq!(type_word("toooi"), "tooi");
    assert_eq!(type_word("hooo"), "hoo");

    // A run of exactly three is `vi`'s business and is untouched, including
    // where its answer is odd: the tone still applies (`hoò`), which is the same
    // rule that makes `moóc` work.
    assert_eq!(type_word("hooof"), "hoò");
    // `a` differs here only because `vi` refuses `âa`, not because of anything
    // this engine does.
    assert_eq!(type_word("haaaf"), "haaf");
}

/// **A digit ends the syllable; a letter extends it.**
///
/// The boundary half of an asymmetry reported as a bug on 2026-09-06: `hoongfc`
/// comes back as raw keys while `hoongf4` stays `hồng4`. This is where the
/// difference starts — `c` extends the syllable and is re-rendered with it,
/// while `4` is a word boundary exactly like a space, arriving after `hồng` has
/// already committed. `is_syllable_char` admits digits only under VNI, where
/// they carry tones.
///
/// The escape to verbatim keys that makes `hoongfc` visible to the user belongs
/// to the mid-word spell check a layer up; `midword_spell_check.rs` pins that
/// half. Pinned in both places so the asymmetry reads as intended rather than as
/// a defect, and so changing `is_syllable_char` has to argue with a test. The
/// rejected alternative — treating a digit glued to a syllable as evidence the
/// token is not Vietnamese — would break `tầng2`, `quận1` and `phường7`, which
/// people type without a space.
#[test]
fn a_digit_ends_the_word_where_a_letter_extends_it() {
    // The letter joins the syllable and is rendered with it.
    assert_eq!(type_word("hoongfc"), "hồngc");

    // The digit ends it: `hồng` had already committed, and the digit is ordinary
    // text after it.
    assert_eq!(type_word("hoongf4"), "hồng4");

    // Which is the same rule a space follows.
    assert_eq!(type_word("hoongf 4"), "hồng 4");

    // A digit opening a word is just a digit — there is no syllable to end.
    assert_eq!(type_word("4hoongf"), "4hồng");
}
