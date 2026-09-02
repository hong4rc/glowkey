//! Auto-fix: at a word boundary, restore the raw keystrokes when the Telex result
//! is not valid Vietnamese (`exit`, not `eĩt`).

use glowkey_engine::{ExclusionList, PlacementStyle, Session};

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
    // meant as English and keeps it. Documents the inherent Telex/English ambiguity.
    let mut s = active_session(true);
    assert_eq!(type_then_commit(&mut s, "was"), "ứa");
}
