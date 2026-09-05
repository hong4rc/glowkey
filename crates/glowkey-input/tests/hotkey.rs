//! Hotkey matching and recording, off any operating system.

use glowkey_input::hotkey::{capture, is_app_toggle, is_correction, resolve, HotkeyCapture};
use glowkey_input::{HotkeyPreset, Key, KeyEvent, Modifiers};

fn mods(control: bool, shift: bool, option: bool, command: bool) -> Modifiers {
    Modifiers {
        control,
        shift,
        option,
        command,
    }
}

fn space(m: Modifiers) -> KeyEvent {
    KeyEvent::key(Key::Space).with_mods(m)
}

fn letter(ch: char, m: Modifiers) -> KeyEvent {
    KeyEvent::character(ch).with_mods(m)
}

#[test]
fn the_presets_match_only_their_own_combination() {
    let ctrl_shift = mods(true, true, false, false);
    let ctrl = mods(true, false, false, false);
    let option = mods(false, false, true, false);

    let ctrl_shift_space = resolve(HotkeyPreset::CtrlShiftSpace, None);
    assert!(ctrl_shift_space.matches(&space(ctrl_shift)));
    assert!(!ctrl_shift_space.matches(&space(ctrl)));

    let ctrl_space = resolve(HotkeyPreset::CtrlSpace, None);
    assert!(ctrl_space.matches(&space(ctrl)));
    // Shift must NOT be held for the plain ⌃Space preset.
    assert!(!ctrl_space.matches(&space(ctrl_shift)));

    let option_space = resolve(HotkeyPreset::OptionSpace, None);
    assert!(option_space.matches(&space(option)));

    let ctrl_shift_z = resolve(HotkeyPreset::CtrlShiftZ, None);
    assert!(ctrl_shift_z.matches(&letter('z', ctrl_shift)));
    // Right modifiers, wrong key.
    assert!(!ctrl_shift_z.matches(&space(ctrl_shift)));
}

/// Command is never a hotkey modifier: those belong to the system and to the
/// focused application.
#[test]
fn command_never_matches() {
    let hotkey = resolve(HotkeyPreset::CtrlShiftSpace, None);
    assert!(!hotkey.matches(&space(mods(true, true, false, true))));
}

/// ⌃⇧Z is matched by the key's identity, not by the character the event carries:
/// with Control held, macOS reports U+001A rather than `z`.
#[test]
fn ctrl_shift_z_matches_without_a_usable_character() {
    let mut event = letter('z', mods(true, true, false, false));
    event.ch = Some('\u{1a}');
    assert!(resolve(HotkeyPreset::CtrlShiftZ, None).matches(&event));
}

#[test]
fn a_recorded_custom_hotkey_matches_the_code_the_platform_recorded() {
    let preset = HotkeyPreset::Custom {
        control: true,
        shift: false,
        option: true,
        key_char: 'K',
        raw_code: Some(40),
    };
    let hotkey = resolve(preset, Some(40));
    assert!(!hotkey.is_char_fallback());

    let combo = letter('k', mods(true, false, true, false)).with_raw_code(40);
    assert!(hotkey.matches(&combo));
    // A different physical key with the same modifiers is not the hotkey.
    assert!(!hotkey.matches(&letter('j', mods(true, false, true, false)).with_raw_code(38)));
}

/// A platform with no key code of its own for a custom hotkey — a Windows build
/// reading a combination recorded on macOS — falls back to the display character
/// rather than matching whatever key happens to share the number.
#[test]
fn a_hotkey_recorded_elsewhere_falls_back_to_the_display_character() {
    let preset = HotkeyPreset::Custom {
        control: true,
        shift: false,
        option: true,
        key_char: 'K',
        raw_code: Some(40),
    };
    let hotkey = resolve(preset, None);
    assert!(
        hotkey.is_char_fallback(),
        "the platform must be able to tell it took the layout-dependent path"
    );

    // Matched by the letter, whatever this platform calls that key code.
    assert!(hotkey.matches(&letter('k', mods(true, false, true, false)).with_raw_code(1234)));
    assert!(!hotkey.matches(&letter('j', mods(true, false, true, false)).with_raw_code(40)));
}

#[test]
fn a_space_hotkey_recorded_elsewhere_still_matches_space() {
    let preset = HotkeyPreset::Custom {
        control: true,
        shift: false,
        option: true,
        key_char: ' ',
        raw_code: Some(49),
    };
    let hotkey = resolve(preset, None);
    assert!(hotkey.matches(&space(mods(true, false, true, false))));
}

#[test]
fn the_fixed_hotkeys_need_exactly_control_and_shift() {
    assert!(is_app_toggle(&letter('e', mods(true, true, false, false))));
    assert!(!is_app_toggle(&letter(
        'e',
        mods(true, false, false, false)
    )));
    assert!(!is_app_toggle(&letter('e', mods(true, true, true, false))));
    assert!(!is_app_toggle(&letter('e', mods(true, true, false, true))));

    assert!(is_correction(&letter('w', mods(true, true, false, false))));
    assert!(!is_correction(&letter('e', mods(true, true, false, false))));
}

// ── Recording ───────────────────────────────────────────────────────────────

/// Typing must never be blocked by an armed recorder, and neither must any ⌘
/// shortcut: an armed-and-forgotten recording can therefore never lock the
/// keyboard.
#[test]
fn recording_lets_ordinary_keys_through() {
    assert_eq!(
        capture(&KeyEvent::character('k')),
        HotkeyCapture::Passthrough
    );
    assert_eq!(
        capture(&letter('q', mods(false, false, false, true))),
        HotkeyCapture::Passthrough
    );
    // Shift alone is not a candidate either.
    assert_eq!(
        capture(&letter('k', mods(false, true, false, false))),
        HotkeyCapture::Passthrough
    );
    // Nor is ⌃⌥ *with* ⌘.
    assert_eq!(
        capture(&letter('k', mods(true, false, true, true))),
        HotkeyCapture::Passthrough
    );
}

#[test]
fn escape_cancels_the_recording() {
    assert_eq!(capture(&KeyEvent::key(Key::Escape)), HotkeyCapture::Cancel);
}

/// Recording ⌃⇧E or ⌃⇧W would shadow a built-in feature with no warning, costing
/// the user both. Swallowed, and the recorder keeps waiting.
#[test]
fn the_fixed_hotkeys_are_refused() {
    assert!(matches!(
        capture(&letter('e', mods(true, true, false, false))),
        HotkeyCapture::Reserved { .. }
    ));
    assert!(matches!(
        capture(&letter('w', mods(true, true, false, false))),
        HotkeyCapture::Reserved { .. }
    ));
}

#[test]
fn a_control_option_combination_is_captured() {
    assert_eq!(
        capture(&letter('k', mods(true, false, true, false))),
        HotkeyCapture::Captured {
            control: true,
            shift: false,
            option: true,
            key_char: 'K',
        }
    );
}

/// With Control held the reported character is a control code (⌃A → U+0001), so
/// the display character has to be mapped back to its letter or the Settings
/// window would show an unprintable box.
#[test]
fn a_control_code_is_shown_as_its_letter() {
    let mut event = letter('a', mods(true, false, true, false));
    event.ch = Some('\u{1}');
    assert_eq!(
        capture(&event),
        HotkeyCapture::Captured {
            control: true,
            shift: false,
            option: true,
            key_char: 'A',
        }
    );
}

/// Space is captured by identity: under modifiers its character is not reliably
/// `' '`.
#[test]
fn space_is_captured_by_identity() {
    let mut event = space(mods(true, false, true, false));
    event.ch = Some('\0');
    assert_eq!(
        capture(&event),
        HotkeyCapture::Captured {
            control: true,
            shift: false,
            option: true,
            key_char: ' ',
        }
    );
}
