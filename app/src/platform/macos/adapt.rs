//! Translation, and only translation: a `CGEvent` in, a neutral [`KeyEvent`] out.
//!
//! This is the whole of what macOS contributes to the decision. Everything the
//! ladder branches on — is this Backspace, is this a caret move, which modifiers
//! are held, what character did the layout produce — is answered here from
//! CoreGraphics and then handed over as plain data. No state, no side effects,
//! nothing that touches the session.
//!
//! The key-code tables below are the *physical* ones. They do not change with the
//! keyboard layout, which is the point: ⌃⇧Z has always meant the key where Z sits
//! on a US board, whatever a Colemak user's layout makes that key type. The
//! character is carried separately, in [`KeyEvent::ch`], and that one *is* the
//! layout's answer.

use std::ptr::NonNull;

use glowkey_input::{Key, KeyEvent, Modifiers};
use objc2_core_graphics::{CGEvent, CGEventField, CGEventFlags};

/// macOS virtual key code for Delete/Backspace.
pub(super) const KEY_CODE_DELETE: i64 = 51;
/// macOS virtual key code for Forward Delete (⌦). Used by the omnibox guard: with
/// a trailing selection it deletes the selection; with the caret at the end of the
/// text (GlowKey's normal position) it is a no-op.
pub(super) const KEY_CODE_FORWARD_DELETE: i64 = 117;
/// macOS virtual key code for Escape — cancels hotkey recording.
pub(super) const KEY_CODE_ESCAPE: i64 = 53;
/// macOS virtual key code for Space.
pub(super) const KEY_CODE_SPACE: i64 = 49;
/// macOS virtual key code for the letter E — the per-app toggle hotkey.
#[cfg(test)]
pub(super) const KEY_CODE_E: i64 = 14;
/// macOS virtual key code for the letter Z — one of the toggle presets.
#[cfg(test)]
pub(super) const KEY_CODE_Z: i64 = 6;
/// macOS virtual key code for the letter W — the correction hotkey.
#[cfg(test)]
pub(super) const KEY_CODE_W: i64 = 13;

/// The letter keys, by physical position on an ANSI board. Not a layout table:
/// this says where the key *is*, so a hotkey survives the user switching layouts.
const LETTER_KEY_CODES: [(i64, char); 26] = [
    (0, 'a'),
    (1, 's'),
    (2, 'd'),
    (3, 'f'),
    (4, 'h'),
    (5, 'g'),
    (6, 'z'),
    (7, 'x'),
    (8, 'c'),
    (9, 'v'),
    (11, 'b'),
    (12, 'q'),
    (13, 'w'),
    (14, 'e'),
    (15, 'r'),
    (16, 'y'),
    (17, 't'),
    (31, 'o'),
    (32, 'u'),
    (34, 'i'),
    (35, 'p'),
    (37, 'l'),
    (38, 'j'),
    (40, 'k'),
    (45, 'n'),
    (46, 'm'),
];

/// Which key this virtual key code is.
fn key_for(keycode: i64) -> Key {
    match keycode {
        KEY_CODE_DELETE => Key::Backspace,
        KEY_CODE_FORWARD_DELETE => Key::ForwardDelete,
        KEY_CODE_ESCAPE => Key::Escape,
        KEY_CODE_SPACE => Key::Space,
        36 | 76 => Key::Return, // Return, and the keypad's Enter
        48 => Key::Tab,
        // Left 123, Right 124, Down 125, Up 126, Home 115, End 119, PgUp 116,
        // PgDn 121. One class: any of them moves the insertion point with no text
        // change, so GlowKey's diff baseline is stale either way.
        123 | 124 | 125 | 126 | 115 | 116 | 119 | 121 => Key::CaretMove,
        _ => LETTER_KEY_CODES
            .iter()
            .find(|(code, _)| *code == keycode)
            .map_or(Key::Other, |(_, letter)| Key::Letter(*letter)),
    }
}

/// Which modifiers the event carries.
fn modifiers(flags: CGEventFlags) -> Modifiers {
    Modifiers {
        control: flags.0 & CGEventFlags::MaskControl.0 != 0,
        shift: flags.0 & CGEventFlags::MaskShift.0 != 0,
        option: flags.0 & CGEventFlags::MaskAlternate.0 != 0,
        command: flags.0 & CGEventFlags::MaskCommand.0 != 0,
    }
}

/// Reads one key-down event into the neutral form the policy takes.
pub(super) fn key_event(event: NonNull<CGEvent>) -> KeyEvent {
    let keycode = integer_field(event, CGEventField::KeyboardEventKeycode);
    KeyEvent {
        ch: unicode_char(event),
        key: key_for(keycode),
        mods: modifiers(unsafe { CGEvent::flags(Some(event.as_ref())) }),
        // Carried, not interpreted: it is how a hotkey the user recorded on this
        // machine is recognised again.
        raw_code: keycode,
    }
}

/// Renders the modifier flags of a key event compactly for the log ("⌘⇧", "-").
/// Without this a logged `q` cannot be told apart from ⌘Q, which is the
/// difference between a plain keystroke and a quit.
pub(super) fn modifier_names(flags: CGEventFlags) -> String {
    let mut names = String::new();
    for (mask, symbol) in [
        (CGEventFlags::MaskCommand, "⌘"),
        (CGEventFlags::MaskControl, "⌃"),
        (CGEventFlags::MaskAlternate, "⌥"),
        (CGEventFlags::MaskShift, "⇧"),
        (CGEventFlags::MaskSecondaryFn, "fn"),
    ] {
        if flags.0 & mask.0 != 0 {
            names.push_str(symbol);
        }
    }
    if names.is_empty() {
        names.push('-');
    }
    names
}

/// Reads an integer field from an event.
pub(super) fn integer_field(event: NonNull<CGEvent>, field: CGEventField) -> i64 {
    unsafe { CGEvent::integer_value_field(Some(event.as_ref()), field) }
}

/// Extracts the typed character (already mapped through the active layout).
pub(super) fn unicode_char(event: NonNull<CGEvent>) -> Option<char> {
    let mut buf = [0u16; 4];
    let mut actual: u64 = 0;
    unsafe {
        CGEvent::keyboard_get_unicode_string(
            Some(event.as_ref()),
            buf.len() as u64,
            &mut actual,
            buf.as_mut_ptr(),
        );
    }
    let len = (actual as usize).min(buf.len());
    String::from_utf16(&buf[..len])
        .ok()
        .and_then(|s| s.chars().next())
}
