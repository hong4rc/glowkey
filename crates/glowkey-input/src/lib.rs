//! GlowKey's input policy, with no operating system in it.
//!
//! GlowKey is a **blind** input method: it never sees the document, so its one
//! invariant is that what the engine believes it rendered is the text tail at the
//! caret. Nothing verifies that. It is maintained purely by deciding correctly,
//! on every single key, whether to pass the key through, swallow it, or replace
//! the tail of the document with an edit — and by flushing the moment anything
//! could have moved the caret. Get it wrong and the failure is not a missing
//! diacritic; it is synthesized backspaces deleting text the user typed.
//!
//! That decision is this crate. It was lifted out of the macOS event tap, where
//! it had been corrected a dozen times by people typing Vietnamese into real
//! applications, and the ordering of [`decide`]'s steps is the record of those
//! corrections. It is a specification, not an implementation detail.
//!
//! # The boundary
//!
//! ```text
//! platform  ──  KeyEvent  ──▶  decide  ──▶  Decision  ──  platform
//!                                 │
//!                                 └──▶  Effects  (log, indicator, save)
//! ```
//!
//! In goes a [`KeyEvent`] — a character, a key identity, some modifiers. Out
//! comes a [`Decision`] and a plain-data list of [`Effects`] the platform is
//! expected to carry out. No input/output, no clock, no window server, no
//! `unsafe`, no `cfg(target_os)`, and no dependency but the engine. CI compiles
//! and tests it on Linux with `-D warnings`, which is what mechanically keeps it
//! that way.

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod decision;
mod event;
pub mod hotkey;
mod ladder;

pub use decision::{Decision, Effects};
pub use event::{Key, KeyEvent, Modifiers};
pub use hotkey::{Hotkey, HotkeyCapture, HotkeyKey, HotkeyPreset};
pub use ladder::{decide, Ctx};
