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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExclusionList {
    bundle_ids: BTreeSet<String>,
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
        }
    }

    /// Whether the application with this bundle identifier is excluded.
    ///
    /// This is the single check the keystroke path makes before touching the
    /// engine. It must be the first decision, ahead of mode and of memory.
    #[must_use]
    pub fn is_excluded(&self, bundle_id: &str) -> bool {
        self.bundle_ids.contains(bundle_id)
    }

    /// Adds a bundle identifier. Returns true if it was newly added.
    pub fn add(&mut self, bundle_id: impl Into<String>) -> bool {
        self.bundle_ids.insert(bundle_id.into())
    }

    /// Removes a bundle identifier. Returns true if it was present.
    pub fn remove(&mut self, bundle_id: &str) -> bool {
        self.bundle_ids.remove(bundle_id)
    }

    /// Toggles a bundle identifier, returning its new excluded state. Backs the
    /// menu bar's "Exclude current application" / its inverse.
    pub fn toggle(&mut self, bundle_id: &str) -> bool {
        if self.remove(bundle_id) {
            false
        } else {
            self.add(bundle_id.to_string());
            true
        }
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
    "net.kovidgoyal.kitty",
    "com.github.wez.wezterm",
    "com.mitchellh.ghostty",
    "org.alacritty",
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
    fn toggle_flips_state() {
        let mut list = ExclusionList::new();
        assert!(list.toggle("com.example.app")); // now excluded
        assert!(list.is_excluded("com.example.app"));
        assert!(!list.toggle("com.example.app")); // now included
        assert!(!list.is_excluded("com.example.app"));
    }

    #[test]
    fn roundtrips_through_ids() {
        let list = ExclusionList::with_defaults();
        let ids: Vec<String> = list.ids().map(String::from).collect();
        let restored = ExclusionList::from_ids(ids);
        assert_eq!(list, restored);
    }
}
