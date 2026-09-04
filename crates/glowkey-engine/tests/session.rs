//! Tests for the ignore-list precedence — GlowKey's primary feature. These prove
//! the rule that must never regress: an excluded application does not transform,
//! and nothing (mode toggle included) overrides that.

use glowkey_engine::{
    BoundaryBackspace, ExclusionList, ExclusionToggle, InputMode, PlacementStyle, Session,
};

mod common;
use common::{a_terminal_default, an_editor_default};

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

    // Focus moves to a shipped default exclusion. The very next key is raw.
    session.set_frontmost_app(a_terminal_default());
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
    // never persisted, so a restart re-excludes it (the accidental-⌃⇧E-in-a-
    // terminal protection). A non-terminal default (an editor) still removes
    // permanently.
    let terminal = a_terminal_default();
    let editor = an_editor_default();
    let mut session = Session::new(PlacementStyle::New, ExclusionList::with_defaults());
    session.set_frontmost_app(terminal);
    assert!(!session.is_active());

    // Toggle: enabled, but session-only.
    assert_eq!(
        session.toggle_app_exclusion(terminal),
        ExclusionToggle::EnabledSessionOnly
    );
    assert!(session.is_active(), "session-suspended terminal transforms");
    // The snapshot (what gets persisted) still excludes it.
    let saved = session.snapshot();
    assert!(saved.exclusions.iter().any(|id| id == terminal));
    // And a fresh session from that snapshot excludes it again.
    let mut restarted = Session::from_settings(&saved);
    restarted.set_frontmost_app(terminal);
    assert!(
        !restarted.is_active(),
        "restart must re-exclude the terminal"
    );

    // Toggling again re-excludes immediately (lifts the suspension).
    assert_eq!(
        session.toggle_app_exclusion(terminal),
        ExclusionToggle::Excluded
    );
    assert!(!session.is_active());

    // An editor default is removed permanently by the same toggle.
    assert_eq!(
        session.toggle_app_exclusion(editor),
        ExclusionToggle::Enabled
    );
    let saved = session.snapshot();
    assert!(!saved.exclusions.iter().any(|id| id == editor));
    assert!(saved
        .removed_default_exclusions
        .iter()
        .any(|id| id == editor));
    // ...and the tombstone keeps it removed across a restart.
    let restarted = Session::from_settings(&saved);
    assert!(!restarted.exclusions().is_excluded(editor));
}

#[test]
fn permanent_terminal_removal_via_editor_still_works() {
    // The Excluded Apps window path (exclusions_mut().remove) is a deliberate,
    // permanent removal — even for a terminal — and tombstones it.
    let terminal = a_terminal_default();
    let mut session = Session::new(PlacementStyle::New, ExclusionList::with_defaults());
    // Asserted, not assumed: every assertion below is satisfied by an identity
    // that was never in the list, so without this the test passes vacuously —
    // which is exactly what it did on Windows while naming a macOS terminal.
    assert!(
        session.exclusions().is_excluded(terminal),
        "{terminal} must ship excluded for this test to mean anything"
    );
    assert!(session.exclusions_mut().remove(terminal));
    let saved = session.snapshot();
    assert!(!saved.exclusions.iter().any(|id| id == terminal));
    let restarted = Session::from_settings(&saved);
    assert!(!restarted.exclusions().is_excluded(terminal));
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
    assert_eq!(
        first.insert, "Ơ",
        "a bracket-started word takes the capital"
    );
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
    assert_eq!(
        s.recompose_after_boundary_backspace(),
        BoundaryBackspace::NotApplicable,
        "the committed word must not re-compose under a changed setting"
    );
}

/// The correction hotkey stays one-shot even though the committed history no
/// longer does.
///
/// The two memories used to share a lifetime and a single clearing function.
/// Splitting them is what lets a committed word survive keystrokes that are
/// later deleted — and the risk of that split is the other direction: ⌃⇧W
/// reaching back to a word the user has since typed past.
#[test]
fn the_correction_memory_stays_one_shot() {
    let mut session = Session::new(PlacementStyle::New, ExclusionList::new());
    session.set_frontmost_app("com.apple.TextEdit");

    for ch in "hoongf".chars() {
        session.process_key(ch);
    }
    let _ = session.commit();
    session.note_boundary(' ');
    // A second word starts. The first is still re-openable...
    session.process_key('s');
    // ...but it is no longer correctable: ⌃⇧W is about the word just typed.
    assert!(
        session.correct_last_word().is_none(),
        "the correction memory must not reach back past a new word"
    );
}

/// Deleting back to a committed word re-opens it, at the engine level.
///
/// The tap-level version of this is in `app/src/tap/tests.rs`; this pins the
/// engine contract the tap depends on.
#[test]
fn deleting_back_to_a_committed_word_reopens_it() {
    let mut session = Session::new(PlacementStyle::New, ExclusionList::new());
    session.set_frontmost_app("com.apple.TextEdit");

    for ch in "hoongf".chars() {
        session.process_key(ch);
    }
    let _ = session.commit();
    session.note_boundary(' ');
    assert_eq!(session.current_word(), "", "committed, nothing composing");

    // Type a word and delete it away again.
    session.process_key('s');
    assert_eq!(
        session.recompose_after_boundary_backspace(),
        BoundaryBackspace::NotApplicable,
        "mid-word"
    );
    session.backspace_visible_char();
    assert_eq!(session.current_word(), "", "the new word is gone");

    // Now the Backspace that removes the boundary re-opens the committed word.
    assert_eq!(
        session.recompose_after_boundary_backspace(),
        BoundaryBackspace::Reopened,
        "deleting the boundary must re-open the word behind it"
    );
    assert_eq!(session.current_word(), "hồng");

    // And it is live: the tone-removal key applies rather than landing literal.
    let r = session.process_key('z');
    assert!(r.handled);
    assert_eq!(session.current_word(), "hông");
}

/// A bare boundary is removed without disturbing the words behind it.
///
/// Pins the engine contract the tap relies on for `hồng, `⌫⌫`z`: the first
/// Backspace answers `BoundaryRemoved` — the host deletes the space, nothing
/// re-opens, and nothing is forgotten — and only the second reaches the word.
#[test]
fn a_bare_boundary_is_removed_before_the_word_behind_it() {
    let mut session = Session::new(PlacementStyle::New, ExclusionList::new());
    session.set_frontmost_app("com.apple.TextEdit");

    for ch in "hoongf".chars() {
        session.process_key(ch);
    }
    let _ = session.commit();
    session.note_boundary(',');
    // The space after the comma commits nothing — this is the case that used to
    // throw the history away.
    let _ = session.commit();
    session.note_boundary(' ');

    assert_eq!(
        session.recompose_after_boundary_backspace(),
        BoundaryBackspace::BoundaryRemoved,
        "the space came off; the word is one more Backspace away"
    );
    assert_eq!(session.current_word(), "", "nothing re-opened yet");
    assert_eq!(
        session.recompose_after_boundary_backspace(),
        BoundaryBackspace::Reopened,
        "the comma came off, which re-opens the word"
    );
    assert_eq!(session.current_word(), "hồng");
}

/// Deleting back past what the engine remembers forgets the caret position in
/// the engine rather than trusting the caller to flush.
///
/// `work ` is restored to its raw keys by auto-fix, which clears the
/// re-composition stack while deliberately leaving the word correctable. The
/// Backspace that follows deletes the boundary character — so the correction
/// edit, which reaches back over that character and puts it back, would now
/// over-delete by one and strand text. The engine has to end the correction
/// window itself: the caller reaching this point calls `flush`, but that is the
/// caller's contract, and this is the engine's.
#[test]
fn deleting_back_past_the_history_ends_the_correction_window() {
    let mut session = Session::new(PlacementStyle::New, ExclusionList::new());
    session.set_frontmost_app("com.apple.TextEdit");

    for ch in "work".chars() {
        session.process_key(ch);
    }
    assert!(
        session.commit().is_some(),
        "auto-fix must restore `ưork` to `work` for this test to be about anything"
    );
    session.note_boundary(' ');
    assert!(
        session.correctable_word().is_some(),
        "a restored word stays correctable"
    );

    assert_eq!(
        session.recompose_after_boundary_backspace(),
        BoundaryBackspace::NotApplicable,
        "a restored word left nothing to re-open"
    );
    assert!(
        session.correct_last_word().is_none(),
        "the Backspace deleted the boundary the correction would put back"
    );
}
