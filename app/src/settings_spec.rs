//! The settings window, described once.
//!
//! Every platform draws the same four tabs, and until this module each drew
//! them from its own copy of the row list — which is how the same checkbox came
//! to be "Launch GlowKey at login" on macOS and "Start at login" on Windows, and
//! how one caption opened "The blunt version:" on one platform and "Off by
//! default:" on the other. The layout is data here; `prefs/tabs.rs` (AppKit) and
//! `platform/windows/settings_ui.rs` (egui) are the two renderers of it.
//!
//! What lives here: tabs, sections, rows, the control each row is, the setting
//! it binds to, its label and caption in both languages, and which other setting
//! it depends on. What does not: colours, fonts, point sizes, wrapping widths,
//! window lifetime. Those belong to the toolkit doing the drawing, and the two
//! toolkits are deliberately different about them (native controls on macOS, a
//! themed egui window elsewhere).
//!
//! Nothing here touches AppKit, egui or Win32, so the module compiles on any
//! target and its tests run on all of them.

use glowkey_engine::{HotkeyPreset, InputMethod, Language, PlacementStyle, Settings};

use crate::strings::t;

// ---------------------------------------------------------------------------
// Vocabulary
// ---------------------------------------------------------------------------

/// A user-visible string in both interface languages.
///
/// Kept as a pair rather than resolved at definition time because the language
/// is a runtime setting: the window rebuilds in the other language the moment
/// the user changes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Text {
    pub en: &'static str,
    pub vi: &'static str,
}

impl Text {
    pub const fn new(en: &'static str, vi: &'static str) -> Self {
        Self { en, vi }
    }

    /// The string for the active interface language.
    #[must_use]
    pub fn get(&self) -> &'static str {
        t(self.en, self.vi)
    }
}

/// A keyboard shortcut named inside a caption.
///
/// Captions used to spell shortcuts out — `⌃⇧E` — which is wrong on Windows,
/// where the same key is Ctrl+Shift+E; the Windows copy simply dropped the
/// sentence. A caption now carries a placeholder (`{toggle_app}`,
/// `{fix_word}`) and the renderer substitutes the platform's spelling through
/// [`expand_shortcuts`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    /// Turns GlowKey off or on in the app in front.
    ToggleApp,
    /// Fixes the word just typed and remembers the choice.
    FixWord,
}

impl Shortcut {
    /// The placeholder as it appears inside a [`Text`].
    const fn placeholder(self) -> &'static str {
        match self {
            Self::ToggleApp => "{toggle_app}",
            Self::FixWord => "{fix_word}",
        }
    }

    const ALL: [Self; 2] = [Self::ToggleApp, Self::FixWord];
}

/// Replaces every shortcut placeholder in `text` with `display(shortcut)`.
#[must_use]
pub fn expand_shortcuts(text: &str, display: impl Fn(Shortcut) -> String) -> String {
    let mut out = text.to_string();
    for shortcut in Shortcut::ALL {
        if out.contains(shortcut.placeholder()) {
            out = out.replace(shortcut.placeholder(), &display(shortcut));
        }
    }
    out
}

/// The platform's spelling of a fixed shortcut.
///
/// These two are not configurable, so their spelling is a constant per
/// platform. The configurable one goes through [`hotkey_display`].
#[must_use]
pub fn shortcut_display(shortcut: Shortcut) -> &'static str {
    match shortcut {
        Shortcut::ToggleApp => key_glyphs().ctrl_shift_e,
        Shortcut::FixWord => key_glyphs().ctrl_shift_w,
    }
}

/// A boolean setting a checkbox binds to.
///
/// One variant per checkbox in the window. [`Toggle::settings_field`] maps a
/// variant to its `Settings` member; [`Toggle::LaunchAtLogin`] has no member
/// because it is operating-system state (a login item on macOS, a Run key on
/// Windows), and the renderer reads and writes it through its own platform
/// call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Toggle {
    LaunchAtLogin,
    OpenSettingsAtLaunch,
    QuickTelex,
    TelexBrackets,
    AutoFix,
    StrictSpellCheck,
    AutoCapitalize,
    RestoreEnglishWords,
    AlwaysMacro,
}

impl Toggle {
    /// The `Settings` member this toggle edits, or `None` for platform state.
    ///
    /// The Windows renderer edits a `Settings` draft through this. macOS edits
    /// the live session through `TapState` setters instead, so it has no call.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub fn settings_field(self, settings: &mut Settings) -> Option<&mut bool> {
        Some(match self {
            Self::LaunchAtLogin => return None,
            Self::OpenSettingsAtLaunch => &mut settings.open_settings_at_launch,
            Self::QuickTelex => &mut settings.quick_telex,
            Self::TelexBrackets => &mut settings.telex_brackets,
            Self::AutoFix => &mut settings.auto_fix,
            Self::StrictSpellCheck => &mut settings.strict_spell_check,
            Self::AutoCapitalize => &mut settings.auto_capitalize,
            Self::RestoreEnglishWords => &mut settings.restore_english_words,
            Self::AlwaysMacro => &mut settings.always_macro,
        })
    }

    /// Every toggle, for the test that checks each is placed exactly once.
    #[cfg(test)]
    const ALL: [Self; 9] = [
        Self::LaunchAtLogin,
        Self::OpenSettingsAtLaunch,
        Self::QuickTelex,
        Self::TelexBrackets,
        Self::AutoFix,
        Self::StrictSpellCheck,
        Self::AutoCapitalize,
        Self::RestoreEnglishWords,
        Self::AlwaysMacro,
    ];
}

/// One of the three lists that open in their own window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ListId {
    ExcludedApps,
    Macros,
    PersonalWords,
}

impl ListId {
    #[cfg(test)]
    const ALL: [Self; 3] = [Self::ExcludedApps, Self::Macros, Self::PersonalWords];
}

/// What a row is.
///
/// The three choice controls are typed rather than sharing an index-based
/// variant so that the mapping from segment to value is written once, here,
/// and neither renderer can get it off by one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Language(&'static [(Text, Language)]),
    InputMethod(&'static [(Text, InputMethod)]),
    ToneMarks(&'static [(Text, PlacementStyle)]),
    Checkbox(Toggle),
    /// The configurable Vietnamese/English hotkey: the presets in
    /// [`HOTKEY_PRESETS`]. macOS appends its recorder as "Custom…"; Windows has
    /// no recorder and shows the presets alone.
    ToggleHotkey,
    /// A fixed shortcut, displayed read-only.
    Shortcut(Shortcut),
    /// A count and a "Manage…" button that opens the list's window. The
    /// renderer supplies the count from its own live data: on Windows the edit
    /// draft, on macOS the running session.
    List(ListId),
}

/// One row of a section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    /// The label-column text. `None` for a checkbox, whose title is its label.
    pub label: Option<Text>,
    pub control: Control,
    /// One sentence under the control. Doubles as the control's accessibility
    /// help, so a screen reader hears the same explanation a sighted user reads.
    pub caption: Option<Text>,
    /// A toggle this row only means something under. The renderer indents the
    /// row and disables it while that toggle is off.
    pub enabled_when: Option<Toggle>,
}

impl Row {
    const fn new(control: Control) -> Self {
        Self {
            label: None,
            control,
            caption: None,
            enabled_when: None,
        }
    }
    const fn label(mut self, en: &'static str, vi: &'static str) -> Self {
        self.label = Some(Text::new(en, vi));
        self
    }
    const fn caption(mut self, en: &'static str, vi: &'static str) -> Self {
        self.caption = Some(Text::new(en, vi));
        self
    }
    const fn enabled_when(mut self, toggle: Toggle) -> Self {
        self.enabled_when = Some(toggle);
        self
    }
}

/// A titled group of rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Section {
    pub title: Text,
    pub rows: &'static [Row],
}

/// One tab of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabSpec {
    pub title: Text,
    pub sections: &'static [Section],
}

// ---------------------------------------------------------------------------
// The window
// ---------------------------------------------------------------------------

/// The window title.
pub const WINDOW_TITLE: Text = Text::new("GlowKey Settings", "Cài đặt GlowKey");

/// The button on a [`Control::List`] row.
pub const MANAGE: Text = Text::new("Manage…", "Quản lý…");

/// The presets the hotkey picker offers, in display order.
pub const HOTKEY_PRESETS: [HotkeyPreset; 4] = [
    HotkeyPreset::CtrlShiftSpace,
    HotkeyPreset::CtrlSpace,
    HotkeyPreset::OptionSpace,
    HotkeyPreset::CtrlShiftZ,
];

/// The language picker's segments, in order. Handlers index this by segment.
pub const LANGUAGES: &[(Text, Language)] = &[
    (Text::new("System", "Hệ thống"), Language::System),
    (Text::new("Tiếng Việt", "Tiếng Việt"), Language::Vietnamese),
    (Text::new("English", "English"), Language::English),
];

/// The input-method picker's segments, in order.
pub const INPUT_METHODS: &[(Text, InputMethod)] = &[
    (Text::new("Telex", "Telex"), InputMethod::Telex),
    (Text::new("VNI", "VNI"), InputMethod::Vni),
    (
        Text::new("Simple Telex", "Telex đơn giản"),
        InputMethod::SimpleTelex,
    ),
];

// The example is the label: a user picks the one that looks right, without
// having to know what "modern" means.
/// The tone-mark picker's segments, in order.
pub const TONE_MARKS: &[(Text, PlacementStyle)] = &[
    (
        Text::new("Modern  hoà", "Kiểu mới  hoà"),
        PlacementStyle::New,
    ),
    (
        Text::new("Classic  hòa", "Kiểu cũ  hòa"),
        PlacementStyle::Old,
    ),
];

const GENERAL: &[Section] = &[
    Section {
        title: Text::new("Interface", "Giao diện"),
        rows: &[Row::new(Control::Language(LANGUAGES)).label("Language", "Ngôn ngữ")],
    },
    Section {
        title: Text::new("Startup", "Khởi động"),
        rows: &[
            Row::new(Control::Checkbox(Toggle::LaunchAtLogin))
                .label("Launch GlowKey at login", "Khởi động GlowKey cùng máy"),
            Row::new(Control::Checkbox(Toggle::OpenSettingsAtLaunch))
                .label("Open this window at launch", "Mở cửa sổ này khi khởi động"),
        ],
    },
    Section {
        title: Text::new("Keyboard", "Bàn phím"),
        rows: &[
            Row::new(Control::ToggleHotkey).label("Toggle Vietnamese", "Chuyển tiếng Việt"),
            Row::new(Control::Shortcut(Shortcut::ToggleApp))
                .label("Toggle current app", "Bật tắt ứng dụng này")
                .caption(
                    "Turns GlowKey off or on in the app in front.",
                    "Tắt hoặc bật GlowKey trong ứng dụng đang mở.",
                ),
        ],
    },
];

const TYPING: &[Section] = &[
    Section {
        title: Text::new("Method", "Kiểu gõ"),
        rows: &[
            Row::new(Control::InputMethod(INPUT_METHODS)).label("Input method", "Kiểu gõ"),
            Row::new(Control::ToneMarks(TONE_MARKS)).label("Tone marks", "Dấu thanh"),
        ],
    },
    Section {
        title: Text::new("Telex extras", "Telex mở rộng"),
        rows: &[
            Row::new(Control::Checkbox(Toggle::QuickTelex))
                .label("Quick Telex", "Gõ tắt phụ âm")
                .caption(
                    "Double a consonant to type its pair: cc→ch, nn→ng, uu→ư.",
                    "Gõ đôi phụ âm để ra phụ âm ghép: cc→ch, nn→ng, uu→ư.",
                ),
            Row::new(Control::Checkbox(Toggle::TelexBrackets))
                .label("Telex bracket shortcuts", "Phím ngoặc kiểu Telex")
                .caption(
                    "[ ] { } type ơ ư Ơ Ư and never reach the app.",
                    "[ ] { } gõ ra ơ ư Ơ Ư và không đến ứng dụng.",
                ),
        ],
    },
];

const CORRECTIONS: &[Section] = &[
    Section {
        title: Text::new("Auto-fix", "Tự sửa"),
        rows: &[
            Row::new(Control::Checkbox(Toggle::AutoFix))
                .label(
                    "Auto-fix non-Vietnamese words",
                    "Tự động khôi phục từ không phải tiếng Việt",
                )
                .caption("Types “exit”, not “eĩt”.", "Gõ ra “exit”, không phải “eĩt”."),
            Row::new(Control::Checkbox(Toggle::StrictSpellCheck))
                .label("Fix as I type, not at the space", "Sửa ngay khi gõ, không đợi dấu cách")
                .caption(
                    "Repairs at the first impossible letter instead of at the space.",
                    "Sửa ngay ở chữ đầu tiên không hợp lệ, không đợi dấu cách.",
                )
                .enabled_when(Toggle::AutoFix),
            Row::new(Control::Checkbox(Toggle::AutoCapitalize))
                .label("Auto-capitalize sentences", "Tự viết hoa đầu câu"),
        ],
    },
    Section {
        title: Text::new("English words", "Từ tiếng Anh"),
        rows: &[
            Row::new(Control::Checkbox(Toggle::RestoreEnglishWords))
                .label("Restore common English words", "Khôi phục từ tiếng Anh thông dụng")
                .caption(
                    "“was” stays “was”. Off by default; some Vietnamese syllables then need a different key order.",
                    "“was” giữ nguyên “was”. Mặc định tắt; một số âm tiết tiếng Việt sẽ phải gõ theo thứ tự khác.",
                ),
            Row::new(Control::List(ListId::PersonalWords))
                .label("Personal words", "Từ riêng")
                .caption(
                    "Words you have decided about. Press {fix_word} right after typing one to fix it and remember.",
                    "Những từ bạn đã quyết định. Bấm {fix_word} ngay sau khi gõ để sửa và ghi nhớ.",
                ),
        ],
    },
];

const APPS: &[Section] = &[
    Section {
        title: Text::new("Apps", "Ứng dụng"),
        rows: &[Row::new(Control::List(ListId::ExcludedApps))
            .label("Excluded apps", "Ứng dụng loại trừ")
            .caption(
                "GlowKey types plain keys here. Terminals and editors by default. {toggle_app} toggles the app in front.",
                "GlowKey gõ phím thường ở đây. Mặc định là terminal và trình soạn thảo. {toggle_app} bật tắt ứng dụng đang mở.",
            )],
    },
    Section {
        title: Text::new("Macros", "Gõ tắt"),
        rows: &[
            Row::new(Control::List(ListId::Macros))
                .label("Macros", "Gõ tắt")
                .caption(
                    "Type a shortcut then a space to expand it: vn → Việt Nam.",
                    "Gõ chữ viết tắt rồi dấu cách để bung ra: vn → Việt Nam.",
                ),
            Row::new(Control::Checkbox(Toggle::AlwaysMacro))
                .label(
                    "Expand macros even when Vietnamese is off",
                    "Bung gõ tắt cả khi đã tắt tiếng Việt",
                )
                .caption("Never in an excluded app.", "Không áp dụng trong ứng dụng đã loại trừ."),
        ],
    },
];

/// The four tabs, in order.
pub const TABS: [TabSpec; 4] = [
    TabSpec {
        title: Text::new("General", "Chung"),
        sections: GENERAL,
    },
    TabSpec {
        title: Text::new("Typing", "Gõ phím"),
        sections: TYPING,
    },
    TabSpec {
        title: Text::new("Corrections", "Sửa lỗi"),
        sections: CORRECTIONS,
    },
    TabSpec {
        title: Text::new("Apps & macros", "Ứng dụng & gõ tắt"),
        sections: APPS,
    },
];

// ---------------------------------------------------------------------------
// Shortcut spelling
// ---------------------------------------------------------------------------

/// How this platform writes modifier keys.
struct KeyGlyphs {
    control: &'static str,
    option: &'static str,
    shift: &'static str,
    /// Separator between a modifier and what follows: none on macOS (`⌃⇧Z`),
    /// `+` on Windows (`Ctrl+Shift+Z`).
    joiner: &'static str,
    ctrl_shift_e: &'static str,
    ctrl_shift_w: &'static str,
}

#[cfg(target_os = "macos")]
const fn key_glyphs() -> KeyGlyphs {
    KeyGlyphs {
        control: "⌃",
        option: "⌥",
        shift: "⇧",
        joiner: "",
        ctrl_shift_e: "⌃⇧E",
        ctrl_shift_w: "⌃⇧W",
    }
}

#[cfg(not(target_os = "macos"))]
const fn key_glyphs() -> KeyGlyphs {
    KeyGlyphs {
        control: "Ctrl",
        option: "Alt",
        shift: "Shift",
        joiner: "+",
        ctrl_shift_e: "Ctrl+Shift+E",
        ctrl_shift_w: "Ctrl+Shift+W",
    }
}

/// A human-readable rendering of a toggle-hotkey preset ("⌃⇧Space" on macOS,
/// "Ctrl+Shift+Space" on Windows).
///
/// One renderer for every place that shows the hotkey — the picker, the menu,
/// the welcome guide — so they cannot disagree, and cannot go stale the way a
/// literal `⌃⇧Space` did the moment the hotkey became configurable.
#[must_use]
pub fn hotkey_display(preset: HotkeyPreset) -> String {
    let g = key_glyphs();
    let (control, option, shift, key) = match preset {
        HotkeyPreset::CtrlShiftSpace => (true, false, true, ' '),
        HotkeyPreset::CtrlSpace => (true, false, false, ' '),
        HotkeyPreset::OptionSpace => (false, true, false, ' '),
        HotkeyPreset::CtrlShiftZ => (true, false, true, 'Z'),
        HotkeyPreset::Custom {
            control,
            shift,
            option,
            key_char,
            ..
        } => (control, option, shift, key_char),
    };
    let mut parts: Vec<&str> = Vec::with_capacity(4);
    if control {
        parts.push(g.control);
    }
    if option {
        parts.push(g.option);
    }
    if shift {
        parts.push(g.shift);
    }
    let key = if key == ' ' {
        "Space".to_string()
    } else {
        key.to_uppercase().to_string()
    };
    parts.push(&key);
    parts.join(g.joiner)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> impl Iterator<Item = &'static Row> {
        TABS.iter()
            .flat_map(|tab| tab.sections.iter())
            .flat_map(|section| section.rows.iter())
    }

    fn texts() -> Vec<Text> {
        let mut out = vec![WINDOW_TITLE, MANAGE];
        for tab in &TABS {
            out.push(tab.title);
            for section in tab.sections {
                out.push(section.title);
                for row in section.rows {
                    out.extend(row.label);
                    out.extend(row.caption);
                    match row.control {
                        Control::Language(opts) => out.extend(opts.iter().map(|(t, _)| *t)),
                        Control::InputMethod(opts) => out.extend(opts.iter().map(|(t, _)| *t)),
                        Control::ToneMarks(opts) => out.extend(opts.iter().map(|(t, _)| *t)),
                        _ => {}
                    }
                }
            }
        }
        out
    }

    /// The tab strip the user learned, in the order they learned it.
    #[test]
    fn four_tabs_with_named_non_empty_sections() {
        assert_eq!(TABS.len(), 4);
        for tab in &TABS {
            assert!(!tab.sections.is_empty(), "{} has no sections", tab.title.en);
            for section in tab.sections {
                assert!(
                    !section.rows.is_empty(),
                    "{} has an empty section",
                    tab.title.en
                );
            }
        }
    }

    /// Both languages, always, and the renderer owns line breaks — a `\n` in a
    /// string wraps at a width chosen for one toolkit and looks wrong in the
    /// other.
    #[test]
    fn every_text_has_both_languages_and_no_hard_breaks() {
        for text in texts() {
            assert!(
                !text.en.trim().is_empty(),
                "empty English for {:?}",
                text.vi
            );
            assert!(
                !text.vi.trim().is_empty(),
                "empty Vietnamese for {:?}",
                text.en
            );
            assert!(!text.en.contains('\n'), "hard break in {:?}", text.en);
            assert!(!text.vi.contains('\n'), "hard break in {:?}", text.vi);
        }
    }

    /// Captions are one sentence with at most one example, not a paragraph.
    #[test]
    fn captions_are_short() {
        for row in rows() {
            for text in row.caption.iter().flat_map(|c| [c.en, c.vi]) {
                // Vietnamese runs longer than English and is the one that
                // overflows a pane, so both are held to the same limit.
                assert!(text.chars().count() <= 120, "caption too long: {text:?}");
            }
        }
    }

    /// Every setting has exactly one home. Twice is two places to disagree;
    /// zero is a setting the user cannot reach.
    #[test]
    fn every_toggle_and_list_is_placed_exactly_once() {
        for toggle in Toggle::ALL {
            let n = rows()
                .filter(|r| r.control == Control::Checkbox(toggle))
                .count();
            assert_eq!(n, 1, "{toggle:?} placed {n} times");
        }
        for list in ListId::ALL {
            let n = rows().filter(|r| r.control == Control::List(list)).count();
            assert_eq!(n, 1, "{list:?} placed {n} times");
        }
        assert_eq!(
            rows()
                .filter(|r| r.control == Control::ToggleHotkey)
                .count(),
            1
        );
    }

    /// A row that depends on a toggle sits after it, in the same section, so
    /// the indent reads as "under".
    #[test]
    fn dependent_rows_follow_their_parent_in_the_same_section() {
        for tab in &TABS {
            for section in tab.sections {
                for (i, row) in section.rows.iter().enumerate() {
                    if let Some(parent) = row.enabled_when {
                        let parent_index = section.rows[..i]
                            .iter()
                            .position(|r| r.control == Control::Checkbox(parent));
                        assert!(
                            parent_index.is_some(),
                            "{:?} depends on {parent:?}, which is not above it",
                            row.control
                        );
                    }
                }
            }
        }
    }

    /// Choice controls carry every variant, so no value is unreachable and no
    /// saved value fails to select a segment.
    #[test]
    fn choice_controls_cover_every_variant() {
        let langs: Vec<Language> = LANGUAGES.iter().map(|(_, v)| *v).collect();
        assert_eq!(
            langs,
            [Language::System, Language::Vietnamese, Language::English]
        );
        let methods: Vec<InputMethod> = INPUT_METHODS.iter().map(|(_, v)| *v).collect();
        assert_eq!(
            methods,
            [
                InputMethod::Telex,
                InputMethod::Vni,
                InputMethod::SimpleTelex
            ]
        );
        let tones: Vec<PlacementStyle> = TONE_MARKS.iter().map(|(_, v)| *v).collect();
        assert_eq!(tones, [PlacementStyle::New, PlacementStyle::Old]);
    }

    /// Every placeholder a caption uses is one the renderer knows how to fill,
    /// and nothing is left in braces afterwards.
    #[test]
    fn shortcut_placeholders_all_expand() {
        let mut seen_any = false;
        for row in rows() {
            for text in row.caption.iter().flat_map(|c| [c.en, c.vi]) {
                let expanded = expand_shortcuts(text, |s| format!("<{s:?}>"));
                seen_any |= expanded != text;
                // Not "no braces": the bracket-shortcuts caption names `{ }`
                // as keys. Only the two placeholders must be gone.
                for shortcut in Shortcut::ALL {
                    assert!(
                        !expanded.contains(shortcut.placeholder()),
                        "unexpanded placeholder in {text:?}"
                    );
                }
            }
        }
        assert!(
            seen_any,
            "no caption names a shortcut; the mechanism is dead"
        );
        assert_eq!(
            expand_shortcuts("a {toggle_app} b", |_| "X".into()),
            "a X b"
        );
    }

    /// Both settings-backed toggles and the platform one resolve as declared.
    #[test]
    fn toggles_bind_to_their_settings_fields() {
        let mut settings = Settings::default();
        assert!(Toggle::LaunchAtLogin
            .settings_field(&mut settings)
            .is_none());
        for toggle in Toggle::ALL {
            if toggle == Toggle::LaunchAtLogin {
                continue;
            }
            let before = settings.clone();
            let field = toggle
                .settings_field(&mut settings)
                .expect("settings-backed toggle");
            *field = !*field;
            assert_ne!(settings, before, "{toggle:?} does not edit any field");
            settings = before;
        }
    }

    /// The picker's spelling of each preset is non-empty and distinct, and a
    /// recorded custom combination renders its own keys.
    #[test]
    fn hotkey_display_names_every_preset_distinctly() {
        let names: Vec<String> = HOTKEY_PRESETS.iter().map(|p| hotkey_display(*p)).collect();
        for name in &names {
            assert!(!name.is_empty());
            assert!(name.contains("Space") || name.ends_with('Z'), "{name}");
        }
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "two presets display alike");

        let custom = hotkey_display(HotkeyPreset::Custom {
            control: true,
            shift: false,
            option: true,
            key_char: 'k',
            macos_keycode: None,
            windows_vk: None,
        });
        assert!(custom.ends_with('K'), "{custom}");
        assert!(
            custom.starts_with('⌃') || custom.starts_with("Ctrl"),
            "{custom}"
        );
    }

    #[test]
    fn fixed_shortcuts_spell_the_two_keys() {
        assert!(shortcut_display(Shortcut::ToggleApp).ends_with('E'));
        assert!(shortcut_display(Shortcut::FixWord).ends_with('W'));
    }
}
