//! `TapState`'s settings surface: the accessors the menu bar and the Settings
//! window call, each one saving as it changes.
//!
//! Separated from the tap itself because it is a different kind of code. Every
//! function here is the same four lines — borrow the session, change one field,
//! write the file — and there are forty of them, which is a wall the eye slides
//! off when it sits between the event-tap logic and the decision function. None
//! of it runs on the keystroke path.
//!
//! An inherent `impl` block may live in any module of the defining crate, and a
//! private field is visible to descendant modules, so this reaches `TapState`'s
//! internals without widening anything.

use std::sync::atomic::Ordering;

use glowkey_input::HotkeyPreset;

use crate::prefs_model::Language;

use super::{TapState, DISABLED};

impl TapState {
    /// Toggles VN/EN mode and saves. Used by the menu bar.
    pub fn toggle_mode_and_save(&self) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.toggle_mode();
        }
        self.save_settings();
    }

    /// Toggles auto-fix and saves. Used by the menu bar.
    pub fn toggle_auto_fix_and_save(&self) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            let on = session.auto_fix();
            session.set_auto_fix(!on);
        }
        self.save_settings();
    }

    /// Toggles a specific app in the ignore list and saves. Used by the menu bar's
    /// "Enable/Disable for <App>" action. Per-app and independent.
    pub fn toggle_app_exclusion_and_save(&self, bundle_id: &str) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.toggle_app_exclusion(bundle_id);
        }
        self.save_settings();
    }

    /// Current state for menu labels: (mode, auto-fix on, is `bundle_id` excluded).
    pub fn menu_state(&self, bundle_id: &str) -> (glowkey_session::InputMode, bool, bool) {
        match self.session.try_borrow() {
            Ok(s) => (
                s.mode(),
                s.auto_fix(),
                s.exclusions().is_excluded(bundle_id),
            ),
            Err(_) => (glowkey_session::InputMode::Vietnamese, true, false),
        }
    }

    /// The bundle identifiers currently excluded (Vietnamese off), sorted. Drives
    /// the Settings window's "Excluded apps" list.
    pub fn exclusion_ids(&self) -> Vec<String> {
        match self.session.try_borrow() {
            Ok(s) => s.exclusions().ids().map(|s| s.to_string()).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Adds an app to the ignore list (disables Vietnamese there) and saves. Used by
    /// the Settings window's "Add App…" picker. Idempotent if already excluded.
    pub fn add_exclusion_and_save(&self, bundle_id: &str) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.exclusions_mut().add(bundle_id.to_string());
        }
        self.save_settings();
    }

    /// Removes an app from the ignore list (re-enables Vietnamese there) and saves.
    /// Used by the Settings window's per-row "Remove" button.
    pub fn remove_exclusion_and_save(&self, bundle_id: &str) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.exclusions_mut().remove(bundle_id);
        }
        self.save_settings();
    }

    /// Whether auto-fix (restore invalid Vietnamese to the raw keys) is on.
    pub fn auto_fix(&self) -> bool {
        self.session
            .try_borrow()
            .map(|s| s.auto_fix())
            .unwrap_or(true)
    }

    /// Whether auto-capitalize (first letter of each sentence) is on.
    pub fn auto_capitalize(&self) -> bool {
        self.session
            .try_borrow()
            .map(|s| s.auto_capitalize())
            .unwrap_or(false)
    }

    /// Sets auto-capitalize and saves.
    pub fn set_auto_capitalize_and_save(&self, on: bool) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.set_auto_capitalize(on);
        }
        self.save_settings();
    }

    /// Sets auto-fix on/off explicitly and saves. Used by the Settings checkbox.
    pub fn set_auto_fix_and_save(&self, on: bool) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.set_auto_fix(on);
        }
        self.save_settings();
    }

    /// Whether the Settings window should open on launch.
    pub fn open_settings_at_launch(&self) -> bool {
        self.prefs
            .try_borrow()
            .map(|p| p.open_settings_at_launch)
            .unwrap_or(true)
    }

    /// Sets the "open Settings on launch" preference and saves.
    pub fn set_open_settings_at_launch_and_save(&self, on: bool) {
        if let Ok(mut prefs) = self.prefs.try_borrow_mut() {
            prefs.open_settings_at_launch = on;
        }
        self.save_settings();
    }

    /// The current input method (Telex/VNI). Drives the Settings control.
    pub fn input_method(&self) -> glowkey_session::InputMethod {
        self.session
            .try_borrow()
            .map(|s| s.input_method())
            .unwrap_or(glowkey_session::InputMethod::Telex)
    }

    /// Sets the input method (Telex/VNI) and saves.
    pub fn set_input_method_and_save(&self, method: glowkey_session::InputMethod) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.set_input_method(method);
        }
        self.save_settings();
    }

    /// The current toggle-hotkey preset. Drives the Settings control.
    pub fn toggle_hotkey(&self) -> HotkeyPreset {
        self.prefs
            .try_borrow()
            .map(|p| p.toggle_hotkey)
            .unwrap_or(HotkeyPreset::CtrlShiftSpace)
    }

    /// Sets the toggle-hotkey preset and saves.
    pub fn set_toggle_hotkey_and_save(&self, preset: HotkeyPreset) {
        if let Ok(mut prefs) = self.prefs.try_borrow_mut() {
            prefs.toggle_hotkey = preset;
        }
        self.save_settings();
    }

    /// Starts recording a custom toggle hotkey: the next key-down with ⌃ or ⌥
    /// becomes the hotkey (Escape cancels). Driven by the Settings window.
    pub fn begin_hotkey_recording(&self) {
        *self.recording_hotkey.borrow_mut() = true;
    }

    /// Whether a hotkey recording is in progress.
    pub fn is_recording_hotkey(&self) -> bool {
        *self.recording_hotkey.borrow()
    }

    /// Whether the opt-in English word restore is on.
    pub fn restore_english_words(&self) -> bool {
        self.session
            .try_borrow()
            .map(|s| s.restore_english_words())
            .unwrap_or(false)
    }

    /// Sets the English word restore and saves.
    pub fn set_restore_english_words_and_save(&self, on: bool) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.set_restore_english_words(on);
        }
        self.save_settings();
    }

    /// The user-interface language preference, for the Settings picker.
    pub fn language(&self) -> Language {
        self.prefs
            .try_borrow()
            .map(|p| p.language)
            .unwrap_or_default()
    }

    /// Sets the interface language, applies it to the live string table, and saves.
    pub fn set_language_and_save(&self, language: Language) {
        if let Ok(mut prefs) = self.prefs.try_borrow_mut() {
            prefs.language = language;
        }
        crate::strings::set_language(language);
        self.save_settings();
    }

    /// Whether macros expand while Vietnamese is off, for the Settings checkbox.
    pub fn always_macro(&self) -> bool {
        self.session
            .try_borrow()
            .map(|s| s.always_macro())
            .unwrap_or(false)
    }

    /// Sets whether macros expand while Vietnamese is off, and saves.
    pub fn set_always_macro_and_save(&self, on: bool) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.set_always_macro(on);
        }
        self.save_settings();
    }

    /// Whether the mid-word spell check is on, for the Settings checkbox.
    pub fn strict_spell_check(&self) -> bool {
        self.session
            .try_borrow()
            .map(|s| s.strict_spell_check())
            .unwrap_or(false)
    }

    /// Sets the mid-word spell check and saves.
    pub fn set_strict_spell_check_and_save(&self, on: bool) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.set_strict_spell_check(on);
        }
        self.save_settings();
    }

    /// Whether the Telex bracket shortcuts are on, for the Settings checkbox.
    pub fn telex_brackets(&self) -> bool {
        self.session
            .try_borrow()
            .map(|s| s.telex_brackets())
            .unwrap_or(false)
    }

    /// Sets the Telex bracket shortcuts and saves.
    pub fn set_telex_brackets_and_save(&self, on: bool) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.set_telex_brackets(on);
        }
        self.save_settings();
    }

    /// Whether Quick Telex is on, for the Settings checkbox.
    pub fn quick_telex(&self) -> bool {
        self.session
            .try_borrow()
            .map(|s| s.quick_telex())
            .unwrap_or(false)
    }

    /// Sets Quick Telex and saves.
    pub fn set_quick_telex_and_save(&self, on: bool) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.set_quick_telex(on);
        }
        self.save_settings();
    }

    /// Merges an imported macro table and saves, returning `(added, skipped)`, or
    /// `None` if the session was busy and nothing was applied — the caller reports
    /// a count, so "did not run" must not read as "imported 0". The merge rule
    /// itself lives in [`Session::import_macros`].
    pub fn import_macros_and_save(
        &self,
        imported: &[glowkey_session::Macro],
        on_conflict: glowkey_session::MacroConflict,
    ) -> Option<(usize, usize)> {
        let counts = self
            .session
            .try_borrow_mut()
            .ok()?
            .import_macros(imported, on_conflict);
        self.save_settings();
        Some(counts)
    }

    /// Whether a macro shortcut is already taken, so the window can ask before
    /// overwriting it.
    pub fn has_macro(&self, shortcut: &str) -> bool {
        self.session
            .try_borrow()
            .map(|s| s.has_macro(shortcut))
            .unwrap_or(false)
    }

    /// How many rows of an import would overwrite an existing shortcut.
    pub fn macro_conflicts(&self, imported: &[glowkey_session::Macro]) -> usize {
        self.session
            .try_borrow()
            .map(|s| s.macro_conflicts(imported))
            .unwrap_or(0)
    }

    /// The text-expansion macros, cloned for the Settings list.
    pub fn macros(&self) -> Vec<glowkey_session::Macro> {
        self.session
            .try_borrow()
            .map(|s| s.macros().to_vec())
            .unwrap_or_default()
    }

    /// Adds (or replaces) a macro and saves. Returns whether it was accepted.
    pub fn add_macro_and_save(&self, shortcut: &str, expansion: &str) -> bool {
        let ok = self
            .session
            .try_borrow_mut()
            .map(|mut s| s.add_macro(shortcut, expansion))
            .unwrap_or(false);
        if ok {
            self.save_settings();
        }
        ok
    }

    /// Removes the macro at `index` and saves.
    pub fn remove_macro_and_save(&self, index: usize) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.remove_macro(index);
        }
        self.save_settings();
    }

    /// The current tone-placement style. Drives the Settings segmented control.
    pub fn style(&self) -> glowkey_session::PlacementStyle {
        self.session
            .try_borrow()
            .map(|s| s.style())
            .unwrap_or(glowkey_session::PlacementStyle::New)
    }

    /// Sets the tone-placement style and saves. Used by the Settings segmented control.
    pub fn set_style_and_save(&self, style: glowkey_session::PlacementStyle) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.set_style(style);
        }
        self.save_settings();
    }

    /// Clears the runaway circuit breaker and any half-typed word, recovering input
    /// if the breaker ever latched (the "Reset input" menu item). Human typing never
    /// trips it, so this is only a safety valve.
    pub fn reset(&self) {
        DISABLED.store(false, Ordering::Relaxed);
        if let Ok(mut emits) = self.recent_emits.try_borrow_mut() {
            emits.clear();
        }
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.flush();
        }
    }
    /// Every recorded word decision, for the Personal Words window.
    pub fn word_overrides(&self) -> Vec<glowkey_session::WordOverride> {
        self.session
            .try_borrow()
            .map(|s| s.word_override_list())
            .unwrap_or_default()
    }

    /// The decision recorded for `keys`, if any.
    pub fn word_override(&self, keys: &str) -> Option<glowkey_session::WordPreference> {
        self.session
            .try_borrow()
            .ok()
            .and_then(|s| s.word_override(keys))
    }

    /// Records a word decision and saves.
    pub fn set_word_override_and_save(&self, keys: &str, prefer: glowkey_session::WordPreference) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.set_word_override(keys, prefer);
        }
        self.save_settings();
    }

    /// Forgets a word decision and saves.
    pub fn remove_word_override_and_save(&self, keys: &str) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.remove_word_override(keys);
        }
        self.save_settings();
    }
}
