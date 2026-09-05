//! End-to-end tests driving the real tap decision path with real CoreGraphics
//! key events (real CGEvent objects, real Unicode decode, real engine). This
//! covers everything except the system-level tap install and event injection,
//! which require Accessibility permission a test process cannot grant.

// `use super::*` reached everything while all of this lived in one file. The
// split means the siblings have to be named — the only change the move made to
// this file; every test body below is byte-identical to before.
use super::adapt::{
    KEY_CODE_DELETE, KEY_CODE_E, KEY_CODE_ESCAPE, KEY_CODE_SPACE, KEY_CODE_W, KEY_CODE_Z,
};
use super::emit::is_chromium_browser;
use super::*;
use glowkey_input::HotkeyPreset;
use glowkey_session::{ExclusionToggle, KeyResponse};
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

/// The presets, matched the way the running app matches them: a real `CGEvent`
/// through the adapter, then `glowkey-input`'s matcher.
///
/// `crates/glowkey-input/tests/hotkey.rs` covers the matching itself. What this
/// adds is the half that only exists here — that keycode 49 really does reach the
/// policy as Space and keycode 6 as the Z key, whatever the user's layout types.
#[test]
fn toggle_hotkey_presets_match_only_their_combo() {
    let source = CGEventSource::new(CGEventSourceStateID::Private).expect("source");
    let ctrl_shift = CGEventFlags(CGEventFlags::MaskControl.0 | CGEventFlags::MaskShift.0);
    let ctrl = CGEventFlags(CGEventFlags::MaskControl.0);
    let option = CGEventFlags(CGEventFlags::MaskAlternate.0);

    let matches = |flags: CGEventFlags, keycode: i64, preset: HotkeyPreset| {
        let event = flagged_event(&source, keycode as u16, flags);
        let key = super::adapt::key_event(NonNull::from(&*event));
        glowkey_input::hotkey::resolve(preset, preset.raw_code()).matches(&key)
    };

    assert!(matches(
        ctrl_shift,
        KEY_CODE_SPACE,
        HotkeyPreset::CtrlShiftSpace
    ));
    assert!(!matches(ctrl, KEY_CODE_SPACE, HotkeyPreset::CtrlShiftSpace));

    assert!(matches(ctrl, KEY_CODE_SPACE, HotkeyPreset::CtrlSpace));
    // Shift must NOT be held for the plain ⌃Space preset.
    assert!(!matches(
        ctrl_shift,
        KEY_CODE_SPACE,
        HotkeyPreset::CtrlSpace
    ));

    assert!(matches(option, KEY_CODE_SPACE, HotkeyPreset::OptionSpace));

    assert!(matches(ctrl_shift, KEY_CODE_Z, HotkeyPreset::CtrlShiftZ));
    // Right modifiers, wrong key.
    assert!(!matches(
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
            raw_code: Some(40),
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
        glowkey_session::InputMode::English
    );
    // …and the old default (⌃⇧Space) no longer does.
    let old = toggle_event(&state.source);
    state.decide(NonNull::from(&*old));
    assert_eq!(
        state.session.borrow().mode(),
        glowkey_session::InputMode::English,
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
    let snapshot = state.snapshot().expect("state not busy");
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
        glowkey_session::InputMode::English
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
    let correct = flagged_event(&state.source, KEY_CODE_W as u16, ctrl_shift);
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
        Some(glowkey_session::WordPreference::Raw)
    );
}

/// With nothing to correct the key is consumed, not passed through: a stray `W`
/// appearing in the document would be worse than a keystroke that did nothing.
#[test]
fn ctrl_shift_w_with_nothing_to_correct_is_consumed() {
    let state = active_state();
    let ctrl_shift = CGEventFlags(CGEventFlags::MaskControl.0 | CGEventFlags::MaskShift.0);
    let correct = flagged_event(&state.source, KEY_CODE_W as u16, ctrl_shift);
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

/// Types a sequence through the real `decide()` path, where `⌫` in the input
/// stands for the Delete key, and returns what the document would show.
///
/// The character-only [`type_via_tap`] cannot express a Backspace, which is why
/// every field report involving one has been checked against a hand-written
/// model in a scratch binary instead of against the tap. A model that shares the
/// author's assumptions cannot contradict them; this drives the same code the
/// app runs.
fn type_with_deletes(state: &TapState, input: &str) -> String {
    let mut screen = String::new();
    let apply = |screen: &mut String, r: &KeyResponse| {
        let units: Vec<u16> = screen.encode_utf16().collect();
        let keep = units.len().saturating_sub(r.backspaces);
        *screen = String::from_utf16(&units[..keep]).unwrap();
        screen.push_str(&r.insert);
    };
    for ch in input.chars() {
        let is_delete = ch == '⌫';
        let event = if is_delete {
            backspace_event(&state.source)
        } else {
            key_event(&state.source, ch)
        };
        match state.decide(NonNull::from(&*event)) {
            // For Delete, a passthrough means the *host* performs the deletion.
            Decision::Passthrough => {
                if is_delete {
                    screen.pop();
                } else {
                    screen.push(ch);
                }
            }
            Decision::Consume | Decision::ToggleApp => {}
            Decision::Emit(r) => apply(&mut screen, &r),
            Decision::EmitThenReplayKey(r) => {
                apply(&mut screen, &r);
                screen.push(ch);
            }
        }
    }
    screen
}

/// The sequences reported from live use, driven through the real tap.
///
/// Reported as producing `hồngz` — the tone key landing as a literal after a
/// word the engine had stopped composing. Pinned here so the claim is checked
/// against the code the app runs rather than against a model of it.
#[test]
fn reported_delete_sequences_land_where_they_should() {
    // Mistyped vowel: the word escapes to its raw keys, and deleting the
    // offending key brings the transformation back.
    let state = active_state();
    state.session.borrow_mut().set_strict_spell_check(true);
    assert_eq!(type_with_deletes(&state, "hoongfa⌫"), "hồng");

    // Mistyped tone key. `hống` is spellable, so nothing escapes; the deletes
    // are ordinary visible-character deletes and `z` removes the tone.
    let state = active_state();
    state.session.borrow_mut().set_strict_spell_check(true);
    assert_eq!(type_with_deletes(&state, "hoongfs⌫⌫z"), "hô");

    // The same with the spell check off — the default — must be identical here,
    // because nothing escapes either way.
    let state = active_state();
    assert_eq!(type_with_deletes(&state, "hoongfs⌫⌫z"), "hô");

    // And the plain case the contract is written around.
    let state = active_state();
    assert_eq!(type_with_deletes(&state, "hoongf⌫z"), "hôn");
}

/// Deleting back to a word re-opens it, however you got there.
///
/// Reported three times in one morning, each time as a different-looking bug.
/// Read off `~/Library/Logs/GlowKey/glowkey.log`, the sequence has a **space**
/// in it — the shorthand `hoongf s(del)(del)z` is seven keystrokes, not six:
///
/// ```text
/// 'f'  Emit bs=3 ins="ồng"   raw="hoongf" rendered="hồng"
/// ' '  Passthrough           raw=""       rendered=""      <- commits the word
/// 's'  Emit bs=0 ins="s"     raw="s"      rendered="s"     <- a new word starts
/// ⌫    Passthrough                                          <- deletes the s
/// ⌫    Passthrough                                          <- deletes the space
/// 'z'  Emit bs=0 ins="z"     raw="z"      rendered="z"     <- z was its own word
/// ```
///
/// The committed word was destroyed by the first keystroke after the boundary,
/// so re-opening only ever worked if the Backspace was *immediate*. Now the
/// history survives keys that are later deleted.
#[test]
fn deleting_back_to_a_word_reopens_it() {
    // The reported sequence.
    let state = active_state();
    assert_eq!(type_with_deletes(&state, "hoongf s⌫⌫z"), "hông");

    // The immediate case, which must not regress.
    let state = active_state();
    assert_eq!(type_with_deletes(&state, "hoongf ⌫z"), "hông");

    // A longer intervening word: four deletes to clear `abc` and the space.
    let state = active_state();
    assert_eq!(type_with_deletes(&state, "hoongf abc⌫⌫⌫⌫z"), "hông");
}

/// Two words back. The history is a stack, so each word empties in turn and the
/// next Backspace re-opens the one before it — no special case for depth.
#[test]
fn deleting_back_through_two_words_reopens_the_right_one() {
    let state = active_state();
    // `hoongf ` commits hồng, `man ` commits man, then `s` starts a third.
    // Six deletes to get back to just after hồng: s(1), space(2, re-opening
    // man), man(3-5), space(6, re-opening hồng).
    let typed = type_with_deletes(&state, "hoongf man s⌫⌫⌫⌫⌫⌫z");
    assert_eq!(
        typed, "hông",
        "the first word re-opened after two words were deleted away, and z removed its tone"
    );
}

/// The chain breaks where the engine loses track mid-word, and that is correct.
///
/// Deleting `viêt` back to `vi` has no single raw-key removal that produces it —
/// `viee` minus an `e` renders `viê`, not `vi` — so the engine cannot stay in
/// step and flushes. After that it no longer knows how many characters of that
/// word remain on screen, so it cannot tell when the caret reaches the boundary,
/// and the history must go with it. Re-opening a word on a guess is the failure
/// this whole feature is built to avoid.
///
/// Pinned so the limitation is a known one rather than a surprise: a word with a
/// transformation in the middle can end the chain when you delete through it.
#[test]
fn losing_track_mid_word_ends_the_chain() {
    let state = active_state();
    let typed = type_with_deletes(&state, "hoongf vieet s⌫⌫⌫⌫⌫⌫⌫z");
    assert_eq!(
        typed, "hồngz",
        "the flush inside viêt cleared the history, so z starts a fresh word"
    );
}

/// The cap is exactly five, and both sides of it are pinned.
///
/// The earlier version of this test deleted the whole document away and asserted
/// the screen was empty, which is true for any cap of two or more — it tested
/// nothing. A cap test has to put a re-composable word on the far side of the
/// boundary and check whether it comes back.
///
/// Each intervening word costs two deletes: one removes the boundary and re-opens
/// the word before it, the next empties that word.
#[test]
fn the_history_cap_is_five_entries() {
    // Five entries — `hồng` and four `a`s — so the oldest is still there and
    // nine deletes re-open it.
    let state = active_state();
    assert_eq!(
        type_with_deletes(&state, "hoongf a a a a ⌫⌫⌫⌫⌫⌫⌫⌫⌫z"),
        "hông",
        "within the cap the first word must still re-open"
    );

    // Six entries: `hồng` falls off the front. Deleting back to it finds an empty
    // stack, so the engine stops vouching for the caret and `z` starts a new word.
    let state = active_state();
    assert_eq!(
        type_with_deletes(&state, "hoongf a a a a a ⌫⌫⌫⌫⌫⌫⌫⌫⌫⌫⌫z"),
        "hồngz",
        "past the cap nothing re-opens"
    );
}

/// A second boundary in a row does not break the chain — the original bug was one
/// comma away from still being reachable.
///
/// `hồng`␣⌫`z` worked, but `hồng``,`␣⌫⌫`z` gave `hồngz`: the space after the
/// comma committed nothing, and a commit with nothing composing used to throw the
/// whole history away. `, ` and `. ` are the two commonest pairs in prose, so the
/// fix is not an edge case. A bare boundary now gets a stack entry of its own,
/// which is what keeps the entries an unbroken account of the document.
#[test]
fn a_second_boundary_in_a_row_keeps_the_chain() {
    // The case that worked, as the control.
    let state = active_state();
    assert_eq!(type_with_deletes(&state, "hoongf ⌫z"), "hông");

    // Comma then space: two deletes to reach the word.
    let state = active_state();
    assert_eq!(type_with_deletes(&state, "hoongf, ⌫⌫z"), "hông");

    // Full stop then space, the other common pair.
    let state = active_state();
    assert_eq!(type_with_deletes(&state, "hoongf. ⌫⌫z"), "hông");

    // A run of them: each boundary is one entry and one delete.
    let state = active_state();
    assert_eq!(type_with_deletes(&state, "hoongf,,, ⌫⌫⌫⌫z"), "hông");
}

/// Bare boundaries sit *between* words in the stack, and deleting through them
/// still lands on the right word.
///
/// This is the case a single trailing-boundary count could not represent: after
/// `hồng, man ` the two boundaries behind `hồng` are no longer the trailing ones,
/// so a count kept only for the tail would have forgotten them and re-opened
/// `hồng` while `,` still sat at the caret. Giving each boundary its own entry
/// makes depth ordinary rather than special.
#[test]
fn deleting_back_through_a_bare_boundary_reopens_the_word_before_it() {
    let state = active_state();
    // `hồng, man ` — six deletes: the space (re-opening `man`), `man` itself,
    // the space after the comma, then the comma (re-opening `hồng`).
    assert_eq!(type_with_deletes(&state, "hoongf, man ⌫⌫⌫⌫⌫⌫z"), "hông");
}

/// Anything that moves the caret where the engine cannot see it clears the whole
/// history. Re-opening a word on a guess is how a blind editor corrupts a
/// document, so this is the property the feature rests on.
#[test]
fn a_caret_move_clears_the_whole_history() {
    // A flush stands for every one of them: mouse-down, arrow keys, ⌘ shortcuts.
    let state = active_state();
    for ch in "hoongf ".chars() {
        let event = key_event(&state.source, ch);
        state.decide(NonNull::from(&*event));
    }
    state.session.borrow_mut().flush();
    let delete = backspace_event(&state.source);
    state.decide(NonNull::from(&*delete));
    let z = key_event(&state.source, 'z');
    match state.decide(NonNull::from(&*z)) {
        Decision::Emit(edit) => assert_eq!(
            edit.insert, "z",
            "z must start a fresh word, not edit a word the engine lost track of"
        ),
        other => panic!("expected a fresh word, got {other:?}"),
    }

    // An app switch is the case with no event to flush on — a call popup, a
    // finished build — so it must clear the history by itself.
    let state = active_state();
    for ch in "hoongf ".chars() {
        let event = key_event(&state.source, ch);
        state.decide(NonNull::from(&*event));
    }
    state
        .session
        .borrow_mut()
        .set_frontmost_app("com.tinyspeck.slackmacgap");
    assert_eq!(type_with_deletes(&state, "⌫z"), "z");
}

/// A word auto-fix restored is not re-composable, and it clears the history
/// rather than merely staying out of it: it still occupies space on screen, so
/// leaving it out would break the invariant that the stack is an unbroken run of
/// words immediately behind the caret.
#[test]
fn a_restored_word_breaks_the_chain() {
    let state = active_state();
    // `work ` is restored by auto-fix, so nothing behind it stays re-openable.
    assert_eq!(type_with_deletes(&state, "hoongf work ⌫z"), "hồng workz");
}
