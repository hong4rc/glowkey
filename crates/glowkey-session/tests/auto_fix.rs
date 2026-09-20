//! Auto-fix: at a word boundary, restore the raw keystrokes when the Telex result
//! is not valid Vietnamese (`exit`, not `eĩt`).

use glowkey_session::{ExclusionList, PlacementStyle, Session};

/// Types `input` in an active session, then commits (word boundary). Returns the
/// on-screen text after applying the per-key edits and any auto-fix restore.
fn type_then_commit(session: &mut Session, input: &str) -> String {
    let mut screen = String::new();
    let apply = |screen: &mut String, backspaces: usize, insert: &str| {
        let units: Vec<u16> = screen.encode_utf16().collect();
        let keep = units.len().saturating_sub(backspaces);
        *screen = String::from_utf16(&units[..keep]).unwrap();
        screen.push_str(insert);
    };
    for ch in input.chars() {
        let r = session.process_key(ch);
        if r.handled {
            apply(&mut screen, r.backspaces, &r.insert);
        } else {
            screen.push(ch);
        }
    }
    if let Some(restore) = session.commit() {
        apply(&mut screen, restore.backspaces, &restore.insert);
    }
    screen
}

fn active_session(auto_fix: bool) -> Session {
    let mut s = Session::new(PlacementStyle::New, ExclusionList::new());
    s.set_frontmost_app("com.apple.TextEdit");
    s.set_auto_fix(auto_fix);
    s
}

#[test]
fn restores_invalid_english_word() {
    // "exit" in Telex mangles (x is the ngã tone). Auto-fix restores it.
    let mut s = active_session(true);
    assert_eq!(type_then_commit(&mut s, "exit"), "exit");
}

#[test]
fn keeps_valid_vietnamese() {
    // A valid word is never restored.
    let mut s = active_session(true);
    assert_eq!(type_then_commit(&mut s, "hoongf"), "hồng");
    let mut s = active_session(true);
    assert_eq!(type_then_commit(&mut s, "nguyeenx"), "nguyễn");
}

#[test]
fn auto_fix_off_leaves_telex_result() {
    // With auto-fix off, the mangled Telex output stays.
    let mut s = active_session(false);
    let result = type_then_commit(&mut s, "exit");
    assert_ne!(
        result, "exit",
        "auto-fix off should leave the transformed result"
    );
}

#[test]
fn plain_english_without_transform_is_untouched() {
    // A word that never transforms (no tone/mod keys) stays as typed.
    let mut s = active_session(true);
    assert_eq!(type_then_commit(&mut s, "the"), "the");
    let mut s = active_session(true);
    assert_eq!(type_then_commit(&mut s, "code"), "code");
}

#[test]
fn batch_of_real_words_not_restored() {
    // Real Vietnamese words must never be wrongly restored to raw.
    for (keys, expected) in [
        ("hoongf", "hồng"),
        ("nguyeenx", "nguyễn"),
        ("dduwowcj", "được"),
        ("quar", "quả"),
        ("tieengs", "tiếng"),
        ("vieetj", "việt"),
    ] {
        let mut s = active_session(true);
        assert_eq!(type_then_commit(&mut s, keys), expected, "input {keys}");
    }
}

#[test]
fn restores_english_words_with_w() {
    // `w` is a Telex transform key (w→ư), so English words starting with w mangle
    // mid-word (work → ưởk) but auto-fix restores them at the boundary.
    let mut s = active_session(true);
    assert_eq!(type_then_commit(&mut s, "work"), "work");
    let mut s = active_session(true);
    assert_eq!(type_then_commit(&mut s, "word"), "word");
    let mut s = active_session(true);
    assert_eq!(type_then_commit(&mut s, "weight"), "weight");
}

#[test]
fn ambiguous_english_that_maps_to_valid_vietnamese_is_kept() {
    // "was" → "ứa" is a *valid* Vietnamese syllable, so auto-fix cannot know it was
    // meant as English and keeps it. Documents the inherent Telex/English ambiguity
    // — resolvable only by the opt-in English restore below.
    let mut s = active_session(true);
    assert_eq!(type_then_commit(&mut s, "was"), "ứa");
}

#[test]
fn english_restore_fixes_valid_vietnamese_collisions() {
    // With the opt-in on, a committed word whose raw keys are a common English
    // word is restored even though the rendering is valid Vietnamese.
    for word in ["was", "how", "now", "cats", "this", "his", "of", "sets"] {
        let mut s = active_session(true);
        s.set_restore_english_words(true);
        assert_eq!(type_then_commit(&mut s, word), word, "input {word}");
    }
    // Case is preserved (the raw keys are restored as typed).
    let mut s = active_session(true);
    s.set_restore_english_words(true);
    assert_eq!(type_then_commit(&mut s, "Was"), "Was");
}

#[test]
fn english_restore_never_touches_vietnamese_words() {
    // Real Vietnamese input is unaffected — its raw keys are not English words.
    for (keys, expected) in [("hoongf", "hồng"), ("vieetj", "việt"), ("chaof", "chào")] {
        let mut s = active_session(true);
        s.set_restore_english_words(true);
        assert_eq!(type_then_commit(&mut s, keys), expected, "input {keys}");
    }
}

#[test]
fn english_restore_off_by_default_keeps_vietnamese_reading() {
    // Off (the default): `cats` still yields `cát` — the Vietnamese-first reading.
    let mut s = active_session(true);
    assert!(!glowkey_session::Session::new(
        glowkey_session::PlacementStyle::default(),
        glowkey_session::ExclusionList::new(),
    )
    .restore_english_words());
    assert_eq!(type_then_commit(&mut s, "cats"), "cát");
}

#[test]
fn english_restore_works_independently_of_auto_fix() {
    // Auto-fix off, English restore on: the listed word still restores.
    let mut s = active_session(false);
    s.set_restore_english_words(true);
    assert_eq!(type_then_commit(&mut s, "was"), "was");
}

#[test]
fn auto_capitalize_sentence_start() {
    // Simulate the shell flow: process letters, and at each boundary char commit()
    // then note_boundary(). Capitalize first letter of the doc and after . ! ?.
    fn run(cap: bool, input: &str) -> String {
        let mut s = active_session(true);
        s.set_auto_capitalize(cap);
        let mut screen = String::new();
        let apply = |screen: &mut String, bs: usize, ins: &str| {
            let u: Vec<u16> = screen.encode_utf16().collect();
            let k = u.len().saturating_sub(bs);
            *screen = String::from_utf16(&u[..k]).unwrap();
            screen.push_str(ins);
        };
        for ch in input.chars() {
            if ch.is_ascii_alphabetic() {
                let r = s.process_key(ch);
                if r.handled {
                    apply(&mut screen, r.backspaces, &r.insert);
                } else {
                    screen.push(ch);
                }
            } else {
                if let Some(restore) = s.commit() {
                    apply(&mut screen, restore.backspaces, &restore.insert);
                }
                s.note_boundary(ch);
                screen.push(ch);
            }
        }
        screen
    }
    // Telex-safe words (no tone/diacritic trigger letters) so only capitalization
    // changes the text.
    assert_eq!(
        run(true, "hi man. big cat! top van? go"),
        "Hi man. Big cat! Top van? Go"
    );
    // Off: no capitalization.
    assert_eq!(run(false, "hi man. big cat"), "hi man. big cat");
    // Vietnamese first letter still capitalizes: "chaof" → Chào at sentence start.
    assert_eq!(run(true, "chaof"), "Chào");
}

#[test]
fn macro_expansion() {
    fn run(input: &str, add: &[(&str, &str)]) -> String {
        let mut s = active_session(true);
        for (sc, ex) in add {
            s.add_macro(sc, ex);
        }
        let mut screen = String::new();
        let apply = |screen: &mut String, bs: usize, ins: &str| {
            let u: Vec<u16> = screen.encode_utf16().collect();
            let k = u.len().saturating_sub(bs);
            *screen = String::from_utf16(&u[..k]).unwrap();
            screen.push_str(ins);
        };
        for ch in input.chars() {
            if ch.is_ascii_alphabetic() {
                let r = s.process_key(ch);
                if r.handled {
                    apply(&mut screen, r.backspaces, &r.insert);
                } else {
                    screen.push(ch);
                }
            } else {
                if let Some(restore) = s.commit() {
                    apply(&mut screen, restore.backspaces, &restore.insert);
                }
                s.note_boundary(ch);
                screen.push(ch);
            }
        }
        screen
    }
    // "vn " → "Việt Nam " (expansion replaces the shortcut at the boundary).
    assert_eq!(run("vn ", &[("vn", "Việt Nam")]), "Việt Nam ");
    // Case-insensitive trigger.
    assert_eq!(run("VN ", &[("vn", "Việt Nam")]), "Việt Nam ");
    // Non-matching word is untouched.
    assert_eq!(run("hi ", &[("vn", "Việt Nam")]), "hi ");
}

#[test]
fn keeps_abbreviations_that_start_with_d_bar() {
    // Reaching a leading đ costs `dd`, which no English word starts with, so the
    // đ is deliberate and auto-fix must leave it alone even though these are not
    // syllables. Restoring them would hand back `ddc`/`ddt`, which is never wanted.
    let mut s = active_session(true);
    assert_eq!(type_then_commit(&mut s, "ddc"), "đc");
    let mut s = active_session(true);
    assert_eq!(type_then_commit(&mut s, "ddt"), "đt");
    let mut s = active_session(true);
    assert_eq!(type_then_commit(&mut s, "dd"), "đ");
}

#[test]
fn restores_a_leading_d_bar_word_once_the_rest_carries_a_diacritic() {
    // `ddeew` is `đ` + `êw`: not an abbreviation candidate (its rest is not
    // plain ASCII) and not a valid syllable either, since `w` never combines
    // with `ê`. The leading-đ exemption must not swallow it — reported live as
    // `ddeew` incorrectly staying `đêw` with auto-fix on.
    let mut s = active_session(true);
    assert_eq!(type_then_commit(&mut s, "ddeew"), "ddeew");
}

#[test]
fn still_restores_english_words_whose_d_bar_is_not_leading() {
    // The exemption is for a *leading* đ only, so English words that merely
    // contain `dd` keep restoring.
    let mut s = active_session(true);
    assert_eq!(type_then_commit(&mut s, "address"), "address");
    let mut s = active_session(true);
    assert_eq!(type_then_commit(&mut s, "odd"), "odd");
    let mut s = active_session(true);
    assert_eq!(type_then_commit(&mut s, "sudden"), "sudden");
    let mut s = active_session(true);
    assert_eq!(type_then_commit(&mut s, "work"), "work");
}

#[test]
fn restores_english_words_broken_by_the_stop_coda_tone_rule() {
    // A syllable closed by c, ch, p or t can only carry sắc or nặng. The `vi`
    // crate does not know that and called these valid Vietnamese, so auto-fix
    // left them transformed: left→lèt, soft→sòt, gift→gìt. Telex's f, r and x are
    // exactly the three forbidden tones, which is why ordinary English hits it.
    for word in ["left", "soft", "gift", "lift", "loft"] {
        let mut s = active_session(true);
        assert_eq!(
            type_then_commit(&mut s, word),
            word,
            "{word} must be restored"
        );
    }
}

#[test]
fn the_stop_coda_rule_leaves_legal_vietnamese_alone() {
    // Sắc and nặng are legal on a stop coda, and these must never be restored.
    for (keys, expected) in [
        ("vieejt", "việt"),
        ("hocj", "học"),
        ("ddaats", "đất"),
        ("nuowcs", "nước"),
        ("ddepj", "đẹp"),
        ("sachs", "sách"),
        ("quyeets", "quyết"),
    ] {
        let mut s = active_session(true);
        assert_eq!(type_then_commit(&mut s, keys), expected, "{keys}");
    }
    // A non-stop coda keeps every tone: sống, tiếng, muốn are all fine.
    for (keys, expected) in [("soongs", "sống"), ("tieengs", "tiếng"), ("laf", "là")] {
        let mut s = active_session(true);
        assert_eq!(type_then_commit(&mut s, keys), expected, "{keys}");
    }
}

/// `hồngu` is not a Vietnamese syllable, so committing it restores the keys.
///
/// Added after a user reported "hồngu does not auto revert" on Windows. It does —
/// but only at the word boundary, which is what `auto_fix` means. Mid-word the
/// composition stands, because the next key could still make it a real word.
/// This pins the distinction so the answer lives in the suite rather than in a
/// conversation.
#[test]
fn an_invalid_syllable_restores_at_the_boundary_not_before() {
    let mut session = active_session(true);
    // The boundary is what triggers the check, and it restores the raw keys.
    assert_eq!(type_then_commit(&mut session, "hoongfu"), "hoongfu");
}

/// **`wasm` must survive as `wasm`.** Reported 2026-09-06.
///
/// In Telex the keys apply faithfully — `w`→ư, `a`, `s`→sắc, `m` — and produce
/// `ưám`, which Vietnamese cannot spell: `ưa` is the open diphthong and takes no
/// coda, so the closed form has to be written `ươ` (`ươm`, `mượn`). `vi`'s
/// validator accepted it, so auto-fix was told the word was fine and left it.
///
/// The same shape hit `wast` (`ưát`) and `wasp` (`ưáp`). Both carry sắc, which is
/// legal on a stop coda, so the older stop-coda tone rule could not catch them
/// either — it took the nucleus rule to reach these.
#[test]
fn restores_words_whose_render_closes_an_open_diphthong() {
    for word in ["wasm", "wast", "wasp"] {
        let mut s = active_session(true);
        assert_eq!(type_then_commit(&mut s, word), word, "{word} must survive");
    }
}

/// And the words the rule must not touch still render as Vietnamese.
///
/// `mưa` is the open form, where `ưa` is the only legal spelling; `mượn` is the
/// same diphthong closed and spelled correctly. A rule that rejected either
/// would break ordinary typing, which is the real risk in a rule that rejects.
#[test]
fn keeps_the_open_diphthong_and_its_closed_spelling() {
    // All three are Telex key sequences. A literal `ư` would be a *word
    // boundary* (`is_syllable_char` takes ASCII letters only), so a pair like
    // ("mưa", "mưa") passes by character echo without ever reaching the rule.
    for (keys, expected) in [("muwa", "mưa"), ("muaw", "mưa"), ("muwonj", "mượn")] {
        let mut s = active_session(true);
        assert_eq!(type_then_commit(&mut s, keys), expected, "{keys}");
    }
}

/// **English words whose render is not a Vietnamese rime at all.** Reported one
/// at a time; fixed together by the rime table.
///
/// `using`→`uíng` was the last of these to get its own hand-written rule, and
/// the rest were still live the day after it shipped: the keys apply faithfully
/// and produce a syllable Vietnamese cannot spell, `vi`'s validator calls it
/// valid, and auto-fix is told the word is fine. They have nothing else in
/// common — different nuclei, different codas, sắc and hỏi and no tone at all —
/// which is why no rule short of the inventory reached them.
#[test]
fn restores_english_words_whose_render_is_not_a_vietnamese_rime() {
    for word in [
        "using", "rising", "basic", "music", "raising", // -ing and -ic: no such rime
        "power", "west", "two", "law", "person", // pởe, ưét, tưo, lă, peón
    ] {
        let mut s = active_session(true);
        assert_eq!(type_then_commit(&mut s, word), word, "{word} must survive");
    }
    // Reported mid-word as `buín`: `uin` is not a rime either, and the boundary
    // restores it. The whole of `business` is a *different* gap — `ss` collapses
    // to `s` and the render is pure ASCII, which the verbatim guard leaves alone.
    let mut s = active_session(true);
    assert_eq!(type_then_commit(&mut s, "busin"), "busin");
}

/// And the rimes `ng`/`c` really does close still render as Vietnamese.
///
/// `tiếng` and `việc` are the words at risk: they contain an `i`, but the vowel
/// the coda closes is the `ê` beside it. A rule matching the first vowel of the
/// syllable rather than the one next to the coda would break both.
#[test]
fn keeps_the_rimes_a_velar_coda_can_close() {
    for (keys, expected) in [
        ("tieengs", "tiếng"),
        ("vieecj", "việc"),
        ("xuoongs", "xuống"),
        ("nuowcs", "nước"),
        ("kinhs", "kính"),
        ("uynhr", "uỷnh"),
    ] {
        let mut s = active_session(true);
        assert_eq!(type_then_commit(&mut s, keys), expected, "{keys}");
    }
}

/// A restore hands back the word, not the rejection keystroke.
///
/// `choose` is typed `chooose` in Telex: the third `o` stops `oo` becoming `ô`
/// and puts nothing on screen. The render, `choóe`, is not Vietnamese, so
/// auto-fix restores — and it used to restore the key log verbatim, handing back
/// the very `chooose` the user had just worked to avoid. Reported 2026-09-11.
#[test]
fn a_restore_does_not_hand_back_the_rejection_keystroke() {
    for (keys, expected) in [("chooose", "choose"), ("chooosee", "choosee")] {
        let mut s = active_session(true);
        assert_eq!(type_then_commit(&mut s, keys), expected, "{keys}");
    }
}

/// The rejection gesture itself is untouched: a word the cancel makes *valid*
/// Vietnamese keeps every key, because nothing restores it.
#[test]
fn the_rejection_gesture_still_types_its_vietnamese_words() {
    for (keys, expected) in [
        ("xooongf", "xoòng"),
        ("mooosc", "moóc"),
        ("booong", "boong"),
    ] {
        let mut s = active_session(true);
        assert_eq!(type_then_commit(&mut s, keys), expected, "{keys}");
    }
}

/// English `-ore` words: the `r` is hỏi and the `e` lands behind it, leaving a
/// syllable with the `o` glide behind an onset that cannot carry it — which
/// Vietnamese does not have, though both the rime and the tone are ordinary.
/// Reported 2026-09-11.
#[test]
fn restores_words_whose_render_puts_the_o_glide_behind_a_closed_onset() {
    for word in ["more", "bore", "core", "store", "moved"] {
        let mut s = active_session(true);
        assert_eq!(type_then_commit(&mut s, word), word, "{word} must survive");
    }
}

/// And the glide after every other onset still renders as Vietnamese.
#[test]
fn keeps_the_o_glide_after_the_onsets_that_carry_it() {
    for (keys, expected) in [
        ("khoer", "khoẻ"),
        ("hoaf", "hoà"),
        ("xoaf", "xoà"),
        ("hoachj", "hoạch"),
        ("muoons", "muốn"),
        ("vuoong", "vuông"),
    ] {
        let mut s = active_session(true);
        assert_eq!(type_then_commit(&mut s, keys), expected, "{keys}");
    }
}

/// The mid-word spell check is auto-fix moved from the space to the offending
/// key, so auto-fix off has to switch it off too.
///
/// It used to reach the engine on its own, and the Settings window greys the
/// control out when auto-fix is off — so a word could be rewritten mid-typing by
/// a checkbox the user could see was ticked and could not untick. `aal` came
/// back as `aal`, the escape the check sets, instead of the `âl` Telex renders.
/// Reported 2026-09-16.
#[test]
fn the_mid_word_spell_check_does_nothing_with_auto_fix_off() {
    let mut s = active_session(false);
    s.set_strict_spell_check(true);
    assert_eq!(type_then_commit(&mut s, "aal"), "âl");
}

/// With auto-fix on it does its job, so the gate is the auto-fix switch and not
/// the check going missing.
#[test]
fn the_mid_word_spell_check_still_works_with_auto_fix_on() {
    let mut s = active_session(true);
    s.set_strict_spell_check(true);
    assert_eq!(type_then_commit(&mut s, "aal"), "aal");
}

/// The tick survives the trip through an auto-fix switch, because the Settings
/// control and the file both read it back off the session: a dormant checkbox
/// that silently cleared itself would lose a choice the user made.
#[test]
fn auto_fix_off_remembers_the_spell_check_tick() {
    let mut s = active_session(true);
    s.set_strict_spell_check(true);
    s.set_auto_fix(false);
    assert!(s.strict_spell_check(), "the tick is the user's, not the gate's");
    s.set_auto_fix(true);
    assert_eq!(type_then_commit(&mut s, "aal"), "aal");
}
