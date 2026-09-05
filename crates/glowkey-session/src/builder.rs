//! Building a session in one expression.

use super::*;

/// Assembles a [`Session`] from its policy knobs.
///
/// Every setter mirrors one on [`Session`]; the builder exists so a consumer
/// reading saved preferences can write the whole configuration as one
/// expression instead of a `let mut` followed by a dozen calls. Defaults are the
/// session's own: Telex, new-style placement, no exclusions, auto-fix on,
/// everything else off.
///
/// "No exclusions" means an [`ExclusionList::new()`] with no shipped defaults
/// behind it: nothing tombstones and nothing counts as a terminal. A product
/// passes [`ExclusionList::with_defaults`] (or `from_saved`) through
/// [`exclusions`](Self::exclusions); that is where the terminal rule comes from.
#[must_use = "a builder does nothing until `build` is called"]
pub struct SessionBuilder {
    session: Session,
}

impl Default for SessionBuilder {
    fn default() -> Self {
        Self {
            session: Session::new(PlacementStyle::default(), ExclusionList::new()),
        }
    }
}

impl SessionBuilder {
    /// A builder with the session's defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Where tone marks go (`hoà` or `hòa`).
    pub fn style(mut self, style: PlacementStyle) -> Self {
        self.session.set_style(style);
        self
    }

    /// Telex or VNI.
    pub fn input_method(mut self, method: InputMethod) -> Self {
        self.session.set_input_method(method);
        self
    }

    /// The per-application ignore list, with the shipped defaults already in it.
    pub fn exclusions(mut self, exclusions: ExclusionList) -> Self {
        *self.session.exclusions_mut() = exclusions;
        self
    }

    /// Restore invalid Vietnamese to its raw keys at a word boundary.
    pub fn auto_fix(mut self, on: bool) -> Self {
        self.session.set_auto_fix(on);
        self
    }

    /// Capitalise the first letter of each sentence.
    pub fn auto_capitalize(mut self, on: bool) -> Self {
        self.session.set_auto_capitalize(on);
        self
    }

    /// Restore common English words even when their rendering is valid Vietnamese.
    pub fn restore_english_words(mut self, on: bool) -> Self {
        self.session.set_restore_english_words(on);
        self
    }

    /// Expand macros while Vietnamese is off.
    pub fn always_macro(mut self, on: bool) -> Self {
        self.session.set_always_macro(on);
        self
    }

    /// Telex's `w` shorthand for a lone `ư`.
    pub fn quick_telex(mut self, on: bool) -> Self {
        self.session.set_quick_telex(on);
        self
    }

    /// Telex's `[` and `]` for `ơ` and `ư`.
    pub fn telex_brackets(mut self, on: bool) -> Self {
        self.session.set_telex_brackets(on);
        self
    }

    /// Stop transforming a word the moment it can no longer be Vietnamese.
    pub fn strict_spell_check(mut self, on: bool) -> Self {
        self.session.set_strict_spell_check(on);
        self
    }

    /// The text-expansion table.
    pub fn macros(mut self, macros: Vec<Macro>) -> Self {
        self.session.set_macros(macros);
        self
    }

    /// The per-word decisions.
    pub fn word_overrides(mut self, overrides: &[WordOverride]) -> Self {
        self.session.set_word_overrides(overrides);
        self
    }

    /// The configured session.
    #[must_use]
    pub fn build(self) -> Session {
        self.session
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_builder_sets_what_it_was_told_and_nothing_else() {
        let session = Session::builder()
            .input_method(InputMethod::Vni)
            .auto_fix(false)
            .always_macro(true)
            .macros(vec![Macro {
                shortcut: "vn".into(),
                expansion: "Việt Nam".into(),
            }])
            .build();
        assert_eq!(session.input_method(), InputMethod::Vni);
        assert!(!session.auto_fix());
        assert!(session.always_macro());
        assert_eq!(session.macros().len(), 1);
        // Untouched knobs keep the session's own defaults.
        assert_eq!(session.style(), PlacementStyle::default());
        assert!(!session.auto_capitalize());
        assert!(session.exclusions().is_empty());
    }

    #[test]
    fn the_default_builder_equals_a_plain_new() {
        let built = SessionBuilder::new().build();
        let plain = Session::new(PlacementStyle::default(), ExclusionList::new());
        assert_eq!(built.input_method(), plain.input_method());
        assert_eq!(built.auto_fix(), plain.auto_fix());
        assert_eq!(built.style(), plain.style());
        assert_eq!(built.exclusions(), plain.exclusions());
    }
}
