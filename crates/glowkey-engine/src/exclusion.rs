//! The per-application ignore list — GlowKey's primary feature.
//!
//! An excluded application never transforms a keystroke, and the exclusion always
//! wins over VN/EN mode and over any remembered per-app state. This module is pure
//! logic keyed on bundle identifiers, so it is fully testable without macOS; the
//! shell supplies the frontmost bundle identifier and persists the set.

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

/// Terminal applications. Synthetic backspaces cannot delete inside a PTY (the
/// shell owns line editing), so Vietnamese in a terminal always produces garbage.
/// Un-excluding one via the ⌃⇧E hotkey is therefore session-only — a deliberate,
/// permanent removal must go through the Excluded Apps window.
pub const TERMINAL_EXCLUSIONS: &[&str] = &[
    "com.apple.Terminal",
    "com.googlecode.iterm2",
    "dev.warp.Warp-Stable",
    "dev.warp.Warp-Preview",
    "net.kovidgoyal.kitty",
    "com.github.wez.wezterm",
    "com.mitchellh.ghostty",
    "org.alacritty",
    "co.zeit.hyper",
];

/// Whether this bundle identifier is a known terminal (see [`TERMINAL_EXCLUSIONS`]).
#[must_use]
pub fn is_terminal(bundle_id: &str) -> bool {
    TERMINAL_EXCLUSIONS.contains(&bundle_id)
}

/// Applications excluded on first run — terminals and editors, where users
/// overwhelmingly want raw ASCII.
pub const DEFAULT_EXCLUSIONS: &[&str] = &[
    "com.apple.Terminal",
    "com.googlecode.iterm2",
    "com.apple.dt.Xcode",
    "com.microsoft.VSCode",
    "com.jetbrains.intellij",
    "com.jetbrains.pycharm",
    "com.jetbrains.WebStorm",
    "dev.warp.Warp-Stable",
    "dev.warp.Warp-Preview",
    "net.kovidgoyal.kitty",
    "com.github.wez.wezterm",
    "com.mitchellh.ghostty",
    "org.alacritty",
    "co.zeit.hyper",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excluded_apps_are_recognized() {
        let list = ExclusionList::with_defaults();
        assert!(list.is_excluded("com.apple.Terminal"));
        assert!(list.is_excluded("com.microsoft.VSCode"));
        assert!(!list.is_excluded("com.tinyspeck.slackmacgap"));
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
        let mut list = ExclusionList::with_defaults();
        list.remove("com.googlecode.iterm2"); // deliberate, tombstoned
        list.add("com.googlecode.iterm2"); // toggled back on
        list.remove("com.googlecode.iterm2"); // and off again
        assert!(list
            .removed_default_ids()
            .any(|id| id == "com.googlecode.iterm2"));
        // A fresh load must not resurrect it via the defaults merge.
        let reloaded = ExclusionList::from_saved(
            list.ids().map(String::from).collect::<Vec<_>>(),
            list.removed_default_ids()
                .map(String::from)
                .collect::<Vec<_>>(),
        );
        assert!(!reloaded.is_excluded("com.googlecode.iterm2"));
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
        // A settings file written before Ghostty was a default: loading must add it.
        let list = ExclusionList::from_saved(
            ["com.apple.Terminal", "com.example.custom"],
            Vec::<String>::new(),
        );
        assert!(list.is_excluded("com.mitchellh.ghostty"));
        assert!(list.is_excluded("com.example.custom"));
    }

    #[test]
    fn from_saved_respects_tombstones() {
        // The user deliberately removed VSCode; the merge must not resurrect it.
        let list = ExclusionList::from_saved(["com.apple.Terminal"], ["com.microsoft.VSCode"]);
        assert!(!list.is_excluded("com.microsoft.VSCode"));
        assert!(list.is_excluded("com.apple.Terminal"));
    }

    #[test]
    fn removing_a_default_tombstones_it() {
        let mut list = ExclusionList::with_defaults();
        assert!(list.remove("com.microsoft.VSCode"));
        let tombstones: Vec<&str> = list.removed_default_ids().collect();
        assert_eq!(tombstones, vec!["com.microsoft.VSCode"]);
        // Re-adding keeps the tombstone (the explicit entry wins over it anyway,
        // and it must survive an accidental remove/add pair).
        list.add("com.microsoft.VSCode");
        assert!(list.is_excluded("com.microsoft.VSCode"));
        assert_eq!(list.removed_default_ids().count(), 1);
        // A non-default removal never tombstones.
        list.add("com.example.app");
        list.remove("com.example.app");
        assert_eq!(list.removed_default_ids().count(), 1);
    }

    #[test]
    fn session_suspension_is_not_a_removal() {
        let mut list = ExclusionList::with_defaults();
        list.suspend_for_session("com.mitchellh.ghostty");
        assert!(!list.is_excluded("com.mitchellh.ghostty"));
        assert!(list.is_session_suspended("com.mitchellh.ghostty"));
        // The persisted ids still contain it — a restart re-excludes.
        assert!(list.ids().any(|id| id == "com.mitchellh.ghostty"));
        // Resuming re-excludes immediately.
        assert!(list.resume("com.mitchellh.ghostty"));
        assert!(list.is_excluded("com.mitchellh.ghostty"));
    }

    #[test]
    fn terminals_are_classified() {
        assert!(is_terminal("com.mitchellh.ghostty"));
        assert!(is_terminal("com.apple.Terminal"));
        assert!(!is_terminal("com.microsoft.VSCode")); // editor, not a terminal
        assert!(!is_terminal("com.google.Chrome"));
        // Every terminal is also a shipped default exclusion.
        for id in TERMINAL_EXCLUSIONS {
            assert!(
                DEFAULT_EXCLUSIONS.contains(id),
                "{id} missing from defaults"
            );
        }
    }
}
