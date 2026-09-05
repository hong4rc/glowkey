//! The interface language (product preference; leaves this crate in a later phase).

use super::*;

/// Which language the user interface is written in.
///
/// Unikey exposes this as a single "Vietnamese interface" checkbox. A checkbox
/// cannot say "whatever the system is set to", which is what a native macOS
/// application should do by default, so this is three-valued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Language {
    /// Follow the system's preferred language.
    #[default]
    System,
    /// Vietnamese interface, whatever the system says.
    Vietnamese,
    /// English interface, whatever the system says.
    English,
}
