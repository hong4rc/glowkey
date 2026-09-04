//! End-to-end tests driving the real tap decision path with real CoreGraphics
//! key events (real CGEvent objects, real Unicode decode, real engine). This
//! covers everything except the system-level tap install and event injection,
//! which require Accessibility permission a test process cannot grant.

// `use super::*` reached everything while all of this lived in one file. The
// split means the siblings have to be named — the only change the move made to
// this file; every test body below is byte-identical to before.
use super::decide::Decision;
use super::emit::is_chromium_browser;
use super::keys::{
    is_toggle_hotkey, KEY_CODE_DELETE, KEY_CODE_E, KEY_CODE_ESCAPE, KEY_CODE_SPACE, KEY_CODE_Z,
};
use super::*;
use glowkey_engine::{ExclusionToggle, HotkeyPreset, KeyResponse};
use objc2_core_graphics::CGEventFlags;

/// Builds a real key-down CGEvent from GlowKey's source carrying `ch` as its
/// Unicode string (keycode 0, no modifiers) — what the tap would see for a
/// letter typed on the active layout.
fn key_event(source: &CGEventSource, ch: char) -> CFRetained<CGEvent> {
    let event = CGEvent::new_keyboard_event(Some(source), 0, true).expect("event");
    let utf16: Vec<u16> = ch.to_string().encode_utf16().collect();
    unsafe {
        CGEvent::keyboard_set_unicode_string(Some(&event), utf16.len() as u64, utf16.as_ptr());
    }
    event
}

/// Builds a real Backspace key-down event (virtual keycode 51).
fn backspace_event(source: &CGEventSource) -> CFRetained<CGEvent> {
    CGEvent::new_keyboard_event(Some(source), KEY_CODE_DELETE as u16, true).expect("event")
}

/// Builds a real key-down event for a caret-navigation key by virtual keycode
/// (e.g. Left = 123), with no Unicode string — as the tap sees an arrow key.
fn nav_event(source: &CGEventSource, keycode: u16) -> CFRetained<CGEvent> {
    CGEvent::new_keyboard_event(Some(source), keycode, true).expect("event")
}

/// Types `input` through the real `decide()` path and returns the resulting
/// on-screen text, applying each Decision exactly as the OS would.
fn type_via_tap(state: &TapState, input: &str) -> String {
    let mut screen = String::new();
    for ch in input.chars() {
        let event = key_event(&state.source, ch);
        let ptr = NonNull::from(&*event);
        let apply = |screen: &mut String, r: &KeyResponse| {
            let units: Vec<u16> = screen.encode_utf16().collect();
            let keep = units.len().saturating_sub(r.backspaces);
            *screen = String::from_utf16(&units[..keep]).unwrap();
            screen.push_str(&r.insert);
        };
        match state.decide(ptr) {
            Decision::Passthrough => screen.push(ch),
            Decision::Consume | Decision::ToggleApp => {}
            Decision::Emit(r) => apply(&mut screen, &r),
            Decision::EmitThenReplayKey(r) => {
                apply(&mut screen, &r);
                screen.push(ch); // the boundary key still types
            }
        }
    }
    screen
}

/// The two shapes reported from the field. An auto-fix restore at a boundary
/// must leave the raw keys followed by the boundary key, in that order. While
/// the boundary key was passed through natively instead of replayed, the host
/// applied it before the posted backspaces and the edit ate it: `ddc`␣ came out
/// `đddc` and `work`␣ came out `ưwork`, both with the space swallowed.
#[test]
fn auto_fix_restore_keeps_the_boundary_key() {
    assert_eq!(type_via_tap(&active_state(), "work "), "work ");
    // A leading đ is exempt from auto-fix, so this one commits with no restore
    // — the boundary key must survive that path too.
    assert_eq!(type_via_tap(&active_state(), "ddc "), "đc ");
}

/// Pins the mechanism, not just the result: the boundary key that triggers an
/// auto-fix restore must be suppressed and replayed, never left to race the
/// edit as a plain passthrough.
#[test]
fn auto_fix_boundary_replays_the_key_rather_than_passing_it_through() {
    let state = active_state();
    for ch in "work".chars() {
        let event = key_event(&state.source, ch);
        state.decide(NonNull::from(&*event));
    }
    let space = key_event(&state.source, ' ');
    match state.decide(NonNull::from(&*space)) {
        Decision::EmitThenReplayKey(_) => {}
        other => panic!("boundary key must be replayed, got {other:?}"),
    }
}

fn active_state() -> TapState {
    let state = TapState::new().expect("event source");
    // A non-excluded app so transformation is active.
    state
        .session
        .borrow_mut()
        .set_frontmost_app("com.apple.TextEdit");
    state
}

#[test]
fn real_events_free_tone_placement() {
    // The headline: real key events, tone key in any position → hồng.
    assert_eq!(type_via_tap(&active_state(), "hoongf"), "hồng");
    assert_eq!(type_via_tap(&active_state(), "hofong"), "hồng");
    assert_eq!(type_via_tap(&active_state(), "hoonfg"), "hồng");
    // Multi-transform word through the real emit path (w horns uo→ươ, f tones).
    assert_eq!(type_via_tap(&active_state(), "nguoiwf"), "người");
    // The user's second example, exactly as typed:
    assert_eq!(type_via_tap(&active_state(), "hofngo"), "hồng");
}

#[test]
fn real_events_words_and_english() {
    assert_eq!(type_via_tap(&active_state(), "nguyeenx"), "nguyễn");
    assert_eq!(type_via_tap(&active_state(), "dduwowcj"), "được");
    assert_eq!(type_via_tap(&active_state(), "Hoongf"), "Hồng");
    // English passes through untouched (fast path).
    assert_eq!(type_via_tap(&active_state(), "hello"), "hello");
}

#[test]
fn real_events_boundary_commits_word() {
    // Space is a boundary: the word is already on screen, space passes through.
    assert_eq!(type_via_tap(&active_state(), "hoongf "), "hồng ");
    // Without deleting the space, a following key starts a NEW word — z is
    // literal, not a modifier of the previous word.
    assert_eq!(type_via_tap(&active_state(), "hoongf z"), "hồng z");
}

#[test]
fn toggle_hotkey_presets_match_only_their_combo() {
    let ctrl_shift = CGEventFlags(CGEventFlags::MaskControl.0 | CGEventFlags::MaskShift.0);
    let ctrl = CGEventFlags(CGEventFlags::MaskControl.0);
    let option = CGEventFlags(CGEventFlags::MaskAlternate.0);

    assert!(is_toggle_hotkey(
        ctrl_shift,
        KEY_CODE_SPACE,
        HotkeyPreset::CtrlShiftSpace
    ));
    assert!(!is_toggle_hotkey(
        ctrl,
        KEY_CODE_SPACE,
        HotkeyPreset::CtrlShiftSpace
    ));

    assert!(is_toggle_hotkey(
        ctrl,
        KEY_CODE_SPACE,
        HotkeyPreset::CtrlSpace
    ));
    // Shift must NOT be held for the plain ⌃Space preset.
    assert!(!is_toggle_hotkey(
        ctrl_shift,
        KEY_CODE_SPACE,
        HotkeyPreset::CtrlSpace
    ));

    assert!(is_toggle_hotkey(
        option,
        KEY_CODE_SPACE,
        HotkeyPreset::OptionSpace
    ));

    assert!(is_toggle_hotkey(
        ctrl_shift,
        KEY_CODE_Z,
        HotkeyPreset::CtrlShiftZ
    ));
    // Right modifiers, wrong key.
    assert!(!is_toggle_hotkey(
        ctrl_shift,
        KEY_CODE_SPACE,
        HotkeyPreset::CtrlShiftZ
    ));
}

#[test]
fn real_events_arrow_key_flushes_engine() {
    // An arrow key mid-word must flush (so a stale baseline can't corrupt later
    // edits) and pass through — never emit an edit.
    let state = active_state();
    for ch in "hoo".chars() {
        let event = key_event(&state.source, ch);
        let _ = state.decide(NonNull::from(&*event));
    }
    assert!(state.session.borrow().is_composing());

    let left = nav_event(&state.source, 123); // Left arrow
    assert!(matches!(
        state.decide(NonNull::from(&*left)),
        Decision::Passthrough
    ));
    assert!(
        !state.session.borrow().is_composing(),
        "arrow key must flush the composing word"
    );
}

#[test]
fn real_events_recompose_after_space_backspace() {
    // hồng, Space, Backspace (delete the space), then z (Telex tone-clear) must
    // re-compose the previous word: hồng + z → hông.
    let state = active_state();
    let mut screen = String::new();
    let apply = |screen: &mut String, r: &KeyResponse| {
        let units: Vec<u16> = screen.encode_utf16().collect();
        let keep = units.len().saturating_sub(r.backspaces);
        *screen = String::from_utf16(&units[..keep]).unwrap();
        screen.push_str(&r.insert);
    };

    for ch in "hoongf".chars() {
        let event = key_event(&state.source, ch);
        match state.decide(NonNull::from(&*event)) {
            Decision::Passthrough => screen.push(ch),
            Decision::Emit(r) => apply(&mut screen, &r),
            other => panic!("unexpected {other:?} for {ch}"),
        }
    }
    assert_eq!(screen, "hồng");

    // Space — boundary commits the (valid) word and passes through.
    let space = key_event(&state.source, ' ');
    match state.decide(NonNull::from(&*space)) {
        Decision::Passthrough => screen.push(' '),
        Decision::EmitThenReplayKey(r) => {
            apply(&mut screen, &r);
            screen.push(' ');
        }
        other => panic!("unexpected {other:?} for space"),
    }
    assert_eq!(screen, "hồng ");

    // Backspace — passes through (host deletes the space); engine re-composes.
    let backspace = backspace_event(&state.source);
    match state.decide(NonNull::from(&*backspace)) {
        Decision::Passthrough => {
            screen.pop(); // host deletes the trailing space
        }
        other => panic!("backspace should pass through, got {other:?}"),
    }
    assert_eq!(screen, "hồng");

    // z — now edits the re-composed word: hồng → hông.
    let z = key_event(&state.source, 'z');
    match state.decide(NonNull::from(&*z)) {
        Decision::Emit(r) => apply(&mut screen, &r),
        Decision::Passthrough => screen.push('z'),
        other => panic!("unexpected {other:?} for z"),
    }
    assert_eq!(screen, "hông");
}

#[test]
fn real_events_backspace_deletes_last_visible_char() {
    let state = active_state();
    assert_eq!(type_via_tap(&state, "hoongf"), "hồng");
    assert!(state.session.borrow().is_composing());

    // Backspace passes through (the host deletes the last visible character,
    // hồng → hồn) and the engine shrinks with it, staying composed so the next
    // key is still a Telex key: z removes the tone rather than typing a literal.
    let bs = backspace_event(&state.source);
    assert!(matches!(
        state.decide(NonNull::from(&*bs)),
        Decision::Passthrough
    ));
    assert!(state.session.borrow().is_composing());
    let (raw, rendered, _, _) = state.session.borrow().debug_state();
    assert_eq!((raw.as_str(), rendered.as_str()), ("hoonf", "hồn"));

    let z = key_event(&state.source, 'z');
    match state.decide(NonNull::from(&*z)) {
        Decision::Emit(r) => {
            let mut screen = String::from("hồn");
            let units: Vec<u16> = screen.encode_utf16().collect();
            screen = String::from_utf16(&units[..units.len() - r.backspaces]).unwrap();
            screen.push_str(&r.insert);
            assert_eq!(screen, "hôn");
        }
        other => panic!("z after a mid-word backspace must edit the word, got {other:?}"),
    }
}

#[test]
fn real_events_shortcut_flushes_engine() {
    // ⌘A (select-all) changes the selection; the engine must flush so the next
    // keystroke is not diffed against a stale baseline (the select-all → hoồng
    // bug). A ⌘-shortcut passes through and clears composing state.
    let state = active_state();
    assert_eq!(type_via_tap(&state, "hoong"), "hông");
    assert!(state.session.borrow().is_composing());

    let event = CGEvent::new_keyboard_event(Some(&state.source), 0, true).expect("event");
    CGEvent::set_flags(Some(&event), CGEventFlags(CGEventFlags::MaskCommand.0));
    assert!(matches!(
        state.decide(NonNull::from(&*event)),
        Decision::Passthrough
    ));
    assert!(!state.session.borrow().is_composing());
}

#[test]
fn real_events_excluded_app_passes_through() {
    let state = TapState::new().expect("source");
    state
        .session
        .borrow_mut()
        .set_frontmost_app("com.apple.Terminal"); // default exclusion
    assert_eq!(type_via_tap(&state, "hoongf"), "hoongf");
}

/// A real ⌃⇧ + `keycode` key event.
fn ctrl_shift_event(source: &CGEventSource, keycode: u16) -> CFRetained<CGEvent> {
    let event = CGEvent::new_keyboard_event(Some(source), keycode, true).expect("event");
    let flags = CGEventFlags(CGEventFlags::MaskControl.0 | CGEventFlags::MaskShift.0);
    CGEvent::set_flags(Some(&event), flags);
    event
}

/// A real Control+Shift+Space key event.
fn toggle_event(source: &CGEventSource) -> CFRetained<CGEvent> {
    ctrl_shift_event(source, KEY_CODE_SPACE as u16)
}

#[test]
fn real_events_app_toggle_hotkey() {
    // ⌃⇧E toggles the current app's ignore-list membership and consumes the key.
    let state = active_state(); // frontmost = TextEdit, not excluded
    assert_eq!(type_via_tap(&state, "hoongf"), "hồng");

    let ev = ctrl_shift_event(&state.source, KEY_CODE_E as u16);
    assert!(matches!(
        state.decide(NonNull::from(&*ev)),
        Decision::ToggleApp
    ));
    // Applying the toggle (as handle_key_down does) excludes TextEdit.
    assert!(state
        .session
        .borrow_mut()
        .toggle_app_exclusion("com.apple.TextEdit")
        .excluded());
    assert_eq!(type_via_tap(&state, "hoongf"), "hoongf");
}

#[test]
fn chromium_browsers_are_classified_by_prefix() {
    assert!(is_chromium_browser("com.google.Chrome"));
    assert!(is_chromium_browser("com.google.Chrome.canary"));
    assert!(is_chromium_browser("com.microsoft.edgemac"));
    assert!(is_chromium_browser("com.brave.Browser"));
    // The omnibox guard must never run outside Chromium browsers.
    assert!(!is_chromium_browser("com.apple.Safari"));
    assert!(!is_chromium_browser("org.mozilla.firefox"));
    assert!(!is_chromium_browser("com.apple.TextEdit"));
}

/// A key event with arbitrary modifier flags.
fn flagged_event(source: &CGEventSource, keycode: u16, flags: CGEventFlags) -> CFRetained<CGEvent> {
    let event = CGEvent::new_keyboard_event(Some(source), keycode, true).expect("event");
    CGEvent::set_flags(Some(&event), flags);
    event
}

#[test]
fn hotkey_recording_captures_a_custom_combo() {
    let state = active_state();
    state.begin_hotkey_recording();
    assert!(state.is_recording_hotkey());

    // A plain letter passes through (typing must never be blocked by an
    // armed recorder) and recording continues.
    let plain = key_event(&state.source, 'k');
    assert!(matches!(
        state.decide(NonNull::from(&*plain)),
        Decision::Passthrough
    ));
    assert!(state.is_recording_hotkey());

    // ⌘ shortcuts pass through too (⌘Q/⌘Tab must keep working).
    let cmd = flagged_event(&state.source, 12, CGEventFlags(CGEventFlags::MaskCommand.0));
    assert!(matches!(
        state.decide(NonNull::from(&*cmd)),
        Decision::Passthrough
    ));
    assert!(state.is_recording_hotkey());

    // ⌃⇧E is reserved for the per-app toggle: swallowed, still recording.
    let reserved = ctrl_shift_event(&state.source, KEY_CODE_E as u16);
    assert!(matches!(
        state.decide(NonNull::from(&*reserved)),
        Decision::Consume
    ));
    assert!(state.is_recording_hotkey());
    assert_eq!(state.toggle_hotkey(), HotkeyPreset::CtrlShiftSpace);

    // ⌃⌥K (keycode 40) becomes the custom hotkey and ends recording.
    let combo = flagged_event(
        &state.source,
        40,
        CGEventFlags(CGEventFlags::MaskControl.0 | CGEventFlags::MaskAlternate.0),
    );
    assert!(matches!(
        state.decide(NonNull::from(&*combo)),
        Decision::Consume
    ));
    assert!(!state.is_recording_hotkey());
    let recorded = state.toggle_hotkey();
    assert!(matches!(
        recorded,
        HotkeyPreset::Custom {
            control: true,
            shift: false,
            option: true,
            keycode: 40,
            ..
        }
    ));

    // The recorded combo now toggles VN/EN…
    let combo = flagged_event(
        &state.source,
        40,
        CGEventFlags(CGEventFlags::MaskControl.0 | CGEventFlags::MaskAlternate.0),
    );
    assert!(matches!(
        state.decide(NonNull::from(&*combo)),
        Decision::Consume
    ));
    assert_eq!(
        state.session.borrow().mode(),
        glowkey_engine::InputMode::English
    );
    // …and the old default (⌃⇧Space) no longer does.
    let old = toggle_event(&state.source);
    state.decide(NonNull::from(&*old));
    assert_eq!(
        state.session.borrow().mode(),
        glowkey_engine::InputMode::English,
        "the replaced preset must not toggle anymore"
    );
}

#[test]
fn hotkey_recording_escape_cancels() {
    let state = active_state();
    state.begin_hotkey_recording();
    let esc = nav_event(&state.source, KEY_CODE_ESCAPE as u16);
    assert!(matches!(
        state.decide(NonNull::from(&*esc)),
        Decision::Consume
    ));
    assert!(!state.is_recording_hotkey());
    // The preset is unchanged.
    assert_eq!(state.toggle_hotkey(), HotkeyPreset::CtrlShiftSpace);
}

#[test]
fn hotkey_recording_cancelled_by_mouse_click() {
    // A mouse click (the tap's flush path) cancels an armed recorder, so a
    // forgotten recording cannot capture a later ⌃/⌥ combo.
    let state = active_state();
    state.begin_hotkey_recording();
    state.flush();
    assert!(!state.is_recording_hotkey());
    assert_eq!(state.toggle_hotkey(), HotkeyPreset::CtrlShiftSpace);
}

#[test]
fn terminal_toggle_via_hotkey_is_session_only() {
    // ⌃⇧E in a terminal enables Vietnamese for the session, but the snapshot
    // (what gets persisted) still excludes it.
    let state = TapState::new().expect("source");
    state
        .session
        .borrow_mut()
        .set_frontmost_app("com.mitchellh.ghostty");
    assert_eq!(type_via_tap(&state, "hoongf"), "hoongf"); // excluded by default

    let outcome = state
        .session
        .borrow_mut()
        .toggle_app_exclusion("com.mitchellh.ghostty");
    assert_eq!(outcome, ExclusionToggle::EnabledSessionOnly);
    assert_eq!(type_via_tap(&state, "hoongf"), "hồng"); // live for the session
    let snapshot = state.session.borrow().snapshot();
    assert!(
        snapshot
            .exclusions
            .iter()
            .any(|id| id == "com.mitchellh.ghostty"),
        "the persisted exclusion must survive a session-only toggle"
    );
}

#[test]
fn real_events_toggle_hotkey_switches_mode() {
    let state = active_state();
    // Vietnamese by default: transforms.
    assert_eq!(type_via_tap(&state, "hoongf"), "hồng");

    // ⌃⇧Space toggles to English — and is consumed (types nothing).
    let toggle = toggle_event(&state.source);
    assert!(matches!(
        state.decide(NonNull::from(&*toggle)),
        Decision::Consume
    ));
    assert_eq!(
        state.session.borrow().mode(),
        glowkey_engine::InputMode::English
    );

    // Now the same keys pass through untransformed.
    assert_eq!(type_via_tap(&state, "hoongf"), "hoongf");

    // Toggle back to Vietnamese.
    let toggle = toggle_event(&state.source);
    assert!(matches!(
        state.decide(NonNull::from(&*toggle)),
        Decision::Consume
    ));
    assert_eq!(type_via_tap(&state, "hoongf"), "hồng");
}

/// ⌃⇧W corrects the word just typed and emits the swap, all the way through the
/// real decision path with a real `CGEvent`.
///
/// The engine half is covered in `crates/glowkey-engine/tests/word_overrides.rs`;
/// what this pins is that `decide` recognises the combo, reaches it before the
/// shortcut filter (which would flush and destroy the very memory it needs), and
/// returns an edit rather than passing a stray `W` into the document.
#[test]
fn ctrl_shift_w_corrects_the_last_word() {
    let state = active_state();
    // Type `was` and a space: `ứa ` is on screen.
    for ch in "was".chars() {
        let event = key_event(&state.source, ch);
        state.decide(NonNull::from(&*event));
    }
    let space = key_event(&state.source, ' ');
    state.decide(NonNull::from(&*space));

    let ctrl_shift = CGEventFlags(CGEventFlags::MaskControl.0 | CGEventFlags::MaskShift.0);
    let correct = flagged_event(&state.source, super::keys::KEY_CODE_W as u16, ctrl_shift);
    match state.decide(NonNull::from(&*correct)) {
        Decision::Emit(edit) => {
            assert!(edit.backspaces > 0, "the on-screen word must be replaced");
            assert!(
                edit.insert.starts_with("was"),
                "expected the raw keys back, got {:?}",
                edit.insert
            );
        }
        other => panic!("expected an edit, got {other:?}"),
    }
    // And the decision was recorded, so the next `was` needs no keystroke.
    assert_eq!(
        state.session.borrow().word_override("was"),
        Some(glowkey_engine::WordPreference::Raw)
    );
}

/// With nothing to correct the key is consumed, not passed through: a stray `W`
/// appearing in the document would be worse than a keystroke that did nothing.
#[test]
fn ctrl_shift_w_with_nothing_to_correct_is_consumed() {
    let state = active_state();
    let ctrl_shift = CGEventFlags(CGEventFlags::MaskControl.0 | CGEventFlags::MaskShift.0);
    let correct = flagged_event(&state.source, super::keys::KEY_CODE_W as u16, ctrl_shift);
    assert!(matches!(
        state.decide(NonNull::from(&*correct)),
        Decision::Consume
    ));
}

/// A Backspace that undoes a spell-check escape is **suppressed** and replaced by
/// one edit, not passed through.
///
/// Reported from live use: `hoongf` `a` ⌫ left `hoongf` on screen instead of
/// restoring `hồng`. The repair cannot be a passthrough plus a posted edit —
/// that mixes a native keystroke with a synthesized one, which is the race the
/// full-suppression model exists to remove — so the tap must own the whole thing.
#[test]
fn backspace_that_unescapes_a_word_emits_instead_of_passing_through() {
    let state = active_state();
    state.session.borrow_mut().set_strict_spell_check(true);

    for ch in "hoongf".chars() {
        let event = key_event(&state.source, ch);
        state.decide(NonNull::from(&*event));
    }
    // The mistake escapes the word to its raw keys.
    let mistake = key_event(&state.source, 'a');
    state.decide(NonNull::from(&*mistake));
    assert_eq!(state.session.borrow().current_word(), "hoongfa");

    let delete = backspace_event(&state.source);
    match state.decide(NonNull::from(&*delete)) {
        Decision::Emit(edit) => {
            assert_eq!(
                edit.backspaces,
                "hoongfa".encode_utf16().count(),
                "the edit replaces the whole on-screen word, including the character \
                 the user asked to delete — the key is suppressed, so nothing else \
                 removes it"
            );
            assert_eq!(edit.insert, "hồng");
        }
        other => panic!("expected the repair to be emitted, got {other:?}"),
    }
}

/// An ordinary mid-word Backspace on a word that was never escaped still passes
/// through, so the common path is untouched for everyone with the option off.
#[test]
fn an_ordinary_mid_word_backspace_still_passes_through() {
    let state = active_state();
    for ch in "hoongf".chars() {
        let event = key_event(&state.source, ch);
        state.decide(NonNull::from(&*event));
    }
    let delete = backspace_event(&state.source);
    assert!(matches!(
        state.decide(NonNull::from(&*delete)),
        Decision::Passthrough
    ));
    assert_eq!(state.session.borrow().current_word(), "hồn");
}
