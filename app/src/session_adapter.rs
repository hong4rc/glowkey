//! Between the preferences file and the engine's session.
//!
//! The engine does not know the file exists; the file does not know how a
//! session is built. This is the one place that knows both: `session_from` puts
//! a session together from saved preferences, and `settings_from` writes a
//! session's state back over the product-only fields the session never held.

use glowkey_session::Session;

use crate::prefs_model::Settings;

/// A session configured as `settings` says.
#[must_use]
pub fn session_from(settings: &Settings) -> Session {
    Session::builder()
        .style(settings.style)
        .exclusions(settings.exclusion_list())
        .input_method(settings.input_method)
        .auto_fix(settings.auto_fix)
        .auto_capitalize(settings.auto_capitalize)
        .restore_english_words(settings.restore_english_words)
        .always_macro(settings.always_macro)
        .quick_telex(settings.quick_telex)
        .telex_brackets(settings.telex_brackets)
        .strict_spell_check(settings.strict_spell_check)
        .macros(settings.macros.clone())
        .word_overrides(&settings.word_overrides)
        .build()
}

/// `prefs` with every field the session owns replaced by the session's current
/// value. The product-only fields — language, launch flags, the hotkey preset —
/// come from `prefs` untouched.
#[must_use]
pub fn settings_from(session: &Session, prefs: &Settings) -> Settings {
    Settings {
        exclusions: session.exclusions().ids().map(String::from).collect(),
        removed_default_exclusions: session
            .exclusions()
            .removed_default_ids()
            .map(String::from)
            .collect(),
        auto_fix: session.auto_fix(),
        style: session.style(),
        input_method: session.input_method(),
        auto_capitalize: session.auto_capitalize(),
        macros: session.macros().to_vec(),
        restore_english_words: session.restore_english_words(),
        always_macro: session.always_macro(),
        word_overrides: session.word_override_list(),
        quick_telex: session.quick_telex(),
        telex_brackets: session.telex_brackets(),
        strict_spell_check: session.strict_spell_check(),
        toggle_hotkey: prefs.toggle_hotkey,
        open_settings_at_launch: prefs.open_settings_at_launch,
        language: prefs.language,
        welcome_shown: prefs.welcome_shown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glowkey_input::HotkeyPreset;
    use glowkey_session::{InputMethod, PlacementStyle};

    /// What goes in comes back out, and the product-only fields survive the
    /// trip through a session that never held them.
    #[test]
    fn settings_survive_a_round_trip_through_a_session() {
        let prefs = Settings {
            style: PlacementStyle::Old,
            input_method: InputMethod::SimpleTelex,
            auto_fix: false,
            auto_capitalize: true,
            restore_english_words: true,
            always_macro: true,
            quick_telex: true,
            telex_brackets: true,
            strict_spell_check: true,
            toggle_hotkey: HotkeyPreset::CtrlShiftZ,
            open_settings_at_launch: false,
            language: crate::prefs_model::Language::Vietnamese,
            welcome_shown: true,
            macros: vec![glowkey_session::Macro {
                shortcut: "vn".into(),
                expansion: "Việt Nam".into(),
            }],
            word_overrides: vec![glowkey_session::WordOverride {
                keys: "cats".into(),
                prefer: glowkey_session::WordPreference::Vietnamese,
            }],
            ..Settings::default()
        };
        let session = session_from(&prefs);
        let back = settings_from(&session, &prefs);
        // The session reports exclusions sorted; the shipped table is not.
        let mut expected = prefs.clone();
        expected.exclusions.sort();
        assert_eq!(back, expected);
    }
}
