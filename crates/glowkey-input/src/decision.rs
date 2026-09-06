//! The outcome of processing one key event, and the things the platform must do
//! about it that are not the keystroke itself.

use glowkey_session::{InputMode, KeyResponse};

/// The outcome of processing one key event.
///
/// Five variants, unchanged from the macOS tap that this was lifted out of.
/// `EmitThenReplayKey` in particular is load-bearing: it is the difference
/// between `ddc`␣ typing `đc ` and typing `đddc`.
#[derive(Debug)]
#[non_exhaustive]
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

/// Why the composing word was discarded.
///
/// A flush is invisible from the outside: the engine simply stops vouching for
/// what is on screen, and the next Backspace passes through instead of being
/// handled. That is correct — the blind model must forget when the caret may
/// have moved without us — but it looks identical to a bug, and until this
/// existed there was no way to tell the two apart from a log.
///
/// Reported rather than logged, because three of the six sites are in this
/// crate, which has no output of its own.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlushCause {
    /// A ⌘/Ctrl/Alt shortcut, which may select, paste or move the caret.
    Shortcut,
    /// A mid-word Backspace the engine could not stay in step with.
    UnfollowableBackspace,
    /// An arrow, Home, End or Page key.
    CaretMove,
    /// A mouse button went down somewhere.
    MouseButton,
    /// The frontmost application changed.
    ///
    /// Includes an application activating *itself* mid-word — a call popup, a
    /// finished build — which takes the word with it and is the hardest version
    /// of this to explain without a log line.
    AppSwitch,
    /// The VN/EN mode was toggled.
    ModeToggle,
    /// The frontmost application was added to or removed from the ignore list.
    ExclusionToggle,
    /// "Reset input" — the user asking for the composing state to be dropped.
    Reset,
    /// The tap was re-enabled after the system disabled it, so keystrokes were
    /// delivered natively while it was off and the render is stale.
    TapRecovered,
}

impl FlushCause {
    /// A short, greppable tag for the log line.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::Shortcut => "shortcut",
            Self::UnfollowableBackspace => "backspace-out-of-step",
            Self::CaretMove => "caret-move",
            Self::MouseButton => "mouse-button",
            Self::AppSwitch => "app-switch",
            Self::ModeToggle => "mode-toggle",
            Self::ExclusionToggle => "exclusion-toggle",
            Self::Reset => "reset-input",
            Self::TapRecovered => "tap-recovered",
        }
    }
}

impl core::fmt::Display for FlushCause {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.tag())
    }
}

/// What the platform must do besides the keystroke, reported as plain data.
///
/// The ladder used to write to the log, flash the on-screen indicator and repaint
/// the menu bar itself. None of that is policy, and all of it is an operating
/// system. Reporting it back instead is what lets the same ladder run under a
/// test with no window server — and the platform performs the effects in field
/// order, immediately after `decide` returns, so the log still reads in the order
/// it always has.
#[non_exhaustive]
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
    /// The composing word was discarded, for this reason.
    ///
    /// Only the ladder's own flushes appear here. A shell that flushes on its
    /// own account — a mouse click, an application switch — reports that itself,
    /// because it never enters `decide`.
    ///
    /// Positioned before `refresh_glyph` deliberately: the effects are carried
    /// out in field order, and the reason a word vanished belongs in the log
    /// before the repaint that followed it.
    pub flushed: Option<FlushCause>,
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
