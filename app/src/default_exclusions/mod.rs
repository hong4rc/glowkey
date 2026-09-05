//! The per-platform application tables: which applications ship excluded, which
//! of those are terminals, and which are Chromium-family browsers.
//!
//! An application's identity is whatever its platform calls it, a bundle
//! identifier on macOS and an executable file name on Windows, so a single
//! table could only ever be right for one of them. The rules that consume these
//! tables live in `glowkey-session` and are handed the data through
//! [`ExclusionDefaults`]; they have no idea which platform they were built for.
//!
//! This module selects *data*, never logic. Behaviour that differs by platform
//! belongs under `platform/`.
//!
//! Linux borrows the macOS table until a Linux shell decides what an
//! application identity even is there. That is not a claim that bundle
//! identifiers exist on Linux; it keeps the crate's stub build, which is what
//! CI checks off macOS and Windows, compiling the same data a real platform
//! ships.

use glowkey_session::ExclusionDefaults;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{CHROMIUM_APP_PREFIXES, DEFAULT_EXCLUSIONS, TERMINAL_EXCLUSIONS};

#[cfg(not(target_os = "windows"))]
mod macos;
#[cfg(not(target_os = "windows"))]
pub use macos::{CHROMIUM_APP_PREFIXES, DEFAULT_EXCLUSIONS, TERMINAL_EXCLUSIONS};

/// The shipped tables in the shape the session takes them.
#[must_use]
pub fn shipped() -> ExclusionDefaults {
    ExclusionDefaults::new(
        DEFAULT_EXCLUSIONS.iter().copied(),
        TERMINAL_EXCLUSIONS.iter().copied(),
    )
}

/// Whether this application identity is a known terminal (see
/// [`TERMINAL_EXCLUSIONS`]). The session answers this at run time from the
/// tables `shipped` hands it; the tests ask the table directly.
#[cfg(test)]
pub fn is_terminal(app_id: &str) -> bool {
    TERMINAL_EXCLUSIONS.contains(&app_id)
}

/// Whether this application identity is a Chromium-family browser (see
/// [`CHROMIUM_APP_PREFIXES`]).
///
/// A prefix match, because macOS ships channel-suffixed bundle identifiers:
/// `com.google.Chrome.canary` is Chrome and must be guarded like Chrome.
#[must_use]
pub fn is_chromium_app(app_id: &str) -> bool {
    CHROMIUM_APP_PREFIXES
        .iter()
        .any(|prefix| app_id.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The invariant that makes the session-only terminal toggle safe: a hotkey
    /// can only *suspend* a terminal's exclusion, and that check reads the
    /// terminal table, so a terminal missing from the defaults would be
    /// permanently un-excludable by accident. The session's constructor closes
    /// the hole; this keeps the tables honest at the source as well.
    #[test]
    fn every_terminal_is_also_a_shipped_default() {
        for id in TERMINAL_EXCLUSIONS {
            assert!(
                DEFAULT_EXCLUSIONS.contains(id),
                "{id} missing from defaults"
            );
        }
    }

    #[test]
    fn terminals_are_told_apart_from_editors() {
        assert!(is_terminal(TERMINAL_EXCLUSIONS[0]));
        assert!(!is_terminal("com.example.definitely-not-a-terminal"));
        // Some shipped default is an editor rather than a terminal, or the
        // session-only rule would apply to everything.
        assert!(
            DEFAULT_EXCLUSIONS.iter().any(|id| !is_terminal(id)),
            "the defaults are all terminals: the editor entries went missing"
        );
    }

    #[test]
    fn the_shipped_defaults_reach_the_session_intact() {
        let defaults = shipped();
        for id in DEFAULT_EXCLUSIONS {
            assert!(defaults.is_default(id), "{id} dropped on the way in");
        }
        for id in TERMINAL_EXCLUSIONS {
            assert!(defaults.is_terminal(id), "{id} lost its terminal status");
        }
        assert_eq!(defaults.excluded().count(), DEFAULT_EXCLUSIONS.len());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn the_macos_table_still_names_the_apps_existing_settings_files_do() {
        // Bundle identifiers a shipped settings file already contains. Changing
        // any of these silently un-excludes an app for every existing user.
        for id in [
            "com.apple.Terminal",
            "com.googlecode.iterm2",
            "com.apple.dt.Xcode",
            "com.microsoft.VSCode",
            "com.mitchellh.ghostty",
        ] {
            assert!(DEFAULT_EXCLUSIONS.contains(&id), "{id} left the defaults");
        }
        assert!(is_terminal("com.mitchellh.ghostty"));
        assert!(!is_terminal("com.microsoft.VSCode")); // editor, not a terminal
        assert!(is_chromium_app("com.google.Chrome"));
        assert!(is_chromium_app("com.google.Chrome.canary"));
        assert!(!is_chromium_app("com.apple.Safari"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn the_windows_table_is_lowercased_executable_names() {
        // The identity Windows supplies is the executable file name, lowercased.
        // An entry with a capital in it would never match anything.
        for id in DEFAULT_EXCLUSIONS.iter().chain(CHROMIUM_APP_PREFIXES) {
            assert_eq!(*id, id.to_ascii_lowercase(), "{id} is not lowercased");
            assert!(id.ends_with(".exe"), "{id} is not an executable name");
        }
        assert!(is_terminal("windowsterminal.exe"));
        assert!(!is_terminal("code.exe")); // editor, not a terminal
        assert!(is_chromium_app("chrome.exe"));
        assert!(!is_chromium_app("firefox.exe"));
    }
}
