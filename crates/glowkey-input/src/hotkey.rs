//! Recognising GlowKey's hotkeys, and recording a new one.
//!
//! Three hotkeys exist. Two are fixed — ⌃⇧E toggles the current application's
//! ignore-list membership, ⌃⇧W corrects the word just typed — and one, the VN/EN
//! toggle, is a preset the user picks or records.
//!
//! Everything here is a pure function of a [`KeyEvent`]. The one place a platform
//! shows through is a *recorded* hotkey: the user pressed a physical key, and the
//! only durable name that key has is the virtual key code the platform reported.
//! That code is carried as an opaque integer through [`HotkeyKey::RawCode`]; this
//! module never interprets it, it only compares it with the one on the event.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// The chosen hotkey for the global Vietnamese/English toggle, as a small preset
/// list (like UniKey/EVKey's hotkey picker). The shell maps each to its modifier
/// mask and key code through [`resolve`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
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
    /// The four presets above are portable: modifiers plus a semantic key. This
    /// one is the awkward case: the user pressed a *physical* key, and the only
    /// durable name it has is the key code the platform reported at the time.
    /// That code is `raw_code`. Only macOS records today; a platform that did
    /// not record it matches by `key_char` instead (see [`resolve`]), so a file
    /// carried between machines still toggles. If a second platform grows a
    /// recorder, add a tag saying which platform the code belongs to; do not
    /// invent a universal key code table for two platforms.
    ///
    /// Command is never allowed (it belongs to the system), so it has no field.
    Custom {
        /// Control held.
        control: bool,
        /// Shift held.
        shift: bool,
        /// Option (Alt) held.
        option: bool,
        /// The character the key produced, for matching where the code is unknown.
        key_char: char,
        /// The platform key code recorded with the combination, if any. Older
        /// files wrote this as `keycode` or `macos_keycode`.
        #[cfg_attr(
            feature = "serde",
            serde(default, alias = "keycode", alias = "macos_keycode")
        )]
        raw_code: Option<i64>,
    },
}

impl HotkeyPreset {
    /// The recorded key code of a custom combination, if there is one.
    #[must_use]
    pub fn raw_code(self) -> Option<i64> {
        match self {
            Self::Custom { raw_code, .. } => raw_code,
            _ => None,
        }
    }
}

use crate::event::{Key, KeyEvent, Modifiers};

/// Which key a hotkey is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyKey {
    /// A key with a neutral identity — how the four shipped presets are defined,
    /// and the only form that means the same thing on every platform.
    Identity(Key),
    /// A virtual key code recorded *on this platform*, compared as an opaque
    /// integer against [`KeyEvent::raw_code`].
    RawCode(i64),
    /// The display character, used when a hotkey was recorded on another platform
    /// and there is no key code this one can match. Layout-dependent, and the
    /// platform is expected to say so in the log — see [`Hotkey::is_char_fallback`].
    Char(char),
}

/// A hotkey, resolved for the platform that is about to match it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    /// Control must be held.
    pub control: bool,
    /// Shift must be held.
    pub shift: bool,
    /// Option must be held.
    pub option: bool,
    /// The key to match.
    pub key: HotkeyKey,
}

impl Hotkey {
    /// Whether this hotkey is matching on the display character because the
    /// platform had no key code of its own for it. Correct only while the user
    /// stays on the layout they recorded it with; the platform logs that it took
    /// this path so a hotkey that stops working has a trail.
    #[must_use]
    pub const fn is_char_fallback(&self) -> bool {
        matches!(self.key, HotkeyKey::Char(_))
    }

    /// Whether this event is the hotkey.
    ///
    /// Command is never allowed: those belong to the system and to the focused
    /// application, and a hotkey that shadowed ⌘Q would be indistinguishable from
    /// a broken keyboard.
    #[must_use]
    pub fn matches(&self, event: &KeyEvent) -> bool {
        if !self.key_matches(event) {
            return false;
        }
        let m = event.mods;
        m.control == self.control && m.shift == self.shift && m.option == self.option && !m.command
    }

    fn key_matches(&self, event: &KeyEvent) -> bool {
        match self.key {
            HotkeyKey::Identity(key) => event.key == key,
            HotkeyKey::RawCode(code) => event.raw_code == code,
            // A letter or Space is matched by identity even here: it is the same
            // key wherever the layout puts it, and with Control held the reported
            // character is a control code rather than the letter.
            HotkeyKey::Char(' ') => event.key == Key::Space,
            HotkeyKey::Char(ch) if ch.is_ascii_alphabetic() => {
                event.key == Key::Letter(ch.to_ascii_lowercase())
            }
            HotkeyKey::Char(ch) => event.ch == Some(ch),
        }
    }
}

/// Resolves a stored preset into something this platform can match.
///
/// `recorded_code` is the virtual key code *this* platform has for a
/// [`HotkeyPreset::Custom`], or `None` when the combination was recorded
/// somewhere else — in which case the display character is the only thing left
/// to go on. The four named presets need neither: they are modifiers plus a
/// semantic key, and mean the same everywhere.
#[must_use]
pub fn resolve(preset: HotkeyPreset, recorded_code: Option<i64>) -> Hotkey {
    match preset {
        HotkeyPreset::CtrlShiftSpace => Hotkey {
            control: true,
            shift: true,
            option: false,
            key: HotkeyKey::Identity(Key::Space),
        },
        HotkeyPreset::CtrlSpace => Hotkey {
            control: true,
            shift: false,
            option: false,
            key: HotkeyKey::Identity(Key::Space),
        },
        HotkeyPreset::OptionSpace => Hotkey {
            control: false,
            shift: false,
            option: true,
            key: HotkeyKey::Identity(Key::Space),
        },
        HotkeyPreset::CtrlShiftZ => Hotkey {
            control: true,
            shift: true,
            option: false,
            key: HotkeyKey::Identity(Key::Letter('z')),
        },
        HotkeyPreset::Custom {
            control,
            shift,
            option,
            key_char,
            ..
        } => Hotkey {
            control,
            shift,
            option,
            key: match recorded_code {
                Some(code) => HotkeyKey::RawCode(code),
                None => HotkeyKey::Char(key_char),
            },
        },
    }
}

/// The per-application enable/disable hotkey: ⌃⇧E.
#[must_use]
pub fn is_app_toggle(event: &KeyEvent) -> bool {
    event.mods.is_ctrl_shift() && event.key == Key::Letter('e')
}

/// The correction hotkey: ⌃⇧W, pressed just after a word to swap it to its other
/// reading and remember that choice.
///
/// Fixed rather than configurable, like ⌃⇧E and unlike the VN/EN toggle. Only the
/// toggle is configurable because it is the one people press constantly and hold
/// opinions about; a second recorder for a key pressed a few times a day would be
/// machinery for its own sake.
#[must_use]
pub fn is_correction(event: &KeyEvent) -> bool {
    event.mods.is_ctrl_shift() && event.key == Key::Letter('w')
}

/// What one keystroke means while the Settings window is recording a hotkey.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyCapture {
    /// Not a candidate combination — let it through untouched, and keep waiting.
    ///
    /// This is why an armed-and-forgotten recorder can never lock the keyboard:
    /// plain typing, shifted letters and every ⌘ shortcut (⌘Q, ⌘Tab, ⌘S…) take
    /// this path.
    Passthrough,
    /// Escape: stop recording, keep the old hotkey, consume the key.
    Cancel,
    /// A combination GlowKey already owns. Swallowed, still recording; `reason`
    /// is the line to log.
    Reserved {
        /// Why it was refused, ready for the log.
        reason: &'static str,
    },
    /// The combination is the new hotkey. The platform stores it with its own key
    /// code and ends the recording.
    Captured {
        /// Control was held.
        control: bool,
        /// Shift was held.
        shift: bool,
        /// Option was held.
        option: bool,
        /// The character to show for the key (uppercased; `' '` means Space).
        key_char: char,
    },
}

/// One step of hotkey recording.
///
/// Only key-downs that could BE the hotkey are intercepted: a ⌃/⌥ combination
/// without ⌘ is captured, and Escape cancels. ⌃⇧E and ⌃⇧W are refused — recording
/// either would shadow a built-in feature with no warning, costing the user both.
#[must_use]
pub fn capture(event: &KeyEvent) -> HotkeyCapture {
    if event.key == Key::Escape {
        return HotkeyCapture::Cancel;
    }
    let Modifiers {
        control,
        shift,
        option,
        command,
    } = event.mods;
    if command || (!control && !option) {
        return HotkeyCapture::Passthrough;
    }
    if is_app_toggle(event) {
        return HotkeyCapture::Reserved {
            reason: "HOTKEY ⌃⇧E is reserved (per-app toggle) — pick another combo",
        };
    }
    if is_correction(event) {
        return HotkeyCapture::Reserved {
            reason: "HOTKEY ⌃⇧W is reserved (correct last word) — pick another combo",
        };
    }
    // Display character: with Control held the event's character is a control
    // code (⌃A → U+0001), so map it back to its letter; Space by identity,
    // because its character is not reliably `' '` under modifiers.
    let key_char = if event.key == Key::Space {
        ' '
    } else {
        match event.ch {
            Some(c) if ('\x01'..='\x1a').contains(&c) => ((c as u8 - 1) + b'A') as char,
            Some(c) if !c.is_control() => c.to_ascii_uppercase(),
            _ => '?',
        }
    };
    HotkeyCapture::Captured {
        control,
        shift,
        option,
        key_char,
    }
}
