//! Persisted settings — the single value the UI edits and the app saves.
//!
//! Platform-free: this holds the data and its JSON (de)serialization. The app
//! crate owns *where* the file lives and the file I/O, so the engine stays
//! testable on any OS.

use serde::{Deserialize, Serialize};

use crate::{ExclusionList, HotkeyPreset, InputMethod, PlacementStyle};

/// Everything the menu bar and preferences window control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Application bundle identifiers where Vietnamese input is off (deny list).
    #[serde(default = "default_exclusions")]
    pub exclusions: Vec<String>,
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
    /// Whether to open the Settings window when the app launches (like EVKey/Unikey
    /// showing their control panel on start). Default on, so a new user sees the
    /// controls; toggled off from the window itself once they know it.
    #[serde(default = "default_true")]
    pub open_settings_at_launch: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            exclusions: default_exclusions(),
            auto_fix: true,
            style: PlacementStyle::default(),
            input_method: InputMethod::default(),
            auto_capitalize: false,
            toggle_hotkey: HotkeyPreset::default(),
            open_settings_at_launch: true,
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

    /// The exclusion list as an [`ExclusionList`].
    #[must_use]
    pub fn exclusion_list(&self) -> ExclusionList {
        ExclusionList::from_ids(self.exclusions.iter().cloned())
    }
}

fn default_true() -> bool {
    true
}

fn default_exclusions() -> Vec<String> {
    crate::exclusion::DEFAULT_EXCLUSIONS
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let settings = Settings {
            exclusions: vec!["com.apple.Terminal".into(), "com.example.app".into()],
            auto_fix: false,
            style: PlacementStyle::Old,
            input_method: InputMethod::Vni,
            auto_capitalize: true,
            toggle_hotkey: HotkeyPreset::CtrlShiftZ,
            open_settings_at_launch: false,
        };
        let restored = Settings::from_json(&settings.to_json());
        assert_eq!(settings, restored);
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
        assert!(s.exclusions.iter().any(|id| id == "com.apple.Terminal"));
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
