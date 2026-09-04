//! The Windows settings window (`egui`/`eframe`), built on demand and fully
//! torn down on close.
//!
//! GlowKey at rest is a keyboard hook, a message loop and a tray icon — nothing
//! that renders. This module exists so the renderer only exists while the
//! window is open: [`show`] creates the `eframe` app, blocks on its event loop,
//! and returns the edited [`Settings`] once the loop (and every resource it
//! held) is gone. Nothing here is reachable from the hook's callback: there is
//! no global state, only values passed in and a value passed back.
//!
//! Mirrors the panes of the macOS settings window
//! (`app/src/platform/macos/prefs/tabs.rs`): General/Typing, Excluded apps,
//! Macros, Personal words, About.
//!
//! **The labels here are English only.** `crate::strings::t` is now portable and
//! the tray already uses it, so this is a mechanical pass over roughly forty
//! labels rather than anything blocked — but it is not done, and a Vietnamese
//! interface that stops at the tray is half an interface. Tracked as remaining
//! Phase 5 work.

use std::cell::RefCell;
use std::rc::Rc;

use eframe::egui;

use glowkey_engine::{
    ExclusionList, InputMethod, Language, Macro, PlacementStyle, Settings, WordOverride,
    WordPreference,
};

/// Opens the settings window and blocks until the user closes it.
///
/// `initial` is the settings to edit. Returns the edited settings if anything
/// changed, or `None` if the user closed the window without changing
/// anything (including if the window failed to open at all — there is nothing
/// to persist either way). The caller is responsible for writing the result to
/// disk; this function only ever returns a value.
#[must_use]
pub fn show(initial: Settings) -> Option<Settings> {
    // `Some(None)` once the app has decided "closing, nothing to save";
    // `Some(Some(settings))` once it has decided "closing, save this".
    // `None` means the loop never got that far (e.g. `run_native` itself
    // failed before a frame was drawn), which is also "nothing to persist".
    let result_slot: Rc<RefCell<Option<Option<Settings>>>> = Rc::new(RefCell::new(None));
    let slot_for_app = Rc::clone(&result_slot);

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("GlowKey Settings")
            .with_inner_size([560.0, 640.0])
            .with_resizable(true),
        ..Default::default()
    };

    let run_result = eframe::run_native(
        "GlowKey Settings",
        native_options,
        Box::new(move |_cc| Ok(Box::new(SettingsApp::new(initial, slot_for_app)))),
    );

    if let Err(err) = run_result {
        // To the log, not to `eprintln!`. GlowKey builds with
        // `windows_subsystem = "windows"` and has no console, so a message
        // printed here goes nowhere at all — and a menu item that does nothing
        // and says nothing is the defect `docs/decisions/0007` is about.
        //
        // The expected error is `RecreationAttempt`. **winit permits exactly one
        // event loop per process**, and there is no reset outside its web
        // backend — so the second time a user picks Settings in a process that
        // has been up for days, this is where they land. That is a real
        // limitation of running the window in-process and it is named here
        // rather than left as a mystery; the fix is a design decision (a
        // separate process, or a dedicated long-lived UI thread), not a patch.
        crate::log::log(&format!(
            "SETTINGS window could not run: {err}. If this says RecreationAttempt, \
             the window has already been opened once this run — restart GlowKey to \
             open it again."
        ));
    }

    let mut slot = result_slot.borrow_mut();
    slot.take().flatten()
}

/// Which pane is currently shown. Mirrors the macOS tab titles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    General,
    Apps,
    Macros,
    Words,
    About,
}

struct SettingsApp {
    /// The value passed in, kept verbatim so the final draft can be compared
    /// against it — the only way to know whether to return `None`.
    initial: Settings,
    /// The value being edited. Every control writes here directly.
    draft: Settings,
    tab: Tab,
    result_slot: Rc<RefCell<Option<Option<Settings>>>>,

    // ----- Excluded apps pane -----
    /// The effective exclusion set (saved ids merged with un-tombstoned
    /// shipped defaults) plus its tombstones. Edited in place with
    /// [`ExclusionList::add`]/[`ExclusionList::remove`] so the tombstoning
    /// rule for a removed shipped default is applied the same way the engine
    /// applies it, not reimplemented here.
    exclusion_list: ExclusionList,
    new_exclusion: String,

    // ----- Macros pane -----
    macro_shortcut: String,
    macro_expansion: String,
    /// `Some(i)` while editing `draft.macros[i]`; the Add/Save button and the
    /// row buttons that set this stay in sync.
    macro_edit_index: Option<usize>,
    /// Scratch buffer for the import/export table (UniKey/EVKey `gõ tắt`
    /// format).
    macro_table_text: String,

    // ----- Personal words pane -----
    word_keys: String,
    word_prefer: WordPreference,
    word_edit_index: Option<usize>,
}

impl SettingsApp {
    fn new(initial: Settings, result_slot: Rc<RefCell<Option<Option<Settings>>>>) -> Self {
        let exclusion_list = initial.exclusion_list();
        Self {
            draft: initial.clone(),
            initial,
            tab: Tab::General,
            result_slot,
            exclusion_list,
            new_exclusion: String::new(),
            macro_shortcut: String::new(),
            macro_expansion: String::new(),
            macro_edit_index: None,
            macro_table_text: String::new(),
            word_keys: String::new(),
            word_prefer: WordPreference::default(),
            word_edit_index: None,
        }
    }

    /// Decides what to hand back to [`show`] and records it. Idempotent, so
    /// it is safe to call from both the window-close event and the explicit
    /// Close button without double-deciding.
    fn finalize(&mut self) {
        if self.result_slot.borrow().is_some() {
            return;
        }

        let mut draft = self.draft.clone();
        let mut ids: Vec<String> = self.exclusion_list.ids().map(str::to_string).collect();
        ids.sort();
        draft.exclusions = ids;
        let mut removed: Vec<String> = self
            .exclusion_list
            .removed_default_ids()
            .map(str::to_string)
            .collect();
        removed.sort();
        draft.removed_default_exclusions = removed;

        // `self.initial`'s exclusion fields are whatever order the settings
        // file (or the unsorted shipped-defaults array, for a fresh install)
        // happened to have them in; `draft`'s are freshly sorted from
        // `exclusion_list`. Comparing the two raw would report "changed" on
        // every single open even with no edits, so both sides go through the
        // same normalization before the comparison that decides what to
        // return.
        let outcome = if draft == normalize_exclusions(&self.initial) {
            None
        } else {
            Some(draft)
        };
        *self.result_slot.borrow_mut() = Some(outcome);
    }

    fn show_general_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Typing");

        ui.label("Input method:");
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.draft.input_method, InputMethod::Telex, "Telex");
            ui.radio_value(&mut self.draft.input_method, InputMethod::Vni, "VNI");
            ui.radio_value(
                &mut self.draft.input_method,
                InputMethod::SimpleTelex,
                "Simple Telex",
            );
        });

        ui.label("Tone marks:");
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.draft.style, PlacementStyle::New, "Modern (hoà)");
            ui.radio_value(&mut self.draft.style, PlacementStyle::Old, "Classic (hòa)");
        });

        ui.separator();

        ui.checkbox(
            &mut self.draft.quick_telex,
            "Quick Telex (doubled consonant shortcuts)",
        );
        ui.label(
            "A doubled consonant at the start of a syllable types its digraph: \
             cc→ch, gg→gi, kk→kh, nn→ng, pp→ph, qq→qu, tt→th, uu→ư.",
        );

        ui.checkbox(
            &mut self.draft.telex_brackets,
            "Telex bracket shortcuts ([ → ơ, ] → ư, { → Ơ, } → Ư)",
        );
        ui.label(
            "Turning this on stops [ and ] (and { and }) reaching the app at \
             all while typing Telex — including where they are shortcuts.",
        );

        ui.separator();

        ui.checkbox(
            &mut self.draft.auto_fix,
            "Auto-fix non-Vietnamese words at the space",
        );
        ui.label(
            "Restores the raw keys at the space when the result isn't valid \
             Vietnamese — types \"exit\", not \"eĩt\".",
        );

        ui.checkbox(
            &mut self.draft.strict_spell_check,
            "Fix as I type, not at the space",
        );
        ui.label(
            "Restores the raw keys the moment a word stops being possible \
             Vietnamese — \"exit\" repairs at the x, not at the space.",
        );

        ui.checkbox(
            &mut self.draft.auto_capitalize,
            "Auto-capitalize first letter of each sentence",
        );

        ui.checkbox(
            &mut self.draft.restore_english_words,
            "Restore common English words",
        );
        ui.label(
            "Off by default: it inverts the ambiguity for Vietnamese words \
             typed with a trailing tone key (cats→cát). Personal Words below \
             decides one word at a time instead, and wins over this.",
        );

        ui.separator();

        ui.checkbox(
            &mut self.draft.always_macro,
            "Expand macros even when Vietnamese is off",
        );
        ui.label("Never applies in an excluded app.");

        ui.separator();
        ui.heading("General");

        ui.checkbox(
            &mut self.draft.open_settings_at_launch,
            "Open this window at launch",
        );

        ui.label("Interface language:");
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.draft.language, Language::System, "System");
            ui.radio_value(&mut self.draft.language, Language::Vietnamese, "Tiếng Việt");
            ui.radio_value(&mut self.draft.language, Language::English, "English");
        });
    }

    fn show_apps_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(
            "Apps where GlowKey stays off, identified by executable name \
             (e.g. code.exe, cmd.exe) rather than a bundle id.",
        );

        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.new_exclusion);
            if ui.button("Add").clicked() {
                if let Some(name) = normalize_exe_name(&self.new_exclusion) {
                    self.exclusion_list.add(name);
                    self.new_exclusion.clear();
                }
            }
        });

        ui.separator();

        let mut ids: Vec<String> = self.exclusion_list.ids().map(str::to_string).collect();
        ids.sort();
        let mut to_remove: Option<String> = None;
        egui::ScrollArea::vertical()
            .id_salt("exclusion_list")
            .max_height(200.0)
            .show(ui, |ui| {
                for id in &ids {
                    ui.horizontal(|ui| {
                        ui.label(id);
                        if ui.small_button("Remove").clicked() {
                            to_remove = Some(id.clone());
                        }
                    });
                }
            });
        if let Some(id) = to_remove {
            // Removing a shipped default is recorded as a tombstone by
            // `ExclusionList::remove` itself, so a later release does not
            // silently resurrect a removal the user made on purpose.
            self.exclusion_list.remove(&id);
        }

        let mut tombstones: Vec<String> = self
            .exclusion_list
            .removed_default_ids()
            .map(str::to_string)
            .collect();
        if !tombstones.is_empty() {
            tombstones.sort();
            ui.separator();
            ui.label(
                "Shipped defaults you deliberately removed. They will not come \
                 back on their own, even in a future release that adds them:",
            );
            let mut to_restore: Option<String> = None;
            for id in &tombstones {
                ui.horizontal(|ui| {
                    ui.label(format!("{id}  (removed default)"));
                    if ui.small_button("Restore default").clicked() {
                        to_restore = Some(id.clone());
                    }
                });
            }
            if let Some(id) = to_restore {
                // Re-adding makes the id explicitly excluded again; the
                // tombstone record itself is harmless once the id is present,
                // since presence always wins over it.
                self.exclusion_list.add(id);
            }
        }
    }

    fn show_macros_tab(&mut self, ui: &mut egui::Ui) {
        ui.label("Text expansion (gõ tắt): type a shortcut then a space to expand it.");

        ui.horizontal(|ui| {
            ui.label("Shortcut:");
            ui.text_edit_singleline(&mut self.macro_shortcut);
            ui.label("Expansion:");
            ui.text_edit_singleline(&mut self.macro_expansion);
            let label = if self.macro_edit_index.is_some() {
                "Save"
            } else {
                "Add"
            };
            if ui.button(label).clicked() {
                if let Some((shortcut, expansion)) =
                    validate_macro(&self.macro_shortcut, &self.macro_expansion)
                {
                    upsert_macro(
                        &mut self.draft.macros,
                        shortcut,
                        expansion,
                        self.macro_edit_index.take(),
                    );
                    self.macro_shortcut.clear();
                    self.macro_expansion.clear();
                }
            }
            if self.macro_edit_index.is_some() && ui.button("Cancel edit").clicked() {
                self.macro_edit_index = None;
                self.macro_shortcut.clear();
                self.macro_expansion.clear();
            }
        });

        ui.separator();

        let mut action: Option<(usize, bool)> = None;
        egui::ScrollArea::vertical()
            .id_salt("macro_list")
            .max_height(160.0)
            .show(ui, |ui| {
                for (i, m) in self.draft.macros.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{} → {}", m.shortcut, m.expansion));
                        if ui.small_button("Edit").clicked() {
                            action = Some((i, true));
                        }
                        if ui.small_button("Remove").clicked() {
                            action = Some((i, false));
                        }
                    });
                }
            });
        if let Some((i, edit)) = action {
            if edit {
                self.macro_shortcut = self.draft.macros[i].shortcut.clone();
                self.macro_expansion = self.draft.macros[i].expansion.clone();
                self.macro_edit_index = Some(i);
            } else {
                self.draft.macros.remove(i);
                if self.macro_edit_index == Some(i) {
                    self.macro_edit_index = None;
                }
            }
        }

        ui.separator();
        ui.label(
            "Import or export a table in the UniKey/EVKey format \
             (shortcut:expansion per line) — the main way a curated list \
             arrives.",
        );
        ui.add(
            egui::TextEdit::multiline(&mut self.macro_table_text)
                .desired_rows(6)
                .font(egui::TextStyle::Monospace),
        );
        ui.horizontal(|ui| {
            if ui.button("Import (merge into list above)").clicked() {
                for m in Macro::parse_table(&self.macro_table_text) {
                    upsert_macro(&mut self.draft.macros, m.shortcut, m.expansion, None);
                }
            }
            if ui.button("Export list above into this box").clicked() {
                self.macro_table_text = Macro::format_table(&self.draft.macros);
            }
        });
    }

    fn show_words_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(
            "Per-word decisions about the English/Telex ambiguity. A word \
             decided here stays decided, and wins over \"Restore common \
             English words\" on the General tab.",
        );

        ui.horizontal(|ui| {
            ui.label("Keys:");
            ui.text_edit_singleline(&mut self.word_keys);
            egui::ComboBox::from_id_salt("word_prefer")
                .selected_text(preference_label(self.word_prefer))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.word_prefer,
                        WordPreference::Raw,
                        preference_label(WordPreference::Raw),
                    );
                    ui.selectable_value(
                        &mut self.word_prefer,
                        WordPreference::Vietnamese,
                        preference_label(WordPreference::Vietnamese),
                    );
                });
            let label = if self.word_edit_index.is_some() {
                "Save"
            } else {
                "Add"
            };
            if ui.button(label).clicked() {
                if let Some(keys) = normalize_word_keys(&self.word_keys) {
                    upsert_word_override(
                        &mut self.draft.word_overrides,
                        keys,
                        self.word_prefer,
                        self.word_edit_index.take(),
                    );
                    self.word_keys.clear();
                }
            }
            if self.word_edit_index.is_some() && ui.button("Cancel edit").clicked() {
                self.word_edit_index = None;
                self.word_keys.clear();
            }
        });

        ui.separator();

        let mut action: Option<(usize, bool)> = None;
        egui::ScrollArea::vertical()
            .id_salt("word_override_list")
            .max_height(200.0)
            .show(ui, |ui| {
                for (i, w) in self.draft.word_overrides.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}  →  {}", w.keys, preference_label(w.prefer)));
                        if ui.small_button("Edit").clicked() {
                            action = Some((i, true));
                        }
                        if ui.small_button("Remove").clicked() {
                            action = Some((i, false));
                        }
                    });
                }
            });
        if let Some((i, edit)) = action {
            if edit {
                self.word_keys = self.draft.word_overrides[i].keys.clone();
                self.word_prefer = self.draft.word_overrides[i].prefer;
                self.word_edit_index = Some(i);
            } else {
                self.draft.word_overrides.remove(i);
                if self.word_edit_index == Some(i) {
                    self.word_edit_index = None;
                }
            }
        }
    }

    fn show_about_tab(&self, ui: &mut egui::Ui) {
        ui.heading("GlowKey");
        ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
        // Set by the build; may be empty (a source build outside CI, or a
        // build where `git` isn't available) rather than absent, so this
        // never fails to compile — it just has nothing to show.
        let commit = option_env!("GLOWKEY_COMMIT").unwrap_or("");
        if !commit.is_empty() {
            ui.label(format!("Commit {commit}"));
        }

        ui.separator();
        ui.label(
            "Windows limitation: Windows blocks synthetic input across \
             integrity levels. If this settings window is running elevated \
             (\"Run as administrator\") while GlowKey's keyboard hook is not, \
             or the other way around, the elevated process cannot receive \
             keystrokes the other one injects. Run GlowKey and this window at \
             the same elevation.",
        );
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // The OS/window-manager close (title-bar X, Alt+F4, …) also has to
        // decide what to hand back, exactly like the explicit Close button.
        if ctx.input(|i| i.viewport().close_requested()) {
            self.finalize();
        }

        egui::TopBottomPanel::top("glowkey_settings_tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (tab, title) in [
                    (Tab::General, "General"),
                    (Tab::Apps, "Excluded apps"),
                    (Tab::Macros, "Macros"),
                    (Tab::Words, "Personal words"),
                    (Tab::About, "About"),
                ] {
                    ui.selectable_value(&mut self.tab, tab, title);
                }
            });
        });

        egui::TopBottomPanel::bottom("glowkey_settings_bottom").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Close").clicked() {
                    self.finalize();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("glowkey_settings_body")
                .show(ui, |ui| match self.tab {
                    Tab::General => self.show_general_tab(ui),
                    Tab::Apps => self.show_apps_tab(ui),
                    Tab::Macros => self.show_macros_tab(ui),
                    Tab::Words => self.show_words_tab(ui),
                    Tab::About => self.show_about_tab(ui),
                });
        });
    }
}

/// Rewrites `exclusions`/`removed_default_exclusions` to the sorted, effective
/// form [`ExclusionList`] computes, so two [`Settings`] values that mean the
/// same thing compare equal regardless of how their raw fields happen to be
/// ordered — notably, the shipped-defaults array itself is not sorted.
fn normalize_exclusions(settings: &Settings) -> Settings {
    let mut normalized = settings.clone();
    let list = settings.exclusion_list();
    let mut ids: Vec<String> = list.ids().map(str::to_string).collect();
    ids.sort();
    normalized.exclusions = ids;
    let mut removed: Vec<String> = list.removed_default_ids().map(str::to_string).collect();
    removed.sort();
    normalized.removed_default_exclusions = removed;
    normalized
}

/// Human label for a [`WordPreference`], matching how the personal-words list
/// is explained to the user.
fn preference_label(pref: WordPreference) -> &'static str {
    match pref {
        WordPreference::Raw => "Keep as typed",
        WordPreference::Vietnamese => "Keep Vietnamese",
    }
}

/// Normalizes a user-entered executable name: trimmed and lowercased (Windows
/// app identities are lowercased executable names, matched case-insensitively
/// at the hook), and rejected if nothing is left.
fn normalize_exe_name(input: &str) -> Option<String> {
    let normalized = input.trim().to_lowercase();
    (!normalized.is_empty()).then_some(normalized)
}

/// Normalizes a personal-word key: trimmed and lowercased, matching
/// [`WordOverride::keys`]'s documented invariant, and rejected if empty.
fn normalize_word_keys(input: &str) -> Option<String> {
    let normalized = input.trim().to_lowercase();
    (!normalized.is_empty()).then_some(normalized)
}

/// Validates a macro's shortcut and expansion for the Add/Save button: both
/// trimmed, both required non-empty (an empty expansion is a no-op the
/// engine's own table reader already discards, so it is refused here too
/// rather than silently accepted and then never round-tripping).
fn validate_macro(shortcut: &str, expansion: &str) -> Option<(String, String)> {
    let shortcut = shortcut.trim().to_string();
    let expansion = expansion.trim().to_string();
    (!shortcut.is_empty() && !expansion.is_empty()).then_some((shortcut, expansion))
}

/// Inserts or updates a macro. `edit_index`, when present and in range,
/// replaces that entry outright (an explicit edit). Otherwise an existing
/// entry with the same shortcut (case-insensitively, matching how the engine
/// matches a typed shortcut) is replaced rather than duplicated; a genuinely
/// new shortcut is appended.
fn upsert_macro(
    macros: &mut Vec<Macro>,
    shortcut: String,
    expansion: String,
    edit_index: Option<usize>,
) {
    if let Some(idx) = edit_index {
        if let Some(slot) = macros.get_mut(idx) {
            *slot = Macro {
                shortcut,
                expansion,
            };
            return;
        }
    }
    if let Some(existing) = macros
        .iter_mut()
        .find(|m| m.shortcut.eq_ignore_ascii_case(&shortcut))
    {
        *existing = Macro {
            shortcut,
            expansion,
        };
    } else {
        macros.push(Macro {
            shortcut,
            expansion,
        });
    }
}

/// Inserts or updates a personal-word decision. Same edit-index-first,
/// then-match-by-key rule as [`upsert_macro`]; `keys` is expected to already
/// be normalized (see [`normalize_word_keys`]).
fn upsert_word_override(
    overrides: &mut Vec<WordOverride>,
    keys: String,
    prefer: WordPreference,
    edit_index: Option<usize>,
) {
    if let Some(idx) = edit_index {
        if let Some(slot) = overrides.get_mut(idx) {
            *slot = WordOverride { keys, prefer };
            return;
        }
    }
    if let Some(existing) = overrides.iter_mut().find(|w| w.keys == keys) {
        existing.prefer = prefer;
    } else {
        overrides.push(WordOverride { keys, prefer });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_exe_name_trims_and_lowercases() {
        assert_eq!(normalize_exe_name("  Code.EXE  "), Some("code.exe".into()));
        assert_eq!(normalize_exe_name("   "), None);
        assert_eq!(normalize_exe_name(""), None);
    }

    #[test]
    fn normalize_word_keys_trims_and_lowercases() {
        assert_eq!(normalize_word_keys("  CATS "), Some("cats".into()));
        assert_eq!(normalize_word_keys(""), None);
    }

    #[test]
    fn validate_macro_rejects_blank_fields() {
        assert_eq!(validate_macro("  ", "x"), None);
        assert_eq!(validate_macro("vn", "  "), None);
        assert_eq!(
            validate_macro(" vn ", " Việt Nam "),
            Some(("vn".into(), "Việt Nam".into()))
        );
    }

    #[test]
    fn upsert_macro_appends_new_shortcut() {
        let mut macros = Vec::new();
        upsert_macro(&mut macros, "vn".into(), "Việt Nam".into(), None);
        assert_eq!(macros.len(), 1);
        assert_eq!(macros[0].shortcut, "vn");
    }

    #[test]
    fn upsert_macro_replaces_case_insensitive_duplicate_instead_of_adding() {
        let mut macros = vec![Macro {
            shortcut: "VN".into(),
            expansion: "old".into(),
        }];
        upsert_macro(&mut macros, "vn".into(), "Việt Nam".into(), None);
        assert_eq!(macros.len(), 1);
        assert_eq!(macros[0].expansion, "Việt Nam");
    }

    #[test]
    fn upsert_macro_with_edit_index_replaces_that_entry_even_if_shortcut_changed() {
        let mut macros = vec![
            Macro {
                shortcut: "vn".into(),
                expansion: "Việt Nam".into(),
            },
            Macro {
                shortcut: "us".into(),
                expansion: "United States".into(),
            },
        ];
        upsert_macro(&mut macros, "vnm".into(), "Việt Nam".into(), Some(0));
        assert_eq!(macros.len(), 2);
        assert_eq!(macros[0].shortcut, "vnm");
        assert_eq!(macros[1].shortcut, "us");
    }

    #[test]
    fn upsert_word_override_replaces_existing_key_instead_of_duplicating() {
        let mut overrides = vec![WordOverride {
            keys: "cats".into(),
            prefer: WordPreference::Raw,
        }];
        upsert_word_override(
            &mut overrides,
            "cats".into(),
            WordPreference::Vietnamese,
            None,
        );
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].prefer, WordPreference::Vietnamese);
    }

    #[test]
    fn upsert_word_override_edit_index_updates_in_place() {
        let mut overrides = vec![WordOverride {
            keys: "cats".into(),
            prefer: WordPreference::Raw,
        }];
        upsert_word_override(
            &mut overrides,
            "cats2".into(),
            WordPreference::Vietnamese,
            Some(0),
        );
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].keys, "cats2");
        assert_eq!(overrides[0].prefer, WordPreference::Vietnamese);
    }

    /// Exercises the exact merge [`SettingsApp::finalize`] performs, without
    /// needing to construct the `egui` app: editing through `ExclusionList`
    /// and writing its `ids()`/`removed_default_ids()` back must round-trip
    /// through `Settings::exclusion_list()` and report a change only when the
    /// effective set actually changed.
    #[test]
    fn exclusion_list_edits_round_trip_into_settings_fields() {
        let initial = Settings::default();
        let mut list = initial.exclusion_list();

        // Not one of the shipped defaults (unlike e.g. `code.exe`), so adding
        // it is a genuine change to the effective set.
        list.add("mycustomapp.exe");
        let mut ids: Vec<String> = list.ids().map(str::to_string).collect();
        ids.sort();
        let mut removed: Vec<String> = list.removed_default_ids().map(str::to_string).collect();
        removed.sort();

        let mut draft = initial.clone();
        draft.exclusions = ids;
        draft.removed_default_exclusions = removed;

        assert_ne!(draft, normalize_exclusions(&initial));
        assert!(draft.exclusion_list().is_excluded("mycustomapp.exe"));
    }

    /// The shipped-defaults array is not sorted, so a settings value that was
    /// never edited must still normalize to itself — otherwise `finalize`
    /// would report a change on every single open of a fresh install.
    #[test]
    fn no_edits_means_no_change_to_report() {
        let initial = Settings::default();
        let list = initial.exclusion_list();
        let mut ids: Vec<String> = list.ids().map(str::to_string).collect();
        ids.sort();
        let mut removed: Vec<String> = list.removed_default_ids().map(str::to_string).collect();
        removed.sort();

        let mut draft = initial.clone();
        draft.exclusions = ids;
        draft.removed_default_exclusions = removed;

        assert_eq!(draft, normalize_exclusions(&initial));
    }
}
