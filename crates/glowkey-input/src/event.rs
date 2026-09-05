//! What the platform saw: one key-down, described without naming an operating
//! system.
//!
//! Character and identity are kept apart because a key can have an identity and
//! no character (Backspace) or a character and no useful identity (a letter typed
//! on whatever layout the user has). The ladder reads the character for text and
//! the identity for control keys, and never has to know which virtual key code
//! table produced either.

/// The key identities the decision ladder actually branches on.
///
/// Deliberately **not** a universal keyboard enum. Every variant here exists
/// because `decide` or a hotkey looks at it; a variant that no policy reads is a
/// guess about a platform that has not been written yet. Adding one when Windows
/// or Linux genuinely needs it is cheap; carrying seventy speculative ones is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Key {
    /// Backspace (macOS calls it Delete, virtual key code 51).
    Backspace,
    /// Forward delete (⌦) — used by the Chromium omnibox guard, not by the ladder.
    ForwardDelete,
    /// Escape — cancels an armed hotkey recording.
    Escape,
    /// The space bar. An identity of its own because three of the four shipped
    /// hotkey presets are built on it, and because under modifiers its character
    /// is not reliably `' '`.
    Space,
    /// Return / Enter.
    Return,
    /// Tab.
    Tab,
    /// Arrows, Home/End, Page Up/Down — one class, exactly as the macOS tap has
    /// always treated them: anything that moves the caret where GlowKey cannot
    /// see it invalidates the diff baseline.
    CaretMove,
    /// A letter key, named by the letter its *physical position* carries on the
    /// platform's reference layout (lowercase ASCII).
    ///
    /// This is the identity, not the typed character: with Control held, macOS
    /// reports ⌃⇧Z's Unicode string as U+001A, and the shipped ⌃⇧Z preset has
    /// always matched the key code rather than that. `ch` carries what was
    /// actually typed.
    Letter(char),
    /// Anything else. The ladder treats it by its character, if it has one.
    Other,
}

/// Which modifiers were held. Command is macOS's ⌘ and Windows' Win key; Option
/// is ⌥ / Alt. The names follow the macOS spelling because that is the vocabulary
/// the settings file and the user interface already use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    /// Control held.
    pub control: bool,
    /// Shift held.
    pub shift: bool,
    /// Option / Alt held.
    pub option: bool,
    /// Command / Win held.
    pub command: bool,
}

impl Modifiers {
    /// No modifiers.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            control: false,
            shift: false,
            option: false,
            command: false,
        }
    }

    /// True when a *shortcut* modifier is held — Command, Control, or Option.
    /// Shift is excluded: it produces uppercase letters, which are ordinary text.
    #[must_use]
    pub const fn is_shortcut(self) -> bool {
        self.control || self.option || self.command
    }

    /// True when exactly Control and Shift are held — the modifier pattern shared
    /// by GlowKey's fixed ⌃⇧ hotkeys.
    #[must_use]
    pub const fn is_ctrl_shift(self) -> bool {
        self.control && self.shift && !self.command && !self.option
    }
}

/// One key-down, as the policy sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    /// The character the active layout produced, if any. Already mapped, so a
    /// Colemak user's `n` arrives as `n`.
    pub ch: Option<char>,
    /// What key it was, for the branches that cannot be decided from a character.
    pub key: Key,
    /// Which modifiers were held.
    pub mods: Modifiers,
    /// The platform's own virtual key code, opaque to the policy.
    ///
    /// The one thing here that is not portable, and it is carried rather than
    /// interpreted: a custom hotkey the user recorded is stored as the key code
    /// the platform reported at the time, so matching it again means comparing
    /// integers that only that platform gives meaning to. Nothing else in this
    /// crate reads it.
    pub raw_code: i64,
}

impl KeyEvent {
    /// A plain, unmodified character key — the shape most tests need.
    #[must_use]
    pub fn character(ch: char) -> Self {
        Self {
            ch: Some(ch),
            key: if ch.is_ascii_alphabetic() {
                Key::Letter(ch.to_ascii_lowercase())
            } else if ch == ' ' {
                Key::Space
            } else {
                Key::Other
            },
            mods: Modifiers::none(),
            raw_code: 0,
        }
    }

    /// A control key with no character.
    #[must_use]
    pub fn key(key: Key) -> Self {
        Self {
            ch: None,
            key,
            mods: Modifiers::none(),
            raw_code: 0,
        }
    }

    /// The same event with these modifiers held.
    #[must_use]
    pub fn with_mods(mut self, mods: Modifiers) -> Self {
        self.mods = mods;
        self
    }

    /// The same event carrying the platform's virtual key code.
    #[must_use]
    pub fn with_raw_code(mut self, raw_code: i64) -> Self {
        self.raw_code = raw_code;
        self
    }
}
