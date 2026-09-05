//! The Windows settings window (`egui`), drawn from the shared
//! `settings_spec` and hosted by the UI thread as a deferred viewport.
//!
//! GlowKey at rest is a keyboard hook, a message loop and a tray icon. The
//! renderer lives on its own thread (`ui_thread`, `decisions/0011`); this module
//! is the window's content. [`SettingsApp`] is handed a [`Settings`] snapshot,
//! [`SettingsApp::draw`] paints one frame, and when the user closes the window
//! [`SettingsApp::finalize`] decides what to hand back. Nothing here reaches the
//! session or the hook: values in, a value out.
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
//! macOS keeps it (`menu_bar.rs`); it is its own viewport (`about_ui`).
//!
//! Two things here are not decoration. The interface font is taken from the
//! system (see [`install_system_font`]), because egui's bundled font cannot draw
//! Vietnamese at all; and the light/dark choice is read from the registry (see
//! [`apply_theme`]), because the toolkit's own detection failed to resolve and
//! its fallback is dark — which is how a light-themed machine got a black
//! window.

use eframe::egui;

use glowkey_engine::{ExclusionList, HotkeyPreset, Macro, Settings, WordOverride, WordPreference};

use crate::settings_spec::{
    expand_shortcuts, hotkey_display, shortcut_display, Control, ListId, Row, TabSpec, Toggle,
    HOTKEY_PRESETS, MANAGE, TABS, WINDOW_TITLE,
};
use crate::strings::t;

/// The settings viewport, before it opens. Opened by `ui_thread` as a deferred
/// viewport, which is what lets it close and open again in one process.
pub fn viewport_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(WINDOW_TITLE.get())
        // The macOS window's content size, to the point (`app/src/prefs/tabs.rs`):
        // a settings window for a background utility is a small window. Points,
        // not pixels — winit reports the monitor's scale factor and egui
        // multiplies by it, so this is the same apparent size at 100% and 150%.
        .with_inner_size([460.0, 540.0])
        // Resizable, but not down to where the four tab titles stop fitting.
        .with_min_inner_size([420.0, 420.0])
        .with_resizable(true)
        .with_icon(window_icon())
}

/// A list editor's viewport, before it opens.
pub(super) fn list_viewport_builder(list: ListId) -> egui::ViewportBuilder {
    let (title, size) = match list {
        ListId::ExcludedApps => (t("Excluded Apps", "Ứng dụng loại trừ"), [380.0, 420.0]),
        ListId::Macros => (t("Macros", "Gõ tắt"), [420.0, 460.0]),
        ListId::PersonalWords => (t("Personal Words", "Từ riêng"), [420.0, 420.0]),
    };
    egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(size)
        .with_min_inner_size([320.0, 300.0])
        .with_resizable(true)
        .with_icon(window_icon())
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
const LABEL_COLUMN: f32 = 92.0; // a floor; see `label_column_width`
/// The width egui's checkbox glyph takes before its text, so a caption under a
/// checkbox starts under the text rather than under the box.
const CHECK_GLYPH: f32 = 18.0;
/// How far a dependent row sits inside the control column, under its parent.
const DEPENDENT_INDENT: f32 = 20.0;
/// Gap between the label column and the control column.
const COLUMN_GAP: f32 = 8.0;
/// Vertical rhythm, measured edge to edge: between a control and its caption,
/// between one row and the next, and before a section header. egui's own item
/// spacing is part of each figure, not added to it.
const CAPTION_GAP: f32 = 6.0;
const ROW_GAP: f32 = 10.0;
const GROUP_GAP: f32 = 18.0;

/// The application icon, for the title bar and the taskbar.
///
/// winit registers its own window class, so without this the window shows the
/// stock Windows application sheet even though the executable carries the icon
/// (`build.rs`). The PNG is a 64-pixel render of the same `AppIcon.ico`; the
/// decoder is the one eframe already ships for exactly this.
pub(super) fn window_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../../../Resources/AppIcon.png"))
        .unwrap_or_default()
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
pub(super) fn install_system_font(ctx: &egui::Context) -> bool {
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
pub(super) fn apply_style(ctx: &egui::Context) {
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

    // The macOS settings window is one flat grey, tabs and pane alike: light
    // 236/236/236, dark 40/40/40. egui's defaults are a near-white and a
    // near-black, and give the tab strip its own colour.
    //
    // Buttons and boxes are painted like macOS push buttons: a lighter fill
    // than the window with a hairline. egui's defaults are a grey two shades off
    // the window with no border, which on this grey is a button nobody can see.
    ctx.style_mut_of(egui::Theme::Light, |style| {
        let grey = egui::Color32::from_gray(236);
        style.visuals.window_fill = grey;
        style.visuals.panel_fill = grey;
        raise_controls(
            &mut style.visuals.widgets,
            egui::Color32::WHITE,
            egui::Color32::from_gray(246),
            egui::Color32::from_gray(222),
            egui::Color32::from_gray(200),
        );
    });
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        let grey = egui::Color32::from_gray(40);
        style.visuals.window_fill = grey;
        style.visuals.panel_fill = grey;
        raise_controls(
            &mut style.visuals.widgets,
            egui::Color32::from_gray(92),
            egui::Color32::from_gray(104),
            egui::Color32::from_gray(76),
            egui::Color32::from_gray(118),
        );
    });
}

/// Fills and hairline for the interactive widgets — buttons, checkboxes, text
/// fields, combo boxes — at rest, hovered and pressed.
fn raise_controls(
    widgets: &mut egui::style::Widgets,
    rest: egui::Color32,
    hovered: egui::Color32,
    pressed: egui::Color32,
    hairline: egui::Color32,
) {
    let stroke = egui::Stroke::new(1.0_f32, hairline);
    for (w, fill) in [
        (&mut widgets.inactive, rest),
        (&mut widgets.hovered, hovered),
        (&mut widgets.active, pressed),
    ] {
        w.weak_bg_fill = fill;
        w.bg_fill = fill;
        w.bg_stroke = stroke;
    }
}

/// The hotkeys the popup offers: the presets, minus Alt+Space (the system-menu
/// key on Windows; whether the hook wins that race is unverified), plus the
/// saved value when it is not among them — a combination recorded on a Mac
/// (there is no recorder here) or Alt+Space itself. The saved choice is shown,
/// never silently replaced by a preset.
fn hotkey_choices(current: HotkeyPreset) -> Vec<HotkeyPreset> {
    let mut offered: Vec<HotkeyPreset> = HOTKEY_PRESETS
        .into_iter()
        .filter(|p| *p != HotkeyPreset::OptionSpace)
        .collect();
    if !offered.contains(&current) {
        offered.push(current);
    }
    offered
}

/// A segmented control: one choice, every option visible, the chosen one raised.
///
/// The shape of `NSSegmentedControl` since Big Sur, which the macOS window uses
/// for every choice: a soft rounded track a shade darker than the window, the
/// selected segment lifted on it — white in light, a lighter grey in dark — with
/// a small shadow, no hairlines anywhere, and every label in the normal text
/// colour. Painted directly rather than through egui's selectable labels, whose
/// selected state draws a stroke and recolours the text, which is what made the
/// first version look like a row of bordered buttons.
///
/// Returns each segment's rectangle, in order; the tests click by position.
fn segmented<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut T,
    options: impl IntoIterator<Item = (T, String)>,
) -> Vec<egui::Rect> {
    const HEIGHT: f32 = 22.0;
    const PAD_X: f32 = 10.0;
    const INSET: f32 = 1.0;
    const ROUNDING: f32 = 6.0;

    let options: Vec<(T, String)> = options.into_iter().collect();
    let font = egui::TextStyle::Body.resolve(ui.style());
    let text_color = ui.visuals().text_color();
    let galleys: Vec<_> = options
        .iter()
        .map(|(_, label)| {
            ui.painter()
                .layout_no_wrap(label.clone(), font.clone(), text_color)
        })
        .collect();
    let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + 2.0 * PAD_X).collect();
    let total = egui::vec2(widths.iter().sum(), HEIGHT);
    let (track, _) = ui.allocate_exact_size(total, egui::Sense::hover());
    // One id per control from the auto-id sequence, so two controls in one
    // scope cannot answer each other's clicks.
    let base_id = ui.next_auto_id();
    ui.skip_ahead_auto_ids(1);

    let dark = ui.visuals().dark_mode;
    let (track_fill, raised_fill, hover_fill, shadow) = if dark {
        (
            egui::Color32::from_gray(58),
            egui::Color32::from_gray(105),
            egui::Color32::from_gray(66),
            egui::Color32::from_black_alpha(100),
        )
    } else {
        (
            egui::Color32::from_gray(220),
            egui::Color32::WHITE,
            egui::Color32::from_gray(228),
            egui::Color32::from_black_alpha(46),
        )
    };

    // Interact first, so a click is known before anything is painted this frame.
    let mut rects = Vec::with_capacity(options.len());
    let mut x = track.min.x;
    for (i, width) in widths.iter().enumerate() {
        let rect =
            egui::Rect::from_min_size(egui::pos2(x, track.min.y), egui::vec2(*width, HEIGHT));
        let response = ui.interact(rect, base_id.with(i), egui::Sense::click());
        if response.clicked() {
            *value = options[i].0;
        }
        rects.push((rect, response.hovered()));
        x += width;
    }

    let painter = ui.painter();
    painter.rect_filled(track, egui::Rounding::same(ROUNDING), track_fill);
    for (i, (rect, hovered)) in rects.iter().enumerate() {
        let inner = rect.shrink(INSET);
        if options[i].0 == *value {
            let raised = egui::epaint::Shadow {
                offset: egui::vec2(0.0, 1.0),
                blur: 3.0,
                spread: 0.0,
                color: shadow,
            };
            painter.add(raised.as_shape(inner, egui::Rounding::same(ROUNDING - INSET)));
            painter.rect_filled(inner, egui::Rounding::same(ROUNDING - INSET), raised_fill);
        } else if *hovered {
            painter.rect_filled(inner, egui::Rounding::same(ROUNDING - INSET), hover_fill);
        }
        let galley = &galleys[i];
        let pos = rect.center() - galley.size() / 2.0;
        painter.galley(pos, galley.clone(), text_color);
    }
    rects.into_iter().map(|(rect, _)| rect).collect()
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
pub(super) fn apply_theme(ctx: &egui::Context) {
    let light = crate::platform::windows::theme::apps_are_light();
    // Once per process, not per frame: this runs every frame of a context that
    // now lives as long as GlowKey, and a line per frame is not a diagnostic,
    // it is a way of hiding one.
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
/// A caption under a control, starting at `x` from the row's left edge — the
/// control column for a form row, the checkbox text for a checkbox row.
fn caption_at(ui: &mut egui::Ui, text: &str, x: f32) {
    egui::Frame::none()
        .inner_margin(egui::Margin {
            left: x,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
        })
        .show(ui, |ui| caption_rich(ui, text));
}

/// A caption with no inset: the introductory line at the top of a pane, which
/// explains the pane rather than a single control.
fn intro(ui: &mut egui::Ui, text: &str) {
    caption_rich(ui, text);
}

/// Caption text: small, secondary, wrapping. Shortcuts stay as words here;
/// keycaps inside running text inflate the line and break it oddly, so they are
/// reserved for the shortcut row.
fn caption_rich(ui: &mut egui::Ui, text: &str) {
    let color = secondary_color(ui);
    ui.label(egui::RichText::new(text).small().color(color));
}

/// Splits a shortcut spelling into its keys: "Ctrl+Shift+E" → Ctrl, Shift, E.
fn split_keys(shortcut: &str) -> Vec<&str> {
    shortcut.split('+').filter(|k| !k.is_empty()).collect()
}

/// A shortcut as a row of keycaps: each key in a small raised badge.
fn keycaps(ui: &mut egui::Ui, shortcut: &str) {
    let dark = ui.visuals().dark_mode;
    let (fill, hairline, ink) = if dark {
        (
            egui::Color32::from_gray(92),
            egui::Color32::from_gray(118),
            egui::Color32::from_gray(230),
        )
    } else {
        (
            egui::Color32::WHITE,
            egui::Color32::from_gray(190),
            egui::Color32::from_gray(40),
        )
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for key in split_keys(shortcut) {
            egui::Frame::none()
                .fill(fill)
                .stroke(egui::Stroke::new(1.0_f32, hairline))
                .rounding(egui::Rounding::same(4.0))
                .inner_margin(egui::Margin::symmetric(5.0, 1.0))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(key).small().color(ink));
                });
        }
    });
}

/// Ends a row: its caption, if any, at `caption_x`, then the gap to the next
/// row. The figures are edge to edge, so egui's item spacing is subtracted.
fn finish_row(ui: &mut egui::Ui, help: Option<&str>, caption_x: f32) {
    let spacing = ui.spacing().item_spacing.y;
    if let Some(text) = help {
        ui.add_space((CAPTION_GAP - spacing).max(0.0));
        caption_at(ui, text, caption_x);
    }
    ui.add_space((ROW_GAP - spacing).max(0.0));
}

/// A checkbox row, optionally with the caption that explains it.
///
/// In the control column, as the macOS form puts it: a checkbox is a control
/// whose label is its own text, so it starts where every other control starts
/// rather than at the pane's left edge. One axis for the eye to follow.
fn checkbox_row(ui: &mut egui::Ui, value: &mut bool, label: &str, help: Option<&str>) {
    let column = label_column_width(ui) + COLUMN_GAP;
    ui.horizontal(|ui| {
        ui.set_min_height(ROW_HEIGHT);
        ui.add_space(column);
        ui.checkbox(value, label);
    });
    finish_row(ui, help, column + CHECK_GLYPH);
}

/// A row with its label in a fixed left column and its control beside it, so
/// every picker in the window lines up on one edge — the macOS `form_row`.
fn control_row(
    ui: &mut egui::Ui,
    label: &str,
    help: Option<&str>,
    add: impl FnOnce(&mut egui::Ui),
) {
    let column = label_column_width(ui);
    ui.horizontal(|ui| {
        ui.set_min_height(ROW_HEIGHT);
        // Right-aligned against the control, as the macOS form is.
        ui.allocate_ui_with_layout(
            egui::vec2(column, ROW_HEIGHT),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.label(label);
            },
        );
        ui.add_space(COLUMN_GAP - ui.spacing().item_spacing.x);
        add(ui);
    });
    finish_row(ui, help, column + COLUMN_GAP);
}

/// The label column's width: wide enough for the longest label in the whole
/// window, in the current language and font, and never under
/// [`LABEL_COLUMN`].
///
/// A fixed column clipped "Toggle current app" to "ggle current app" — right
/// alignment cuts from the left. Measuring every frame costs twenty small
/// layouts and keeps the column right when the language changes mid-session.
fn label_column_width(ui: &egui::Ui) -> f32 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let widest = TABS
        .iter()
        .flat_map(|tab| tab.sections.iter())
        .flat_map(|section| section.rows.iter())
        .filter(|row| !matches!(row.control, Control::Checkbox(_)))
        .filter_map(|row| row.label)
        .map(|label| {
            ui.painter()
                .layout_no_wrap(label.get().to_string(), font.clone(), egui::Color32::BLACK)
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    (widest + 4.0).max(LABEL_COLUMN)
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
    ui.add_space((GROUP_GAP - ui.spacing().item_spacing.y).max(0.0));
}

/// A section title, in the macOS settings shape: bold, small, secondary.
fn section_header(ui: &mut egui::Ui, title: &str) {
    let color = secondary_color(ui);
    ui.label(egui::RichText::new(title).small().strong().color(color));
    ui.add_space(2.0);
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

pub(super) struct SettingsApp {
    /// The value passed in, kept verbatim so the final draft can be compared
    /// against it — the only way to know whether to return `None`.
    initial: Settings,
    /// The value being edited. Every control writes here directly.
    pub(super) draft: Settings,
    tab: Tab,
    /// `Some(None)` once the window has decided "closing, nothing to save";
    /// `Some(Some(settings))` once it has decided "closing, save this". Read
    /// once by the root through [`SettingsApp::take_result`].
    result: Option<Option<Settings>>,

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
    pub(super) fn new(initial: Settings) -> Self {
        let exclusion_list = initial.exclusion_list();
        Self {
            draft: initial.clone(),
            initial,
            tab: Tab::General,
            result: None,
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

    /// Decides what to hand back to the main thread and records it. Idempotent, so
    /// it is safe to call from both the window-close event and the explicit
    /// Close button without double-deciding.
    pub(super) fn finalize(&mut self) {
        if self.result.is_some() {
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
        self.result = Some(outcome);
    }

    /// The decision, once made. `Some(None)` is "closed, nothing changed".
    pub(super) fn take_result(&mut self) -> Option<Option<Settings>> {
        self.result.take()
    }

    /// The settings the window was opened on, in the form [`finalize`] compared
    /// the draft against: the baseline every edit is a diff against when the
    /// main thread merges it into the live session.
    ///
    /// Normalized, not raw. `finalize` rewrites the exclusion fields to their
    /// sorted effective form before comparing, so the value it returns always
    /// carries that form. Handing the merge the raw file order alongside it
    /// would read as "the user edited the exclusions" on every open, and the
    /// window's list would overwrite an app the tray excluded meanwhile.
    ///
    /// [`finalize`]: SettingsApp::finalize
    pub(super) fn baseline(&self) -> Settings {
        normalize_exclusions(&self.initial)
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
            DEPENDENT_INDENT
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
                    segmented(
                        ui,
                        &mut self.draft.language,
                        options
                            .iter()
                            .map(|(text, value)| (*value, text.get().to_string())),
                    );
                });
                if self.draft.language != before {
                    crate::strings::set_language(self.draft.language);
                }
            }
            Control::InputMethod(options) => {
                control_row(ui, label, caption_text, |ui| {
                    segmented(
                        ui,
                        &mut self.draft.input_method,
                        options
                            .iter()
                            .map(|(text, value)| (*value, text.get().to_string())),
                    );
                });
            }
            Control::ToneMarks(options) => {
                control_row(ui, label, caption_text, |ui| {
                    segmented(
                        ui,
                        &mut self.draft.style,
                        options
                            .iter()
                            .map(|(text, value)| (*value, text.get().to_string())),
                    );
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
                // A popup rather than a segmented control: "Ctrl+Shift+Space"
                // three times over does not fit a 460-point window, and the HIG
                // reserves segments for a few short labels. macOS keeps its
                // segmented glyphs; the spec does not care which.
                control_row(ui, label, caption_text, |ui| {
                    let current = self.draft.toggle_hotkey;
                    egui::ComboBox::from_id_salt("toggle_hotkey")
                        .selected_text(hotkey_display(current))
                        .width(190.0)
                        .show_ui(ui, |ui| {
                            for preset in hotkey_choices(current) {
                                ui.selectable_value(
                                    &mut self.draft.toggle_hotkey,
                                    preset,
                                    hotkey_display(preset),
                                );
                            }
                        });
                });
            }
            Control::Shortcut(shortcut) => {
                control_row(ui, label, caption_text, |ui| {
                    keycaps(ui, shortcut_display(shortcut));
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
                let unit = match list {
                    ListId::ExcludedApps => t("apps", "ứng dụng"),
                    ListId::Macros => t("macros", "gõ tắt"),
                    ListId::PersonalWords => t("words", "từ"),
                };
                let mut open = false;
                control_row(ui, label, caption_text, |ui| {
                    let color = secondary_color(ui);
                    ui.label(egui::RichText::new(format!("{count} {unit}")).color(color));
                    open = ui.button(MANAGE.get()).clicked();
                });
                if open {
                    self.set_list_open(list, true);
                    // Only the root asks for windows, and this runs inside the
                    // settings viewport: without a root repaint the flag would sit
                    // unread until something else happened to wake it.
                    ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
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
                // Leaves room for the import/export box under the list.
                .max_height((ui.available_height() - 170.0).max(120.0))
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
    /// Whether a list editor's window is open. The root asks for the viewport
    /// while this is true.
    pub(super) fn list_open(&self, list: ListId) -> bool {
        match list {
            ListId::ExcludedApps => self.excluded_open,
            ListId::Macros => self.macros_open,
            ListId::PersonalWords => self.words_open,
        }
    }

    pub(super) fn set_list_open(&mut self, list: ListId, open: bool) {
        match list {
            ListId::ExcludedApps => self.excluded_open = open,
            ListId::Macros => self.macros_open = open,
            ListId::PersonalWords => self.words_open = open,
        }
    }

    /// One frame of a list editor's window. Its own viewport, like About: it
    /// used to be an `egui::Window` overlay inside Settings, which covered the
    /// tabs, followed the user from tab to tab, and had no taskbar entry.
    pub(super) fn draw_list(&mut self, list: ListId, ctx: &egui::Context) {
        apply_theme(ctx);
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(ctx.style().visuals.window_fill)
                    .inner_margin(egui::Margin::symmetric(16.0, 12.0)),
            )
            .show(ctx, |ui| match list {
                ListId::ExcludedApps => self.excluded_body(ui),
                ListId::Macros => self.macros_body(ui),
                ListId::PersonalWords => self.words_body(ui),
            });
    }

    /// The whole window, one frame of it.
    ///
    /// Separate from [`eframe::App::update`] so a frame can be built against a
    /// bare [`egui::Context`] with no window, no GPU and no event loop — which
    /// is how the tabs are smoke-tested. `eframe::Frame` is not used here and
    /// there is nothing else to keep the two together.
    /// One frame of the window.
    pub(super) fn draw(&mut self, ctx: &egui::Context) {
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
            .frame(
                egui::Frame::none()
                    .fill(ctx.style().visuals.window_fill)
                    .inner_margin(egui::Margin {
                        left: 10.0,
                        right: 10.0,
                        top: 12.0,
                        bottom: 4.0,
                    }),
            )
            .show(ctx, |ui| {
                // A centred segmented control on the pane's own grey, which is
                // what an `NSTabView` draws.
                ui.vertical_centered(|ui| {
                    segmented(
                        ui,
                        &mut self.tab,
                        Tab::all().map(|(tab, title)| (tab, title.to_string())),
                    );
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
        let mut app = SettingsApp::new(settings.clone());
        for tab in [Tab::General, Tab::Corrections] {
            app.tab = tab;
            for _ in 0..2 {
                let _ = ctx.run(egui::RawInput::default(), |ctx| app.draw(ctx));
            }
        }
        // Rendering must not have edited anything: the foreign hotkey is shown,
        // not replaced.
        assert_eq!(app.draft, settings);
        assert!(app.take_result().is_none());
    }

    /// A light system gets a light window.
    ///
    /// This is the regression: the window asked the toolkit to follow the system
    /// theme, the toolkit could not tell, and its fallback is dark — so a
    /// machine with `AppsUseLightTheme = 1` got a black window with a black Done
    /// button. The choice is now made here, from the registry value, and there
    /// is no third answer to fall back to.
    /// The embedded PNG must decode, or the window silently falls back to the
    /// stock sheet — the very thing this exists to replace.
    #[test]
    fn the_window_icon_decodes() {
        let icon = window_icon();
        assert_eq!((icon.width, icon.height), (64, 64));
        assert_eq!(icon.rgba.len(), 64 * 64 * 4);
    }

    /// One rect per option, and a click on a segment selects its option.
    #[test]
    fn a_segment_click_selects_its_option() {
        let ctx = egui::Context::default();
        apply_style(&ctx);
        let options = || {
            [
                (0u8, "One".to_string()),
                (1, "Two".to_string()),
                (2, "Three".to_string()),
            ]
        };
        let mut value = 0u8;
        let mut rects: Vec<egui::Rect> = Vec::new();
        let frame = |ctx: &egui::Context, value: &mut u8, rects: &mut Vec<egui::Rect>| {
            egui::Area::new(egui::Id::new("segmented_test"))
                .fixed_pos(egui::pos2(0.0, 0.0))
                .show(ctx, |ui| {
                    *rects = segmented(ui, value, options());
                });
        };
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            frame(ctx, &mut value, &mut rects)
        });
        assert_eq!(rects.len(), 3);
        assert!(rects[1].min.x >= rects[0].max.x - 0.01, "segments overlap");

        // egui hit-tests a press against where the pointer already was, so the
        // move lands in its own frame before the press.
        let target = rects[1].center();
        let mut moved = egui::RawInput::default();
        moved.events.push(egui::Event::PointerMoved(target));
        let _ = ctx.run(moved, |ctx| frame(ctx, &mut value, &mut rects));
        let mut press = egui::RawInput::default();
        press.events.push(egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        let _ = ctx.run(press, |ctx| frame(ctx, &mut value, &mut rects));
        let mut release = egui::RawInput::default();
        release.events.push(egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        let _ = ctx.run(release, |ctx| frame(ctx, &mut value, &mut rects));
        assert_eq!(value, 1, "the second segment was clicked");
    }

    #[test]
    fn keycaps_split_on_plus() {
        assert_eq!(split_keys("Ctrl+Shift+E"), vec!["Ctrl", "Shift", "E"]);
        assert_eq!(
            split_keys("Ctrl+Shift+Space"),
            vec!["Ctrl", "Shift", "Space"]
        );
    }

    /// The popup never offers Alt+Space, and never hides what the file holds.
    #[test]
    fn the_hotkey_popup_offers_presets_and_the_saved_value() {
        let plain = hotkey_choices(HotkeyPreset::CtrlSpace);
        assert!(!plain.contains(&HotkeyPreset::OptionSpace));
        assert_eq!(plain.len(), 3);
        let saved = hotkey_choices(HotkeyPreset::OptionSpace);
        assert_eq!(saved.last(), Some(&HotkeyPreset::OptionSpace));
        let custom = HotkeyPreset::Custom {
            control: true,
            shift: false,
            option: true,
            key_char: 'k',
            macos_keycode: None,
            windows_vk: None,
        };
        assert_eq!(hotkey_choices(custom).last(), Some(&custom));
    }

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
        let mut app = SettingsApp::new(settings);
        for list in ListId::ALL {
            app.set_list_open(list, true);
        }
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
                let _ = ctx.run(egui::RawInput::default(), |ctx| app.draw(ctx));
            }
        }

        // Each list editor draws in its own window.
        for list in ListId::ALL {
            for _ in 0..2 {
                let _ = ctx.run(egui::RawInput::default(), |ctx| app.draw_list(list, ctx));
            }
            assert!(app.list_open(list), "a list window closed itself");
        }
        assert!(
            app.take_result().is_none(),
            "building a pane must not decide the window is closing"
        );
    }
}
