//! The outcome of processing one key event, and the things the platform must do
//! about it that are not the keystroke itself.

use glowkey_session::{InputMode, KeyResponse};

/// The outcome of processing one key event.
///
/// Five variants, unchanged from the macOS tap that this was lifted out of.
/// `EmitThenReplayKey` in particular is load-bearing: it is the difference
/// between `ddc`␣ typing `đc ` and typing `đddc`.
#[derive(Debug)]
pub enum Decision {
    /// Let the original keystroke through unchanged.
    Passthrough,
    /// Suppress the original with no output (e.g. the VN/EN toggle hotkey).
    Consume,
    /// Toggle the current app's ignore-list membership, then consume the key.
    ToggleApp,
    /// Suppress the original and apply this edit (backspaces + insert).
    Emit(KeyResponse),
    /// Apply this edit (e.g. an auto-fix restore) and then replay the original
    /// key from GlowKey's own source, so the boundary key that triggered the
    /// commit still types — but lands *after* the edit rather than racing it.
    EmitThenReplayKey(KeyResponse),
}

/// What the platform must do besides the keystroke, reported as plain data.
///
/// The ladder used to write to the log, flash the on-screen indicator and repaint
/// the menu bar itself. None of that is policy, and all of it is an operating
/// system. Reporting it back instead is what lets the same ladder run under a
/// test with no window server — and the platform performs the effects in field
/// order, immediately after `decide` returns, so the log still reads in the order
/// it always has.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Effects {
    /// The VN/EN mode was toggled to this. The platform announces it and flashes
    /// the on-screen indicator.
    pub mode_toggled: Option<InputMode>,
    /// The personal-words list changed, so any open editor should reload.
    pub personal_words_changed: bool,
    /// A word was corrected by ⌃⇧W: `(what was on screen, what replaces it)`.
    /// Absent even on a successful correction when the engine had nothing to
    /// describe.
    pub corrected: Option<(String, String)>,
    /// The menu-bar glyph no longer reflects the state.
    pub refresh_glyph: bool,
    /// Something changed that has to survive a quit; write the settings file.
    ///
    /// Deliberately not done inside the policy: keeping `decide` free of disk
    /// side effects is what lets the tests drive it against a real session
    /// without writing to the user's settings file.
    pub save_settings: bool,
}

impl Effects {
    /// Clears every field, so one buffer can be reused across keystrokes.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
