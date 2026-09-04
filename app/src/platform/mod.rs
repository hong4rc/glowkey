//! The platform backends: everything that touches an operating system.
//!
//! GlowKey's policy — what to do with a keystroke — lives in
//! `crates/glowkey-input` and has no operating system in it at all. What is left
//! here is translation and input/output: reading a native key event into a
//! neutral one, carrying out a `Decision` by synthesizing keystrokes, and the
//! shell around that (the permission gate, the health monitor, the settings
//! accessors).
//!
//! Exactly one backend compiles at a time. Windows and Linux join macOS here as
//! Phases 4 and 9 of the port land.
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;
