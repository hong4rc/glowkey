//! The per-application ignore list: GlowKey's primary feature.
//!
//! An excluded application never transforms a keystroke, and the exclusion always
//! wins over VN/EN mode and over any remembered per-app state.
//!
//! The rules here are pure and know nothing about what an application identity
//! *is*: macOS supplies a bundle identifier, Windows an executable file name.
//! Which applications ship excluded, and which of those are terminals, is data
//! the shell hands in as [`ExclusionDefaults`]; the list only applies the rules.

use std::collections::BTreeSet;

/// The application tables a product ships: what is excluded on first run, and
/// which of those are terminals.
///
/// Supplied by the shell, because an application's identity is whatever its
/// platform calls it and a table can only be right for one platform. An empty
/// `ExclusionDefaults` is a valid one: nothing ships excluded, and the terminal
/// rule never applies.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExclusionDefaults {
    /// Excluded on first run, and merged into settings files written before an
    /// entry existed (unless the user removed that entry on purpose).
    excluded: BTreeSet<String>,
    /// The subset of `excluded` that are terminals. A hotkey only *suspends* a
    /// terminal's exclusion: a PTY ignores synthetic backspaces, so an accidental
    /// press must not permanently disarm the protection.
    terminals: BTreeSet<String>,
}

impl ExclusionDefaults {
    /// Builds the tables. Every terminal is also a default: a terminal that did
    /// not ship excluded would be one nobody is protected from, so it is added
    /// to `excluded` here rather than trusted to be there.
    pub fn new<I, S, J, T>(excluded: I, terminals: J) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
        J: IntoIterator<Item = T>,
        T: Into<String>,
    {
        let terminals: BTreeSet<String> = terminals.into_iter().map(Into::into).collect();
        let mut excluded: BTreeSet<String> = excluded.into_iter().map(Into::into).collect();
        excluded.extend(terminals.iter().cloned());
        Self {
            excluded,
            terminals,
        }
    }

    /// Whether this identity ships excluded.
    #[must_use]
    pub fn is_default(&self, app_id: &str) -> bool {
        self.excluded.contains(app_id)
    }

    /// Whether this identity is a known terminal.
    #[must_use]
    pub fn is_terminal(&self, app_id: &str) -> bool {
        self.terminals.contains(app_id)
    }

    /// The shipped exclusions, sorted.
    pub fn excluded(&self) -> impl Iterator<Item = &str> {
        self.excluded.iter().map(String::as_str)
    }
}

/// The set of application identities where Vietnamese input is suppressed.
///
/// Ordered for stable, deterministic serialization when the shell persists it.
///
/// Two auxiliary sets refine the persisted `app_ids`:
/// - `removed_defaults` (persisted): defaults the user deliberately removed. At
///   load, the effective list is `saved ∪ (defaults − removed_defaults)`, so a
///   default added in a new release reaches existing settings files without
///   resurrecting ones the user removed on purpose.
/// - `session_removed` (never persisted): exclusions suspended until restart,
///   used when a known terminal is un-excluded by hotkey, so an accidental ⌃⇧E in
///   a terminal cannot permanently disable its protection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExclusionList {
    app_ids: BTreeSet<String>,
    /// Default exclusions the user deliberately removed (tombstones, persisted).
    removed_defaults: BTreeSet<String>,
    /// Exclusions suspended for this session only (not persisted).
    session_removed: BTreeSet<String>,
    /// The shipped tables this list was built against. What `remove` tombstones
    /// and what the terminal rule applies to.
    defaults: ExclusionDefaults,
}

impl ExclusionList {
    /// An empty list with no shipped defaults behind it.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The list as it stands on first run: every shipped default excluded.
    #[must_use]
    pub fn with_defaults(defaults: ExclusionDefaults) -> Self {
        Self {
            app_ids: defaults.excluded.clone(),
            defaults,
            ..Self::default()
        }
    }

    /// Builds a list from stored identities, with no shipped defaults behind it.
    pub fn from_ids<I, S>(ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            app_ids: ids.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    /// Builds the effective list from a settings file: the saved ids, plus every
    /// shipped default the user has not deliberately removed. This is how a
    /// default added in a new release (e.g. a new terminal) reaches settings
    /// files written before it existed.
    pub fn from_saved<I, S, J, T>(ids: I, removed_defaults: J, defaults: ExclusionDefaults) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
        J: IntoIterator<Item = T>,
        T: Into<String>,
    {
        let removed_defaults: BTreeSet<String> =
            removed_defaults.into_iter().map(Into::into).collect();
        let mut app_ids: BTreeSet<String> = ids.into_iter().map(Into::into).collect();
        for id in &defaults.excluded {
            if !removed_defaults.contains(id) {
                app_ids.insert(id.clone());
            }
        }
        Self {
            app_ids,
            removed_defaults,
            session_removed: BTreeSet::new(),
            defaults,
        }
    }

    /// The shipped tables this list was built against.
    #[must_use]
    pub fn defaults(&self) -> &ExclusionDefaults {
        &self.defaults
    }

    /// Whether this identity is a known terminal, per the shipped tables.
    #[must_use]
    pub fn is_terminal(&self, app_id: &str) -> bool {
        self.defaults.is_terminal(app_id)
    }

    /// Whether the application with this identity is excluded.
    ///
    /// This is the single check the keystroke path makes before touching the
    /// engine. It must be the first decision, ahead of mode and of memory.
    #[must_use]
    pub fn is_excluded(&self, app_id: &str) -> bool {
        self.app_ids.contains(app_id) && !self.session_removed.contains(app_id)
    }

    /// Adds an identity. Returns true if it was newly added. Clears any session
    /// suspension. A tombstone is deliberately KEPT: it only suppresses the
    /// defaults merge at load, and an explicit entry wins over it anyway.
    /// Clearing it would let an accidental toggle pair (remove + re-add) silently
    /// destroy the user's recorded removal of a default.
    pub fn add(&mut self, app_id: impl Into<String>) -> bool {
        let app_id = app_id.into();
        self.session_removed.remove(&app_id);
        self.app_ids.insert(app_id)
    }

    /// Removes an identity permanently. Returns true if it was present.
    /// A removed shipped default is tombstoned so a later load does not re-add it.
    pub fn remove(&mut self, app_id: &str) -> bool {
        self.session_removed.remove(app_id);
        if self.defaults.is_default(app_id) {
            self.removed_defaults.insert(app_id.to_string());
        }
        self.app_ids.remove(app_id)
    }

    /// Suspends an exclusion until restart: the app stops being excluded now, but
    /// the persisted list still contains it, so the next launch re-excludes it.
    /// Used for known terminals un-excluded by hotkey. No-op if not excluded.
    pub fn suspend_for_session(&mut self, app_id: &str) {
        if self.app_ids.contains(app_id) {
            self.session_removed.insert(app_id.to_string());
        }
    }

    /// Lifts a session suspension (the app is excluded again). Returns whether a
    /// suspension existed.
    pub fn resume(&mut self, app_id: &str) -> bool {
        self.session_removed.remove(app_id)
    }

    /// Whether this app's exclusion is currently suspended for the session.
    #[must_use]
    pub fn is_session_suspended(&self, app_id: &str) -> bool {
        self.session_removed.contains(app_id)
    }

    /// The tombstoned defaults (deliberately removed by the user), for persistence.
    pub fn removed_default_ids(&self) -> impl Iterator<Item = &str> {
        self.removed_defaults.iter().map(String::as_str)
    }

    /// The excluded identities, sorted, for persistence and for the editor.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.app_ids.iter().map(String::as_str)
    }

    /// How many applications are excluded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.app_ids.len()
    }

    /// Whether the list is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.app_ids.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shipped terminal and a shipped editor. Invented names: the rules under
    /// test do not care what a platform calls an application, and spelling a
    /// real identity here would make this a test of one platform's table.
    const TERMINAL: &str = "example.terminal";
    const EDITOR: &str = "example.editor";
    const ANOTHER_EDITOR: &str = "example.other-editor";

    fn shipped() -> ExclusionDefaults {
        ExclusionDefaults::new([TERMINAL, EDITOR, ANOTHER_EDITOR], [TERMINAL])
    }

    #[test]
    fn excluded_apps_are_recognized() {
        let list = ExclusionList::with_defaults(shipped());
        assert!(list.is_excluded(TERMINAL));
        assert!(list.is_excluded(EDITOR));
        assert!(!list.is_excluded("example.definitely-not-shipped"));
    }

    #[test]
    fn add_and_remove() {
        let mut list = ExclusionList::new();
        assert!(list.add("example.app"));
        assert!(!list.add("example.app")); // already present
        assert!(list.is_excluded("example.app"));
        assert!(list.remove("example.app"));
        assert!(!list.is_excluded("example.app"));
    }

    #[test]
    fn tombstone_survives_a_remove_add_pair() {
        // A deliberate removal must not be silently destroyed by a later
        // add/remove pair (e.g. two accidental ⌃⇧E presses).
        let mut list = ExclusionList::with_defaults(shipped());
        list.remove(EDITOR); // deliberate, tombstoned
        list.add(EDITOR); // toggled back on
        list.remove(EDITOR); // and off again
        assert!(list.removed_default_ids().any(|id| id == EDITOR));
        // A fresh load must not resurrect it via the defaults merge.
        let reloaded = ExclusionList::from_saved(
            list.ids().map(String::from).collect::<Vec<_>>(),
            list.removed_default_ids()
                .map(String::from)
                .collect::<Vec<_>>(),
            shipped(),
        );
        assert!(!reloaded.is_excluded(EDITOR));
    }

    #[test]
    fn roundtrips_through_ids() {
        let list = ExclusionList::with_defaults(shipped());
        let ids: Vec<String> = list.ids().map(String::from).collect();
        let restored = ExclusionList::from_ids(ids);
        assert!(list.ids().eq(restored.ids()));
    }

    #[test]
    fn from_saved_merges_new_defaults_into_old_files() {
        // A settings file written before ANOTHER_EDITOR shipped: loading must
        // add it, and must keep the user's own entry.
        let list = ExclusionList::from_saved(
            [TERMINAL, "example.custom"],
            Vec::<String>::new(),
            shipped(),
        );
        assert!(list.is_excluded(ANOTHER_EDITOR));
        assert!(list.is_excluded("example.custom"));
    }

    #[test]
    fn from_saved_respects_tombstones() {
        // The user deliberately removed one; the merge must not resurrect it.
        let list = ExclusionList::from_saved([TERMINAL], [EDITOR], shipped());
        assert!(!list.is_excluded(EDITOR));
        assert!(list.is_excluded(TERMINAL));
    }

    #[test]
    fn removing_a_default_tombstones_it() {
        let mut list = ExclusionList::with_defaults(shipped());
        assert!(list.remove(EDITOR));
        let tombstones: Vec<&str> = list.removed_default_ids().collect();
        assert_eq!(tombstones, vec![EDITOR]);
        // Re-adding keeps the tombstone (the explicit entry wins over it anyway,
        // and it must survive an accidental remove/add pair).
        list.add(EDITOR);
        assert!(list.is_excluded(EDITOR));
        assert_eq!(list.removed_default_ids().count(), 1);
        // A non-default removal never tombstones.
        list.add("example.app");
        list.remove("example.app");
        assert_eq!(list.removed_default_ids().count(), 1);
    }

    #[test]
    fn a_list_without_defaults_never_tombstones() {
        let mut list = ExclusionList::from_ids([TERMINAL]);
        assert!(list.remove(TERMINAL));
        assert_eq!(list.removed_default_ids().count(), 0);
        assert!(!list.is_terminal(TERMINAL));
    }

    #[test]
    fn session_suspension_is_not_a_removal() {
        let mut list = ExclusionList::with_defaults(shipped());
        list.suspend_for_session(TERMINAL);
        assert!(!list.is_excluded(TERMINAL));
        assert!(list.is_session_suspended(TERMINAL));
        // The persisted ids still contain it: a restart re-excludes.
        assert!(list.ids().any(|id| id == TERMINAL));
        // Resuming re-excludes immediately.
        assert!(list.resume(TERMINAL));
        assert!(list.is_excluded(TERMINAL));
    }

    /// The invariant that makes the session-only terminal toggle safe: a hotkey
    /// can only *suspend* a terminal's exclusion, so a terminal missing from the
    /// defaults would be permanently un-excludable by accident. The constructor
    /// closes that hole rather than trusting the caller's tables to agree.
    #[test]
    fn every_terminal_is_also_a_default() {
        let defaults = ExclusionDefaults::new([EDITOR], [TERMINAL]);
        assert!(defaults.is_default(TERMINAL));
        assert!(defaults.is_terminal(TERMINAL));
        assert!(!defaults.is_terminal(EDITOR));
        assert!(ExclusionList::with_defaults(defaults).is_excluded(TERMINAL));
    }
}
