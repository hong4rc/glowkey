//! The toggle hotkey preset (leaves this crate in a later phase).

use super::*;

/// The chosen hotkey for the global Vietnamese/English toggle, as a small preset
/// list (like Unikey/EVKey's hotkey picker). The shell maps each to its modifier
/// mask and key code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HotkeyPreset {
    /// ⌃⇧Space — the default.
    #[default]
    CtrlShiftSpace,
    /// ⌃Space.
    CtrlSpace,
    /// ⌥Space.
    OptionSpace,
    /// ⌃⇧Z.
    CtrlShiftZ,
    /// A user-recorded combination.
    ///
    /// The four named presets above are already portable — modifiers plus a
    /// semantic key — and need nothing. This one is the awkward case: the user
    /// pressed a *physical* key, and the only durable name that key has is the
    /// virtual key code the platform reported at the time. So the code is stored
    /// per platform, explicitly, rather than as one number that would mean a
    /// different key on the next machine.
    ///
    /// This is deliberately not a universal keycode table. Two platforms do not
    /// justify inventing a third keyboard model, and the settings file has to
    /// stay something a person can read and edit.
    ///
    /// Command is never allowed (it belongs to the system), so it has no field.
    Custom {
        /// Control key required.
        control: bool,
        /// Shift key required.
        shift: bool,
        /// Option key required.
        option: bool,
        /// Display character for the key (uppercased; `' '` means Space), and the
        /// cross-platform fallback matcher. Captured from the event, so it
        /// reflects the layout the user recorded it on — which is also why it is
        /// a fallback and not the primary matcher.
        key_char: char,
        /// macOS virtual key code, when the combination was recorded on macOS.
        ///
        /// The `keycode` alias is what every settings file written before the
        /// port calls this field. Reading one must not reinterpret the user's
        /// hotkey into some other key — a hotkey that silently starts doing
        /// something else is worse than one that fails loudly — so the old
        /// spelling keeps working forever.
        #[serde(default, alias = "keycode")]
        macos_keycode: Option<i64>,
        /// Windows virtual-key code, when the combination was recorded on Windows.
        #[serde(default)]
        windows_vk: Option<u16>,
    },
}

impl HotkeyPreset {
    /// The macOS virtual key code recorded for this hotkey, if there is one.
    /// `None` for the named presets (they need no code) and for a custom
    /// combination recorded on another platform.
    #[must_use]
    pub fn macos_keycode(self) -> Option<i64> {
        match self {
            Self::Custom { macos_keycode, .. } => macos_keycode,
            _ => None,
        }
    }

    /// The Windows virtual-key code recorded for this hotkey, if there is one.
    #[must_use]
    pub fn windows_vk(self) -> Option<u16> {
        match self {
            Self::Custom { windows_vk, .. } => windows_vk,
            _ => None,
        }
    }
}
