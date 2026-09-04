//! The per-platform application tables: which applications ship excluded, which
//! of those are terminals, and which are Chromium-family browsers.
//!
//! **This is the one place in the engine that may see a target operating
//! system, and it is allowed here because it selects *data*, never logic.** An
//! application's identity is whatever its platform calls it — a bundle
//! identifier on macOS, an executable file name on Windows — so a single table
//! could only ever be right for one of them. The rules that consume these
//! tables (`ExclusionList`, `is_terminal`, `is_chromium_app`) are shared and
//! have no idea which one they were handed.
//!
//! Nothing else in the crate may follow this precedent. A `cfg` around
//! behaviour would mean the Vietnamese engine does something different
//! depending on where it runs, which is the one thing the port must not do.
//!
//! Linux borrows the macOS table until Phase 8 decides what an application
//! identity even is there. That is not a claim that bundle identifiers exist on
//! Linux; it keeps the Linux CI job — which is what stops platform code leaking
//! into this crate at all — testing the same data a real platform ships.

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{CHROMIUM_APP_PREFIXES, DEFAULT_EXCLUSIONS, TERMINAL_EXCLUSIONS};

#[cfg(not(target_os = "windows"))]
mod macos;
#[cfg(not(target_os = "windows"))]
pub use macos::{CHROMIUM_APP_PREFIXES, DEFAULT_EXCLUSIONS, TERMINAL_EXCLUSIONS};
