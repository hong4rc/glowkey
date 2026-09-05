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
//! **Shaped after the macOS window, deliberately.** GlowKey is one product, and
//! the macOS settings window (`app/src/prefs/`) is where its shape was decided:
//! four tabs of 460×540 points — General, Typing, Corrections, Apps & macros —
//! with the three list editors (Excluded apps, Macros, Personal words) as their
//! own small windows opened from inside those tabs. This file reproduces that,
//! tab for tab and window for window, rather than inventing a second layout; the
//! `t(english, vietnamese)` pairs are copied verbatim from the macOS source so
//! the two interfaces cannot drift into naming the same setting two different
//! things. The list editors are `egui::Window` overlays — the nearest thing this
//! toolkit has to an auxiliary window.
//!
//! About is **not** here. It belongs next to Settings in the tray menu, where
//! macOS keeps it (`menu_bar.rs`), and it is a native message box
//! (`shell::show_about`) because winit permits one event loop per process: once
//! this window has opened, a second toolkit window cannot.
//!
//! Two things here are not decoration. The interface font is taken from the
//! system (see [`install_system_font`]), because egui's bundled font cannot draw
//! Vietnamese at all; and the light/dark choice is read from the registry (see
//! [`apply_theme`]), because the toolkit's own detection failed to resolve and
//! its fallback is dark — which is how a light-themed machine got a black
//! window.

use std::cell::RefCell;
use std::rc::Rc;

use eframe::egui;

use glowkey_engine::{ExclusionList, HotkeyPreset, Macro, Settings, WordOverride, WordPreference};

use crate::settings_spec::{
    expand_shortcuts, hotkey_display, shortcut_display, Control, ListId, Row, TabSpec, Toggle,
    HOTKEY_PRESETS, MANAGE, TABS, WINDOW_TITLE,
};
use crate::strings::t;

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
            .with_title(WINDOW_TITLE.get())
            // The macOS window's content size, to the point
            // (`app/src/prefs/tabs.rs`): a settings window for a background
            // utility is a small window, and this one had grown to nearly twice
            // that. Points, not pixels — winit reports the monitor's scale
            // factor and egui multiplies by it, so this is the same apparent
            // size at 100% and at 150%.
            .with_inner_size([460.0, 540.0])
            // Resizable, but not down to where the four tab titles stop fitting
            // on one row.
            .with_min_inner_size([420.0, 420.0])
            .with_resizable(true),
        ..Default::default()
    };

    let run_result = eframe::run_native(
        "GlowKey Settings",
        native_options,
        Box::new(move |cc| {
            install_system_font(&cc.egui_ctx);
            apply_style(&cc.egui_ctx);
            Ok(Box::new(SettingsApp::new(initial, slot_for_app)))
        }),
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

// ---------------------------------------------------------------------------
// Look and feel
// ---------------------------------------------------------------------------

/// Height a settings row occupies, so checkboxes, pickers and list rows sit on
/// one rhythm instead of each taking its own content's height.
const ROW_HEIGHT: f32 = 24.0;
/// Width of the label column in a label + control row — the macOS window aligns
/// its controls on one edge (`prefs/widgets.rs`'s `LABEL_COLUMN_WIDTH`) and so
/// does this one.
const LABEL_COLUMN: f32 = 110.0;
/// How far a caption is inset under the control it explains — roughly a checkbox
/// plus its gap, so the text starts under the label, not under the box.
const INDENT: f32 = 22.0;
/// Gap between one group of settings and the next.
const GROUP_GAP: f32 = 14.0;

/// The size of each list-editor window.
///
/// The macOS ones are 420×380 and 460×400 (`prefs/excluded.rs`,
/// `macros_window.rs`, `personal_words.rs`), and those are *separate* windows
/// there. Here they are overlays inside a 460×540 frame, so at their macOS
/// widths they covered it edge to edge with no frame left showing — which reads
/// as the window having been replaced rather than something having opened on top
/// of it. Inset to leave a margin of the parent visible on every side; they are
/// resizable, so the macOS size is still one drag away.
const EXCLUDED_SIZE: [f32; 2] = [372.0, 320.0];
const MACROS_SIZE: [f32; 2] = [396.0, 340.0];
const WORDS_SIZE: [f32; 2] = [396.0, 340.0];

/// The surface the tab strip and the button bar sit on.
///
/// Painted rather than left transparent. An unfilled panel shows whatever the
/// clear colour is, which is how the tab strip and the button bar stayed black
/// while the content between them was themed — the two halves of the same window
/// disagreeing about what colour it was.
///
/// A shade off the content fill, with a hairline against it, so the chrome reads
/// as chrome. Both values come from the active visuals, so this follows the
/// light/dark switch instead of pinning a colour that is right in one theme only
/// — the token-driven rule, applied to the one place that was breaking it.
fn chrome_frame(ctx: &egui::Context) -> egui::Frame {
    let visuals = &ctx.style().visuals;
    egui::Frame::none()
        .fill(visuals.panel_fill)
        .stroke(egui::Stroke::new(
            1.0_f32,
            visuals.widgets.noninteractive.bg_stroke.color,
        ))
}

/// Loads the system UI font into egui.
///
/// Not cosmetic: **egui's bundled proportional font has no Vietnamese glyphs.**
/// `Ubuntu-Light` covers Latin-1 and Latin Extended-A, and Vietnamese needs
/// Latin Extended Additional (`ế ộ ữ ậ …`), so a Vietnamese interface drawn in
/// the default font is a wall of missing-glyph boxes — the one failure mode this
/// window cannot ship with. Segoe UI (the Windows 11 UI font) covers it, and
/// makes the window look like the system's own besides.
///
/// Best effort by design: the egui defaults stay in the family as fallbacks, so
/// a machine where the file cannot be read gets the old rendering rather than no
/// window. Returns whether the proportional font was installed — which is
/// exactly what the window's ability to draw Vietnamese depends on.
fn install_system_font(ctx: &egui::Context) -> bool {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    let mut fonts = egui::FontDefinitions::default();

    let mut load = |name: &str, file: &str, family: egui::FontFamily| {
        let Ok(bytes) = std::fs::read(format!("{root}\\Fonts\\{file}")) else {
            return false;
        };
        fonts
            .font_data
            .insert(name.to_owned(), egui::FontData::from_owned(bytes));
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, name.to_owned());
        true
    };

    let proportional = load("Segoe UI", "segoeui.ttf", egui::FontFamily::Proportional);
    // Consolas covers Vietnamese too, which matters here: the macro import box
    // is monospaced and the expansions people paste into it are Vietnamese.
    load("Consolas", "consola.ttf", egui::FontFamily::Monospace);

    ctx.set_fonts(fonts);
    proportional
}

/// Type scale, spacing and control metrics.
///
/// The scale is Windows' own — Segoe UI at roughly 9pt for body text, the size
/// every other settings dialog on the machine uses — rather than egui's
/// defaults, which are a third larger and made this window feel oversized in a
/// 460-point frame.
///
/// Applied through [`egui::Context::all_styles_mut`] so it survives a light/dark
/// switch: egui keeps one `Style` per theme and swaps them when the system
/// preference changes, so anything written to only one of them is lost the
/// moment the user turns on dark mode.
fn apply_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        use egui::FontFamily::{Monospace, Proportional};
        use egui::{FontId, TextStyle};

        style.text_styles = [
            (TextStyle::Heading, FontId::new(15.0, Proportional)),
            (TextStyle::Body, FontId::new(13.0, Proportional)),
            (TextStyle::Button, FontId::new(13.0, Proportional)),
            (TextStyle::Monospace, FontId::new(12.0, Monospace)),
            (TextStyle::Small, FontId::new(11.5, Proportional)),
        ]
        .into();

        style.spacing.item_spacing = egui::vec2(6.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.interact_size.y = 20.0;
        style.spacing.menu_margin = egui::Margin::same(6.0);
        for widget in [
            &mut style.visuals.widgets.noninteractive,
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
        ] {
            widget.rounding = egui::Rounding::same(4.0);
        }
    });
}

/// Light or dark, asked of Windows rather than of the toolkit.
///
/// `ThemePreference::System` was wrong here: winit's theme detection does not
/// resolve on every machine, and egui's fallback when it cannot tell is **dark**
/// — so the window came up black, Done button and all, on a system whose apps
/// are set to light. `theme::apps_are_light` reads the value Windows actually
/// keeps (`AppsUseLightTheme`, the one about application windows, not the
/// taskbar's `SystemUsesLightTheme` — users mix the two).
///
/// Called every frame, not once: the user can switch theme while the window is
/// open, and a registry read on a repaint costs nothing. Nothing on the hook's
/// path calls this.
fn apply_theme(ctx: &egui::Context) {
    let light = crate::platform::windows::theme::apps_are_light();
    // Once per window, not per frame: this runs every frame and a line per frame
    // is not a diagnostic, it is a way of hiding one.
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        static REPORTED: AtomicBool = AtomicBool::new(false);
        if !REPORTED.swap(true, Ordering::Relaxed) {
            crate::log::log(&format!(
                "SETTINGS theme: apps_are_light={light} -> {}",
                if light { "Light" } else { "Dark" }
            ));
        }
    }
    ctx.set_theme(theme_preference(light));
}

/// The mapping itself, separated from the registry so it can be asserted.
fn theme_preference(apps_are_light: bool) -> egui::ThemePreference {
    if apps_are_light {
        egui::ThemePreference::Light
    } else {
        egui::ThemePreference::Dark
    }
}

/// The colour of a caption or any other secondary line.
///
/// Not `Visuals::weak_text_color`: that greys the text *towards the background*,
/// which on the light theme lands near 3:1 against the panel — under the 4.5:1
/// this text needs at 11.5 points. These two are the macOS `secondaryLabelColor`
/// equivalents, dark enough on light and light enough on dark to stay readable
/// in both.
fn secondary_color(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_gray(170)
    } else {
        egui::Color32::from_gray(90)
    }
}

/// The secondary line under a control whose label cannot carry its meaning — the
/// macOS window's `caption`. The text comes from the engine's own documentation
/// of what the option does and why its default is what it is.
fn caption(ui: &mut egui::Ui, text: &str) {
    caption_inset(ui, text, INDENT);
}

/// A caption with no inset: the introductory line at the top of a pane, which
/// explains the pane rather than a single control.
fn intro(ui: &mut egui::Ui, text: &str) {
    caption_inset(ui, text, 0.0);
}

fn caption_inset(ui: &mut egui::Ui, text: &str, inset: f32) {
    let color = secondary_color(ui);
    egui::Frame::none()
        .inner_margin(egui::Margin {
            left: inset,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
        })
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).small().color(color));
        });
}

/// A checkbox row, optionally with the caption that explains it.
fn checkbox_row(ui: &mut egui::Ui, value: &mut bool, label: &str, help: Option<&str>) {
    ui.horizontal(|ui| {
        ui.set_min_height(ROW_HEIGHT);
        ui.checkbox(value, label);
    });
    if let Some(text) = help {
        caption(ui, text);
    }
    ui.add_space(4.0);
}

/// A row with its label in a fixed left column and its control beside it, so
/// every picker in the window lines up on one edge — the macOS `form_row`.
fn control_row(
    ui: &mut egui::Ui,
    label: &str,
    help: Option<&str>,
    add: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.set_min_height(ROW_HEIGHT);
        ui.allocate_ui_with_layout(
            egui::vec2(LABEL_COLUMN, ROW_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(label);
            },
        );
        add(ui);
    });
    if let Some(text) = help {
        caption(ui, text);
    }
    ui.add_space(4.0);
}

/// A list row: its text on the left, its buttons flush to the right edge, on the
/// same row height as everything else.
fn list_row(ui: &mut egui::Ui, text: &str, buttons: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(ROW_HEIGHT);
        ui.label(text);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), buttons);
    });
}

/// The gap that separates one group of settings from the next, where the macOS
/// stack uses `setCustomSpacing:`.
fn group_gap(ui: &mut egui::Ui) {
    ui.add_space(GROUP_GAP);
}

/// A section title, in the macOS settings shape: bold, small, secondary.
fn section_header(ui: &mut egui::Ui, title: &str) {
    let color = secondary_color(ui);
    ui.label(egui::RichText::new(title).small().strong().color(color));
    ui.add_space(2.0);
}

/// An auxiliary window: the toolkit's nearest equivalent of the separate windows
/// the macOS build opens for About and the three list editors. Centred on first
/// open (as `NSWindow::center` does) and draggable after, with the close button
/// its `open` flag provides.
fn aux_window(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    size: [f32; 2],
    resizable: bool,
    open: &mut bool,
    add: impl FnOnce(&mut egui::Ui),
) {
    let center = ctx.screen_rect().center();
    egui::Window::new(title)
        .id(egui::Id::new(id))
        .open(open)
        .collapsible(false)
        .resizable(resizable)
        .default_size(size)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(center)
        .show(ctx, |ui| {
            ui.add_space(2.0);
            add(ui);
        });
}

// ---------------------------------------------------------------------------
// The app
// ---------------------------------------------------------------------------

/// Which tab is currently shown. The four the macOS window has
/// (`app/src/prefs/tabs.rs`), in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    General,
    Typing,
    Corrections,
    Apps,
}

impl Tab {
    const ORDER: [Self; 4] = [Self::General, Self::Typing, Self::Corrections, Self::Apps];

    /// The tab's definition in the shared spec, by position.
    fn spec(self) -> &'static TabSpec {
        let index = Self::ORDER
            .iter()
            .position(|t| *t == self)
            .expect("every tab is in ORDER");
        &TABS[index]
    }

    /// The tab titles, in order, from the spec the macOS window also reads.
    fn all() -> [(Self, &'static str); 4] {
        let mut out = [(Self::General, ""); 4];
        for (slot, tab) in out.iter_mut().zip(Self::ORDER) {
            *slot = (tab, tab.spec().title.get());
        }
        out
    }
}

struct SettingsApp {
    /// The value passed in, kept verbatim so the final draft can be compared
    /// against it — the only way to know whether to return `None`.
    initial: Settings,
    /// The value being edited. Every control writes here directly.
    draft: Settings,
    tab: Tab,
    result_slot: Rc<RefCell<Option<Option<Settings>>>>,

    // ----- The list-editor windows, open or not -----
    excluded_open: bool,
    macros_open: bool,
    words_open: bool,

    // ----- Excluded apps window -----
    /// The effective exclusion set (saved ids merged with un-tombstoned
    /// shipped defaults) plus its tombstones. Edited in place with
    /// [`ExclusionList::add`]/[`ExclusionList::remove`] so the tombstoning
    /// rule for a removed shipped default is applied the same way the engine
    /// applies it, not reimplemented here.
    exclusion_list: ExclusionList,
    new_exclusion: String,

    // ----- Macros window -----
    macro_shortcut: String,
    macro_expansion: String,
    /// `Some(i)` while editing `draft.macros[i]`; the Add/Save button and the
    /// row buttons that set this stay in sync.
    macro_edit_index: Option<usize>,
    /// Scratch buffer for the import/export table (UniKey/EVKey `gõ tắt`
    /// format).
    macro_table_text: String,

    // ----- Personal words window -----
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
            excluded_open: false,
            macros_open: false,
            words_open: false,
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

    // ----- Tabs -------------------------------------------------------------
    //
    // The four tabs are not written here. They are `settings_spec::TABS`, the
    // same data the macOS window is built from, and this is one of the two
    // renderers of it. Everything below is "what does an egui row for this
    // control look like"; what the rows *are* is decided in one place.

    /// Draws one tab: its sections, each a header and its rows.
    fn render_tab(&mut self, ui: &mut egui::Ui, tab: &TabSpec) {
        for (i, section) in tab.sections.iter().enumerate() {
            if i > 0 {
                group_gap(ui);
            }
            section_header(ui, section.title.get());
            for row in section.rows {
                self.render_row(ui, row);
            }
        }
    }

    /// Draws one row of the spec.
    ///
    /// A row that depends on another toggle is indented under it and disabled
    /// while that toggle is off, so "Fix as I type" reads as the refinement of
    /// "Auto-fix" it is rather than as an equal.
    fn render_row(&mut self, ui: &mut egui::Ui, row: &Row) {
        let enabled = row
            .enabled_when
            .is_none_or(|parent| self.toggle_is_on(parent));
        let inset = if row.enabled_when.is_some() {
            INDENT
        } else {
            0.0
        };
        let caption_text = row
            .caption
            .map(|c| expand_shortcuts(c.get(), |s| shortcut_display(s).to_string()));

        egui::Frame::none()
            .inner_margin(egui::Margin {
                left: inset,
                right: 0.0,
                top: 0.0,
                bottom: 0.0,
            })
            .show(ui, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    self.render_control(ui, row, caption_text.as_deref());
                });
            });
    }

    fn render_control(&mut self, ui: &mut egui::Ui, row: &Row, caption_text: Option<&str>) {
        let label = row.label.map(|l| l.get()).unwrap_or("");
        match row.control {
            Control::Language(options) => {
                // Applies immediately rather than at save, so the window is in
                // the chosen language before the user has to decide whether they
                // chose right. `set_language` is the same call the app makes at
                // startup; the value is persisted like any other edit.
                let before = self.draft.language;
                control_row(ui, label, caption_text, |ui| {
                    for (text, value) in options {
                        ui.radio_value(&mut self.draft.language, *value, text.get());
                    }
                });
                if self.draft.language != before {
                    crate::strings::set_language(self.draft.language);
                }
            }
            Control::InputMethod(options) => {
                control_row(ui, label, caption_text, |ui| {
                    for (text, value) in options {
                        ui.radio_value(&mut self.draft.input_method, *value, text.get());
                    }
                });
            }
            Control::ToneMarks(options) => {
                control_row(ui, label, caption_text, |ui| {
                    for (text, value) in options {
                        ui.radio_value(&mut self.draft.style, *value, text.get());
                    }
                });
            }
            Control::Checkbox(Toggle::LaunchAtLogin) => {
                // Registry state rather than a field of `Settings`, so it cannot
                // ride along with the rest of the draft. Read fresh every frame
                // so the checkbox cannot drift out of step with the tray item
                // toggling the same thing; a registry read on a repaint of an
                // open window is nowhere near the keystroke path.
                let mut at_login = crate::platform::windows::startup::is_enabled();
                let was = at_login;
                checkbox_row(ui, &mut at_login, label, caption_text);
                if at_login != was && !crate::platform::windows::startup::set_enabled(at_login) {
                    // The write failed, so the checkbox must not go on claiming
                    // it worked. Nothing is stored here, so the next frame
                    // re-reads the registry and shows the truth.
                    crate::log::log("SETTINGS could not change the startup entry");
                }
            }
            Control::Checkbox(toggle) => {
                let value = toggle
                    .settings_field(&mut self.draft)
                    .expect("every toggle but LaunchAtLogin is a Settings field");
                checkbox_row(ui, value, label, caption_text);
            }
            Control::ToggleHotkey => {
                control_row(ui, label, caption_text, |ui| {
                    ui.vertical(|ui| {
                        // Alt+Space is the system-menu key on Windows. Whether
                        // the hook wins that race is unverified, so it is not
                        // offered here; a settings file that already has it
                        // still shows it below.
                        let offered = HOTKEY_PRESETS
                            .into_iter()
                            .filter(|p| *p != HotkeyPreset::OptionSpace);
                        for preset in offered {
                            ui.radio_value(
                                &mut self.draft.toggle_hotkey,
                                preset,
                                hotkey_display(preset),
                            );
                        }
                        // The saved choice when it is not one of the above: a
                        // combination recorded on a Mac (there is no recorder on
                        // this platform), or Alt+Space. Shown as the current
                        // choice rather than silently replaced by a preset.
                        let current = self.draft.toggle_hotkey;
                        if matches!(current, HotkeyPreset::Custom { .. })
                            || current == HotkeyPreset::OptionSpace
                        {
                            ui.radio_value(
                                &mut self.draft.toggle_hotkey,
                                current,
                                hotkey_display(current),
                            );
                        }
                    });
                });
            }
            Control::Shortcut(shortcut) => {
                control_row(ui, label, caption_text, |ui| {
                    ui.label(shortcut_display(shortcut));
                });
            }
            Control::List(list) => {
                let count = match list {
                    // The live edit set, not the draft's saved fields: those are
                    // written back only when the window closes.
                    ListId::ExcludedApps => self.exclusion_list.ids().count(),
                    ListId::Macros => self.draft.macros.len(),
                    ListId::PersonalWords => self.draft.word_overrides.len(),
                };
                let mut open = false;
                control_row(ui, label, caption_text, |ui| {
                    ui.label(count.to_string());
                    open = ui.button(MANAGE.get()).clicked();
                });
                if open {
                    match list {
                        ListId::ExcludedApps => self.excluded_open = true,
                        ListId::Macros => self.macros_open = true,
                        ListId::PersonalWords => self.words_open = true,
                    }
                }
            }
        }
    }

    /// The current value of a settings-backed toggle. `LaunchAtLogin` is never
    /// a parent, so it reads as off here rather than costing a registry read.
    fn toggle_is_on(&mut self, toggle: Toggle) -> bool {
        toggle
            .settings_field(&mut self.draft)
            .is_some_and(|value| *value)
    }

    // ----- The list-editor windows -----------------------------------------

    /// Excluded apps, as its own window — `app/src/prefs/excluded.rs`.
    fn excluded_body(&mut self, ui: &mut egui::Ui) {
        intro(
            ui,
            t(
                "GlowKey types plain keys in these apps. Add one below by the name of its \
                 program file, as Task Manager shows it, or toggle the app in front from \
                 the tray menu.",
                "GlowKey gõ phím thường trong các ứng dụng này. Thêm ở dưới theo tên tệp \
                 chạy như trong Task Manager, hoặc bật tắt ứng dụng đang mở từ menu khay \
                 hệ thống.",
            ),
        );
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.set_min_height(ROW_HEIGHT);
            ui.add(
                egui::TextEdit::singleline(&mut self.new_exclusion)
                    .desired_width(200.0)
                    .hint_text(t("program name", "tên chương trình")),
            );
            if ui.button(t("Add App", "Thêm ứng dụng")).clicked() {
                if let Some(name) = normalize_exe_name(&self.new_exclusion) {
                    self.exclusion_list.add(name);
                    self.new_exclusion.clear();
                }
            }
        });

        ui.add_space(6.0);
        ui.separator();

        let mut ids: Vec<String> = self.exclusion_list.ids().map(str::to_string).collect();
        ids.sort();
        let mut to_remove: Option<String> = None;
        if ids.is_empty() {
            intro(ui, t("No apps excluded.", "Chưa có ứng dụng nào."));
        } else {
            egui::ScrollArea::vertical()
                .id_salt("exclusion_list")
                .max_height(200.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for id in &ids {
                        list_row(ui, id, |ui| {
                            if ui.button(t("Remove", "Xóa")).clicked() {
                                to_remove = Some(id.clone());
                            }
                        });
                    }
                });
        }
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
        if tombstones.is_empty() {
            return;
        }
        tombstones.sort();
        ui.add_space(6.0);
        ui.separator();
        intro(
            ui,
            t(
                "Defaults you removed. They will not come back on their own, even in a \
                 future release that ships them again.",
                "Những mặc định bạn đã bỏ. Chúng sẽ không tự quay lại, kể cả ở bản cập nhật \
                 sau có kèm chúng.",
            ),
        );
        let mut to_restore: Option<String> = None;
        for id in &tombstones {
            list_row(ui, id, |ui| {
                if ui.button(t("Restore", "Khôi phục")).clicked() {
                    to_restore = Some(id.clone());
                }
            });
        }
        if let Some(id) = to_restore {
            // Re-adding makes the id explicitly excluded again; the tombstone
            // record itself is harmless once the id is present, since presence
            // always wins over it.
            self.exclusion_list.add(id);
        }
    }

    /// Macros, as its own window — `app/src/prefs/macros_window.rs`, plus the
    /// import/export box this platform has instead of an `NSOpenPanel`.
    fn macros_body(&mut self, ui: &mut egui::Ui) {
        intro(
            ui,
            t(
                "Type the shortcut then a space to expand it — e.g. \"vn\" → \"Việt Nam\".",
                "Gõ chữ viết tắt rồi dấu cách để bung ra — ví dụ “vn” → “Việt Nam”.",
            ),
        );
        ui.add_space(6.0);

        let editing = self.macro_edit_index.is_some();
        ui.horizontal(|ui| {
            ui.set_min_height(ROW_HEIGHT);
            ui.add(
                egui::TextEdit::singleline(&mut self.macro_shortcut)
                    .desired_width(90.0)
                    .hint_text(t("shortcut", "viết tắt")),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.macro_expansion)
                    .desired_width(190.0)
                    .hint_text(t("expansion", "nội dung")),
            );
            let label = if editing {
                t("Save", "Lưu")
            } else {
                t("Add", "Thêm")
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
            if editing && ui.button(t("Cancel", "Huỷ")).clicked() {
                self.macro_edit_index = None;
                self.macro_shortcut.clear();
                self.macro_expansion.clear();
            }
        });

        ui.add_space(6.0);
        ui.separator();

        let mut action: Option<(usize, bool)> = None;
        if self.draft.macros.is_empty() {
            intro(ui, t("No macros yet.", "Chưa có gõ tắt nào."));
        } else {
            egui::ScrollArea::vertical()
                .id_salt("macro_list")
                .max_height(150.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (i, m) in self.draft.macros.iter().enumerate() {
                        list_row(ui, &format!("{}  →  {}", m.shortcut, m.expansion), |ui| {
                            if ui.button(t("Remove", "Xóa")).clicked() {
                                action = Some((i, false));
                            }
                            if ui.button(t("Edit", "Sửa")).clicked() {
                                action = Some((i, true));
                            }
                        });
                    }
                });
        }
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

        ui.add_space(6.0);
        ui.separator();
        intro(
            ui,
            t(
                "Import or export a table in the UniKey/EVKey format, one \
                 shortcut:expansion per line — the main way a curated list arrives.",
                "Nhập hoặc xuất bảng theo định dạng UniKey/EVKey, mỗi dòng một mục \
                 viết tắt:nội dung — cách phổ biến nhất để mang một danh sách có sẵn sang.",
            ),
        );
        ui.add_space(4.0);
        ui.add(
            egui::TextEdit::multiline(&mut self.macro_table_text)
                .desired_rows(4)
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.set_min_height(ROW_HEIGHT);
            if ui
                .button(t("Import into list", "Nhập vào danh sách"))
                .clicked()
            {
                for m in Macro::parse_table(&self.macro_table_text) {
                    upsert_macro(&mut self.draft.macros, m.shortcut, m.expansion, None);
                }
            }
            if ui
                .button(t("Export list here", "Xuất danh sách ra đây"))
                .clicked()
            {
                self.macro_table_text = Macro::format_table(&self.draft.macros);
            }
        });
    }

    /// Personal words, as its own window — `app/src/prefs/personal_words.rs`.
    fn words_body(&mut self, ui: &mut egui::Ui) {
        intro(
            ui,
            t(
                "Words you have decided about. \"Keep as typed\" keeps the keys as typed \
                 (was → was); \"Keep Vietnamese\" keeps the accented form (cats → cát). \
                 These win over auto-fix and over the English-word setting.",
                "Những từ bạn đã quyết định. “Giữ như gõ” giữ nguyên các phím (was → was); \
                 “Giữ tiếng Việt” giữ dạng có dấu (cats → cát). Chúng được ưu tiên hơn tự \
                 động sửa và hơn tùy chọn từ tiếng Anh.",
            ),
        );
        ui.add_space(6.0);

        let editing = self.word_edit_index.is_some();
        ui.horizontal(|ui| {
            ui.set_min_height(ROW_HEIGHT);
            ui.add(
                egui::TextEdit::singleline(&mut self.word_keys)
                    .desired_width(120.0)
                    .hint_text(t("word as typed", "từ như đã gõ")),
            );
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
            let label = if editing {
                t("Save", "Lưu")
            } else {
                t("Add", "Thêm")
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
            if editing && ui.button(t("Cancel", "Huỷ")).clicked() {
                self.word_edit_index = None;
                self.word_keys.clear();
            }
        });

        ui.add_space(6.0);
        ui.separator();

        let mut action: Option<(usize, bool)> = None;
        if self.draft.word_overrides.is_empty() {
            intro(ui, t("No words yet.", "Chưa có từ nào."));
        } else {
            egui::ScrollArea::vertical()
                .id_salt("word_override_list")
                .max_height(200.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (i, w) in self.draft.word_overrides.iter().enumerate() {
                        list_row(
                            ui,
                            &format!("{}  →  {}", w.keys, preference_label(w.prefer)),
                            |ui| {
                                if ui.button(t("Remove", "Xóa")).clicked() {
                                    action = Some((i, false));
                                }
                                if ui.button(t("Edit", "Sửa")).clicked() {
                                    action = Some((i, true));
                                }
                            },
                        );
                    }
                });
        }
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

    /// Dispatches to the tab's builder.
    fn show_tab(&mut self, ui: &mut egui::Ui) {
        let spec = self.tab.spec();
        self.render_tab(ui, spec);
    }

    /// Every auxiliary window, drawn over the tabs.
    fn show_aux_windows(&mut self, ctx: &egui::Context) {
        let mut open = self.excluded_open;
        if open {
            aux_window(
                ctx,
                "glowkey_excluded",
                t("Excluded Apps", "Ứng dụng loại trừ"),
                EXCLUDED_SIZE,
                true,
                &mut open,
                |ui| self.excluded_body(ui),
            );
            self.excluded_open = open;
        }

        let mut open = self.macros_open;
        if open {
            aux_window(
                ctx,
                "glowkey_macros",
                t("Macros", "Gõ tắt"),
                MACROS_SIZE,
                true,
                &mut open,
                |ui| self.macros_body(ui),
            );
            self.macros_open = open;
        }

        let mut open = self.words_open;
        if open {
            aux_window(
                ctx,
                "glowkey_words",
                t("Personal Words", "Từ riêng"),
                WORDS_SIZE,
                true,
                &mut open,
                |ui| self.words_body(ui),
            );
            self.words_open = open;
        }
    }

    /// The whole window, one frame of it.
    ///
    /// Separate from [`eframe::App::update`] so a frame can be built against a
    /// bare [`egui::Context`] with no window, no GPU and no event loop — which
    /// is how the tabs are smoke-tested. `eframe::Frame` is not used here and
    /// there is nothing else to keep the two together.
    fn ui(&mut self, ctx: &egui::Context) {
        // Before anything is laid out, so a theme the user switched while the
        // window was open takes effect on this frame rather than the next one.
        apply_theme(ctx);

        // The OS/window-manager close (title-bar X, Alt+F4, …) also has to
        // decide what to hand back; there is no Done button, this is the only
        // way out.
        if ctx.input(|i| i.viewport().close_requested()) {
            self.finalize();
        }

        egui::TopBottomPanel::top("glowkey_settings_tabs")
            .frame(chrome_frame(ctx).inner_margin(egui::Margin {
                left: 10.0,
                right: 10.0,
                top: 8.0,
                bottom: 6.0,
            }))
            .show(ctx, |ui| {
                // Centred, like the tab strip of an `NSTabView`.
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        for (tab, title) in Tab::all() {
                            ui.selectable_value(&mut self.tab, tab, title);
                        }
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(ctx.style().visuals.window_fill)
                    .inner_margin(egui::Margin::symmetric(20.0, 14.0)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("glowkey_settings_body")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.show_tab(ui);
                    });
            });

        self.show_aux_windows(ctx);
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }

    /// The colour behind everything.
    ///
    /// **eframe's default ignores the theme.** It returns a hardcoded
    /// `rgba(12, 12, 12, 180)` — near-black — whatever the visuals say, and every
    /// panel that does not paint its own background shows it. That is how a
    /// window whose theme had already been corrected still came up black: the
    /// theme was right and the surface underneath it was not.
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.window_fill().to_normalized_gamma_f32()
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
/// is explained to the user — and matching the macOS window's wording, so the
/// same decision is not named two different things on two platforms.
fn preference_label(pref: WordPreference) -> &'static str {
    match pref {
        WordPreference::Raw => t("Keep as typed", "Giữ như gõ"),
        WordPreference::Vietnamese => t("Keep Vietnamese", "Giữ tiếng Việt"),
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

    /// The four tabs of the macOS window, in its order and under its titles. The
    /// two interfaces are the same product, and a tab that exists on one
    /// platform and not the other is how they start diverging.
    #[test]
    fn the_tabs_are_the_four_the_macos_window_has() {
        let tabs: Vec<Tab> = Tab::all().into_iter().map(|(tab, _)| tab).collect();
        assert_eq!(
            tabs,
            vec![Tab::General, Tab::Typing, Tab::Corrections, Tab::Apps]
        );
        for (tab, title) in Tab::all() {
            assert!(!title.trim().is_empty(), "{tab:?} has no title");
        }
        // The enum is paired with `TABS` by position; a reorder of either
        // would put one tab's title over another's body.
        assert_eq!(Tab::General.spec().title.en, "General");
        assert_eq!(Tab::Typing.spec().title.en, "Typing");
        assert_eq!(Tab::Corrections.spec().title.en, "Corrections");
        assert_eq!(Tab::Apps.spec().title.en, "Apps & macros");
    }

    /// The two states the default settings never put a tab in: auto-fix off,
    /// so the dependent row renders disabled, and a hotkey recorded on a Mac,
    /// which has no radio of its own until the renderer adds one.
    #[test]
    fn dependent_and_foreign_hotkey_states_render() {
        let ctx = egui::Context::default();
        apply_style(&ctx);
        let settings = Settings {
            auto_fix: false,
            toggle_hotkey: HotkeyPreset::Custom {
                control: true,
                shift: false,
                option: true,
                key_char: 'k',
                macos_keycode: Some(40),
                windows_vk: None,
            },
            ..Settings::default()
        };
        let slot = Rc::new(RefCell::new(None));
        let mut app = SettingsApp::new(settings.clone(), Rc::clone(&slot));
        for tab in [Tab::General, Tab::Corrections] {
            app.tab = tab;
            for _ in 0..2 {
                let _ = ctx.run(egui::RawInput::default(), |ctx| app.ui(ctx));
            }
        }
        // Rendering must not have edited anything: the foreign hotkey is shown,
        // not replaced.
        assert_eq!(app.draft, settings);
        assert!(slot.borrow().is_none());
    }

    /// A light system gets a light window.
    ///
    /// This is the regression: the window asked the toolkit to follow the system
    /// theme, the toolkit could not tell, and its fallback is dark — so a
    /// machine with `AppsUseLightTheme = 1` got a black window with a black Done
    /// button. The choice is now made here, from the registry value, and there
    /// is no third answer to fall back to.
    #[test]
    fn the_theme_follows_windows_rather_than_guessing() {
        assert_eq!(theme_preference(true), egui::ThemePreference::Light);
        assert_eq!(theme_preference(false), egui::ThemePreference::Dark);
    }

    /// Secondary text has to stay readable in both themes.
    ///
    /// Captions carry the reason a setting exists, at 11.5 points, and
    /// `weak_text_color` fades towards the background — under 4.5:1 on light.
    /// Both of these clear it: gray 90 on the light panel and gray 170 on the
    /// dark one are roughly 6.6:1 and 7.4:1.
    #[test]
    fn caption_colour_contrasts_in_both_themes() {
        let ctx = egui::Context::default();
        for (preference, expected) in [
            (egui::ThemePreference::Light, egui::Color32::from_gray(90)),
            (egui::ThemePreference::Dark, egui::Color32::from_gray(170)),
        ] {
            ctx.set_theme(preference);
            let mut seen = None;
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    seen = Some(secondary_color(ui));
                });
            });
            assert_eq!(seen, Some(expected), "{preference:?}");
        }
    }

    /// The interface font has to be able to *draw* Vietnamese.
    ///
    /// egui's bundled proportional font stops at Latin Extended-A, so every
    /// `ế ộ ữ` in this window would be a missing-glyph box — a Vietnamese
    /// interface nobody can read. This is the assertion that the substitution in
    /// [`install_system_font`] actually covers the alphabet, and it needs no
    /// window: a bare context, one frame, and the font is queryable.
    #[test]
    fn the_interface_font_can_draw_vietnamese() {
        let ctx = egui::Context::default();
        if !install_system_font(&ctx) {
            // No Segoe UI on this machine (not a Windows desktop image): there
            // is no substitution to check, and failing here would only report
            // the machine.
            return;
        }
        apply_style(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |_| {});

        let body = egui::TextStyle::Body.resolve(&ctx.style());
        let alphabet = "ăâđêôơư ẹẻẽếệỉịọỏốộớởụủứữựỳỷỹ Gõ tắt, khôi phục, ưu tiên";
        assert!(
            ctx.fonts(|f| f.has_glyphs(&body, alphabet)),
            "the UI font cannot draw Vietnamese"
        );
    }

    /// Every tab and every auxiliary window must survive being built.
    ///
    /// A layout mistake in egui — a duplicate id, a widget allocated outside the
    /// space it was given — panics at build time, not at draw time, so a bare
    /// context with no window and no GPU is enough to catch it. This is not a
    /// test of how the window *looks*; it is a test that each pane can be built
    /// at all, including the four popups a user only reaches by clicking.
    #[test]
    fn every_tab_and_window_builds() {
        let ctx = egui::Context::default();
        apply_style(&ctx);

        let mut settings = Settings::default();
        settings.macros.push(Macro {
            shortcut: "vn".into(),
            expansion: "Việt Nam".into(),
        });
        settings.word_overrides.push(WordOverride {
            keys: "cats".into(),
            prefer: WordPreference::Vietnamese,
        });
        let slot = Rc::new(RefCell::new(None));
        let mut app = SettingsApp::new(settings, Rc::clone(&slot));
        // Every auxiliary window open at once, so all four are built.
        app.excluded_open = true;
        app.macros_open = true;
        app.words_open = true;
        // A removed shipped default, so the tombstone list is built too.
        let removed = app.exclusion_list.ids().next().map(str::to_string);
        if let Some(id) = removed {
            app.exclusion_list.remove(&id);
        }

        for (tab, _) in Tab::all() {
            app.tab = tab;
            // Twice: the second frame is the one where widgets see the ids and
            // state the first frame stored.
            for _ in 0..2 {
                let _ = ctx.run(egui::RawInput::default(), |ctx| app.ui(ctx));
            }
        }

        assert!(
            app.excluded_open && app.macros_open && app.words_open,
            "an auxiliary window closed itself"
        );
        assert!(
            slot.borrow().is_none(),
            "building a pane must not decide the window is closing"
        );
    }
}
