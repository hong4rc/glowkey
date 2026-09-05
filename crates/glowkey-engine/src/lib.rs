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
//! VN/EN mode — those are the shell's concern. When Vietnamese input is off, the
//! shell simply never calls [`Engine::process_key`].

#![warn(missing_docs)]

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use vi::methods::{Action, IncrementalBuffer};
use vi::processor::AccentStyle;
use vi::processor::{LetterModification, ToneMark};

pub mod config;
mod english;
pub mod exclusion;
mod exclusion_defaults;

pub use config::Settings;
pub use exclusion::ExclusionList;

mod engine;
mod hotkey;
mod language;
mod macros;
mod method;
mod overrides;
mod session;
mod tones;

pub use engine::*;
pub use hotkey::*;
pub use language::*;
pub use macros::*;
pub use method::*;
pub use overrides::*;
pub use session::*;
pub use tones::*;
