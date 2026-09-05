//! Shipped application identities for the integration tests.
//!
//! The session does not know what an application is called: the shell hands it
//! the shipped tables as an `ExclusionDefaults`, so these tests hand it one too.
//! The names are invented. A test about the *merge rule* wants any default, a
//! test about the terminal suspension wants any shipped terminal, and neither
//! cares what a real platform calls them. Spelling `com.apple.Terminal` into an
//! assertion made six of these tests macOS tests wearing a portable test's
//! clothes, and they failed the moment the suite ran on Windows.

#![allow(dead_code)] // Each integration test binary uses its own subset.

use glowkey_session::{ExclusionDefaults, ExclusionList};

/// A shipped default that is a **terminal**, so the session-only un-exclusion
/// rule applies to it.
pub const A_TERMINAL: &str = "example.terminal";

/// A shipped default that is **not** a terminal, an editor, so removing it via
/// the toggle is permanent rather than session-only.
pub const AN_EDITOR: &str = "example.editor";

/// An identity that ships excluded on no platform.
///
/// Spelled as a reverse-DNS name that is also not a plausible executable, so it
/// cannot collide with either real table's shape as they grow.
pub const NOT_SHIPPED: &str = "com.example.definitely-not-shipped";

/// The tables a shell would ship: one terminal, one editor.
pub fn defaults() -> ExclusionDefaults {
    ExclusionDefaults::new([A_TERMINAL, AN_EDITOR], [A_TERMINAL])
}

/// The exclusion list as it stands on first run.
pub fn shipped() -> ExclusionList {
    ExclusionList::with_defaults(defaults())
}

/// A shipped default exclusion.
pub fn a_default() -> &'static str {
    A_TERMINAL
}

/// A shipped default that is a terminal.
pub fn a_terminal_default() -> &'static str {
    A_TERMINAL
}

/// A shipped default that is an editor.
pub fn an_editor_default() -> &'static str {
    AN_EDITOR
}
