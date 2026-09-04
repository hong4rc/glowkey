//! Shipped application identities, asked of the table rather than spelled out.
//!
//! An application's identity is whatever its platform calls it — a bundle
//! identifier on macOS, an executable file name on Windows — so the shipped
//! exclusion table is per-target and a test that writes `com.apple.Terminal`
//! into an assertion is a macOS test wearing a portable test's clothes. Six of
//! them were, and they failed the moment the suite ran on Windows.
//!
//! The repair is not to `cfg` the tests. It is to stop naming platform
//! identities at all: a test about the *merge rule* wants any default, a test
//! about the terminal suspension wants any shipped terminal, and neither cares
//! which. These helpers answer those questions from the public table, so a test
//! written against them holds on every platform the table is defined for —
//! including ones that do not exist yet.
//!
//! The same idiom already lives in `exclusion.rs`'s own unit tests, which can
//! reach the table directly. This module is that idiom made available to the
//! integration tests, which cannot.

#![allow(dead_code)] // Each integration test binary uses its own subset.

use glowkey_engine::exclusion::{is_terminal, DEFAULT_EXCLUSIONS, TERMINAL_EXCLUSIONS};

/// A shipped default exclusion, whatever this platform's table calls it.
pub fn a_default() -> &'static str {
    DEFAULT_EXCLUSIONS
        .first()
        .expect("the shipped exclusion table is never empty")
}

/// A shipped default that is a **terminal**, so the session-only un-exclusion
/// rule applies to it.
///
/// The terminal rule is the one place the two tables have to agree: a name in
/// `TERMINAL_EXCLUSIONS` that is not also a default would be a terminal nobody
/// is protected from. `exclusion.rs` has a test for that agreement; this
/// function depends on it, so it asserts rather than assumes.
pub fn a_terminal_default() -> &'static str {
    let id = TERMINAL_EXCLUSIONS
        .first()
        .expect("the shipped terminal table is never empty");
    assert!(
        DEFAULT_EXCLUSIONS.contains(id),
        "{id} is a known terminal but not a shipped default — \
         the terminal protection does not reach it"
    );
    id
}

/// A shipped default that is **not** a terminal — an editor — so removing it via
/// the toggle is permanent rather than session-only.
pub fn an_editor_default() -> &'static str {
    DEFAULT_EXCLUSIONS
        .iter()
        .find(|id| !is_terminal(id))
        .expect("the shipped table ships editors as well as terminals")
}

/// An identity that ships excluded on no platform.
///
/// Spelled as a reverse-DNS name that is also not a plausible executable, so it
/// cannot collide with either table's shape as they grow.
pub const NOT_SHIPPED: &str = "com.example.definitely-not-shipped";
