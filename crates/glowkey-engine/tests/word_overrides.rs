//! Per-word decisions about the English/Telex ambiguity.
//!
//! `docs/handoff.md` §6.3 records that ambiguity as inherent: the same keystrokes
//! are legitimate Vietnamese and legitimate English, so `was` is `ứa` and `cats`
//! is `cát`, and no blind rule decides which the user meant. Before this, the
//! only answer was one global switch whose trade-off made a dozen ordinary
//! Vietnamese words untypeable in their natural key order.
//!
//! These tests pin the pairs that *could not both work* under that switch. That
//! is the whole point of the feature, so it is what the tests are about.

use glowkey_engine::{BoundaryBackspace, ExclusionList, PlacementStyle, Session, WordPreference};

/// An app that is not excluded, so the session transforms. `is_active` fails
/// closed on an unknown app, so this is not optional.
const TEST_APP: &str = "com.apple.TextEdit";

fn session() -> Session {
    let mut s = Session::new(PlacementStyle::New, ExclusionList::new());
    s.set_frontmost_app(TEST_APP);
    s
}

/// Types `word` then a space, and returns what the document would show.
fn type_and_commit(session: &mut Session, word: &str) -> String {
    let mut screen = String::new();
    for ch in word.chars() {
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
    if let Some(r) = session.commit() {
        let units: Vec<u16> = screen.encode_utf16().collect();
        let keep = units.len().saturating_sub(r.backspaces);
        screen = String::from_utf16(&units[..keep]).unwrap();
        screen.push_str(&r.insert);
    }
    screen
}

/// The pair that motivates the whole feature. With the global English-restore
/// switch these two cannot both be right: on, `cats` wins and `cát` is
/// untypeable; off, `cát` wins and `was` comes out as `ứa`.
#[test]
fn the_two_words_a_global_switch_cannot_both_get_right() {
    let mut s = session();
    // Global switch OFF — the shipped default.
    assert!(!s.restore_english_words());
    s.set_word_override("was", WordPreference::Raw);

    assert_eq!(type_and_commit(&mut s, "was"), "was");
    assert_eq!(type_and_commit(&mut s, "cats"), "cát");
}

#[test]
fn an_override_wins_with_the_global_switch_on_too() {
    let mut s = session();
    s.set_restore_english_words(true);
    s.set_word_override("cats", WordPreference::Vietnamese);

    // The global switch would restore `cats`; the override says otherwise.
    assert_eq!(type_and_commit(&mut s, "cats"), "cát");
    // A word with no override still obeys the switch.
    assert_eq!(type_and_commit(&mut s, "was"), "was");
}

/// Pinning a word to Vietnamese beats auto-fix, even when the result is not
/// valid Vietnamese. Perverse, and deliberately allowed: it is an explicit
/// instruction, and the code must not second-guess the person typing.
#[test]
fn an_override_beats_auto_fix_even_when_the_result_is_invalid() {
    let mut s = session();
    assert!(s.auto_fix());
    // Without the override, auto-fix rescues this.
    assert_eq!(type_and_commit(&mut s, "exit"), "exit");

    s.set_word_override("exit", WordPreference::Vietnamese);
    let rendered = type_and_commit(&mut s, "exit");
    assert_ne!(rendered, "exit", "the override must beat auto-fix");
    assert_eq!(rendered, "eĩt");
}

/// A macro is a stronger statement than a word preference — the user wrote it
/// out explicitly — and `commit` checks macros first. That order must hold.
#[test]
fn a_macro_still_wins_over_an_override() {
    let mut s = session();
    s.add_macro("vn", "Việt Nam");
    s.set_word_override("vn", WordPreference::Raw);
    assert_eq!(type_and_commit(&mut s, "vn"), "Việt Nam");
}

/// An override whose two readings are identical must emit nothing, exactly as
/// the plain restore does. A no-op edit still costs a backspace round-trip in
/// the host, and in a Chromium omnibox it would wake the accessibility guard.
#[test]
fn an_override_that_changes_nothing_emits_nothing() {
    let mut s = session();
    // `man` renders as itself — no transformation to disagree about.
    s.set_word_override("man", WordPreference::Raw);
    for ch in "man".chars() {
        s.process_key(ch);
    }
    assert!(s.commit().is_none());
}

#[test]
fn overrides_are_case_insensitive_on_both_write_and_read() {
    let mut s = session();
    s.set_word_override("WAS", WordPreference::Raw);
    assert_eq!(s.word_override("was"), Some(WordPreference::Raw));
    // A capital at a sentence start must obey the same decision.
    assert_eq!(type_and_commit(&mut s, "Was"), "Was");
}

#[test]
fn setting_the_same_word_twice_replaces_rather_than_duplicates() {
    let mut s = session();
    s.set_word_override("cats", WordPreference::Raw);
    s.set_word_override("cats", WordPreference::Vietnamese);
    assert_eq!(s.word_override_list().len(), 1);
    assert_eq!(s.word_override("cats"), Some(WordPreference::Vietnamese));
}

#[test]
fn removing_an_override_restores_the_rule() {
    let mut s = session();
    s.set_word_override("exit", WordPreference::Vietnamese);
    assert_eq!(type_and_commit(&mut s, "exit"), "eĩt");

    assert!(s.remove_word_override("exit"));
    assert!(
        !s.remove_word_override("exit"),
        "removing twice is not an error"
    );
    assert_eq!(type_and_commit(&mut s, "exit"), "exit");
}

#[test]
fn blank_keys_are_not_recorded() {
    let mut s = session();
    s.set_word_override("   ", WordPreference::Raw);
    s.set_word_override("", WordPreference::Raw);
    assert!(s.word_override_list().is_empty());
}

/// The list is what gets written to `settings.json`, so its order must be stable
/// — otherwise every save produces a spurious diff.
#[test]
fn the_list_is_sorted_for_a_stable_settings_file() {
    let mut s = session();
    for w in ["was", "cats", "exit", "and"] {
        s.set_word_override(w, WordPreference::Raw);
    }
    let keys: Vec<String> = s.word_override_list().into_iter().map(|o| o.keys).collect();
    assert_eq!(keys, ["and", "cats", "exit", "was"]);
}

#[test]
fn overrides_survive_a_persisted_round_trip() {
    let mut s = session();
    s.set_word_override("was", WordPreference::Raw);
    s.set_word_override("cats", WordPreference::Vietnamese);

    // The list as the settings file carries it, put back into a fresh session.
    let mut restored = session();
    restored.set_word_overrides(&s.word_override_list());
    assert_eq!(restored.word_override("was"), Some(WordPreference::Raw));
    assert_eq!(
        restored.word_override("cats"),
        Some(WordPreference::Vietnamese)
    );
}

// ---------------------------------------------------------------------------
// The correction hotkey (⌃⇧W in the shell) — one keystroke that fixes the word
// just typed and records the decision, so it never has to be made again.
// ---------------------------------------------------------------------------

/// Applies an edit to a screen string the way the tap does.
fn apply(screen: &mut String, r: &glowkey_engine::KeyResponse) {
    let units: Vec<u16> = screen.encode_utf16().collect();
    let keep = units.len().saturating_sub(r.backspaces);
    *screen = String::from_utf16(&units[..keep]).unwrap();
    screen.push_str(&r.insert);
}

/// Types a word, the boundary key, and returns the screen — mirroring the tap:
/// `commit()`, apply any restore, then `note_boundary(ch)` and the key lands.
fn type_word_and_boundary(session: &mut Session, word: &str, boundary: char) -> String {
    let mut screen = String::new();
    for ch in word.chars() {
        let r = session.process_key(ch);
        if r.handled {
            apply(&mut screen, &r);
        } else {
            screen.push(ch);
        }
    }
    if let Some(r) = session.commit() {
        apply(&mut screen, &r);
    }
    session.note_boundary(boundary);
    screen.push(boundary);
    screen
}

/// The headline behaviour: type it, press the key, and it is fixed and learned.
#[test]
fn correcting_a_word_swaps_it_and_remembers_the_choice() {
    let mut s = session();
    let mut screen = type_word_and_boundary(&mut s, "was", ' ');
    assert_eq!(screen, "ứa ", "the default, before any correction");

    let edit = s.correct_last_word().expect("there is a word to correct");
    apply(&mut screen, &edit);
    assert_eq!(screen, "was ", "the boundary survives the swap");
    assert_eq!(s.word_override("was"), Some(WordPreference::Raw));

    // And the decision holds next time, with no keystroke.
    assert_eq!(type_and_commit(&mut s, "was"), "was");
}

/// The direction follows what is on screen, not a fixed side — so the same key
/// corrects in both directions depending on what happened.
#[test]
fn correction_direction_follows_what_is_on_screen() {
    let mut s = session();
    // Auto-fix restored this one, so the raw keys are on screen.
    let mut screen = type_word_and_boundary(&mut s, "exit", ' ');
    assert_eq!(screen, "exit ");

    let edit = s.correct_last_word().expect("correctable");
    apply(&mut screen, &edit);
    assert_eq!(screen, "eĩt ", "corrects towards Vietnamese this time");
    assert_eq!(s.word_override("exit"), Some(WordPreference::Vietnamese));
}

/// One-shot on purpose: a second press must not toggle back, because it would be
/// recorded as a fresh decision and the list would learn whichever direction the
/// user happened to stop on.
#[test]
fn a_second_correction_does_nothing() {
    let mut s = session();
    type_word_and_boundary(&mut s, "was", ' ');
    assert!(s.correct_last_word().is_some());
    assert!(s.correct_last_word().is_none(), "must be one-shot");
    // And the first decision stands.
    assert_eq!(s.word_override("was"), Some(WordPreference::Raw));
}

/// Everything that could have moved the caret must make the key inert: the blind
/// model cannot check where the caret is, so "does nothing" is the only safe
/// failure mode for an edit that reaches back over a boundary.
#[test]
fn anything_that_moves_the_caret_makes_the_correction_inert() {
    // A flush — what the tap does on a mouse click, an arrow key, or an app switch.
    let mut s = session();
    type_word_and_boundary(&mut s, "was", ' ');
    s.flush();
    assert!(
        s.correct_last_word().is_none(),
        "flush must forget the word"
    );

    // Starting the next word.
    let mut s = session();
    type_word_and_boundary(&mut s, "was", ' ');
    s.process_key('h');
    assert!(
        s.correct_last_word().is_none(),
        "a new word means the caret is no longer after the old one"
    );

    // Deleting the boundary to re-compose.
    let mut s = session();
    type_word_and_boundary(&mut s, "hoongf", ' ');
    assert_eq!(
        s.recompose_after_boundary_backspace(),
        BoundaryBackspace::Reopened
    );
    assert!(s.correct_last_word().is_none());

    // Changing a setting that would re-render the word differently.
    let mut s = session();
    type_word_and_boundary(&mut s, "was", ' ');
    s.set_quick_telex(true);
    assert!(s.correct_last_word().is_none());
}

/// A word with only one reading has nothing to correct, so the key stays inert
/// rather than emitting a no-op edit.
#[test]
fn a_word_that_renders_as_itself_is_not_correctable() {
    let mut s = session();
    assert_eq!(type_word_and_boundary(&mut s, "man", ' '), "man ");
    assert!(s.correct_last_word().is_none());
}

/// A macro expansion is not a word with two readings.
#[test]
fn a_macro_expansion_is_not_correctable() {
    let mut s = session();
    s.add_macro("vn", "Việt Nam");
    type_word_and_boundary(&mut s, "vn", ' ');
    assert!(s.correct_last_word().is_none());
}

/// The boundary is put back exactly as typed, for every boundary the host
/// actually inserts — including sentence punctuation, which also has to keep
/// priming the next capital.
#[test]
fn an_inserted_boundary_character_is_preserved_whatever_it_was() {
    for boundary in ['.', ',', '!', ' ', ';', ')'] {
        let mut s = session();
        let mut screen = type_word_and_boundary(&mut s, "was", boundary);
        let edit = s.correct_last_word().expect("correctable");
        apply(&mut screen, &edit);
        assert_eq!(screen, format!("was{boundary}"));
    }
}

/// A boundary key that inserts **nothing** at the caret must leave nothing to
/// correct.
///
/// This test previously asserted the opposite for `\t`, which encoded a bug as
/// the specification. Several keys reach the boundary path while putting no text
/// at the caret — Escape, the function keys, keypad Enter, Help and
/// forward-delete all arrive as control characters — and charging a backspace
/// for one of them ate the space belonging to the *previous* word and typed a
/// control code into the document:
///
/// ```text
/// "xin chào ứa"  + Escape + ⌃⇧W  →  "xin chàowas\u{1b}"
/// ```
///
/// Tab and Return are control characters too, and worse: they move the caret
/// entirely, so a correction after Tab posted its edit into the next field, and
/// after Return in a send-on-enter application into a message already sent.
#[test]
fn a_boundary_key_that_inserts_nothing_leaves_nothing_to_correct() {
    for boundary in [
        '\u{1b}', // Escape
        '\u{10}', // a function key
        '\u{3}',  // keypad Enter
        '\u{5}',  // Help
        '\u{7f}', // forward delete
        '\t',     // Tab — moves focus
        '\r',     // Return — may send, and moves the caret
        '\n',
    ] {
        let mut s = session();
        type_word_and_boundary(&mut s, "was", boundary);
        assert!(
            s.correct_last_word().is_none(),
            "{boundary:?} inserts nothing at the caret, so there is nothing to correct"
        );
    }
}

/// Belt and braces for the worst of that class: the edit must never reach back
/// into the previous word. Before the fix this produced `"xin chàowas\u{1b}"`.
#[test]
fn escape_after_a_word_cannot_eat_the_preceding_space() {
    let mut s = session();
    let mut screen = String::from("xin chào ");
    for ch in "was".chars() {
        let r = s.process_key(ch);
        if r.handled {
            apply(&mut screen, &r);
        }
    }
    if let Some(r) = s.commit() {
        apply(&mut screen, &r);
    }
    s.note_boundary('\u{1b}'); // Escape: dismisses a popover, inserts nothing
    assert_eq!(screen, "xin chào ứa");
    assert!(s.correct_last_word().is_none());
}

/// The edit must delete exactly what is on screen and no more. An over-delete
/// here would eat the preceding word — the worst failure this code can produce.
#[test]
fn the_correction_never_deletes_more_than_the_word_and_its_boundary() {
    let mut s = session();
    let mut screen = String::from("xin chào ");
    // Type a second word after existing text.
    for ch in "was".chars() {
        let r = s.process_key(ch);
        if r.handled {
            apply(&mut screen, &r);
        }
    }
    if let Some(r) = s.commit() {
        apply(&mut screen, &r);
    }
    s.note_boundary(' ');
    screen.push(' ');
    assert_eq!(screen, "xin chào ứa ");

    let edit = s.correct_last_word().expect("correctable");
    assert_eq!(
        edit.backspaces,
        "ứa ".encode_utf16().count(),
        "exactly the on-screen word plus its boundary"
    );
    apply(&mut screen, &edit);
    assert_eq!(screen, "xin chào was ", "the preceding text is untouched");
}

/// After a correction the word is **not** re-composable, and forgetting that
/// corrupted the document in three ordinary keystrokes.
///
/// The correction used to clear only its own memory, leaving `last_committed`
/// describing a word that was no longer on screen. The following Backspace then
/// re-composed the *old* rendering, and the next letter was diffed against a
/// baseline that no longer matched:
///
/// ```text
/// was␣      → "ứa "
/// ⌃⇧W       → "was "
/// ⌫         → "was"   but the engine re-composed "ứa"
/// f         → emits bs=2 ins="ừa"  →  "wừa"
/// ```
#[test]
fn a_corrected_word_is_no_longer_recomposable() {
    let mut s = session();
    let mut screen = type_word_and_boundary(&mut s, "was", ' ');
    let edit = s.correct_last_word().expect("correctable");
    apply(&mut screen, &edit);
    assert_eq!(screen, "was ");

    // The host deletes the boundary. The engine must NOT reopen the old word.
    assert_eq!(
        s.recompose_after_boundary_backspace(),
        BoundaryBackspace::NotApplicable,
        "a corrected word must not re-compose — its identity on screen has changed"
    );
    screen.pop();
    assert_eq!(screen, "was");

    // And the next keystroke starts a fresh word rather than editing a ghost.
    let r = s.process_key('f');
    assert_eq!(
        r.backspaces, 0,
        "nothing on screen belongs to the engine yet"
    );
    apply(&mut screen, &r);
    assert_eq!(screen, "wasf");
}

/// Switching application must forget the word: an app that activates itself — a
/// call popup, a finished build, `open -a` — changes focus with no event GlowKey
/// can flush on, and the correction would then post its edit into the new app.
#[test]
fn switching_app_forgets_the_correctable_word() {
    let mut s = session();
    type_word_and_boundary(&mut s, "was", ' ');
    s.set_frontmost_app("com.tinyspeck.slackmacgap");
    assert!(s.correct_last_word().is_none());
}

/// Same for the mode toggle and the per-app toggle, which also reset the engine.
#[test]
fn toggling_mode_or_exclusion_forgets_the_correctable_word() {
    let mut s = session();
    type_word_and_boundary(&mut s, "was", ' ');
    s.toggle_mode();
    assert!(s.correct_last_word().is_none());

    let mut s = session();
    type_word_and_boundary(&mut s, "was", ' ');
    s.toggle_app_exclusion(TEST_APP);
    assert!(s.correct_last_word().is_none());
}
