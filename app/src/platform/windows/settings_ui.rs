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
//! with About and the three list editors (Excluded apps, Macros, Personal words)
//! as their own small windows opened from inside those tabs. This file
//! reproduces that, tab for tab and window for window, rather than inventing a
//! second layout; the `t(english, vietnamese)` pairs are copied verbatim from
//! the macOS source so the two interfaces cannot drift into naming the same
//! setting two different things. The separate windows are `egui::Window`
//! overlays — the nearest thing this toolkit has to an auxiliary window.
//!
//! One thing here is not decoration: the interface font is taken from the system
//! (see [`install_system_font`]), because egui's bundled font cannot draw
//! Vietnamese at all.

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

/// The size of each auxiliary window, matching its macOS counterpart:
/// `about_window.rs` and the three list windows in `prefs/`.
const ABOUT_SIZE: [f32; 2] = [340.0, 180.0];
const EXCLUDED_SIZE: [f32; 2] = [420.0, 380.0];
const MACROS_SIZE: [f32; 2] = [460.0, 400.0];
const WORDS_SIZE: [f32; 2] = [460.0, 400.0];

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
    // The default already is "follow the system"; saying so is cheap and makes
    // the intent visible next to the rest of the theming.
    ctx.set_theme(egui::ThemePreference::System);

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
    let color = ui.visuals().weak_text_color();
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
    /// The tab titles, in order. The same `t` pairs the macOS `NSTabView` uses.
    fn all() -> [(Self, &'static str); 4] {
        [
            (Self::General, t("General", "Chung")),
            (Self::Typing, t("Typing", "Gõ phím")),
            (Self::Corrections, t("Corrections", "Sửa lỗi")),
            (Self::Apps, t("Apps & macros", "Ứng dụng & gõ tắt")),
        ]
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

    // ----- The auxiliary windows, open or not -----
    about_open: bool,
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
            about_open: false,
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

    /// General: the interface language and what happens at launch. (The macOS
    /// tab also carries "Launch at login" and the toggle-hotkey picker; on
    /// Windows both live in the tray menu, which owns them.)
    fn general_tab(&mut self, ui: &mut egui::Ui) {
        // The picker applies immediately rather than at save, so the window is
        // in the chosen language before the user has to decide whether they
        // chose right. `set_language` is the same call the app makes at startup;
        // the value is returned and persisted like any other edit.
        let before = self.draft.language;
        control_row(ui, t("Language", "Ngôn ngữ"), None, |ui| {
            ui.radio_value(
                &mut self.draft.language,
                Language::System,
                t("System", "Hệ thống"),
            );
            ui.radio_value(&mut self.draft.language, Language::Vietnamese, "Tiếng Việt");
            ui.radio_value(&mut self.draft.language, Language::English, "English");
        });
        if self.draft.language != before {
            crate::strings::set_language(self.draft.language);
        }

        group_gap(ui);

        checkbox_row(
            ui,
            &mut self.draft.open_settings_at_launch,
            t("Open this window at launch", "Mở cửa sổ này khi khởi động"),
            Some(t(
                "On by default, so a new user finds the controls. GlowKey keeps \
                 running in the notification area either way.",
                "Mặc định bật, để người dùng mới tìm thấy các tuỳ chọn. Dù bật hay tắt, \
                 GlowKey vẫn chạy dưới khay hệ thống.",
            )),
        );

        group_gap(ui);

        if ui
            .button(t("About GlowKey", "Giới thiệu GlowKey"))
            .clicked()
        {
            self.about_open = true;
        }
    }

    /// Typing: how keys become Vietnamese.
    fn typing_tab(&mut self, ui: &mut egui::Ui) {
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

        group_gap(ui);

        checkbox_row(
            ui,
            &mut self.draft.quick_telex,
            t("Quick Telex", "Gõ tắt phụ âm"),
            Some(t(
                "A doubled consonant at the start of a syllable types its digraph: \
                 cc→ch, gg→gi, kk→kh, nn→ng, pp→ph, qq→qu, tt→th, uu→ư.",
                "Phụ âm gõ đôi ở đầu âm tiết cho ra phụ âm ghép: cc→ch, gg→gi, kk→kh, \
                 nn→ng, pp→ph, qq→qu, tt→th, uu→ư.",
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
    }

    /// Corrections: what happens when a word is not Vietnamese — and the
    /// Personal Words window, directly under the global switch it supersedes.
    fn corrections_tab(&mut self, ui: &mut egui::Ui) {
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
            &mut self.draft.auto_capitalize,
            t(
                "Auto-capitalize first letter of each sentence",
                "Tự động viết hoa chữ đầu câu",
            ),
            None,
        );

        group_gap(ui);

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
                 a different key order. Personal Words decides one word at a time \
                 instead, and wins over this.",
                "Mặc định tắt: “was” giữ nguyên “was”, nhưng mọi âm tiết trùng phím với từ \
                 trong danh sách (á→as, í→is, cát→cats, cả→car, hải→hair) sẽ phải gõ theo \
                 thứ tự khác. “Từ riêng” quyết định từng từ một, và được ưu tiên hơn.",
            )),
        );

        if ui.button(t("Personal Words…", "Từ riêng…")).clicked() {
            self.words_open = true;
        }
        caption(
            ui,
            t(
                "Decide a single word and it stays decided.",
                "Quyết định một từ và nó được giữ nguyên.",
            ),
        );
    }

    /// Apps & macros: the two list editors, and the one macro switch that is a
    /// setting rather than a list.
    fn apps_tab(&mut self, ui: &mut egui::Ui) {
        intro(
            ui,
            t(
                "Apps where GlowKey stays off — terminals and editors by default, so it \
                 never mangles a command.",
                "Những ứng dụng GlowKey luôn tắt — mặc định là terminal và trình soạn thảo, \
                 để không làm hỏng câu lệnh.",
            ),
        );
        ui.add_space(4.0);
        if ui
            .button(t("Manage Excluded Apps…", "Quản lý ứng dụng loại trừ…"))
            .clicked()
        {
            self.excluded_open = true;
        }

        group_gap(ui);

        intro(
            ui,
            t(
                "Text expansion (gõ tắt): type a shortcut then a space to expand it.",
                "Gõ tắt: gõ chữ viết tắt rồi dấu cách để bung ra.",
            ),
        );
        ui.add_space(4.0);
        if ui.button(t("Manage Macros…", "Quản lý gõ tắt…")).clicked() {
            self.macros_open = true;
        }

        group_gap(ui);

        checkbox_row(
            ui,
            &mut self.draft.always_macro,
            t(
                "Expand macros even when Vietnamese is off",
                "Bung gõ tắt cả khi đã tắt tiếng Việt",
            ),
            Some(t(
                "Never in an excluded app.",
                "Không áp dụng trong ứng dụng đã loại trừ.",
            )),
        );
    }

    // ----- The auxiliary windows -------------------------------------------

    /// About: name, version, commit and the one Windows limitation worth
    /// warning about. Mirrors `app/src/about_window.rs` — plain, centred, text
    /// only — with the elevation note this platform needs and macOS does not.
    fn about_body(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("GlowKey").heading().strong());

            // The version alone does not identify a build: GlowKey is installed
            // from a working tree as often as from a tag, so the commit is the
            // part that answers "which GlowKey are you running?". Set by the
            // build and possibly empty (a source build with no `git`), so this
            // never fails to compile — it just has nothing to add.
            let commit = option_env!("GLOWKEY_COMMIT").unwrap_or("");
            let build = if commit.is_empty() {
                env!("CARGO_PKG_VERSION").to_string()
            } else {
                format!("{} ({commit})", env!("CARGO_PKG_VERSION"))
            };
            let color = ui.visuals().weak_text_color();
            // Selectable, because this is the one string in the app someone is
            // ever asked to quote back, and retyping a commit hash by eye is how
            // the wrong build gets investigated.
            ui.add(
                egui::Label::new(
                    egui::RichText::new(t("Version {}", "Phiên bản {}").replace("{}", &build))
                        .small()
                        .color(color),
                )
                .selectable(true),
            );

            ui.add_space(6.0);
            ui.label(t(
                "Vietnamese Telex & VNI input for Windows",
                "Bộ gõ tiếng Việt Telex & VNI cho Windows",
            ));
            ui.label(
                egui::RichText::new(t(
                    "A UniKey-style input method, written entirely in Rust.",
                    "Bộ gõ kiểu UniKey, viết hoàn toàn bằng Rust.",
                ))
                .small()
                .color(color),
            );
        });

        ui.add_space(8.0);
        ui.separator();
        intro(
            ui,
            t(
                "Windows blocks synthetic input across integrity levels: if a program \
                 runs as administrator and GlowKey does not, it cannot receive the \
                 keystrokes GlowKey injects. Run both at the same elevation.",
                "Windows chặn phím giả lập giữa hai mức toàn vẹn: nếu một chương trình chạy \
                 với quyền quản trị còn GlowKey thì không, chương trình đó sẽ không nhận \
                 được phím do GlowKey gửi. Hãy chạy cả hai ở cùng một mức quyền.",
            ),
        );
    }

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
        match self.tab {
            Tab::General => self.general_tab(ui),
            Tab::Typing => self.typing_tab(ui),
            Tab::Corrections => self.corrections_tab(ui),
            Tab::Apps => self.apps_tab(ui),
        }
    }

    /// Every auxiliary window, drawn over the tabs.
    fn show_aux_windows(&mut self, ctx: &egui::Context) {
        let mut open = self.about_open;
        if open {
            aux_window(
                ctx,
                "glowkey_about",
                t("About GlowKey", "Giới thiệu GlowKey"),
                ABOUT_SIZE,
                false,
                &mut open,
                |ui| self.about_body(ui),
            );
            self.about_open = open;
        }

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
        // The OS/window-manager close (title-bar X, Alt+F4, …) also has to
        // decide what to hand back, exactly like the explicit Close button.
        if ctx.input(|i| i.viewport().close_requested()) {
            self.finalize();
        }

        egui::TopBottomPanel::top("glowkey_settings_tabs")
            .frame(egui::Frame::none().inner_margin(egui::Margin {
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

        egui::TopBottomPanel::bottom("glowkey_settings_bottom")
            .frame(egui::Frame::none().inner_margin(egui::Margin::symmetric(16.0, 10.0)))
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t("Done", "Xong")).clicked() {
                        self.finalize();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
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
        app.about_open = true;
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
            app.about_open && app.excluded_open && app.macros_open && app.words_open,
            "an auxiliary window closed itself"
        );
        assert!(
            slot.borrow().is_none(),
            "building a pane must not decide the window is closing"
        );
    }
}
