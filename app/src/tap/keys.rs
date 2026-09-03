//! Reading a key event, and recognising GlowKey's hotkeys in one.
//!
//! Everything here is a pure function of a `CGEvent` and its flags: which key it
//! was, which modifiers were held, whether that combination is one of GlowKey's
//! hotkeys, and whether it moves the caret. No state, no side effects, nothing
//! that touches the session — which is why it can sit apart from the decision
//! that consumes it.

use std::ptr::NonNull;

use glowkey_engine::HotkeyPreset;
use objc2_core_graphics::{CGEvent, CGEventField, CGEventFlags};

/// macOS virtual key code for Delete/Backspace.
pub(super) const KEY_CODE_DELETE: i64 = 51;
/// macOS virtual key code for Forward Delete (⌦). Used by the omnibox guard: with
/// a trailing selection it deletes the selection; with the caret at the end of the
/// text (GlowKey's normal position) it is a no-op.
pub(super) const KEY_CODE_FORWARD_DELETE: i64 = 117;
/// macOS virtual key code for Escape — cancels hotkey recording.
pub(super) const KEY_CODE_ESCAPE: i64 = 53;

/// Whether `keycode` is a caret-navigation key (arrows, Home/End, Page Up/Down).
/// These move the insertion point without any text change, so GlowKey must flush
/// its diff baseline when one is pressed.
pub(super) fn is_caret_move(keycode: i64) -> bool {
    // Left 123, Right 124, Down 125, Up 126, Home 115, End 119, PgUp 116, PgDn 121.
    matches!(keycode, 123 | 124 | 125 | 126 | 115 | 116 | 119 | 121)
}

/// macOS virtual key code for Space.
pub(super) const KEY_CODE_SPACE: i64 = 49;
/// macOS virtual key code for the letter E.
pub(super) const KEY_CODE_E: i64 = 14;
/// macOS virtual key code for the letter Z.
pub(super) const KEY_CODE_Z: i64 = 6;
/// macOS virtual key code for the letter W — the correction hotkey.
pub(super) const KEY_CODE_W: i64 = 13;

/// True when only Control and Shift are held (no Command or Option) and the key is
/// `keycode` — the modifier pattern shared by GlowKey's ⌃⇧ hotkeys.
pub(super) fn is_ctrl_shift(flags: CGEventFlags, keycode: i64, target: i64) -> bool {
    if keycode != target {
        return false;
    }
    let control = flags.0 & CGEventFlags::MaskControl.0 != 0;
    let shift = flags.0 & CGEventFlags::MaskShift.0 != 0;
    let command = flags.0 & CGEventFlags::MaskCommand.0 != 0;
    let option = flags.0 & CGEventFlags::MaskAlternate.0 != 0;
    control && shift && !command && !option
}

/// The VN/EN toggle hotkey — matches the chosen preset or a recorded custom combo.
pub(super) fn is_toggle_hotkey(flags: CGEventFlags, keycode: i64, preset: HotkeyPreset) -> bool {
    // (control, shift, option, keycode) for each preset. Command is never allowed.
    let (ctrl, shift, option, target) = match preset {
        HotkeyPreset::CtrlShiftSpace => (true, true, false, KEY_CODE_SPACE),
        HotkeyPreset::CtrlSpace => (true, false, false, KEY_CODE_SPACE),
        HotkeyPreset::OptionSpace => (false, false, true, KEY_CODE_SPACE),
        HotkeyPreset::CtrlShiftZ => (true, true, false, KEY_CODE_Z),
        HotkeyPreset::Custom {
            control,
            shift,
            option,
            keycode,
            ..
        } => (control, shift, option, keycode),
    };
    if keycode != target {
        return false;
    }
    let f_ctrl = flags.0 & CGEventFlags::MaskControl.0 != 0;
    let f_shift = flags.0 & CGEventFlags::MaskShift.0 != 0;
    let f_command = flags.0 & CGEventFlags::MaskCommand.0 != 0;
    let f_option = flags.0 & CGEventFlags::MaskAlternate.0 != 0;
    f_ctrl == ctrl && f_shift == shift && f_option == option && !f_command
}

/// The per-app enable/disable hotkey: ⌃⇧E.
pub(super) fn is_app_toggle_hotkey(flags: CGEventFlags, keycode: i64) -> bool {
    is_ctrl_shift(flags, keycode, KEY_CODE_E)
}

/// The correction hotkey: ⌃⇧W, pressed just after a word to swap it to its other
/// reading and remember that choice.
///
/// Fixed rather than configurable, like ⌃⇧E and unlike the VN/EN toggle. Only the
/// toggle is configurable because it is the one people press constantly and hold
/// opinions about; a second recorder for a key pressed a few times a day would be
/// machinery for its own sake.
pub(super) fn is_correction_hotkey(flags: CGEventFlags, keycode: i64) -> bool {
    is_ctrl_shift(flags, keycode, KEY_CODE_W)
}

/// True when a shortcut modifier is held — Command, Control, or Option. Shift is
/// excluded (it produces uppercase letters).
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

pub(super) fn is_shortcut(flags: CGEventFlags) -> bool {
    let shortcut =
        CGEventFlags::MaskCommand.0 | CGEventFlags::MaskControl.0 | CGEventFlags::MaskAlternate.0;
    flags.0 & shortcut != 0
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
