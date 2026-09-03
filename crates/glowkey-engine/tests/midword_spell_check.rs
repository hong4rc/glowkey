//! UniKey's second spell-check option (`spellCheckEnabled`): refuse a diacritic
//! that would make the word impossible in Vietnamese, at the keystroke — as
//! distinct from auto-fix, which restores raw keys at the word boundary.
//!
//! The whole risk is false rejections: wrongly refusing a keystroke corrupts a
//! word the user typed correctly. The corpus below is the guard, and it was run
//! before the feature was built to prove the rule was viable at all.

use glowkey_engine::{Engine, PlacementStyle};

const CORPUS: &[(&str, &str)] = &[
    ("chaof","chào"),("vieejt","việt"),("nam","nam"),("hoongf","hồng"),
    ("nguyeenx","nguyễn"),("ddaij","đại"),("hocj","học"),("sinh","sinh"),
    ("truowngf","trường"),("nguowif","người"),("ddaats","đất"),("nuowcs","nước"),
    ("ngayf","ngày"),("thangs","tháng"),("nawm","năm"),("tuooir","tuổi"),
    ("ddepj","đẹp"),("gioir","giỏi"),("khoer","khoẻ"),("camr","cảm"),
    ("own","ơn"),("looix","lỗi"),("bawts","bắt"),("ddaauf","đầu"),
    ("quyeets","quyết"),("nghieepj","nghiệp"),("thuowngr","thưởng"),
    ("cuoocj","cuộc"),("soongs","sống"),("tieengs","tiếng"),("nois","nói"),
    ("bieets","biết"),("muoons","muốn"),("nhuwng","nhưng"),("dduowcj","được"),
    ("khoong","không"),("nhieeuf","nhiều"),("theer","thể"),("nhuw","như"),
    ("cuar","của"),("nhuwngx","những"),("veef","về"),("laf","là"),
    ("mootj","một"),("cows","cớ"),("ddi","đi"),("laamf","lầm"),
    ("xuoongs","xuống"),("truowcs","trước"),("giuwax","giữa"),("ngoaif","ngoài"),
];

fn typed(input: &str, strict: bool) -> String {
    let mut engine = Engine::new(PlacementStyle::New);
    engine.set_strict_spell_check(strict);
    for ch in input.chars() {
        engine.process_key(ch);
    }
    engine.current_word().to_string()
}

#[test]
fn no_false_rejection_across_real_vietnamese() {
    // Every word must type identically with the option on and off. A difference
    // means the check refused a legitimate keystroke.
    for (keys, expected) in CORPUS {
        assert_eq!(typed(keys, false), *expected, "corpus entry {keys} is wrong");
        assert_eq!(
            typed(keys, true),
            *expected,
            "{keys} was corrupted by the mid-word spell check"
        );
    }
}

#[test]
fn off_by_default() {
    assert!(!Engine::new(PlacementStyle::New).strict_spell_check());
}

#[test]
fn the_repeat_key_escape_hatch_still_works() {
    // Pressing the diacritic key again is a deliberate rejection by the user and
    // must not be double-refused by the check.
    for (keys, expected) in [
        ("cass", "cas"),
        ("aaa", "aa"),
        ("ddd", "dd"),
        ("hoongff", "hôngf"),
    ] {
        assert_eq!(typed(keys, true), expected, "{keys} with strict check on");
    }
}

#[test]
fn english_words_are_never_refused() {
    // A pure-ASCII render is what the user typed verbatim, so the check never
    // fires on it — English stays the business of auto-fix.
    for word in ["hello", "the", "code", "print", "value"] {
        assert_eq!(typed(word, true), typed(word, false), "{word}");
    }
}

/// Drives keys against a screen that already holds text, the way a document does.
fn screen_after(prefix: &str, keys: &str, strict: bool) -> String {
    let mut engine = Engine::new(PlacementStyle::New);
    engine.set_strict_spell_check(strict);
    let mut screen: Vec<u16> = prefix.encode_utf16().collect();
    for ch in keys.chars() {
        let r = engine.process_key(ch);
        if r.handled {
            for _ in 0..r.backspaces {
                screen.pop();
            }
            screen.extend(r.insert.encode_utf16());
        } else {
            screen.push(ch as u16);
        }
    }
    String::from_utf16(&screen).unwrap()
}

#[test]
fn never_touches_the_document_before_the_word() {
    // The refusal used to render twice and diff the second against the first,
    // while the screen still held neither — so the backspace count overshot and
    // ate one character to the LEFT of the word. It hit about a quarter of
    // English words; `aal` swallowed the preceding space.
    for keys in ["aal", "vieejtw", "nguowifw", "afire", "academic", "aardvark"] {
        let out = screen_after("Xin chao ", keys, true);
        assert!(
            out.starts_with("Xin chao "),
            "{keys} destroyed the text before it: {out:?}"
        );
    }
}

#[test]
fn the_escape_does_not_outlive_the_word() {
    // Deleting an escaped word away used to leave the flag set, because
    // backspace_visible_char reported success on an emptied word so the shell
    // never flushed. Vietnamese then stayed dead for the next word.
    let mut engine = Engine::new(PlacementStyle::New);
    engine.set_strict_spell_check(true);
    for ch in "aal".chars() {
        engine.process_key(ch);
    }
    for _ in 0..3 {
        engine.backspace_visible_char();
    }
    for ch in "hoongf".chars() {
        engine.process_key(ch);
    }
    assert_eq!(engine.current_word(), "hồng");
}

#[test]
fn keeps_the_leading_d_bar_carve_out() {
    // Auto-fix exempts a leading đ so the chat abbreviations survive; the
    // mid-word check shares the predicate and must exempt them too.
    for (keys, expected) in [("ddc", "đc"), ("ddt", "đt"), ("ddk", "đk")] {
        assert_eq!(typed(keys, true), expected, "{keys} under strict check");
    }
}
