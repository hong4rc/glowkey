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
//! (`app/src/platform/macos/prefs/tabs.rs`): General, Typing, Corrections,
//! Excluded apps, Macros, Personal words, About — behind a left sidebar rather
//! than a row of tab buttons, because the list of panes is the navigation and a
//! settings window on this desktop is expected to look like the system's own.
//!
//! Two things about the presentation are not decoration and are commented where
//! they happen: the interface font is taken from the system (`docs/ui-design.md`
//! asks for "looks like it came with the system", and egui's bundled font has no
//! Vietnamese glyphs at all), and every label goes through
//! [`crate::strings::t`], because the users are Vietnamese and an input method
//! is the last place to make someone read a second language.

use std::cell::RefCell;
use std::rc::Rc;

use eframe::egui;

use glowkey_engine::{
    ExclusionList, InputMethod, Language, Macro, PlacementStyle, Settings, WordOverride,
    WordPreference,
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
            .with_title(t("GlowKey Settings", "Cài đặt GlowKey"))
            // Sized in points, not pixels: winit reports the monitor's scale
            // factor and egui multiplies by it, so this is the same apparent
            // size at 100% and at 150%. The minimum is what the widest pane
            // (Macros: two fields, three buttons) needs before it starts
            // wrapping into an unreadable shape.
            .with_inner_size([860.0, 600.0])
            .with_min_inner_size([680.0, 460.0])
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

/// Width of the navigation sidebar, in points.
const SIDEBAR_WIDTH: f32 = 200.0;
/// Height a single settings row occupies, so checkboxes, pickers and list rows
/// all sit on the same rhythm instead of each taking its content's height.
const ROW_HEIGHT: f32 = 28.0;
/// Width of the label column in a label + control row.
const LABEL_COLUMN: f32 = 140.0;
/// How far a description line is inset under the control it explains — roughly
/// a checkbox plus its gap, so the text starts under the label, not the box.
const INDENT: f32 = 24.0;
/// Corner radius of a settings group.
const CARD_ROUNDING: f32 = 8.0;
/// Vertical gap above a section header.
const SECTION_GAP: f32 = 18.0;

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
/// Applied through [`egui::Context::all_styles_mut`] so it survives a light/dark
/// switch: egui keeps one `Style` per theme and swaps them when the system
/// preference changes, so anything written to only one of them is lost the
/// moment the user turns on dark mode.
fn apply_style(ctx: &egui::Context) {
    // The default already is "follow the system"; saying so is cheap and makes
    // the intent visible next to the rest of the theming.
    ctx.set_theme(egui::ThemePreference::System);

    ctx.all_styles_mut(|style| {
        use egui::FontFamily::{Monospace, Proportional};
        use egui::{FontId, TextStyle};

        style.text_styles = [
            (TextStyle::Heading, FontId::new(20.0, Proportional)),
            (TextStyle::Body, FontId::new(14.5, Proportional)),
            (TextStyle::Button, FontId::new(14.5, Proportional)),
            (TextStyle::Monospace, FontId::new(13.5, Monospace)),
            (TextStyle::Small, FontId::new(12.5, Proportional)),
        ]
        .into();

        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.interact_size.y = 24.0;
        style.spacing.menu_margin = egui::Margin::same(6.0);
        style.visuals.widgets.noninteractive.rounding = egui::Rounding::same(6.0);
        style.visuals.widgets.inactive.rounding = egui::Rounding::same(6.0);
        style.visuals.widgets.hovered.rounding = egui::Rounding::same(6.0);
        style.visuals.widgets.active.rounding = egui::Rounding::same(6.0);
    });
}

/// A section: a quiet header, then a rounded group holding its rows. The
/// grouping is the whole point — a settings window that is one flat column of
/// checkboxes gives the reader nothing to navigate by.
fn section<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let color = ui.visuals().strong_text_color();
    ui.add_space(SECTION_GAP);
    ui.label(egui::RichText::new(title).strong().color(color));
    ui.add_space(6.0);
    card(ui, add)
}

/// The rounded, faintly filled container a section's rows sit in.
fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let fill = ui.visuals().faint_bg_color;
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    egui::Frame::none()
        .fill(fill)
        .stroke(stroke)
        .rounding(CARD_ROUNDING)
        .inner_margin(egui::Margin::symmetric(14.0, 12.0))
        .show(ui, add)
        .inner
}

/// The explanation under a control whose label cannot carry it. The text comes
/// from the engine's own documentation of what the option does and why its
/// default is what it is — the reason a user can act on, not a restatement of
/// the label.
fn description(ui: &mut egui::Ui, text: &str) {
    let color = ui.visuals().weak_text_color();
    egui::Frame::none()
        .inner_margin(egui::Margin {
            left: INDENT,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
        })
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).small().color(color));
        });
}

/// A checkbox row, optionally with a description under it.
fn checkbox_row(ui: &mut egui::Ui, value: &mut bool, label: &str, help: Option<&str>) {
    ui.horizontal(|ui| {
        ui.set_min_height(ROW_HEIGHT);
        ui.checkbox(value, label);
    });
    if let Some(text) = help {
        description(ui, text);
    }
    ui.add_space(4.0);
}

/// A row with a left-aligned label in a fixed column and its control beside it,
/// so every picker in the window lines up on the same edge.
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
        description(ui, text);
    }
    ui.add_space(4.0);
}

/// A list row: text on the left, its buttons flush to the right edge, on the
/// same row height as everything else.
fn list_row(ui: &mut egui::Ui, text: &str, buttons: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(ROW_HEIGHT);
        ui.label(text);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), buttons);
    });
}

/// One entry in the sidebar. Drawn rather than composed from a `SelectableLabel`
/// so the highlight spans the full row and the text stays left-aligned, which is
/// what a system settings sidebar looks like.
fn nav_item(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 30.0), egui::Sense::click());

    let visuals = ui.visuals();
    let (fill, text_color) = if selected {
        (visuals.selection.bg_fill, visuals.selection.stroke.color)
    } else if response.hovered() {
        (
            visuals.widgets.hovered.weak_bg_fill,
            visuals.widgets.hovered.text_color(),
        )
    } else {
        (egui::Color32::TRANSPARENT, visuals.text_color())
    };
    let font = egui::TextStyle::Body.resolve(ui.style());

    ui.painter().rect_filled(rect, 6.0, fill);
    ui.painter().text(
        egui::pos2(rect.left() + 10.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        font,
        text_color,
    );
    response
}

// ---------------------------------------------------------------------------
// The app
// ---------------------------------------------------------------------------

/// Which pane is currently shown. Mirrors the macOS tab titles, split the same
/// way: one pane per question, short enough to read without scrolling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    General,
    Typing,
    Corrections,
    Apps,
    Macros,
    Words,
    About,
}

impl Tab {
    /// The sidebar entries, in order.
    fn all() -> [(Self, &'static str); 7] {
        [
            (Self::General, t("General", "Chung")),
            (Self::Typing, t("Typing", "Gõ phím")),
            (Self::Corrections, t("Corrections", "Sửa lỗi")),
            (Self::Apps, t("Excluded apps", "Ứng dụng loại trừ")),
            (Self::Macros, t("Macros", "Gõ tắt")),
            (Self::Words, t("Personal words", "Từ riêng")),
            (Self::About, t("About", "Giới thiệu")),
        ]
    }

    /// The one-line explanation under the pane title.
    fn subtitle(self) -> &'static str {
        match self {
            Self::General => t(
                "Interface language and what happens at launch.",
                "Ngôn ngữ giao diện và những gì xảy ra khi khởi động.",
            ),
            Self::Typing => t(
                "How keys become Vietnamese.",
                "Cách các phím trở thành tiếng Việt.",
            ),
            Self::Corrections => t(
                "What GlowKey does when a word isn't Vietnamese.",
                "GlowKey làm gì khi một từ không phải tiếng Việt.",
            ),
            Self::Apps => t(
                "Apps where GlowKey stays off, so it never mangles a command.",
                "Những ứng dụng GlowKey luôn tắt, để không làm hỏng câu lệnh.",
            ),
            Self::Macros => t(
                "Type a shortcut then a space to expand it.",
                "Gõ chữ viết tắt rồi dấu cách để bung ra.",
            ),
            Self::Words => t(
                "Decide a single word once and it stays decided.",
                "Quyết định một từ một lần và nó được giữ nguyên.",
            ),
            Self::About => t(
                "Version and known limits.",
                "Phiên bản và giới hạn đã biết.",
            ),
        }
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
        section(ui, t("Interface", "Giao diện"), |ui| {
            // The picker applies immediately rather than at save, so the window
            // is in the chosen language before the user has to decide whether
            // they chose right. `set_language` is the same call the app makes at
            // startup; the value is returned and persisted like any other edit.
            let before = self.draft.language;
            control_row(
                ui,
                t("Language", "Ngôn ngữ"),
                Some(t(
                    "\"System\" follows the language Windows is set to.",
                    "“Hệ thống” đi theo ngôn ngữ của Windows.",
                )),
                |ui| {
                    ui.radio_value(
                        &mut self.draft.language,
                        Language::System,
                        t("System", "Hệ thống"),
                    );
                    ui.radio_value(&mut self.draft.language, Language::Vietnamese, "Tiếng Việt");
                    ui.radio_value(&mut self.draft.language, Language::English, "English");
                },
            );
            if self.draft.language != before {
                crate::strings::set_language(self.draft.language);
            }
        });

        section(ui, t("At launch", "Khi khởi động"), |ui| {
            checkbox_row(
                ui,
                &mut self.draft.open_settings_at_launch,
                t(
                    "Open this window at launch",
                    "Mở cửa sổ này khi khởi động máy",
                ),
                Some(t(
                    "On by default, so a new user sees the controls. GlowKey keeps \
                     running in the notification area either way.",
                    "Mặc định bật, để người dùng mới thấy được các tuỳ chọn. Dù bật hay \
                     tắt, GlowKey vẫn chạy dưới khay hệ thống.",
                )),
            );
        });
    }

    fn show_typing_tab(&mut self, ui: &mut egui::Ui) {
        section(
            ui,
            t("Input method and tone marks", "Kiểu gõ và dấu thanh"),
            |ui| {
                control_row(ui, t("Input method", "Kiểu gõ"), None, |ui| {
                    ui.radio_value(&mut self.draft.input_method, InputMethod::Telex, "Telex");
                    ui.radio_value(&mut self.draft.input_method, InputMethod::Vni, "VNI");
                    ui.radio_value(
                        &mut self.draft.input_method,
                        InputMethod::SimpleTelex,
                        t("Simple Telex", "Telex đơn giản"),
                    );
                });

                control_row(ui, t("Tone marks", "Dấu thanh"), None, |ui| {
                    ui.radio_value(
                        &mut self.draft.style,
                        PlacementStyle::New,
                        t("Modern  hoà", "Kiểu mới  hoà"),
                    );
                    ui.radio_value(
                        &mut self.draft.style,
                        PlacementStyle::Old,
                        t("Classic  hòa", "Kiểu cũ  hòa"),
                    );
                });
            },
        );

        section(ui, t("Shortcuts", "Phím tắt khi gõ"), |ui| {
            checkbox_row(
                ui,
                &mut self.draft.quick_telex,
                t("Quick Telex", "Gõ tắt phụ âm"),
                Some(t(
                    "A doubled consonant at the start of a syllable types its digraph: \
                     cc→ch, gg→gi, kk→kh, nn→ng, pp→ph, qq→qu, tt→th, uu→ư. Off by \
                     default: it changes what plain consonant pairs mean.",
                    "Phụ âm gõ đôi ở đầu âm tiết cho ra phụ âm ghép: cc→ch, gg→gi, kk→kh, \
                     nn→ng, pp→ph, qq→qu, tt→th, uu→ư. Mặc định tắt vì nó thay đổi ý nghĩa \
                     của các cặp phụ âm thường.",
                )),
            );

            checkbox_row(
                ui,
                &mut self.draft.telex_brackets,
                t("Telex bracket shortcuts", "Phím ngoặc kiểu Telex"),
                Some(t(
                    "[ → ơ, ] → ư, { → Ơ, } → Ư while typing Telex. These four keys stop \
                     reaching the app entirely, including where they are shortcuts.",
                    "[ → ơ, ] → ư, { → Ơ, } → Ư khi gõ Telex. Bốn phím này sẽ không đến \
                     ứng dụng nữa, kể cả khi chúng là phím tắt.",
                )),
            );
        });
    }

    fn show_corrections_tab(&mut self, ui: &mut egui::Ui) {
        section(
            ui,
            t("Non-Vietnamese words", "Từ không phải tiếng Việt"),
            |ui| {
                checkbox_row(
                    ui,
                    &mut self.draft.auto_fix,
                    t(
                        "Auto-fix non-Vietnamese words",
                        "Tự động khôi phục từ không phải tiếng Việt",
                    ),
                    Some(t(
                        "Restores the raw keys at the space when the result isn't valid \
                     Vietnamese — types \"exit\", not \"eĩt\".",
                        "Khôi phục phím gốc ở dấu cách khi kết quả không phải tiếng Việt — \
                     gõ ra “exit”, không phải “eĩt”.",
                    )),
                );

                checkbox_row(
                    ui,
                    &mut self.draft.strict_spell_check,
                    t(
                        "Fix as I type, not at the space",
                        "Sửa ngay khi gõ, không đợi dấu cách",
                    ),
                    Some(t(
                        "Restores the raw keys the moment a word stops being possible \
                     Vietnamese — \"exit\" repairs at the x, not at the space.",
                        "Khôi phục phím gốc ngay khi từ không còn là tiếng Việt hợp lệ — \
                     “exit” được sửa ngay ở chữ x, không đợi dấu cách.",
                    )),
                );

                checkbox_row(
                    ui,
                    &mut self.draft.restore_english_words,
                    t(
                        "Restore common English words",
                        "Khôi phục từ tiếng Anh thông dụng",
                    ),
                    Some(t(
                        "Off by default: \"was\" stays \"was\", but every syllable sharing keys \
                     with a listed word (á→as, í→is, cát→cats, cả→car, hải→hair) then needs \
                     a different key order. Personal words decides one word at a time \
                     instead, and wins over this.",
                        "Mặc định tắt: “was” giữ nguyên “was”, nhưng mọi âm tiết trùng phím với \
                     từ trong danh sách (á→as, í→is, cát→cats, cả→car, hải→hair) sẽ phải gõ \
                     theo thứ tự khác. “Từ riêng” quyết định từng từ một, và được ưu tiên hơn.",
                    )),
                );
            },
        );

        section(ui, t("Capitalization", "Viết hoa"), |ui| {
            checkbox_row(
                ui,
                &mut self.draft.auto_capitalize,
                t(
                    "Auto-capitalize first letter of each sentence",
                    "Tự động viết hoa chữ đầu câu",
                ),
                None,
            );
        });
    }

    fn show_apps_tab(&mut self, ui: &mut egui::Ui) {
        section(ui, t("Add an app", "Thêm ứng dụng"), |ui| {
            control_row(
                ui,
                t("Program name", "Tên chương trình"),
                Some(t(
                    "The executable's own name, as Task Manager shows it — code.exe, \
                     cmd.exe, WindowsTerminal.exe.",
                    "Tên tệp chạy của chương trình, như trong Task Manager — code.exe, \
                     cmd.exe, WindowsTerminal.exe.",
                )),
                |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_exclusion)
                            .desired_width(220.0)
                            .hint_text("code.exe"),
                    );
                    if ui.button(t("Add", "Thêm")).clicked() {
                        if let Some(name) = normalize_exe_name(&self.new_exclusion) {
                            self.exclusion_list.add(name);
                            self.new_exclusion.clear();
                        }
                    }
                },
            );
        });

        let mut ids: Vec<String> = self.exclusion_list.ids().map(str::to_string).collect();
        ids.sort();
        let mut to_remove: Option<String> = None;
        section(ui, t("Current list", "Danh sách hiện có"), |ui| {
            if ids.is_empty() {
                description(ui, t("No apps excluded.", "Chưa có ứng dụng nào."));
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt("exclusion_list")
                .max_height(260.0)
                .show(ui, |ui| {
                    for id in &ids {
                        list_row(ui, id, |ui| {
                            if ui.button(t("Remove", "Xóa")).clicked() {
                                to_remove = Some(id.clone());
                            }
                        });
                    }
                });
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
        if tombstones.is_empty() {
            return;
        }
        tombstones.sort();
        let mut to_restore: Option<String> = None;
        section(
            ui,
            t("Defaults you removed", "Mặc định bạn đã bỏ"),
            |ui| {
                description(
                    ui,
                    t(
                        "These will not come back on their own, even in a future release \
                         that ships them again.",
                        "Những mục này sẽ không tự quay lại, kể cả ở bản cập nhật sau có \
                         kèm chúng.",
                    ),
                );
                ui.add_space(4.0);
                for id in &tombstones {
                    list_row(ui, id, |ui| {
                        if ui.button(t("Restore", "Khôi phục")).clicked() {
                            to_restore = Some(id.clone());
                        }
                    });
                }
            },
        );
        if let Some(id) = to_restore {
            // Re-adding makes the id explicitly excluded again; the tombstone
            // record itself is harmless once the id is present, since presence
            // always wins over it.
            self.exclusion_list.add(id);
        }
    }

    fn show_macros_tab(&mut self, ui: &mut egui::Ui) {
        let editing = self.macro_edit_index.is_some();
        section(
            ui,
            if editing {
                t("Edit macro", "Sửa gõ tắt")
            } else {
                t("Add a macro", "Thêm gõ tắt")
            },
            |ui| {
                control_row(ui, t("Shortcut", "Viết tắt"), None, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.macro_shortcut)
                            .desired_width(140.0)
                            .hint_text("vn"),
                    );
                });
                control_row(ui, t("Expands to", "Nội dung"), None, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.macro_expansion)
                            .desired_width(320.0)
                            .hint_text("Việt Nam"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.set_min_height(ROW_HEIGHT);
                    ui.add_space(LABEL_COLUMN);
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
            },
        );

        let mut action: Option<(usize, bool)> = None;
        section(ui, t("Your macros", "Danh sách gõ tắt"), |ui| {
            if self.draft.macros.is_empty() {
                description(ui, t("No macros yet.", "Chưa có gõ tắt nào."));
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt("macro_list")
                .max_height(200.0)
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

        section(ui, t("Import and export", "Nhập và xuất"), |ui| {
            description(
                ui,
                t(
                    "A table in the UniKey/EVKey format, one shortcut:expansion per line — \
                     the main way a curated list arrives.",
                    "Bảng theo định dạng UniKey/EVKey, mỗi dòng một mục viết tắt:nội dung — \
                     cách phổ biến nhất để mang một danh sách có sẵn sang.",
                ),
            );
            ui.add_space(6.0);
            ui.add(
                egui::TextEdit::multiline(&mut self.macro_table_text)
                    .desired_rows(6)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace),
            );
            ui.add_space(6.0);
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
        });

        section(
            ui,
            t("While Vietnamese is off", "Khi đã tắt tiếng Việt"),
            |ui| {
                checkbox_row(
                    ui,
                    &mut self.draft.always_macro,
                    t(
                        "Expand macros even when Vietnamese is off",
                        "Bung gõ tắt cả khi đã tắt tiếng Việt",
                    ),
                    Some(t(
                        "Never applies in an excluded app.",
                        "Không áp dụng trong ứng dụng đã loại trừ.",
                    )),
                );
            },
        );
    }

    fn show_words_tab(&mut self, ui: &mut egui::Ui) {
        let editing = self.word_edit_index.is_some();
        section(
            ui,
            if editing {
                t("Edit word", "Sửa từ")
            } else {
                t("Add a word", "Thêm từ")
            },
            |ui| {
                description(
                    ui,
                    t(
                        "Per-word decisions about the English/Telex ambiguity. A word decided \
                         here stays decided, and wins over \"Restore common English words\" \
                         under Corrections.",
                        "Quyết định từng từ một cho những trường hợp trùng phím giữa tiếng Anh \
                         và Telex. Từ đã quyết định ở đây sẽ được giữ nguyên, và được ưu tiên \
                         hơn “Khôi phục từ tiếng Anh thông dụng” ở mục Sửa lỗi.",
                    ),
                );
                ui.add_space(6.0);
                control_row(ui, t("Word as typed", "Từ như đã gõ"), None, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.word_keys)
                            .desired_width(180.0)
                            .hint_text("cats"),
                    );
                });
                control_row(ui, t("Keep it as", "Giữ thành"), None, |ui| {
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
                });
                ui.horizontal(|ui| {
                    ui.set_min_height(ROW_HEIGHT);
                    ui.add_space(LABEL_COLUMN);
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
            },
        );

        let mut action: Option<(usize, bool)> = None;
        section(ui, t("Your words", "Danh sách từ riêng"), |ui| {
            if self.draft.word_overrides.is_empty() {
                description(ui, t("No words yet.", "Chưa có từ nào."));
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt("word_override_list")
                .max_height(240.0)
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
        section(ui, t("Version", "Phiên bản"), |ui| {
            list_row(ui, t("GlowKey", "GlowKey"), |ui| {
                ui.label(env!("CARGO_PKG_VERSION"));
            });
            // Set by the build; may be empty (a source build outside CI, or a
            // build where `git` isn't available) rather than absent, so this
            // never fails to compile — it just has nothing to show.
            let commit = option_env!("GLOWKEY_COMMIT").unwrap_or("");
            if !commit.is_empty() {
                list_row(ui, t("Build", "Bản dựng"), |ui| {
                    ui.label(commit);
                });
            }
        });

        section(ui, t("Known limit", "Giới hạn đã biết"), |ui| {
            description(
                ui,
                t(
                    "Windows blocks synthetic input across integrity levels. If this \
                     settings window is running elevated (\"Run as administrator\") while \
                     GlowKey's keyboard hook is not, or the other way around, the elevated \
                     program cannot receive the keystrokes the other one injects. Run \
                     GlowKey and that program at the same elevation.",
                    "Windows chặn phím giả lập giữa hai mức toàn vẹn khác nhau. Nếu cửa sổ \
                     này chạy với quyền quản trị (“Run as administrator”) còn móc bàn phím \
                     của GlowKey thì không, hoặc ngược lại, chương trình chạy quyền cao hơn \
                     sẽ không nhận được phím do bên kia gửi. Hãy chạy GlowKey và chương \
                     trình đó ở cùng một mức quyền.",
                ),
            );
        });
    }

    /// Dispatches to the pane's builder.
    fn show_tab(&mut self, ui: &mut egui::Ui) {
        match self.tab {
            Tab::General => self.show_general_tab(ui),
            Tab::Typing => self.show_typing_tab(ui),
            Tab::Corrections => self.show_corrections_tab(ui),
            Tab::Apps => self.show_apps_tab(ui),
            Tab::Macros => self.show_macros_tab(ui),
            Tab::Words => self.show_words_tab(ui),
            Tab::About => self.show_about_tab(ui),
        }
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }
}

impl SettingsApp {
    /// The whole window, one frame of it.
    ///
    /// Separate from [`eframe::App::update`] so a frame can be built against a
    /// bare [`egui::Context`] with no window, no GPU and no event loop — which
    /// is how the panes are smoke-tested. `eframe::Frame` is not used here and
    /// there is nothing else to keep the two together.
    fn ui(&mut self, ctx: &egui::Context) {
        // The OS/window-manager close (title-bar X, Alt+F4, …) also has to
        // decide what to hand back, exactly like the explicit Close button.
        if ctx.input(|i| i.viewport().close_requested()) {
            self.finalize();
        }

        egui::SidePanel::left("glowkey_settings_nav")
            .exact_width(SIDEBAR_WIDTH)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(egui::Margin::symmetric(10.0, 14.0)),
            )
            .show(ctx, |ui| {
                let color = ui.visuals().weak_text_color();
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("GlowKey").strong());
                });
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(t("Vietnamese input", "Bộ gõ tiếng Việt"))
                            .small()
                            .color(color),
                    );
                });
                ui.add_space(14.0);

                for (tab, title) in Tab::all() {
                    if nav_item(ui, self.tab == tab, title).clicked() {
                        self.tab = tab;
                    }
                    ui.add_space(2.0);
                }
            });

        egui::TopBottomPanel::bottom("glowkey_settings_bottom")
            .frame(egui::Frame::none().inner_margin(egui::Margin::symmetric(20.0, 12.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.set_min_height(ROW_HEIGHT);
                    let color = ui.visuals().weak_text_color();
                    ui.label(
                        egui::RichText::new(t(
                            "Changes are saved when you close this window.",
                            "Thay đổi được lưu khi bạn đóng cửa sổ này.",
                        ))
                        .small()
                        .color(color),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("Done", "Xong")).clicked() {
                            self.finalize();
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(ctx.style().visuals.window_fill)
                    .inner_margin(egui::Margin::symmetric(24.0, 18.0)),
            )
            .show(ctx, |ui| {
                let title = Tab::all()
                    .into_iter()
                    .find(|(tab, _)| *tab == self.tab)
                    .map_or("", |(_, title)| title);
                let color = ui.visuals().weak_text_color();
                ui.heading(title);
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(self.tab.subtitle())
                        .small()
                        .color(color),
                );

                egui::ScrollArea::vertical()
                    .id_salt("glowkey_settings_body")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.show_tab(ui);
                        ui.add_space(SECTION_GAP);
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

    /// Every pane must be reachable from the sidebar: the navigation *is* the
    /// list of panes now, so a pane missing from it is a pane with no way in.
    #[test]
    fn every_pane_has_a_sidebar_entry() {
        let tabs: Vec<Tab> = Tab::all().into_iter().map(|(tab, _)| tab).collect();
        for expected in [
            Tab::General,
            Tab::Typing,
            Tab::Corrections,
            Tab::Apps,
            Tab::Macros,
            Tab::Words,
            Tab::About,
        ] {
            assert!(
                tabs.contains(&expected),
                "{expected:?} is not in the sidebar"
            );
        }
        assert_eq!(tabs.len(), 7, "an entry was added without a pane");
    }

    /// Nothing in the sidebar may be blank, in either language: an empty row is
    /// a pane the user cannot name.
    #[test]
    fn sidebar_titles_and_subtitles_are_present() {
        for (tab, title) in Tab::all() {
            assert!(!title.trim().is_empty(), "{tab:?} has no title");
            assert!(!tab.subtitle().trim().is_empty(), "{tab:?} has no subtitle");
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

    /// Every pane must survive being built.
    ///
    /// A layout mistake in egui — a duplicate id, a widget allocated outside the
    /// space it was given — panics at build time, not at draw time, so a bare
    /// context with no window and no GPU is enough to catch it. This is not a
    /// test of how the window *looks*; it is a test that each pane can be built
    /// at all, including the ones a user only reaches by clicking through.
    #[test]
    fn every_pane_builds() {
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
        // A removed shipped default, so the tombstone section is built too.
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
            slot.borrow().is_none(),
            "building a pane must not decide the window is closing"
        );
    }
}
