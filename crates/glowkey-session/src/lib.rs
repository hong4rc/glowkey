//! Typing policy over the GlowKey engine.
//!
//! [`glowkey_engine`] turns keystrokes into Vietnamese and knows nothing else.
//! This crate is the layer a *product* needs on top of it, and it is still free
//! of any operating system:
//!
//! - [`Session`]: the facade. VN/EN mode, the frontmost application, the word
//!   history that lets a committed word be re-opened, auto-fix at a boundary,
//!   sentence capitalisation, and the correction hotkey.
//! - [`ExclusionList`]: the per-application ignore list, the feature that
//!   defines GlowKey. Which applications ship excluded is not decided here: the
//!   shell hands in an [`ExclusionDefaults`] built from its own tables, because
//!   an application's identity is whatever its platform calls it.
//! - [`AppId`]: that identity, opaque to this crate.
//! - [`Macro`] and [`WordOverride`]: text expansion and per-word decisions.
//!
//! A consumer who wants only the Vietnamese transformation takes the engine
//! crate alone. One who wants an input method's behaviour takes this crate,
//! builds a [`Session`] through [`Session::builder`], tells it which application
//! is frontmost, and feeds it keys. The engine's public types are re-exported so
//! that consumer names one crate.

#![warn(missing_docs)]

pub use glowkey_engine::*;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

mod app_id;
mod builder;
mod english;
mod exclusion;
mod macros;
mod overrides;
mod session;

pub use app_id::AppId;
pub use builder::SessionBuilder;
pub use exclusion::{ExclusionDefaults, ExclusionList};
pub use macros::*;
pub use overrides::*;
pub use session::*;
