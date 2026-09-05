//! GlowKey's preferences file — the single value the UI edits and the app saves.
//!
//! This is the product's model, not the engine's: it names the interface
//! language, whether the settings window opens at launch, whether the welcome
//! was shown, and the toggle hotkey, none of which a bare Vietnamese engine
//! has any use for. The engine gets a `Session` built from it
//! (`session_adapter`); `settings_store` owns where the file lives.
//! `settings.json` written by any earlier build loads unchanged: the field
//! names and defaults here are the file format.

use serde::{Deserialize, Serialize};

use glowkey_engine::{ExclusionList, InputMethod, Macro, PlacementStyle, WordOverride};
use glowkey_input::HotkeyPreset;

/// Everything the menu bar and preferences window control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Application bundle identifiers where Vietnamese input is off (deny list).
    #[serde(default = "default_exclusions")]
    pub exclusions: Vec<String>,
    /// Shipped default exclusions the user deliberately removed. At load, the
    /// effective list is `exclusions ∪ (DEFAULT_EXCLUSIONS − these)`, so a new
    /// release's defaults reach old settings files without resurrecting removals.
    #[serde(default)]
    pub removed_default_exclusions: Vec<String>,
    /// Whether to restore a word to its raw keys when the Telex result is not
    /// valid Vietnamese (`exit`, not `eĩt`).
    #[serde(default = "default_true")]
    pub auto_fix: bool,
    /// Tone-mark placement style.
    #[serde(default)]
    pub style: PlacementStyle,
    /// Keyboard input method (Telex or VNI).
    #[serde(default)]
    pub input_method: InputMethod,
    /// Capitalize the first letter of each sentence (Unikey's "Viết hoa chữ đầu
    /// câu"). Off by default.
    #[serde(default)]
    pub auto_capitalize: bool,
    /// The hotkey preset for the global Vietnamese/English toggle.
    #[serde(default)]
    pub toggle_hotkey: HotkeyPreset,
    /// Text-expansion macros (Unikey's "gõ tắt").
    #[serde(default)]
    pub macros: Vec<Macro>,
    /// Opt-in: restore a committed word to its raw keys when they form a common
    /// English word, even if the rendering is valid Vietnamese (`was`→`ứa`). Off
    /// by default — it inverts the ambiguity for Vietnamese words typed with a
    /// trailing tone key (`cats`→`cát`).
    #[serde(default)]
    pub restore_english_words: bool,
    /// Whether to open the Settings window when the app launches (like EVKey/Unikey
    /// showing their control panel on start). Default on, so a new user sees the
    /// controls; toggled off from the window itself once they know it.
    #[serde(default = "default_true")]
    pub open_settings_at_launch: bool,
    /// Language of the user interface (Unikey's "Vietnamese interface"). Defaults
    /// to following the system.
    #[serde(default)]
    pub language: Language,
    /// Opt-in "Quick Telex": a doubled consonant at the start of a syllable
    /// expands to its digraph (`cc`→`ch`, `nn`→`ng`). Off by default; it changes
    /// what plain consonant pairs mean.
    #[serde(default)]
    pub quick_telex: bool,
    /// Opt-in: UniKey's Telex bracket shortcuts — `[`→ơ, `]`→ư, `{`→Ơ, `}`→Ư.
    /// Off by default; turning it on stops `[` and `]` typing brackets.
    #[serde(default)]
    pub telex_brackets: bool,
    /// Opt-in: UniKey's `spellCheckEnabled` — refuse a diacritic that would make
    /// the word impossible in Vietnamese, at the keystroke rather than at the
    /// word boundary (which is what `auto_fix` does).
    #[serde(default)]
    pub strict_spell_check: bool,
    /// Opt-in: UniKey's `alwaysMacro` — expand macros even while Vietnamese is
    /// switched off. Never applies in an excluded application.
    #[serde(default)]
    pub always_macro: bool,
    /// Whether the one-time welcome has been shown. GlowKey is a background agent
    /// with no Dock icon: without this it grants itself a permission, puts a glyph
    /// in the menu bar and then says nothing, leaving the two hotkeys and the
    /// per-app ignore list — the whole point of the app — undiscoverable. Missing
    /// from an existing settings file means `false`, so an established user sees
    /// it once too.
    #[serde(default)]
    pub welcome_shown: bool,
    /// Per-word decisions about the English/Telex ambiguity — the one limitation
    /// no rule can resolve (`docs/handoff.md` §6.3). Empty by default, and an
    /// existing settings file gains an empty list rather than failing to load.
    #[serde(default)]
    pub word_overrides: Vec<WordOverride>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            exclusions: default_exclusions(),
            removed_default_exclusions: Vec::new(),
            auto_fix: true,
            style: PlacementStyle::default(),
            input_method: InputMethod::default(),
            auto_capitalize: false,
            toggle_hotkey: HotkeyPreset::default(),
            macros: Vec::new(),
            restore_english_words: false,
            open_settings_at_launch: true,
            language: Language::default(),
            quick_telex: false,
            telex_brackets: false,
            strict_spell_check: false,
            always_macro: false,
            welcome_shown: false,
            word_overrides: Vec::new(),
        }
    }
}

impl Settings {
    /// Parses settings from JSON, falling back to defaults on any error so a
    /// corrupt or partial file never stops the app.
    #[must_use]
    pub fn from_json(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }

    /// Serializes to pretty JSON for the user-inspectable settings file.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// The effective exclusion list: the saved ids merged with any shipped default
    /// the user has not deliberately removed (see `removed_default_exclusions`).
    #[must_use]
    pub fn exclusion_list(&self) -> ExclusionList {
        ExclusionList::from_saved(
            self.exclusions.iter().cloned(),
            self.removed_default_exclusions.iter().cloned(),
        )
    }
}

fn default_true() -> bool {
    true
}

fn default_exclusions() -> Vec<String> {
    glowkey_engine::exclusion::DEFAULT_EXCLUSIONS
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let settings = Settings {
            exclusions: vec!["com.apple.Terminal".into(), "com.example.app".into()],
            removed_default_exclusions: vec!["com.microsoft.VSCode".into()],
            auto_fix: false,
            style: PlacementStyle::Old,
            input_method: InputMethod::Vni,
            auto_capitalize: true,
            toggle_hotkey: HotkeyPreset::CtrlShiftZ,
            macros: vec![Macro {
                shortcut: "vn".into(),
                expansion: "Việt Nam".into(),
            }],
            restore_english_words: true,
            open_settings_at_launch: false,
            language: Language::Vietnamese,
            quick_telex: true,
            telex_brackets: true,
            strict_spell_check: true,
            always_macro: true,
            welcome_shown: true,
            word_overrides: vec![WordOverride {
                keys: "cats".into(),
                prefer: glowkey_engine::WordPreference::Vietnamese,
            }],
        };
        let restored = Settings::from_json(&settings.to_json());
        assert_eq!(settings, restored);
    }

    /// One mistyped verdict in a hand-edited list must not take the whole
    /// settings file with it.
    ///
    /// `from_json` falls back to defaults on any parse error, so before the
    /// field-level defaults a single bad entry silently discarded the user's
    /// exclusions, macros and every toggle — and the next UI change wrote the
    /// defaults back over them.
    #[test]
    fn a_malformed_word_override_does_not_discard_the_rest_of_the_file() {
        let json = r#"{
            "exclusions": ["com.mycompany.SecretApp"],
            "restore_english_words": true,
            "word_overrides": [{"keys": "was", "prefer": "nonsense"}]
        }"#;
        let settings = Settings::from_json(json);
        assert!(
            settings
                .exclusions
                .iter()
                .any(|e| e == "com.mycompany.SecretApp"),
            "a bad override entry must not cost the user their exclusions"
        );
        assert!(settings.restore_english_words);
    }

    /// The list is meant to be hand-edited, so the spellings a person actually
    /// writes have to parse.
    #[test]
    fn hand_written_verdicts_parse() {
        let json = r#"{"word_overrides":[
            {"keys":"was","prefer":"raw"},
            {"keys":"cats","prefer":"vietnamese"},
            {"keys":"exit","prefer":"Vietnamese"}
        ]}"#;
        let settings = Settings::from_json(json);
        assert_eq!(settings.word_overrides.len(), 3);
        assert_eq!(
            settings.word_overrides[1].prefer,
            glowkey_engine::WordPreference::Vietnamese
        );
    }

    /// An existing settings file predates `word_overrides` too, so it must load
    /// with an empty list rather than failing — the same tolerance every other
    /// added key relies on.
    #[test]
    fn a_settings_file_without_word_overrides_loads_with_none() {
        let old = r#"{"exclusions":["com.apple.Terminal"],"auto_fix":true}"#;
        assert!(Settings::from_json(old).word_overrides.is_empty());
    }

    /// An existing settings file predates `welcome_shown`, so the key is absent.
    /// It must read as `false` — otherwise an established user would be the one
    /// person who never sees the guide, which is backwards.
    #[test]
    fn a_settings_file_without_the_welcome_key_has_not_seen_it() {
        let old = r#"{"exclusions":["com.apple.Terminal"],"auto_fix":true}"#;
        let settings = Settings::from_json(old);
        assert!(!settings.welcome_shown);
    }

    #[test]
    fn custom_hotkey_round_trips() {
        let settings = Settings {
            toggle_hotkey: HotkeyPreset::Custom {
                control: true,
                shift: false,
                option: true,
                key_char: 'K',
                raw_code: Some(40),
            },
            ..Settings::default()
        };
        let restored = Settings::from_json(&settings.to_json());
        assert_eq!(settings, restored);
    }

    /// A settings file written by the build that shipped before the port, with a
    /// custom hotkey in it, captured verbatim and committed as a fixture.
    ///
    /// **If this fails, the schema change is wrong.** The fixture is evidence,
    /// not an expectation to be updated: regenerating it would turn a silent
    /// reinterpretation of somebody's hotkey into a green test, and a hotkey that
    /// quietly starts doing something else is the worst outcome this whole change
    /// can produce — worse than refusing to load, because nothing tells the user.
    #[test]
    fn a_pre_port_custom_hotkey_loads_as_the_key_it_was_recorded_on() {
        let settings = Settings::from_json(include_str!(
            "../tests/fixtures/settings-macos-custom-hotkey.json"
        ));
        assert_eq!(
            settings.toggle_hotkey,
            HotkeyPreset::Custom {
                control: true,
                shift: false,
                option: true,
                key_char: 'K',
                // The old file's `keycode: 40`, read through the alias. Not a
                // Windows key, and not a character match: the same physical key.
                raw_code: Some(40),
            }
        );
        assert_eq!(settings.toggle_hotkey.raw_code(), Some(40));

        // The file's exclusions are macOS bundle identifiers, and they must load
        // verbatim wherever the file is read. Naming them here is right — this is
        // a claim about the fixture's own content, not about what this platform
        // ships. The test used to compare the whole value against
        // `Settings::default()`, which conflated the two and failed on Windows
        // for the only correct reason: the shipped table there is executables.
        assert!(
            settings
                .exclusions
                .iter()
                .any(|id| id == "com.apple.Terminal"),
            "a macOS settings file must keep its own exclusions on any platform"
        );

        // Nothing else in the file may have shifted on the way through either.
        // Compared as a whole value so a field added later is covered without
        // anyone remembering to add it here; only the two fields this test has
        // already checked are excused.
        assert_eq!(
            Settings {
                toggle_hotkey: HotkeyPreset::default(),
                exclusions: default_exclusions(),
                ..settings.clone()
            },
            Settings::default()
        );
    }

    /// A real settings file lifted off a working installation. It has no custom
    /// hotkey, which is the common case, and it exists to catch a schema change
    /// that breaks every *ordinary* file while the interesting one still loads.
    #[test]
    fn a_real_settings_file_loads_field_for_field() {
        let settings =
            Settings::from_json(include_str!("../tests/fixtures/settings-real-macos.json"));
        assert_eq!(settings.toggle_hotkey, HotkeyPreset::CtrlShiftSpace);
        assert_eq!(settings.style, PlacementStyle::Old);
        assert_eq!(settings.input_method, InputMethod::Telex);
        assert!(settings.auto_fix);
        assert!(settings.auto_capitalize);
        assert!(settings.quick_telex);
        assert!(settings.strict_spell_check);
        assert!(settings.welcome_shown);
        assert!(settings.open_settings_at_launch);
        assert!(!settings.telex_brackets);
        assert!(!settings.always_macro);
        assert!(!settings.restore_english_words);
        assert!(settings.removed_default_exclusions.is_empty());
        // An entry the user added by hand, on top of the shipped defaults.
        assert!(settings
            .exclusion_list()
            .is_excluded("com.apple.ActivityMonitor"));
    }

    #[test]
    fn exclusion_list_merges_defaults_and_respects_tombstones() {
        // An old file: one default missing from `exclusions` (it wasn't a default
        // when the file was written), another deliberately removed. Loading must
        // add the first back and leave the second out — `saved ∪ (defaults −
        // tombstones)`.
        //
        // Drawn from the shipped table rather than named, because the rule under
        // test is the merge and the merge does not care what an application is
        // called. Spelling macOS identities here is what broke this on Windows.
        let defaults = glowkey_engine::exclusion::DEFAULT_EXCLUSIONS;
        let kept = defaults[0];
        let missing = defaults[1];
        let tombstoned = defaults[defaults.len() - 1];
        assert_ne!(
            missing, tombstoned,
            "the table is too small for this test to distinguish its two cases"
        );

        let s = Settings::from_json(&format!(
            r#"{{
                "exclusions": ["{kept}"],
                "removed_default_exclusions": ["{tombstoned}"]
            }}"#
        ));
        let list = s.exclusion_list();
        assert!(list.is_excluded(kept), "the user's own entry survives");
        assert!(
            list.is_excluded(missing),
            "a default absent from the file is merged in"
        );
        assert!(
            !list.is_excluded(tombstoned),
            "a deliberately removed default is not resurrected"
        );
    }

    #[test]
    fn corrupt_json_falls_back_to_default() {
        assert_eq!(Settings::from_json("not json at all"), Settings::default());
        assert_eq!(Settings::from_json(""), Settings::default());
    }

    #[test]
    fn partial_json_fills_missing_fields_with_defaults() {
        // Only auto_fix present: the rest must default.
        let s = Settings::from_json(r#"{"auto_fix": false}"#);
        assert!(!s.auto_fix);
        assert_eq!(s.style, PlacementStyle::default());
        assert_eq!(s.exclusions, default_exclusions());
    }

    #[test]
    fn defaults_match_shipped_behavior() {
        let s = Settings::default();
        assert!(s.auto_fix);
        // The shipped table, not a spelling of it: the identities are per-target
        // (bundle identifiers on macOS, executable names on Windows) and naming
        // one here is what made this a macOS-only test.
        //
        // What is worth defending is the property, and it is that a fresh install
        // protects terminals — the failure the ignore list exists to prevent is
        // synthesized backspaces mangling a shell's line editing.
        assert_eq!(s.exclusions, default_exclusions());
        assert!(
            !glowkey_engine::exclusion::TERMINAL_EXCLUSIONS.is_empty(),
            "this platform ships no terminals to protect"
        );
        for terminal in glowkey_engine::exclusion::TERMINAL_EXCLUSIONS {
            assert!(
                s.exclusions.iter().any(|id| id == terminal),
                "{terminal} is a known terminal but is not excluded on a fresh install"
            );
        }
    }

    #[test]
    fn legacy_default_mode_key_is_ignored() {
        // Old files persisted a `default_mode`; it is no longer a field. Loading
        // must still succeed (unknown key ignored) and keep the other settings.
        let s = Settings::from_json(r#"{"auto_fix": false, "default_mode": "English"}"#);
        assert!(!s.auto_fix);
        assert_eq!(s.style, PlacementStyle::default());
    }
}

#[cfg(test)]
mod fixtures {
    //! Files written by earlier builds must load and round-trip. These are the
    //! two shapes in the field: a Windows file with the shipped exclusions, and
    //! a macOS file carrying a recorded custom hotkey under its old field name.
    use super::*;

    #[test]
    fn a_windows_file_round_trips() {
        let json = include_str!("../tests/fixtures/settings-windows.json");
        let loaded = Settings::from_json(json);
        assert_eq!(loaded.exclusions.len(), 3);
        assert_eq!(loaded.style, PlacementStyle::Old);
        assert!(loaded.open_settings_at_launch);
        let again = Settings::from_json(&loaded.to_json());
        assert_eq!(again, loaded);
    }

    #[test]
    fn a_macos_file_with_a_recorded_hotkey_round_trips() {
        let json = include_str!("../tests/fixtures/settings-macos-custom-hotkey.json");
        let loaded = Settings::from_json(json);
        assert_eq!(
            loaded.toggle_hotkey.raw_code(),
            Some(40),
            "the old `keycode` field name is still read"
        );
        let again = Settings::from_json(&loaded.to_json());
        assert_eq!(again, loaded);
    }
}
