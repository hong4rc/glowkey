//! Tests for the ignore-list precedence — GlowKey's primary feature. These prove
//! the rule that must never regress: an excluded application does not transform,
//! and nothing (mode toggle included) overrides that.

use glowkey_engine::{ExclusionList, InputMode, PlacementStyle, Session};

fn session_with_excluded(bundle_id: &str) -> Session {
    let mut excl = ExclusionList::new();
    excl.add(bundle_id);
    Session::new(PlacementStyle::New, excl)
}

/// Types a word through a session and returns what the host application would end
/// up showing (handled edits applied; unhandled keys inserted verbatim).
fn type_through(session: &mut Session, input: &str) -> String {
    let mut screen = String::new();
    for ch in input.chars() {
        let r = session.process_key(ch);
        if r.handled {
            let units: Vec<u16> = screen.encode_utf16().collect();
            let keep = units.len().saturating_sub(r.backspaces);
            screen = String::from_utf16(&units[..keep]).unwrap();
            screen.push_str(&r.insert);
        } else {
            screen.push(ch);
        }
    }
    screen
}

#[test]
fn transforms_in_a_normal_app() {
    let mut session = Session::new(PlacementStyle::New, ExclusionList::new());
    session.set_frontmost_app("com.tinyspeck.slackmacgap");
    assert_eq!(type_through(&mut session, "hoongf"), "hồng");
}

#[test]
fn unknown_app_does_not_transform() {
    // Fail closed: before the shell reports the frontmost app, nothing transforms.
    let mut session = Session::new(PlacementStyle::New, ExclusionList::new());
    assert!(!session.is_active());
    assert_eq!(type_through(&mut session, "hoongf"), "hoongf");
}

#[test]
fn excluding_current_app_mid_word_does_not_corrupt() {
    // Type part of a word, then exclude the app mid-word. The next keys pass
    // through and must not splice a tone mark into already-committed text.
    let mut session = Session::new(PlacementStyle::New, ExclusionList::new());
    session.set_frontmost_app("com.tinyspeck.slackmacgap");
    let mut screen = String::new();
    for ch in "hoo".chars() {
        let r = session.process_key(ch);
        if r.handled {
            let units: Vec<u16> = screen.encode_utf16().collect();
            let keep = units.len().saturating_sub(r.backspaces);
            screen = String::from_utf16(&units[..keep]).unwrap();
            screen.push_str(&r.insert);
        } else {
            screen.push(ch);
        }
    }
    assert_eq!(screen, "hô");

    // User excludes the current app, then keeps typing.
    session.exclusions_mut().add("com.tinyspeck.slackmacgap");
    for ch in "XYZ".chars() {
        let r = session.process_key(ch);
        assert!(!r.handled, "excluded app must pass keys through");
        screen.push(ch);
    }
    // Un-exclude and type a tone key — it must start a fresh word, not edit "hô".
    session.exclusions_mut().remove("com.tinyspeck.slackmacgap");
    let r = session.process_key('f');
    let units: Vec<u16> = screen.encode_utf16().collect();
    let keep = units.len().saturating_sub(r.backspaces);
    screen = String::from_utf16(&units[..keep]).unwrap();
    screen.push_str(&r.insert);
    assert_eq!(
        screen, "hôXYZf",
        "no character deleted or spliced mid-document"
    );
}

#[test]
fn excluded_app_never_transforms() {
    let mut session = session_with_excluded("com.apple.Terminal");
    session.set_frontmost_app("com.apple.Terminal");
    // Raw ASCII passes straight through.
    assert_eq!(type_through(&mut session, "hoongf"), "hoongf");
    assert!(!session.is_active());
}

#[test]
fn exclusion_beats_the_mode_toggle() {
    // The critical rule: pressing the VN/EN hotkey inside an excluded app must not
    // re-enable Vietnamese there.
    let mut session = session_with_excluded("com.apple.Terminal");
    session.set_frontmost_app("com.apple.Terminal");
    assert!(!session.is_active());

    session.toggle_mode(); // -> English
    assert!(!session.is_active());
    session.toggle_mode(); // -> Vietnamese again
    assert!(
        !session.is_active(),
        "exclusion must still win after toggling back to VN"
    );

    assert_eq!(type_through(&mut session, "hoongf"), "hoongf");
}

#[test]
fn english_mode_passes_through_in_a_normal_app() {
    let mut session = Session::new(PlacementStyle::New, ExclusionList::new());
    session.set_frontmost_app("com.tinyspeck.slackmacgap");
    assert_eq!(session.toggle_mode(), InputMode::English);
    assert_eq!(type_through(&mut session, "hoongf"), "hoongf");
}

#[test]
fn switching_into_excluded_app_stops_transformation_immediately() {
    let mut session = Session::new(PlacementStyle::New, ExclusionList::with_defaults());
    session.set_frontmost_app("com.tinyspeck.slackmacgap");
    assert_eq!(type_through(&mut session, "hoongf "), "hồng ");

    // Focus moves to Terminal (a default exclusion). The very next key is raw.
    session.set_frontmost_app("com.apple.Terminal");
    assert_eq!(type_through(&mut session, "hoongf"), "hoongf");
}

#[test]
fn focus_change_flushes_in_progress_word() {
    let mut session = Session::new(PlacementStyle::New, ExclusionList::new());
    session.set_frontmost_app("com.tinyspeck.slackmacgap");
    session.process_key('h');
    session.process_key('o'); // mid-word
                              // Switching apps must not carry the half-typed word into the next field.
    session.set_frontmost_app("com.apple.Notes");
    assert_eq!(type_through(&mut session, "oo"), "ô");
}
