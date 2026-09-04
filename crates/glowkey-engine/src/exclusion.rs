//! The per-application ignore list — GlowKey's primary feature.
//!
//! An excluded application never transforms a keystroke, and the exclusion always
//! wins over VN/EN mode and over any remembered per-app state.
//!
//! The rules here are pure and know nothing about what an application identity
//! *is*: macOS supplies a bundle identifier, Windows an executable file name.
//! Only the shipped tables differ, and they live in
//! [`crate::exclusion_defaults`]. The shell supplies the frontmost identity and
//! persists the set.

use std::collections::BTreeSet;

/// The set of application bundle identifiers where Vietnamese input is suppressed.
///
/// Ordered for stable, deterministic serialization when the shell persists it.
///
/// Two auxiliary sets refine the persisted `bundle_ids`:
/// - `removed_defaults` (persisted): defaults the user deliberately removed. At
///   load, the effective list is `saved ∪ (DEFAULT_EXCLUSIONS − removed_defaults)`,
///   so a default added in a new release reaches existing settings files without
///   resurrecting ones the user removed on purpose.
/// - `session_removed` (never persisted): exclusions suspended until restart —
///   used when a known terminal is un-excluded by hotkey, so an accidental ⌃⇧E in
///   a terminal cannot permanently disable its protection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExclusionList {
    bundle_ids: BTreeSet<String>,
    /// Default exclusions the user deliberately removed (tombstones, persisted).
    removed_defaults: BTreeSet<String>,
    /// Exclusions suspended for this session only (not persisted).
    session_removed: BTreeSet<String>,
}

impl ExclusionList {
    /// An empty list.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The default exclusions seeded on first run: the applications people most
    /// often want left in raw ASCII — terminals and editors.
    #[must_use]
    pub fn with_defaults() -> Self {
        let mut list = Self::new();
        for id in DEFAULT_EXCLUSIONS {
            list.add(*id);
        }
        list
    }

    /// Builds a list from stored bundle identifiers.
    pub fn from_ids<I, S>(ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            bundle_ids: ids.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    /// Builds the effective list from a settings file: the saved ids, plus every
    /// shipped default the user has not deliberately removed. This is how a
    /// default added in a new release (e.g. a new terminal) reaches settings
    /// files written before it existed.
    pub fn from_saved<I, S, J, T>(ids: I, removed_defaults: J) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
        J: IntoIterator<Item = T>,
        T: Into<String>,
    {
        let removed_defaults: BTreeSet<String> =
            removed_defaults.into_iter().map(Into::into).collect();
        let mut bundle_ids: BTreeSet<String> = ids.into_iter().map(Into::into).collect();
        for id in DEFAULT_EXCLUSIONS {
            if !removed_defaults.contains(*id) {
                bundle_ids.insert((*id).to_string());
            }
        }
        Self {
            bundle_ids,
            removed_defaults,
            session_removed: BTreeSet::new(),
        }
    }

    /// Whether the application with this bundle identifier is excluded.
    ///
    /// This is the single check the keystroke path makes before touching the
    /// engine. It must be the first decision, ahead of mode and of memory.
    #[must_use]
    pub fn is_excluded(&self, bundle_id: &str) -> bool {
        self.bundle_ids.contains(bundle_id) && !self.session_removed.contains(bundle_id)
    }

    /// Adds a bundle identifier. Returns true if it was newly added. Clears any
    /// session suspension. A tombstone is deliberately KEPT: it only suppresses
    /// the defaults merge at load, and an explicit entry wins over it anyway —
    /// clearing it would let an accidental toggle pair (remove + re-add) silently
    /// destroy the user's recorded removal of a default.
    pub fn add(&mut self, bundle_id: impl Into<String>) -> bool {
        let bundle_id = bundle_id.into();
        self.session_removed.remove(&bundle_id);
        self.bundle_ids.insert(bundle_id)
    }

    /// Removes a bundle identifier permanently. Returns true if it was present.
    /// A removed shipped default is tombstoned so a later load does not re-add it.
    pub fn remove(&mut self, bundle_id: &str) -> bool {
        self.session_removed.remove(bundle_id);
        if DEFAULT_EXCLUSIONS.contains(&bundle_id) {
            self.removed_defaults.insert(bundle_id.to_string());
        }
        self.bundle_ids.remove(bundle_id)
    }

    /// Suspends an exclusion until restart: the app stops being excluded now, but
    /// the persisted list still contains it, so the next launch re-excludes it.
    /// Used for known terminals un-excluded by hotkey. No-op if not excluded.
    pub fn suspend_for_session(&mut self, bundle_id: &str) {
        if self.bundle_ids.contains(bundle_id) {
            self.session_removed.insert(bundle_id.to_string());
        }
    }

    /// Lifts a session suspension (the app is excluded again). Returns whether a
    /// suspension existed.
    pub fn resume(&mut self, bundle_id: &str) -> bool {
        self.session_removed.remove(bundle_id)
    }

    /// Whether this app's exclusion is currently suspended for the session.
    #[must_use]
    pub fn is_session_suspended(&self, bundle_id: &str) -> bool {
        self.session_removed.contains(bundle_id)
    }

    /// The tombstoned defaults (deliberately removed by the user), for persistence.
    pub fn removed_default_ids(&self) -> impl Iterator<Item = &str> {
        self.removed_defaults.iter().map(String::as_str)
    }

    /// The excluded bundle identifiers, sorted — for persistence and for the editor.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.bundle_ids.iter().map(String::as_str)
    }

    /// How many applications are excluded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bundle_ids.len()
    }

    /// Whether the list is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bundle_ids.is_empty()
    }
}

pub use crate::exclusion_defaults::{
    CHROMIUM_APP_PREFIXES, DEFAULT_EXCLUSIONS, TERMINAL_EXCLUSIONS,
};

/// Whether this application identity is a known terminal (see
/// [`TERMINAL_EXCLUSIONS`]).
#[must_use]
pub fn is_terminal(app_id: &str) -> bool {
    TERMINAL_EXCLUSIONS.contains(&app_id)
}

/// Whether this application identity is a Chromium-family browser (see
/// [`CHROMIUM_APP_PREFIXES`]).
///
/// A prefix match, because macOS ships channel-suffixed bundle identifiers —
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

    /// A shipped default and a shipped terminal, whatever this platform's table
    /// calls them. Written this way so the same tests hold on either side of the
    /// macOS/Windows table split rather than quietly becoming macOS-only.
    fn a_default() -> &'static str {
        DEFAULT_EXCLUSIONS[0]
    }
    fn another_default() -> &'static str {
        DEFAULT_EXCLUSIONS[1]
    }

    #[test]
    fn excluded_apps_are_recognized() {
        let list = ExclusionList::with_defaults();
        assert!(list.is_excluded(a_default()));
        assert!(list.is_excluded(another_default()));
        assert!(!list.is_excluded("com.example.definitely-not-shipped"));
    }

    #[test]
    fn add_and_remove() {
        let mut list = ExclusionList::new();
        assert!(list.add("com.example.app"));
        assert!(!list.add("com.example.app")); // already present
        assert!(list.is_excluded("com.example.app"));
        assert!(list.remove("com.example.app"));
        assert!(!list.is_excluded("com.example.app"));
    }

    #[test]
    fn tombstone_survives_a_remove_add_pair() {
        // A deliberate removal must not be silently destroyed by a later
        // add/remove pair (e.g. two accidental ⌃⇧E presses).
        let victim = another_default();
        let mut list = ExclusionList::with_defaults();
        list.remove(victim); // deliberate, tombstoned
        list.add(victim); // toggled back on
        list.remove(victim); // and off again
        assert!(list.removed_default_ids().any(|id| id == victim));
        // A fresh load must not resurrect it via the defaults merge.
        let reloaded = ExclusionList::from_saved(
            list.ids().map(String::from).collect::<Vec<_>>(),
            list.removed_default_ids()
                .map(String::from)
                .collect::<Vec<_>>(),
        );
        assert!(!reloaded.is_excluded(victim));
    }

    #[test]
    fn roundtrips_through_ids() {
        let list = ExclusionList::with_defaults();
        let ids: Vec<String> = list.ids().map(String::from).collect();
        let restored = ExclusionList::from_ids(ids);
        assert_eq!(list, restored);
    }

    #[test]
    fn from_saved_merges_new_defaults_into_old_files() {
        // A settings file written before the last default was shipped: loading
        // must add it, and must keep the user's own entry.
        let list =
            ExclusionList::from_saved([a_default(), "com.example.custom"], Vec::<String>::new());
        assert!(list.is_excluded(DEFAULT_EXCLUSIONS[DEFAULT_EXCLUSIONS.len() - 1]));
        assert!(list.is_excluded("com.example.custom"));
    }

    #[test]
    fn from_saved_respects_tombstones() {
        // The user deliberately removed one; the merge must not resurrect it.
        let list = ExclusionList::from_saved([a_default()], [another_default()]);
        assert!(!list.is_excluded(another_default()));
        assert!(list.is_excluded(a_default()));
    }

    #[test]
    fn removing_a_default_tombstones_it() {
        let mut list = ExclusionList::with_defaults();
        assert!(list.remove(another_default()));
        let tombstones: Vec<&str> = list.removed_default_ids().collect();
        assert_eq!(tombstones, vec![another_default()]);
        // Re-adding keeps the tombstone (the explicit entry wins over it anyway,
        // and it must survive an accidental remove/add pair).
        list.add(another_default());
        assert!(list.is_excluded(another_default()));
        assert_eq!(list.removed_default_ids().count(), 1);
        // A non-default removal never tombstones.
        list.add("com.example.app");
        list.remove("com.example.app");
        assert_eq!(list.removed_default_ids().count(), 1);
    }

    #[test]
    fn session_suspension_is_not_a_removal() {
        let terminal = TERMINAL_EXCLUSIONS[0];
        let mut list = ExclusionList::with_defaults();
        list.suspend_for_session(terminal);
        assert!(!list.is_excluded(terminal));
        assert!(list.is_session_suspended(terminal));
        // The persisted ids still contain it — a restart re-excludes.
        assert!(list.ids().any(|id| id == terminal));
        // Resuming re-excludes immediately.
        assert!(list.resume(terminal));
        assert!(list.is_excluded(terminal));
    }

    /// The invariant that makes the session-only terminal toggle safe: a hotkey
    /// can only *suspend* a terminal's exclusion, and that check reads the
    /// terminal table, so a terminal missing from the defaults would be
    /// permanently un-excludable by accident.
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
            "the defaults are all terminals — the editor entries went missing"
        );
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
