//! GlowKey's platform-free Vietnamese Telex transformation engine.
//!
//! This crate owns *all* Vietnamese logic and knows nothing about macOS. It wraps
//! [`vi`]'s incremental Telex buffer and turns each keystroke into a minimal edit —
//! how many trailing code units to delete, and what text to insert in their place —
//! so the platform shell can render the change with either marked text or an
//! insert-plus-backspace sequence without caring which.
//!
//! Design (matches the surveyed shipping engines, notably `xkey`): keep the raw
//! keystroke log for the word being typed and re-derive the whole rendering from
//! it on every keystroke. At a word's length this costs nothing, and it gives one
//! code path for forward typing, backspace, and case handling.
//!
//! The engine is intentionally ignorant of the per-application ignore list and of
//! VN/EN mode: those belong to `glowkey-session`, the policy layer built on top
//! of this crate. When Vietnamese input is off, that layer simply never calls
//! [`Engine::process_key`].
//!
//! `serde` support for [`InputMethod`] and [`PlacementStyle`] is behind the
//! `serde` feature, so a consumer that does not persist settings does not pay
//! for it.

#![warn(missing_docs)]

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use vi::methods::{Action, IncrementalBuffer};
use vi::processor::AccentStyle;
use vi::processor::{LetterModification, ToneMark};

mod engine;
mod method;
mod tones;

pub use engine::*;
pub use method::*;
pub use tones::*;
