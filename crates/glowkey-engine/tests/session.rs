//! Tests for the ignore-list precedence — GlowKey's primary feature. These prove
//! the rule that must never regress: an excluded application does not transform,
//! and nothing (mode toggle included) overrides that.

use glowkey_engine::{ExclusionList, ExclusionToggle, InputMode, PlacementStyle, Session};

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

#[test]
fn per_app_exclusion_is_independent() {
    // Each app's enabled/disabled state is its own. Disabling app A must not
    // change app B, and B stays as it was set — not affected by A.
    let mut session = Session::new(PlacementStyle::New, ExclusionList::new());

    // In app A, disable it.
    session.set_frontmost_app("com.app.A");
    assert!(session.toggle_app_exclusion("com.app.A").excluded()); // A now excluded
    assert!(!session.is_active());

    // Switch to app B (never toggled): it is still enabled — its own state,
    // unaffected by A being disabled.
    session.set_frontmost_app("com.app.B");
    assert!(session.is_active());
    assert_eq!(type_through(&mut session, "hoongf"), "hồng");

    // Disable B independently.
    assert!(session.toggle_app_exclusion("com.app.B").excluded());
    assert!(!session.is_active());

    // Back to A: still disabled (its own remembered state), and B unaffected.
    session.set_frontmost_app("com.app.A");
    assert!(!session.is_active());

    // Re-enable A; B must remain disabled.
    assert!(!session.toggle_app_exclusion("com.app.A").excluded()); // A now enabled
    assert!(session.is_active());
    session.set_frontmost_app("com.app.B");
    assert!(!session.is_active(), "B stays disabled, independent of A");
}

#[test]
fn terminal_hotkey_unexclusion_is_session_only() {
    // Un-excluding a known terminal via the toggle works for the session but is
    // never persisted, so a restart re-excludes it (the accidental-⌃⇧E-in-Ghostty
    // protection). A non-terminal default (an editor) still removes permanently.
    let mut session = Session::new(PlacementStyle::New, ExclusionList::with_defaults());
    session.set_frontmost_app("com.mitchellh.ghostty");
    assert!(!session.is_active());

    // Toggle: enabled, but session-only.
    assert_eq!(
        session.toggle_app_exclusion("com.mitchellh.ghostty"),
        ExclusionToggle::EnabledSessionOnly
    );
    assert!(session.is_active(), "session-suspended terminal transforms");
    // The snapshot (what gets persisted) still excludes it.
    let saved = session.snapshot();
    assert!(saved.exclusions.iter().any(|id| id == "com.mitchellh.ghostty"));
    // And a fresh session from that snapshot excludes it again.
    let mut restarted = Session::from_settings(&saved);
    restarted.set_frontmost_app("com.mitchellh.ghostty");
    assert!(!restarted.is_active(), "restart must re-exclude the terminal");

    // Toggling again re-excludes immediately (lifts the suspension).
    assert_eq!(
        session.toggle_app_exclusion("com.mitchellh.ghostty"),
        ExclusionToggle::Excluded
    );
    assert!(!session.is_active());

    // An editor default is removed permanently by the same toggle.
    assert_eq!(
        session.toggle_app_exclusion("com.microsoft.VSCode"),
        ExclusionToggle::Enabled
    );
    let saved = session.snapshot();
    assert!(!saved.exclusions.iter().any(|id| id == "com.microsoft.VSCode"));
    assert!(saved
        .removed_default_exclusions
        .iter()
        .any(|id| id == "com.microsoft.VSCode"));
    // ...and the tombstone keeps it removed across a restart.
    let restarted = Session::from_settings(&saved);
    assert!(!restarted.exclusions().is_excluded("com.microsoft.VSCode"));
}

#[test]
fn permanent_terminal_removal_via_editor_still_works() {
    // The Excluded Apps window path (exclusions_mut().remove) is a deliberate,
    // permanent removal — even for a terminal — and tombstones it.
    let mut session = Session::new(PlacementStyle::New, ExclusionList::with_defaults());
    session.exclusions_mut().remove("com.apple.Terminal");
    let saved = session.snapshot();
    assert!(!saved.exclusions.iter().any(|id| id == "com.apple.Terminal"));
    let restarted = Session::from_settings(&saved);
    assert!(!restarted.exclusions().is_excluded("com.apple.Terminal"));
}

#[test]
fn auto_capitalize_handles_a_word_starting_with_a_bracket() {
    // A bracket shortcut is a vowel key, so a word can begin with one. Falling
    // through as "not a letter" left the pending capital armed and it landed on
    // the following word instead.
    let mut s = Session::new(PlacementStyle::New, ExclusionList::new());
    s.set_frontmost_app("com.apple.TextEdit");
    s.set_auto_capitalize(true);
    s.set_telex_brackets(true);

    s.note_boundary('.');
    s.process_key(' ');
    let first = s.process_key('[');
    assert_eq!(first.insert, "Ơ", "a bracket-started word takes the capital");
    s.commit();

    // And the capital is spent, so the next word is not also capitalized.
    let next = s.process_key('a');
    assert_eq!(next.insert, "a");
}

#[test]
fn changing_a_typing_option_forgets_the_re_composition_memory() {
    // The engine reset does not reach `last_committed`, so a word remembered
    // under the old setting used to re-compose under the new one and rewrite
    // text already on screen.
    let mut s = Session::new(PlacementStyle::New, ExclusionList::new());
    s.set_frontmost_app("com.apple.TextEdit");
    s.set_telex_brackets(true);
    for ch in "t[".chars() {
        s.process_key(ch);
    }
    s.commit();
    s.set_telex_brackets(false);
    assert!(
        !s.recompose_after_boundary_backspace(),
        "the committed word must not re-compose under a changed setting"
    );
}
