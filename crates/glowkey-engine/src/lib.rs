//! GlowKey's platform-free Vietnamese Telex transformation engine.
//!
//! This crate owns *all* Vietnamese logic and knows nothing about macOS. It wraps
//! [`vi`]'s incremental Telex buffer and turns each keystroke into a minimal edit —
//! how many trailing code units to delete, and what text to insert in their place —
//! so the platform shell can render the change with either marked text or an
//! insert-plus-backspace sequence without caring which.
//!
//! Design (matches the surveyed shipping engines, notably `xkey`): keep the raw
//! keystroke log for the word being typed and re-derive the whole rendering from
//! it on every keystroke. At a word's length this costs nothing, and it gives one
//! code path for forward typing, backspace, and case handling.
//!
//! The engine is intentionally ignorant of the per-application ignore list and of
//! VN/EN mode — those are the shell's concern. When Vietnamese input is off, the
//! shell simply never calls [`Engine::process_key`].

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use vi::methods::{Action, IncrementalBuffer};
use vi::processor::AccentStyle;
use vi::processor::{LetterModification, ToneMark};

pub mod config;
mod english;
pub mod exclusion;
mod exclusion_defaults;

pub use config::Settings;
pub use exclusion::ExclusionList;

/// Tone-mark placement convention.
///
/// New style is the modern default (`hoà`, `thuý`); old style is the traditional
/// convention (`hòa`, `thúy`). Mirrors [`AccentStyle`] but keeps `vi` out of the
/// shell's type surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PlacementStyle {
    /// Modern orthography — the software default.
    #[default]
    New,
    /// Traditional orthography.
    Old,
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
    Vietnamese,
    English,
}

impl InputMethod {
    /// Whether this is one of the Telex variants. Quick Telex applies to both,
    /// since its digraphs are plain letters; the bracket shortcuts do not,
    /// because UniKey's Simple Telex mapping deliberately drops them.
    #[must_use]
    pub fn is_telex_family(self) -> bool {
        matches!(self, Self::Telex | Self::SimpleTelex)
    }
}

/// Strips Vietnamese diacritics from text, leaving plain ASCII letters —
/// UniKey's "bỏ dấu" tool (`m_removeTone`).
///
/// `đ`/`Đ` become `d`/`D`; every toned or modified vowel falls back to its base
/// letter. Everything else, including text that was never Vietnamese, passes
/// through untouched. Useful for filenames, URLs and search boxes.
#[must_use]
pub fn remove_tones(text: &str) -> String {
    /// Base letter for each Vietnamese vowel form, lowercase. Uppercase is
    /// handled by casing the result, so only one table is needed.
    const BASES: [(&str, char); 12] = [
        ("aàáảãạăằắẳẵặâầấẩẫậ", 'a'),
        ("eèéẻẽẹêềếểễệ", 'e'),
        ("iìíỉĩị", 'i'),
        ("oòóỏõọôồốổỗộơờớởỡợ", 'o'),
        ("uùúủũụưừứửữự", 'u'),
        ("yỳýỷỹỵ", 'y'),
        ("dđ", 'd'),
        ("AÀÁẢÃẠĂẰẮẲẴẶÂẦẤẨẪẬ", 'A'),
        ("EÈÉẺẼẸÊỀẾỂỄỆ", 'E'),
        ("IÌÍỈĨỊ", 'I'),
        ("OÒÓỎÕỌÔỒỐỔỖỘƠỜỚỞỠỢ", 'O'),
        ("UÙÚỦŨỤƯỪỨỬỮỰ", 'U'),
    ];

    text.chars()
        .map(|ch| {
            if ch.is_ascii() {
                return ch;
            }
            for (forms, base) in BASES {
                if forms.contains(ch) {
                    return base;
                }
            }
            // The two remaining uppercase families, kept out of the table so the
            // lines stay readable.
            match ch {
                'Ỳ' | 'Ý' | 'Ỷ' | 'Ỹ' | 'Ỵ' => 'Y',
                'Đ' => 'D',
                other => other,
            }
        })
        .collect()
}

impl From<PlacementStyle> for AccentStyle {
    fn from(style: PlacementStyle) -> Self {
        match style {
            PlacementStyle::New => AccentStyle::New,
            PlacementStyle::Old => AccentStyle::Old,
        }
    }
}

/// The keyboard input method for Vietnamese, as in Unikey/EVKey. Telex uses letter
/// keys (`aa`→â, `f`→huyền); VNI uses digits (`a6`→â, `2`→huyền).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum InputMethod {
    /// Telex — the software default.
    #[default]
    Telex,
    /// VNI — tone and diacritic digits.
    Vni,
    /// Simple Telex — UniKey's `UkSimpleTelex`. Telex with one difference: `w`
    /// only ever adds a horn or a breve to a vowel already typed, so it never
    /// stands alone as `ư`.
    SimpleTelex,
}

/// The chosen hotkey for the global Vietnamese/English toggle, as a small preset
/// list (like Unikey/EVKey's hotkey picker). The shell maps each to its modifier
/// mask and key code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HotkeyPreset {
    /// ⌃⇧Space — the default.
    #[default]
    CtrlShiftSpace,
    /// ⌃Space.
    CtrlSpace,
    /// ⌥Space.
    OptionSpace,
    /// ⌃⇧Z.
    CtrlShiftZ,
    /// A user-recorded combination.
    ///
    /// The four named presets above are already portable — modifiers plus a
    /// semantic key — and need nothing. This one is the awkward case: the user
    /// pressed a *physical* key, and the only durable name that key has is the
    /// virtual key code the platform reported at the time. So the code is stored
    /// per platform, explicitly, rather than as one number that would mean a
    /// different key on the next machine.
    ///
    /// This is deliberately not a universal keycode table. Two platforms do not
    /// justify inventing a third keyboard model, and the settings file has to
    /// stay something a person can read and edit.
    ///
    /// Command is never allowed (it belongs to the system), so it has no field.
    Custom {
        /// Control key required.
        control: bool,
        /// Shift key required.
        shift: bool,
        /// Option key required.
        option: bool,
        /// Display character for the key (uppercased; `' '` means Space), and the
        /// cross-platform fallback matcher. Captured from the event, so it
        /// reflects the layout the user recorded it on — which is also why it is
        /// a fallback and not the primary matcher.
        key_char: char,
        /// macOS virtual key code, when the combination was recorded on macOS.
        ///
        /// The `keycode` alias is what every settings file written before the
        /// port calls this field. Reading one must not reinterpret the user's
        /// hotkey into some other key — a hotkey that silently starts doing
        /// something else is worse than one that fails loudly — so the old
        /// spelling keeps working forever.
        #[serde(default, alias = "keycode")]
        macos_keycode: Option<i64>,
        /// Windows virtual-key code, when the combination was recorded on Windows.
        #[serde(default)]
        windows_vk: Option<u16>,
    },
}

impl HotkeyPreset {
    /// The macOS virtual key code recorded for this hotkey, if there is one.
    /// `None` for the named presets (they need no code) and for a custom
    /// combination recorded on another platform.
    #[must_use]
    pub fn macos_keycode(self) -> Option<i64> {
        match self {
            Self::Custom { macos_keycode, .. } => macos_keycode,
            _ => None,
        }
    }

    /// The Windows virtual-key code recorded for this hotkey, if there is one.
    #[must_use]
    pub fn windows_vk(self) -> Option<u16> {
        match self {
            Self::Custom { windows_vk, .. } => windows_vk,
            _ => None,
        }
    }
}

/// The result of toggling an application's exclusion — tells the shell what
/// feedback to show and whether the change survives a restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExclusionToggle {
    /// The app is now excluded (Vietnamese off there), persisted.
    Excluded,
    /// The app is now enabled (removed from the list), persisted.
    Enabled,
    /// A known terminal was re-enabled by hotkey: active for this session only.
    /// The persisted exclusion stays, so the next launch re-excludes it —
    /// terminals mangle Vietnamese (a PTY ignores synthetic backspaces), so an
    /// accidental ⌃⇧E must not permanently disarm the protection.
    EnabledSessionOnly,
}

impl ExclusionToggle {
    /// Whether the app ends up excluded.
    #[must_use]
    pub fn excluded(self) -> bool {
        matches!(self, Self::Excluded)
    }
}

/// A text-expansion macro (Unikey's "gõ tắt"): typing `shortcut` then a boundary
/// replaces it with `expansion`. E.g. `vn` → `Việt Nam`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Macro {
    /// The typed keys that trigger the expansion (matched case-insensitively).
    pub shortcut: String,
    /// The text inserted in place of the shortcut.
    pub expansion: String,
}

/// How many entries of the document behind the caret to remember. Deleting back
/// further than this leaves the engine unable to vouch for where the caret is,
/// so it flushes instead of guessing.
const COMMITTED_HISTORY: usize = 5;

/// A word that was committed and is still sitting behind the caret, kept so
/// that deleting back to it re-opens it for editing.
#[derive(Debug, Clone)]
struct CommittedWord {
    /// The raw keystrokes, so the word can be re-opened for editing.
    raw: Vec<char>,
    /// What it renders to — the text that must be at the caret when it reopens.
    rendered: String,
}

/// One step of the document behind the caret: a boundary character, and the word
/// that came before it if there was one.
///
/// The stack these live in *is* the caret position: the engine's picture of the
/// document is `[entry₁][entry₂]…[composing]` with the caret at the end, so the
/// entry immediately behind the caret is always the top of the stack, and every
/// entry accounts for exactly one boundary character on screen. Storing an
/// offset per entry would be a second source of truth able to disagree with the
/// first, and there is nothing for it to add.
#[derive(Debug, Clone)]
enum Behind {
    /// A word and the boundary character that committed it — `hồng` then `␣`.
    Word(CommittedWord),
    /// A boundary character with no word before it, which is what a second
    /// boundary in a row is: the `␣` of `hồng, `. Without an entry of its own it
    /// had nowhere to be recorded, and the whole history was thrown away
    /// instead — so `hoongf, ⌫⌫z` gave `hồngz` where `hoongf ⌫z` gave `hông`.
    /// `, ` and `. ` are the two commonest pairs in prose.
    Boundary,
}

/// A separate memory from [`Session::committed`], which exists for
/// re-composition and is deliberately **not** set when auto-fix restored the
/// word. The correction hotkey needs the opposite: a word that auto-fix already
/// rewrote is exactly the one the user most often wants to argue with.
#[derive(Debug, Clone)]
struct CorrectableWord {
    /// The keys as typed.
    raw: String,
    /// The Vietnamese rendering of those keys.
    rendered: String,
    /// The exact text sitting on screen, stored rather than derived.
    ///
    /// An enum saying "raw" or "Vietnamese" cannot express what is actually
    /// there the moment `commit` can restore to a *third* string — which is
    /// precisely what the planned ASCII-render restore does. The backspace count
    /// comes from this field, so it can never disagree with the screen.
    on_screen: String,
    /// The boundary character the host inserted after the word.
    ///
    /// `None` until `note_boundary` supplies it, and the correction refuses while
    /// it is `None`. That removes an ordering requirement rather than documenting
    /// one: a caller that commits without noting a boundary gets an inert
    /// keystroke instead of an edit that under-deletes by one and strands a
    /// character.
    boundary: Option<char>,
}

/// What an import should do when a shortcut it carries already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroConflict {
    /// Leave the existing expansion alone and count the row as skipped.
    Skip,
    /// Overwrite the existing expansion.
    Replace,
}

/// Which reading of one set of typed keys the user wants.
///
/// The English/Telex ambiguity is not resolvable by rule — the same keystrokes
/// are legitimate Vietnamese and legitimate English (`docs/handoff.md` §6.3), so
/// `cats` is both `cats` and `cát` and no amount of cleverness decides which.
/// This is the user answering, one word at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WordPreference {
    /// Keep the keys as typed: `cats` stays `cats`.
    ///
    /// The aliases exist because this list is meant to be hand-editable, and
    /// `"raw"` is what a person writes.
    #[default]
    #[serde(alias = "raw", alias = "typed")]
    Raw,
    /// Keep the Vietnamese rendering: `cats` becomes `cát`.
    #[serde(alias = "vietnamese", alias = "vi")]
    Vietnamese,
}

/// One word the user has decided about.
///
/// Keyed on the **raw keys**, lowercased, because the raw keys are what the
/// ambiguity is about: one key sequence, two readings. `cats` is the question;
/// `cats` and `cát` are the two answers. Lowercased to match
/// `english::is_common_english`, so a capitalised word at a sentence start obeys
/// the same decision as the same word mid-sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordOverride {
    /// The typed keys, lowercase.
    #[serde(default)]
    pub keys: String,
    /// Which reading to keep. An unrecognised or missing value reads as "keep
    /// what was typed", the safe answer.
    ///
    /// Lenient on purpose, because hand-editing this list is a documented
    /// workflow and the file is parsed with `unwrap_or_default`: one mistyped
    /// verdict used to discard **every** setting in it — exclusions, macros,
    /// hotkey, the lot — and the next change from the UI then wrote the defaults
    /// back over them. Losing a curated exclusion list to a typo in an unrelated
    /// field is not a trade anyone would accept, so this field refuses to be the
    /// thing that fails the document.
    #[serde(default, deserialize_with = "lenient_preference")]
    pub prefer: WordPreference,
}

/// Reads a word preference, treating anything unrecognised as the default.
///
/// See [`WordOverride::prefer`] for why this cannot be allowed to fail.
fn lenient_preference<'de, D>(deserializer: D) -> Result<WordPreference, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(deserializer).unwrap_or_default();
    Ok(match raw.to_ascii_lowercase().as_str() {
        "vietnamese" | "vi" => WordPreference::Vietnamese,
        _ => WordPreference::Raw,
    })
}

/// The `version` a UniKey macro-table header declares for a UTF-8 body. Anything
/// else means the body is VIQR (`UKMACRO_VERSION_UTF8` in UniKey's `mactab.cpp`).
const UNIKEY_MACRO_VERSION_UTF8: i32 = 1;

/// Whether a line is UniKey's macro-table header.
fn is_unikey_header(line: &str) -> bool {
    unikey_header_version(line).is_some()
}

/// The version declared by a UniKey macro-table header line, if it is one.
/// The header is written as `;DO NOT DELETE THIS LINE*** version=1 ***`, with
/// the leading `;` only on Windows.
fn unikey_header_version(line: &str) -> Option<i32> {
    let line = line.trim_start_matches('\u{feff}').trim();
    let line = line.strip_prefix(';').unwrap_or(line);
    if !line.starts_with("DO NOT DELETE THIS LINE") {
        return None;
    }
    let (_, rest) = line.split_once("version=")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

impl Macro {
    /// Parses a macro table.
    ///
    /// The line format is `shortcut:expansion`, split on the **first** colon, as
    /// UniKey's `CMacroTable::addItem` does — that is the file people arrive
    /// with, from UniKey or EVKey.
    ///
    /// A real UniKey export also carries a header line
    /// (`;DO NOT DELETE THIS LINE*** version=1 ***`), preceded by a byte-order
    /// mark on Windows. Both are handled: the mark is stripped, the header is
    /// recognised rather than surviving by accident, and a header naming any
    /// version other than 1 means the body is VIQR rather than UTF-8 — see
    /// [`table_is_legacy_viqr`](Self::table_is_legacy_viqr).
    ///
    /// Neither field is trimmed, matching UniKey, so a trailing space in an
    /// expansion survives — ordinary in gõ tắt, where `vn` should expand to
    /// `Việt Nam ` with the space. The shortcut is the exception: it is matched
    /// against typed keys, which cannot contain a space, so a stray one there
    /// would only make the macro unreachable.
    ///
    /// A leading `[` switches to this application's own JSON, so a table
    /// exported here round-trips losslessly.
    ///
    /// Unparseable lines are skipped rather than failing the whole import: a
    /// table hand-edited over years usually has a stray line in it, and losing
    /// the other five hundred entries over one is not a kindness. Blank lines and
    /// `#` comments are ignored.
    #[must_use]
    pub fn parse_table(text: &str) -> Vec<Self> {
        let text = text.trim_start_matches('\u{feff}');
        let trimmed = text.trim_start();
        if trimmed.starts_with('[') {
            // Broken JSON returns nothing rather than falling through to the line
            // reader, which would report "expected shortcut:expansion" about a
            // file that is plainly not in that format.
            return serde_json::from_str(trimmed).unwrap_or_default();
        }
        text.lines()
            .filter(|line| !is_unikey_header(line))
            .filter(|line| {
                let head = line.trim_start();
                !head.is_empty() && !head.starts_with('#')
            })
            .filter_map(|line| {
                // First colon only, so an expansion may contain one.
                let (shortcut, expansion) = line.split_once(':')?;
                let shortcut = shortcut.trim();
                (!shortcut.is_empty() && !expansion.is_empty()).then(|| Self {
                    shortcut: shortcut.to_string(),
                    expansion: expansion.to_string(),
                })
            })
            .collect()
    }

    /// Whether this is an old UniKey export whose body is VIQR-encoded rather
    /// than UTF-8 — its header names a version other than 1.
    ///
    /// GlowKey does not do VIQR (a standing decision: every modern macOS
    /// application is Unicode), so the caller should refuse such a file and say
    /// why, rather than importing `Vie^.t Nam` as literal text.
    #[must_use]
    pub fn table_is_legacy_viqr(text: &str) -> bool {
        text.trim_start_matches('\u{feff}')
            .lines()
            .next()
            .and_then(unikey_header_version)
            .is_some_and(|version| version != UNIKEY_MACRO_VERSION_UTF8)
    }

    /// Serializes a macro table.
    ///
    /// Writes the line format, which Unikey and EVKey can read, unless some
    /// expansion contains a newline or a shortcut contains a colon — neither
    /// survives a line-based file, so those tables are written as JSON instead
    /// and are still readable by [`parse_table`](Self::parse_table).
    #[must_use]
    pub fn format_table(macros: &[Self]) -> String {
        // Anything the line reader would alter or drop forces the JSON path, so
        // that export followed by import is lossless. The reader splits on the
        // first colon, skips `#` comments and blank expansions, and trims both
        // fields — and a trailing space in an expansion is ordinary in gõ tắt.
        let line_safe = macros.iter().all(|m| {
            !m.shortcut.contains(':')
                && !m.shortcut.starts_with('#')
                && !m.expansion.is_empty()
                && m.shortcut.trim() == m.shortcut
                && !m.shortcut.contains('\n')
                && !m.expansion.contains('\n')
        });
        if !line_safe {
            return serde_json::to_string_pretty(macros).unwrap_or_default();
        }
        let mut out = String::new();
        for m in macros {
            out.push_str(&m.shortcut);
            out.push(':');
            out.push_str(&m.expansion);
            out.push('\n');
        }
        out
    }
}

/// What a mid-word Backspace did to the engine, and what the shell owes the
/// document as a result.
///
/// Three named answers rather than a `bool`, because [`Self::Repair`] must be
/// treated differently *in kind*: it means the shell has to suppress the
/// keystroke and emit an edit instead of letting the host delete. A boolean that
/// sometimes also meant "apply this" is the sort of contract that gets misread
/// once and eats a character of the user's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackspaceOutcome {
    /// Nothing composed, or no single key removal reproduces what the screen will
    /// show. The caller flushes and lets the delete happen normally.
    Flush,
    /// The engine is in step with what the host's delete will leave behind. The
    /// caller passes the keystroke through, as it always has.
    InStep,
    /// The word was being rendered verbatim because the mid-word spell check had
    /// refused it, and deleting this character makes it spellable again — so the
    /// transformation comes back.
    ///
    /// The caller must **suppress** the Backspace and apply this edit: the
    /// user's delete is accounted for inside it, and the backspace count covers
    /// the whole on-screen word. Letting the host delete and then posting this
    /// would mix a native keystroke with a synthesized edit, which is the race
    /// the full-suppression model exists to remove (`docs/handoff.md` §5).
    Repair(KeyResponse),
}

/// What a Backspace landing on a word boundary did.
///
/// [`Self::Reopened`] and [`Self::BoundaryRemoved`] are both "the host performs
/// the delete and the engine is still in step", but they are not the same event
/// and collapsing them into a `bool` is what hid the double-boundary bug: the
/// second one used to be indistinguishable from "nothing remembered", which the
/// caller answers by flushing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryBackspace {
    /// The word behind the caret is open for editing again. The caller passes
    /// the keystroke through so the host deletes the boundary character.
    Reopened,
    /// A boundary character with no word in front of it came off — the `␣` of
    /// `hồng, `. Nothing re-opened, but the entries behind it are still an
    /// accurate account of the document, so the caller must **not** flush.
    BoundaryRemoved,
    /// Not this path's business: mid-word, or deleted back past what the engine
    /// remembers. The caller carries on with the mid-word handling.
    NotApplicable,
}

/// The edit the shell must apply to the document for one keystroke.
///
/// `backspaces` counts **UTF-16 code units** to delete from the end of the text
/// already committed for the current word — the unit `NSRange` and
/// `NSTextInputClient` use — then `insert` (always NFC) is inserted. When
/// `handled` is false the engine consumed no state and the host application should
/// process the key normally (a space, a digit, a shortcut).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyResponse {
    /// Whether the engine consumed this key. False means "let the host handle it".
    pub handled: bool,
    /// UTF-16 code units to delete from the current word's committed tail.
    pub backspaces: usize,
    /// Text to insert after deleting, in NFC.
    pub insert: String,
}

impl KeyResponse {
    /// A key the engine did not consume — the host should insert it itself.
    fn passthrough() -> Self {
        Self::default()
    }
}

/// The Telex transformation engine for one input session (one text field).
///
/// Each keystroke re-derives the whole word from the raw key log. At a word's
/// length (a handful of characters) this costs nothing measurable, and it keeps a
/// single code path for forward typing, backspace, and case handling — the
/// hybrid the surveyed shipping engines converge on.
pub struct Engine {
    style: PlacementStyle,
    /// Telex or VNI — which key definition drives the transformation.
    method: InputMethod,
    /// Raw keystrokes of the word being typed, in their original case.
    raw: Vec<char>,
    /// The text currently on screen for this word — the diff baseline.
    rendered: String,
    /// "Quick Telex": expand a doubled consonant at the start of a syllable to
    /// its digraph. Opt-in.
    quick_telex: bool,
    /// UniKey's Telex bracket shortcuts — `[`→ơ, `]`→ư, `{`→Ơ, `}`→Ư. Opt-in.
    telex_brackets: bool,
    /// UniKey's `spellCheckEnabled`: refuse a diacritic that would make the word
    /// impossible in Vietnamese, at the keystroke. Opt-in.
    strict_spell_check: bool,
    /// Set when the spell check refused this word: it stops transforming and
    /// renders its raw keys until the next boundary. Cleared by `reset`.
    escaped: bool,
}

impl Engine {
    /// Creates an engine with the given placement style.
    #[must_use]
    pub fn new(style: PlacementStyle) -> Self {
        Self {
            style,
            method: InputMethod::default(),
            raw: Vec::new(),
            rendered: String::new(),
            quick_telex: false,
            telex_brackets: false,
            strict_spell_check: false,
            escaped: false,
        }
    }

    /// Changes the placement style. Takes effect on the next word.
    pub fn set_style(&mut self, style: PlacementStyle) {
        self.style = style;
        // Any in-progress word keeps its style; flush so the next word uses the new one.
        self.reset();
    }

    /// Turns "Quick Telex" on or off. Flushes so the next word uses it.
    pub fn set_quick_telex(&mut self, on: bool) {
        self.quick_telex = on;
        self.reset();
    }

    /// Whether Quick Telex is on.
    #[must_use]
    pub fn quick_telex(&self) -> bool {
        self.quick_telex
    }

    /// Turns the Telex bracket shortcuts on or off. Flushes so the next word
    /// uses the new setting.
    pub fn set_telex_brackets(&mut self, on: bool) {
        self.telex_brackets = on;
        self.reset();
    }

    /// Whether the Telex bracket shortcuts are on.
    #[must_use]
    pub fn telex_brackets(&self) -> bool {
        self.telex_brackets
    }

    /// Turns the mid-word spell check on or off. Flushes so the next word uses
    /// the new setting.
    pub fn set_strict_spell_check(&mut self, on: bool) {
        self.strict_spell_check = on;
        self.reset();
    }

    /// Whether the mid-word spell check is on.
    #[must_use]
    pub fn strict_spell_check(&self) -> bool {
        self.strict_spell_check
    }

    /// Changes the input method (Telex/VNI). Flushes so the next word uses it.
    pub fn set_method(&mut self, method: InputMethod) {
        self.method = method;
        self.reset();
    }

    /// Clears all in-progress state. Call on focus change, app switch, or after a
    /// word boundary — a stale word must never bleed into a new field.
    pub fn reset(&mut self) {
        self.raw.clear();
        self.rendered.clear();
        self.escaped = false;
    }

    /// Whether a word is currently being composed.
    #[must_use]
    pub fn is_composing(&self) -> bool {
        !self.raw.is_empty()
    }

    /// The current rendering of the word being composed. This is what a
    /// marked-text shell displays as the composing (underlined) text.
    #[must_use]
    pub fn current_word(&self) -> &str {
        &self.rendered
    }

    /// The raw keystrokes of the word being composed, exactly as typed — what
    /// auto-fix restores when the rendering is not valid Vietnamese.
    #[must_use]
    pub fn raw_string(&self) -> String {
        self.raw.iter().collect()
    }

    /// A copy of the raw keystrokes, for remembering a just-committed word so it can
    /// be re-composed if its trailing boundary is deleted.
    #[must_use]
    pub fn raw_vec(&self) -> Vec<char> {
        self.raw.clone()
    }

    /// The current input method (Telex/VNI).
    #[must_use]
    pub fn method(&self) -> InputMethod {
        self.method
    }

    /// Re-enters composing with a previously committed word's `raw` keys and its
    /// on-screen `rendered` form, so the next keystrokes keep editing it (Telex
    /// re-composition after the trailing boundary is backspaced).
    pub fn restore(&mut self, raw: Vec<char>, rendered: String) {
        self.raw = raw;
        self.rendered = rendered;
    }

    /// Feeds one typed character to the engine.
    ///
    /// A character that can extend a Vietnamese syllable (an ASCII letter) is added
    /// to the word and the resulting edit is returned. Anything else is a word
    /// boundary: the engine flushes and reports the key as unhandled so the host
    /// inserts it verbatim.
    pub fn process_key(&mut self, ch: char) -> KeyResponse {
        if !self.is_syllable_char(ch) {
            // Word boundary (space, punctuation — and digits in Telex). Commit and
            // hand the key back.
            self.reset();
            return KeyResponse::passthrough();
        }
        self.raw.push(ch);
        if self.strict_spell_check && !self.escaped && self.last_key_made_it_impossible() {
            // The keystroke produced something Vietnamese cannot spell. Refuse the
            // transformation for the rest of the word: the raw keys come back and
            // stay literal until the next boundary. That is UniKey's
            // `spellCheckEnabled` — the same repair auto-fix performs, but at the
            // keystroke instead of at the space.
            //
            // Escaping the whole word rather than the single key is deliberate:
            // the engine re-derives everything from the raw log on every
            // keystroke, so a key merely dropped here would be re-applied by the
            // next one.
            self.escaped = true;
        }
        self.rerender()
    }

    /// Whether the render is now something Vietnamese cannot produce.
    ///
    /// Judged on the **render**, never the raw keys: the raw prefix `nguow` is not
    /// a syllable, but what it renders to — `ngươ`, an ordinary step in typing
    /// `người` — is. Pure-ASCII renders are what the user typed verbatim and are
    /// never refused, which is also what keeps English out of this path.
    fn last_key_made_it_impossible(&self) -> bool {
        // The exact complement of [`Self::can_unescape`], and deliberately
        // written as such: the rule that refuses a word and the rule that lets it
        // back in have to agree, and two hand-written copies of "is this
        // spellable" would eventually disagree. Only reached with `escaped`
        // false, so `render_keys` and `can_unescape`'s render are the same thing.
        //
        // Rendering a candidate rather than calling `rerender` matters: `rerender`
        // installs its result as `self.rendered`, the diff baseline, and
        // committing a render the shell never applied then diffing the next one
        // against it made the emitted backspace count overshoot by exactly the
        // discarded edit — eating a character of the document to the left.
        //
        // There is no carve-out for the repeat-key rejection gesture. There used
        // to be: `hoongff` → `hôngf` was exempted on the grounds that refusing a
        // rejection undoes what the user asked for. Removed 2026-09-04 at the
        // owner's direction, from live use — with this check on, `hôngf` is not
        // something Vietnamese can spell, and the check's whole promise is to show
        // you what you typed when the result is impossible. The gesture itself is
        // untouched with the check off, which is the default.
        !self.can_unescape()
    }

    /// Composes without transforming: the keys accumulate so a macro can still be
    /// matched at the boundary, but they render exactly as typed. Reuses the same
    /// escape the spell check sets, so there is one verbatim path, not two.
    pub fn process_key_verbatim(&mut self, ch: char) -> KeyResponse {
        self.escaped = true;
        self.process_key(ch)
    }

    /// Whether `ch` can extend the current word. Letters always; digits only in VNI,
    /// where they carry tone and diacritic marks (`a6`→â, `viet65`→việt).
    #[must_use]
    pub fn is_syllable_char(&self, ch: char) -> bool {
        ch.is_ascii_alphabetic()
            || (self.method == InputMethod::Vni && ch.is_ascii_digit())
            // With the bracket shortcuts on, these four are vowel keys rather
            // than punctuation, so they extend the word instead of ending it.
            || (self.telex_brackets
                && self.method == InputMethod::Telex
                && matches!(ch, '[' | ']' | '{' | '}'))
    }

    /// Handles a Backspace keystroke while a word is being composed.
    ///
    /// Drops the last raw key and re-derives, so deleting mid-word restores exactly
    /// the state that produced the earlier text. Returns [`KeyResponse::passthrough`]
    /// when nothing is being composed, so the host deletes normally.
    pub fn backspace(&mut self) -> KeyResponse {
        if self.raw.is_empty() {
            return KeyResponse::passthrough();
        }
        self.raw.pop();
        if self.raw.is_empty() {
            self.escaped = false;
        }
        let mut response = self.rerender();
        // Always consumed: even an empty word means we just deleted our last char.
        response.handled = true;
        response
    }

    /// Shrinks the composition by one **visible** character, keeping the raw key
    /// log in step, and reports whether it managed to.
    ///
    /// This is the mid-word Backspace the host performs itself: the keystroke
    /// passes straight through, so the engine has to land on exactly what the host
    /// will show — the rendering minus its last character. `hồng`⌫ is `hồn`, which
    /// means dropping the raw `g` and *keeping* the tone key `f`.
    /// [`backspace`](Self::backspace) cannot do that: popping the last key drops
    /// the `f` and gives `hông`, so the tone the user never touched disappears and
    /// the engine's idea of the text no longer matches the screen. So search the
    /// raw log from the end for the one key whose removal re-renders to the target.
    ///
    /// Returns [`BackspaceOutcome::Flush`] when no single removal reproduces the
    /// target (the caller then flushes and stops composing), which also covers
    /// deleting the last character of a word that only exists through a
    /// transformation (`oo`⌫).
    ///
    /// **Deleting the key that caused an escape undoes the escape.** The mid-word
    /// spell check renders a refused word verbatim, and that used to be one-way:
    /// `hoongf` gave `hồng`, a mistyped `a` escaped the word to `hoongfa`, and
    /// Backspace left `hoongf` stuck as literal keys for the rest of the word's
    /// life. This function made it worse by working correctly — while escaped the
    /// render *is* the raw keys, so dropping the `a` reproduced the screen
    /// exactly and the engine happily stayed escaped. Now the shortened word is
    /// re-judged by the same question that refused it, and if it is spellable
    /// again the transformation comes back.
    pub fn backspace_visible_char(&mut self) -> BackspaceOutcome {
        if self.raw.is_empty() || self.rendered.is_empty() {
            return BackspaceOutcome::Flush;
        }
        // What the document shows right now. A repair replaces all of it, so this
        // has to be read before anything below changes it.
        let on_screen = self.rendered.clone();
        let mut target = self.rendered.clone();
        target.pop();

        for index in (0..self.raw.len()).rev() {
            let mut candidate = self.raw.clone();
            candidate.remove(index);
            if self.render_keys(&candidate) == target {
                self.raw = candidate;
                self.rendered = target;
                // Deleting the word away also ends the escape. Without this the
                // flag latched: the caller sees "in step" so it never flushes, and
                // the next word silently refused to transform.
                if self.raw.is_empty() {
                    self.escaped = false;
                    return BackspaceOutcome::InStep;
                }
                // Only the spell check's escape can be lifted here in practice.
                // `process_key_verbatim` sets the same flag for the always-macro
                // path, but that is unreachable through `Session`: it needs
                // English mode, where `is_active()` is false and this returns
                // `Flush` before the engine is consulted — and every route back
                // to an active session (`toggle_mode`, `set_frontmost_app`,
                // `toggle_app_exclusion`) resets the engine on the way. Worth
                // stating rather than re-deriving: `Engine` is public, and the
                // two escapes share one flag.
                if self.escaped && self.can_unescape() {
                    self.escaped = false;
                    self.rendered = self.render_keys(&self.raw);
                    return BackspaceOutcome::Repair(KeyResponse {
                        handled: true,
                        backspaces: on_screen.encode_utf16().count(),
                        insert: self.rendered.clone(),
                    });
                }
                return BackspaceOutcome::InStep;
            }
        }
        BackspaceOutcome::Flush
    }

    /// Whether the escaped word would be spellable again if the escape were
    /// lifted right now.
    ///
    /// Asks the same question that set the escape rather than a second one of its
    /// own: an entry rule and an exit rule that have to agree are best written
    /// once. A render that is pure ASCII was typed verbatim and was never the
    /// spell check's business, so it un-escapes freely.
    fn can_unescape(&self) -> bool {
        let candidate = render(
            &self.raw,
            self.style,
            self.method,
            self.quick_telex,
            self.telex_brackets,
        );
        !is_invalid_vietnamese(&candidate)
    }

    /// Renders a raw key sequence under this engine's settings, honouring an
    /// escape: once the spell check has refused the word, it renders verbatim.
    fn render_keys(&self, raw: &[char]) -> String {
        if self.escaped {
            return raw.iter().collect();
        }
        render(
            raw,
            self.style,
            self.method,
            self.quick_telex,
            self.telex_brackets,
        )
    }

    /// Re-derives the rendered word from the raw key log and returns the edit that
    /// turns the previous rendering into the new one.
    fn rerender(&mut self) -> KeyResponse {
        let next = self.render_keys(&self.raw);
        let response = diff(&self.rendered, &next);
        self.rendered = next;
        response
    }
}

/// UniKey's Simple Telex (`SimpleTelexMethodMapping`, `inputproc.cpp:119`).
///
/// Identical to Telex but for `w`, which UniKey maps to Hook-All rather than to
/// its special Telex-W: it adds a horn to `u`/`o` or a breve to `a`, and does
/// nothing on its own. Full Telex additionally lets a bare `w` stand for `ư`,
/// which is the behaviour people either rely on or trip over — hence the
/// separate method.
///
/// Spelled out as its own definition rather than patched at the key level: `vi`
/// takes a whole `Definition`, and copying ten unchanged entries is clearer than
/// intercepting one key on the way past.
static SIMPLE_TELEX: vi::methods::Definition = phf::phf_map! {
    's' => &[Action::AddTonemark(ToneMark::Acute)],
    'f' => &[Action::AddTonemark(ToneMark::Grave)],
    'r' => &[Action::AddTonemark(ToneMark::HookAbove)],
    'x' => &[Action::AddTonemark(ToneMark::Tilde)],
    'j' => &[Action::AddTonemark(ToneMark::Underdot)],
    'a' => &[Action::ModifyLetterOnCharacterFamily(LetterModification::Circumflex, 'a')],
    'e' => &[Action::ModifyLetterOnCharacterFamily(LetterModification::Circumflex, 'e')],
    'o' => &[Action::ModifyLetterOnCharacterFamily(LetterModification::Circumflex, 'o')],
    'w' => &[Action::ModifyLetter(LetterModification::Horn), Action::ModifyLetter(LetterModification::Breve)],
    'd' => &[Action::ModifyLetter(LetterModification::Dyet)],
    'z' => &[Action::RemoveToneMark],
};

/// "Quick Telex": a doubled consonant at the **start** of the syllable stands for
/// its digraph, so `cc` types `ch` and `nn` types `ng`. EVKey and later UniKey
/// releases offer this; it is absent from the 2015 UniKey source.
///
/// Only the syllable-initial position expands. That is where these digraphs are
/// legal Vietnamese onsets, and it is what keeps English out of trouble: the
/// doubled consonants in `letter`, `happy` and `accept` all sit mid-word, so
/// none of them expand.
///
/// `uu` expands to the Telex keys `uw` rather than to `ư` directly, so the
/// substitution stays inside the Telex alphabet and `vi` still does the work.
fn expand_quick_telex(raw: &[char]) -> Vec<char> {
    /// Doubled key at the syllable start, and the keys it stands for.
    const EXPANSIONS: [(char, &str); 8] = [
        ('c', "ch"),
        ('g', "gi"),
        ('k', "kh"),
        ('n', "ng"),
        ('p', "ph"),
        ('q', "qu"),
        ('t', "th"),
        ('u', "uw"),
    ];

    let (Some(first), Some(second)) = (raw.first(), raw.get(1)) else {
        return raw.to_vec();
    };
    let lowered = first.to_ascii_lowercase();
    if lowered != second.to_ascii_lowercase() {
        return raw.to_vec();
    }
    let Some((_, keys)) = EXPANSIONS.iter().find(|(key, _)| *key == lowered) else {
        return raw.to_vec();
    };

    // Keep the case the user typed. Both keys shifted means caps lock is on and
    // the whole digraph is uppercase (`CCAO`→`CHAO`); only the first shifted is
    // the ordinary Title-case gesture (`Ccao`→`Chao`). Uppercasing just the head
    // in the caps-lock case left a lowercase key in the slice, which then defeated
    // `apply_case`'s all-caps test and downgraded the whole word (`CCAO`→`ChAO`).
    let mut out: Vec<char> = keys.chars().collect();
    if first.is_uppercase() {
        if second.is_uppercase() {
            for ch in &mut out {
                *ch = ch.to_ascii_uppercase();
            }
        } else if let Some(head) = out.first_mut() {
            *head = head.to_ascii_uppercase();
        }
    }
    out.extend_from_slice(&raw[2..]);
    out
}

/// UniKey's Telex bracket shortcuts: `[`→ơ, `]`→ư, `{`→Ơ, `}`→Ư
/// (`TelexMethodMapping` in UniKey's `inputproc.cpp`).
///
/// Each bracket is replaced by the **Telex keys** that spell the vowel rather
/// than by the character itself, so the substitution stays inside the Telex
/// alphabet and a tone key typed afterwards still lands: `[f` goes through
/// `owf` to `ờ`. Inserting a precomposed `ơ` would leave `vi` with a character
/// it cannot then modify.
fn expand_telex_brackets(raw: &[char]) -> Vec<char> {
    // The injected keys have to carry the case of the word around them. Caps Lock
    // does not shift `[`, so a caps-lock user types `[`, not `{`, and injecting a
    // lowercase `o`/`w` left a lowercase key in the slice — which defeated
    // `apply_case`'s all-caps test and downgraded the whole word (`TH[`→`Thơ`
    // instead of `THƠ`). The shifted forms `{`/`}` are always uppercase, because
    // typing them is a deliberate request for the capital.
    // Two or more capitals means Caps Lock; a single one is just Title case, and
    // `T[` should give `Tơ`, not `TƠ`. This is the same distinction `apply_case`
    // draws between an all-caps word and a capitalised one.
    let mut capitals = 0;
    let mut letters = 0;
    for ch in raw.iter().filter(|c| c.is_alphabetic()) {
        letters += 1;
        if ch.is_uppercase() {
            capitals += 1;
        }
    }
    let all_caps = letters >= 2 && capitals == letters;

    let mut out = Vec::with_capacity(raw.len() + 2);
    for &ch in raw {
        let keys: &[char] = match (ch, all_caps) {
            ('[', false) => &['o', 'w'],
            ('[', true) | ('{', _) => &['O', 'W'],
            (']', false) => &['u', 'w'],
            (']', true) | ('}', _) => &['U', 'W'],
            _ => {
                out.push(ch);
                continue;
            }
        };
        out.extend_from_slice(keys);
    }
    out
}

/// Transforms a raw keystroke sequence into its Vietnamese rendering.
///
/// `vi` mishandles case for whole-word uppercase (e.g. `NGUYEENX` places the tone
/// on the wrong vowel), so transformation runs on the lowercased keys and case is
/// re-applied afterward. Crucially, when `vi` applied *no* Vietnamese
/// transformation — the output equals the lowercased input — the original keys are
/// emitted verbatim, so mixed-case words that are not Vietnamese (`iPhone`,
/// `JavaScript`, `macOS`) keep their exact case instead of being flattened.
/// For words that do transform, the two case patterns users actually produce,
/// ALL-CAPS and Title-case, are handled exactly; other interior case is
/// best-effort (nobody types `nGuyễn`).
fn render(
    raw: &[char],
    style: PlacementStyle,
    method: InputMethod,
    quick_telex: bool,
    telex_brackets: bool,
) -> String {
    let expanded;
    // Telex only. The expansions are Telex key sequences — `uu` stands for the
    // keys `uw` — so running them under VNI puts a literal `w` on screen that the
    // user never typed, and auto-fix cannot repair it because the result is plain
    // ASCII and so counts as "typed verbatim".
    let raw = if quick_telex && method.is_telex_family() {
        expanded = expand_quick_telex(raw);
        expanded.as_slice()
    } else {
        raw
    };
    // Brackets run *after* Quick Telex, which inspects the first two raw keys:
    // Quick Telex is about the literal doubled keystroke the user made, and
    // substituting brackets first would change the pair it looks at.
    let bracketed;
    let raw = if telex_brackets && method == InputMethod::Telex {
        bracketed = expand_telex_brackets(raw);
        bracketed.as_slice()
    } else {
        raw
    };
    let lowered: String = raw.iter().map(|c| c.to_ascii_lowercase()).collect();
    let definition = match method {
        InputMethod::Telex => &vi::TELEX,
        InputMethod::Vni => &vi::VNI,
        InputMethod::SimpleTelex => &SIMPLE_TELEX,
    };
    let mut buffer = IncrementalBuffer::new_with_style(definition, style.into());
    for ch in lowered.chars() {
        buffer.push(ch);
    }
    let out = buffer.view();

    // No Vietnamese transformation occurred: emit the keys exactly as typed so all
    // original case survives. This is the common case for English words.
    if out == lowered {
        return raw.iter().collect();
    }

    apply_case(out, raw)
}

/// Re-applies the raw keys' case pattern to a transformed lowercase rendering.
///
/// `raw` is always non-empty here (an empty or untransformed word takes the
/// verbatim path in [`render`]) and contains only ASCII letters (only
/// [`is_syllable_char`] keys reach the buffer).
fn apply_case(lower: &str, raw: &[char]) -> String {
    if raw.iter().all(|c| c.is_ascii_uppercase()) {
        return lower.to_uppercase();
    }
    if raw[0].is_ascii_uppercase() {
        // Title-case: uppercase the first character of the rendering.
        let mut chars = lower.chars();
        return match chars.next() {
            Some(first) => first.to_uppercase().chain(chars).collect(),
            None => String::new(),
        };
    }
    lower.to_string()
}

/// Whether the session currently transforms input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum InputMode {
    /// Vietnamese transformation is active.
    #[default]
    Vietnamese,
    /// Pass-through — keys reach the host unchanged.
    English,
}

/// Ties the engine, the VN/EN mode, and the ignore list together and enforces the
/// one precedence rule that defines GlowKey's primary feature:
///
/// ```text
/// excluded application  -> never transform. Nothing overrides this.
/// English mode          -> pass through.
/// Vietnamese mode       -> transform.
/// ```
///
/// The exclusion check comes first, so neither the VN/EN toggle nor any future
/// per-app memory can re-enable transformation inside an application the user
/// chose to exclude.
pub struct Session {
    engine: Engine,
    mode: InputMode,
    exclusions: ExclusionList,
    /// Placement style, kept so it can be snapshotted back into [`Settings`].
    style: PlacementStyle,
    /// Whether to restore invalid Vietnamese to raw keys at a word boundary.
    auto_fix: bool,
    /// Bundle identifier of the frontmost application, set by the shell on focus
    /// change. `None` before the first application is known.
    current_bundle_id: Option<String>,
    /// The document behind the caret, most recent last: an unbroken run of
    /// boundary characters and the words that preceded them. Deleting back to a
    /// committed word re-opens it, which is what makes `hồng`␣⌫`z` → `hông` work
    /// however many keystrokes ago the word was committed.
    ///
    /// Capped at [`COMMITTED_HISTORY`]: five is well past what anyone deletes
    /// back through, and the cap is really about bounding how far a wrong
    /// assumption about the caret could reach.
    committed: VecDeque<Behind>,
    /// Persisted preference: open the Settings window on launch.
    open_settings_at_launch: bool,
    /// Capitalize the first letter of each sentence.
    auto_capitalize: bool,
    /// True when the next typed letter starts a sentence (document start, or after
    /// `.`/`!`/`?`). Consumed by the first letter of the following word.
    pending_capital: bool,
    /// The hotkey preset for the global Vietnamese/English toggle.
    toggle_hotkey: HotkeyPreset,
    /// Text-expansion macros (shortcut → expansion).
    macros: Vec<Macro>,
    /// Opt-in: at a boundary, restore a committed word to its raw keys when those
    /// keys form a common English word — even if the rendering is valid
    /// Vietnamese (`was`→`ứa`). Off by default: it inverts the ambiguity for
    /// Vietnamese words typed with a trailing tone key (`cats`→`cát`).
    restore_english_words: bool,
    /// UniKey's `alwaysMacro`: expand macros even while Vietnamese is off.
    always_macro: bool,
    /// Whether the one-time welcome has been shown (persisted).
    welcome_shown: bool,
    /// The word just committed, for the correction hotkey. One-shot: cleared by
    /// the correction itself and by anything that could move the caret.
    correctable: Option<CorrectableWord>,
    /// Per-word decisions, indexed for lookup at the word boundary. The persisted
    /// form is a `Vec<WordOverride>` in `Settings` — stable and diffable, like
    /// `macros`; this map is the index over it, rebuilt on load.
    word_overrides: HashMap<String, WordPreference>,
    /// Language of the user interface. The engine never renders text; this rides
    /// along so the one settings file stays the single persisted surface.
    language: Language,
}

impl Session {
    /// Creates a session with the given placement style and ignore list.
    #[must_use]
    pub fn new(style: PlacementStyle, exclusions: ExclusionList) -> Self {
        Self {
            engine: Engine::new(style),
            mode: InputMode::default(),
            exclusions,
            style,
            auto_fix: true,
            current_bundle_id: None,
            committed: VecDeque::new(),
            open_settings_at_launch: true,
            auto_capitalize: false,
            pending_capital: true,
            toggle_hotkey: HotkeyPreset::default(),
            macros: Vec::new(),
            restore_english_words: false,
            language: Language::default(),
            always_macro: false,
            welcome_shown: false,
            correctable: None,
            word_overrides: HashMap::new(),
        }
    }

    /// Builds a session from persisted [`Settings`].
    #[must_use]
    pub fn from_settings(settings: &Settings) -> Self {
        // Mode is deliberately NOT restored: GlowKey always launches in Vietnamese
        // (the point of the app). ⌃⇧Space is a session-only toggle, so an accidental
        // toggle can never leave the app launching disabled. Only the ignore list,
        // auto-fix, tone style, and input method persist.
        let mut session = Self::new(settings.style, settings.exclusion_list());
        session.auto_fix = settings.auto_fix;
        session.open_settings_at_launch = settings.open_settings_at_launch;
        session.auto_capitalize = settings.auto_capitalize;
        session.toggle_hotkey = settings.toggle_hotkey;
        session.macros = settings.macros.clone();
        session.restore_english_words = settings.restore_english_words;
        session.language = settings.language;
        session.always_macro = settings.always_macro;
        session.welcome_shown = settings.welcome_shown;
        session.word_overrides = settings
            .word_overrides
            .iter()
            .map(|o| (o.keys.to_ascii_lowercase(), o.prefer))
            .collect();
        session.engine.set_quick_telex(settings.quick_telex);
        session.engine.set_telex_brackets(settings.telex_brackets);
        session
            .engine
            .set_strict_spell_check(settings.strict_spell_check);
        session.engine.set_method(settings.input_method);
        session
    }

    /// Snapshots the user-controlled state back into [`Settings`] for saving.
    /// A session-suspended terminal is still in `exclusions` (by design — the
    /// suspension must not survive a restart).
    #[must_use]
    pub fn snapshot(&self) -> Settings {
        Settings {
            exclusions: self.exclusions.ids().map(String::from).collect(),
            removed_default_exclusions: self
                .exclusions
                .removed_default_ids()
                .map(String::from)
                .collect(),
            auto_fix: self.auto_fix,
            style: self.style,
            open_settings_at_launch: self.open_settings_at_launch,
            input_method: self.engine.method(),
            auto_capitalize: self.auto_capitalize,
            toggle_hotkey: self.toggle_hotkey,
            macros: self.macros.clone(),
            restore_english_words: self.restore_english_words,
            language: self.language,
            always_macro: self.always_macro,
            welcome_shown: self.welcome_shown,
            word_overrides: self.word_override_list(),
            quick_telex: self.engine.quick_telex(),
            telex_brackets: self.engine.telex_brackets(),
            strict_spell_check: self.engine.strict_spell_check(),
        }
    }

    /// The current input method (Telex/VNI). Drives the Settings control.
    #[must_use]
    pub fn input_method(&self) -> InputMethod {
        self.engine.method()
    }

    /// Sets the input method (Telex/VNI). Flushes the in-progress word.
    pub fn set_input_method(&mut self, method: InputMethod) {
        self.engine.set_method(method);
        self.forget_position();
    }

    /// Whether to open the Settings window on launch.
    #[must_use]
    pub fn open_settings_at_launch(&self) -> bool {
        self.open_settings_at_launch
    }

    /// Sets the "open Settings on launch" preference.
    pub fn set_open_settings_at_launch(&mut self, on: bool) {
        self.open_settings_at_launch = on;
    }

    /// Whether auto-fix (restore invalid Vietnamese to raw keys) is enabled.
    #[must_use]
    pub fn auto_fix(&self) -> bool {
        self.auto_fix
    }

    /// Enables or disables auto-fix.
    pub fn set_auto_fix(&mut self, on: bool) {
        self.auto_fix = on;
    }

    /// Whether transformation is active *right now* — Vietnamese mode and the
    /// current application known and not excluded.
    ///
    /// Fails **closed**: until the shell has told the session which application is
    /// frontmost (via [`set_frontmost_app`](Self::set_frontmost_app)), nothing
    /// transforms. For a tool whose primary feature is *not* transforming in
    /// excluded apps, an unknown app must not transform — otherwise a shell that
    /// forgets to resolve the bundle id would transform everywhere, including the
    /// terminals and editors the ignore list exists to protect.
    #[must_use]
    pub fn is_active(&self) -> bool {
        if self.mode == InputMode::English {
            return false;
        }
        match &self.current_bundle_id {
            Some(id) => !self.exclusions.is_excluded(id),
            None => false,
        }
    }

    /// Processes a typed character, honoring exclusion and mode. When inactive the
    /// key passes through untouched and any in-progress word is flushed, so a
    /// mid-word transition to inactive (e.g. the user excludes the current app)
    /// cannot leave a stale diff baseline that later corrupts the document.
    pub fn process_key(&mut self, ch: char) -> KeyResponse {
        // A new word starts in *front* of the committed ones; it does not move
        // them, so the history stays and only the one-shot correction memory
        // ends. This used to clear both, which is why deleting a mistyped word
        // away left no way back into the word before it.
        self.start_new_word();
        if self.is_active() {
            let ch = self.maybe_capitalize(ch);
            self.engine.process_key(ch)
        } else if self.macros_active() {
            self.engine.process_key_verbatim(ch)
        } else {
            self.engine.reset();
            KeyResponse::passthrough()
        }
    }

    /// Whether macros should still run with Vietnamese switched off — UniKey's
    /// `alwaysMacro`.
    ///
    /// An excluded application is never included: excluded means hands off, and a
    /// terminal that silently expanded `vn` into `Việt Nam` would be a worse bug
    /// than the one exclusions exist to prevent. With no macros defined there is
    /// nothing to match, so the whole path stays off and English typing keeps its
    /// untouched passthrough.
    #[must_use]
    pub fn macros_active(&self) -> bool {
        self.always_macro
            && !self.macros.is_empty()
            && self.mode == InputMode::English
            && self
                .current_bundle_id
                .as_ref()
                .is_some_and(|id| !self.exclusions.is_excluded(id))
    }

    /// Whether macros expand while Vietnamese is off.
    #[must_use]
    pub fn always_macro(&self) -> bool {
        self.always_macro
    }

    /// Sets whether macros expand while Vietnamese is off.
    pub fn set_always_macro(&mut self, on: bool) {
        self.always_macro = on;
    }

    /// Forgets where the caret is, for both memories that depend on it:
    /// re-composition (`hồng`␣⌫`z`) and the correction hotkey.
    ///
    /// Every caller that clears one clears both, with no exceptions — each site
    /// is the same situation, namely that the caret may no longer be where the
    /// engine thinks, or that the words behind it would now render differently.
    /// Two fields cleared through one function cannot drift apart, which is the
    /// point: a `correctable` left behind after a caret move would let one
    /// keystroke rewrite text somewhere else entirely.
    fn forget_position(&mut self) {
        self.committed.clear();
        self.correctable = None;
    }

    /// A new word has started. The words *behind* the caret have not moved, so
    /// the history stays; only the one-shot correction memory ends.
    ///
    /// This is the whole fix for "typing past a word and deleting back should
    /// restore it". The two memories used to share one lifetime, and the
    /// committed word was destroyed by the first keystroke after the boundary —
    /// so re-composition only ever worked if the Backspace was *immediate*.
    fn start_new_word(&mut self) {
        self.correctable = None;
    }

    /// Records one more step of the document behind the caret, dropping the
    /// oldest once the cap is reached. Dropping from the *front* is what makes
    /// the cap safe: the entries that remain are still an unbroken run ending at
    /// the caret, so deleting back past the cap runs the stack empty and the
    /// engine stops vouching rather than re-opening the wrong word.
    fn push_behind(&mut self, entry: Behind) {
        self.committed.push_back(entry);
        while self.committed.len() > COMMITTED_HISTORY {
            self.committed.pop_front();
        }
    }

    /// Every recorded word decision, sorted by keys so the settings file has a
    /// stable order and a hand-edit produces a readable diff.
    #[must_use]
    pub fn word_override_list(&self) -> Vec<WordOverride> {
        let mut list: Vec<WordOverride> = self
            .word_overrides
            .iter()
            .map(|(keys, prefer)| WordOverride {
                keys: keys.clone(),
                prefer: *prefer,
            })
            .collect();
        list.sort_by(|a, b| a.keys.cmp(&b.keys));
        list
    }

    /// The decision recorded for `keys`, if any.
    #[must_use]
    pub fn word_override(&self, keys: &str) -> Option<WordPreference> {
        self.word_overrides.get(&keys.to_ascii_lowercase()).copied()
    }

    /// Records (or replaces) the decision for `keys`.
    pub fn set_word_override(&mut self, keys: &str, prefer: WordPreference) {
        let keys = keys.trim().to_ascii_lowercase();
        if keys.is_empty() {
            return;
        }
        self.word_overrides.insert(keys, prefer);
    }

    /// Forgets the decision for `keys`, returning whether there was one.
    pub fn remove_word_override(&mut self, keys: &str) -> bool {
        self.word_overrides
            .remove(&keys.to_ascii_lowercase())
            .is_some()
    }

    /// Whether the one-time welcome has already been shown.
    #[must_use]
    pub fn welcome_shown(&self) -> bool {
        self.welcome_shown
    }

    /// Marks the welcome as shown, so it never appears unbidden again. The menu's
    /// "Quick Guide" reopens it on demand, which is what keeps dismissing it a
    /// safe thing to do rather than a destructive one.
    pub fn set_welcome_shown(&mut self, shown: bool) {
        self.welcome_shown = shown;
    }

    /// Applies sentence-start capitalization to the first letter of a word when the
    /// option is on. Consumes the pending-capital flag on the first letter typed.
    fn maybe_capitalize(&mut self, ch: char) -> char {
        // A bracket shortcut is a vowel key, so a word can begin with one. Letting
        // it fall through as "not a letter" left `pending_capital` armed, and the
        // capital then landed on the *following* word.
        let bracket = self.engine.telex_brackets() && matches!(ch, '[' | ']' | '{' | '}');
        if (!ch.is_ascii_alphabetic() && !bracket) || self.engine.is_composing() {
            return ch; // not the first letter of a word
        }
        let out = if self.auto_capitalize && self.pending_capital {
            match ch {
                '[' => '{',
                ']' => '}',
                other => other.to_ascii_uppercase(),
            }
        } else {
            ch
        };
        self.pending_capital = false;
        out
    }

    /// Notes a word-boundary character so the next sentence can be capitalized:
    /// `.`/`!`/`?` starts a new sentence. Called by the shell at a boundary.
    pub fn note_boundary(&mut self, ch: char) {
        if matches!(ch, '.' | '!' | '?') {
            self.pending_capital = true;
        }
        // `commit` runs before this and cannot know which key ended the word, so
        // it leaves the boundary empty and this fills it in. A correction has to
        // step back over that character and put it back afterwards.
        //
        // **Only a character the host actually inserts counts.** Several keys
        // reach the boundary path while putting nothing at the caret — Escape,
        // the function keys, keypad Enter, Help and forward-delete all arrive as
        // control characters — and charging a backspace for one of them ate the
        // space belonging to the *previous* word and typed a control code into
        // the document. Tab and Return are control characters too, and they are
        // worse: they move the caret entirely, so a correction after Tab posted
        // its edit into the next field, and after Return in a send-on-enter app
        // into a conversation that had already been sent. One rule covers all of
        // them, and the conservative reading of the blind model (§5) is the right
        // one: if the caret may have moved, there is nothing to correct.
        if ch.is_control() {
            self.forget_position();
        } else if let Some(word) = self.correctable.as_mut() {
            word.boundary = Some(ch);
        }
    }

    /// Whether auto-capitalize is on.
    #[must_use]
    pub fn auto_capitalize(&self) -> bool {
        self.auto_capitalize
    }

    /// Sets auto-capitalize.
    pub fn set_auto_capitalize(&mut self, on: bool) {
        self.auto_capitalize = on;
    }

    /// The text-expansion macros (shortcut → expansion).
    #[must_use]
    pub fn macros(&self) -> &[Macro] {
        &self.macros
    }

    /// Adds or replaces a macro by shortcut (case-insensitive). Empty shortcut is
    /// ignored. Returns false if it was ignored.
    pub fn add_macro(&mut self, shortcut: &str, expansion: &str) -> bool {
        let shortcut = shortcut.trim();
        if shortcut.is_empty() {
            return false;
        }
        self.macros
            .retain(|m| !m.shortcut.eq_ignore_ascii_case(shortcut));
        self.macros.push(Macro {
            shortcut: shortcut.to_string(),
            expansion: expansion.to_string(),
        });
        true
    }

    /// Merges an imported macro table, returning `(added, skipped)`.
    ///
    /// A shortcut that already exists is **skipped, never overwritten** — an
    /// import must not silently replace something the user typed by hand. Note
    /// that [`add_macro`](Self::add_macro) is add-*or-replace* and answers `true`
    /// either way, so the collision has to be caught before calling it.
    ///
    /// The existing shortcuts are indexed once: a curated table can hold
    /// thousands of entries, and rescanning the list per entry would be quadratic
    /// on the shell's main thread, where it stalls the event tap.
    pub fn import_macros(
        &mut self,
        imported: &[Macro],
        on_conflict: MacroConflict,
    ) -> (usize, usize) {
        // Two different questions, so two sets. What the *table* already held is
        // what the user was asked about; what the *file* has already used is not a
        // choice at all — a shortcut listed twice is still one macro, and the
        // first spelling of it wins whichever answer was given. Conflating the two
        // made `Replace` count a repeated row as a second macro and let the later
        // line quietly beat the earlier one.
        let existing: std::collections::HashSet<String> = self
            .macros
            .iter()
            .map(|m| m.shortcut.to_lowercase())
            .collect();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let (mut added, mut skipped) = (0, 0);
        for entry in imported {
            let key = entry.shortcut.trim().to_lowercase();
            if key.is_empty() || !seen.insert(key.clone()) {
                skipped += 1;
                continue;
            }
            if existing.contains(&key) && on_conflict == MacroConflict::Skip {
                skipped += 1;
                continue;
            }
            if self.add_macro(&entry.shortcut, &entry.expansion) {
                added += 1;
            } else {
                skipped += 1;
            }
        }
        (added, skipped)
    }

    /// Whether a shortcut is already taken, so the caller can ask before
    /// overwriting it.
    ///
    /// [`add_macro`](Self::add_macro) replaces silently, which is the right
    /// primitive and the wrong interaction: the window that calls it also has an
    /// Import that refused to overwrite and reported what it skipped, so one
    /// window enforced two opposite rules. The engine keeps both behaviours
    /// available and the shell decides, having asked.
    #[must_use]
    pub fn has_macro(&self, shortcut: &str) -> bool {
        let shortcut = shortcut.trim();
        self.macros
            .iter()
            .any(|m| m.shortcut.eq_ignore_ascii_case(shortcut))
    }

    /// How many of `imported` would land on a shortcut that already exists.
    ///
    /// Counted before anything is written so the question can be asked once for
    /// the whole file rather than once per row.
    #[must_use]
    pub fn macro_conflicts(&self, imported: &[Macro]) -> usize {
        let existing: std::collections::HashSet<String> = self
            .macros
            .iter()
            .map(|m| m.shortcut.to_lowercase())
            .collect();
        // Counts shortcuts, not rows: a file listing the same colliding shortcut
        // twice is one thing to decide about, and the number here is read by a
        // person deciding it. A repeat *within* the file is not a conflict — it
        // collapses either way and there is nothing to choose.
        let mut counted: std::collections::HashSet<String> = std::collections::HashSet::new();
        imported
            .iter()
            .filter(|entry| {
                let key = entry.shortcut.trim().to_lowercase();
                !key.is_empty() && existing.contains(&key) && counted.insert(key)
            })
            .count()
    }

    /// Removes the macro at `index` (as listed by [`macros`](Self::macros)).
    pub fn remove_macro(&mut self, index: usize) {
        if index < self.macros.len() {
            self.macros.remove(index);
        }
    }

    /// Whether the opt-in English word restore is on.
    #[must_use]
    pub fn restore_english_words(&self) -> bool {
        self.restore_english_words
    }

    /// Enables or disables the English word restore.
    pub fn set_restore_english_words(&mut self, on: bool) {
        self.restore_english_words = on;
    }

    /// The current toggle-hotkey preset.
    #[must_use]
    pub fn toggle_hotkey(&self) -> HotkeyPreset {
        self.toggle_hotkey
    }

    /// Sets the toggle-hotkey preset.
    pub fn set_toggle_hotkey(&mut self, preset: HotkeyPreset) {
        self.toggle_hotkey = preset;
    }

    /// Processes a Backspace, honoring exclusion and mode.
    pub fn backspace(&mut self) -> KeyResponse {
        if self.is_active() {
            self.engine.backspace()
        } else {
            self.engine.reset();
            KeyResponse::passthrough()
        }
    }

    /// Records the frontmost application and flushes any in-progress word so it
    /// cannot leak across a focus change.
    pub fn set_frontmost_app(&mut self, bundle_id: impl Into<String>) {
        // A change of app, mode or exclusion means the caret is somewhere else or
        // the word is no longer ours to edit. `forget_position`'s own comment
        // claimed this happened; it did not, and an app that activates itself (a
        // call popup, a finished build) changes focus with no event to flush on.
        self.forget_position();
        self.current_bundle_id = Some(bundle_id.into());
        self.engine.reset();
    }

    /// The frontmost application's bundle identifier, if known.
    #[must_use]
    pub fn current_bundle_id(&self) -> Option<&str> {
        self.current_bundle_id.as_deref()
    }

    /// Toggles a specific application in the ignore list. Each app's membership is
    /// independent — toggling one never changes another. Also records it as the
    /// current app so the change takes effect on the next keystroke.
    ///
    /// Un-excluding a known **terminal** is session-only: the live check stops
    /// excluding it, but the persisted list keeps it, so a restart re-excludes.
    /// Terminals always mangle Vietnamese (a PTY ignores synthetic backspaces), so
    /// an accidental hotkey press must not permanently remove the protection; a
    /// deliberate, permanent removal goes through the Excluded Apps editor
    /// ([`exclusions_mut`](Self::exclusions_mut) → `remove`).
    pub fn toggle_app_exclusion(&mut self, bundle_id: &str) -> ExclusionToggle {
        // A change of app, mode or exclusion means the caret is somewhere else or
        // the word is no longer ours to edit. `forget_position`'s own comment
        // claimed this happened; it did not, and an app that activates itself (a
        // call popup, a finished build) changes focus with no event to flush on.
        self.forget_position();
        self.current_bundle_id = Some(bundle_id.to_string());
        self.engine.reset();
        if self.exclusions.is_excluded(bundle_id) {
            if exclusion::is_terminal(bundle_id) {
                self.exclusions.suspend_for_session(bundle_id);
                ExclusionToggle::EnabledSessionOnly
            } else {
                self.exclusions.remove(bundle_id);
                ExclusionToggle::Enabled
            }
        } else {
            if !self.exclusions.resume(bundle_id) {
                self.exclusions.add(bundle_id.to_string());
            }
            ExclusionToggle::Excluded
        }
    }

    /// Toggles VN/EN mode and flushes the current word. Has no effect on whether an
    /// excluded application transforms — exclusion still wins.
    pub fn toggle_mode(&mut self) -> InputMode {
        // A change of app, mode or exclusion means the caret is somewhere else or
        // the word is no longer ours to edit. `forget_position`'s own comment
        // claimed this happened; it did not, and an app that activates itself (a
        // call popup, a finished build) changes focus with no event to flush on.
        self.forget_position();
        self.mode = match self.mode {
            InputMode::Vietnamese => InputMode::English,
            InputMode::English => InputMode::Vietnamese,
        };
        self.engine.reset();
        self.mode
    }

    /// The current VN/EN mode (independent of exclusion).
    #[must_use]
    pub fn mode(&self) -> InputMode {
        self.mode
    }

    /// Mutable access to the ignore list, for the editor and the menu bar action.
    pub fn exclusions_mut(&mut self) -> &mut ExclusionList {
        &mut self.exclusions
    }

    /// Read access to the ignore list.
    #[must_use]
    pub fn exclusions(&self) -> &ExclusionList {
        &self.exclusions
    }

    /// Changes the placement style for subsequent words.
    pub fn set_style(&mut self, style: PlacementStyle) {
        self.style = style;
        self.engine.set_style(style);
        self.forget_position();
    }

    /// The current placement style.
    #[must_use]
    pub fn style(&self) -> PlacementStyle {
        self.style
    }

    /// Whether transformation is currently active (see [`is_active`](Self::is_active))
    /// AND a word is in progress — i.e. there is marked text on screen.
    #[must_use]
    pub fn is_composing(&self) -> bool {
        self.engine.is_composing()
    }

    /// The current composing word for a marked-text shell to display.
    #[must_use]
    pub fn current_word(&self) -> &str {
        self.engine.current_word()
    }

    /// A diagnostic snapshot for logging: `(raw keys, rendered word, mode, active)`.
    /// Lets the shell record engine state alongside each emit without exposing the
    /// engine internals.
    #[must_use]
    pub fn debug_state(&self) -> (String, String, InputMode, bool) {
        (
            self.engine.raw_string(),
            self.engine.current_word().to_string(),
            self.mode(),
            self.is_active(),
        )
    }

    /// Finalizes the composing word: returns its text and clears the engine, so the
    /// shell can commit it (insert it as ordinary text) at a word boundary.
    pub fn commit_word(&mut self) -> String {
        let word = self.engine.current_word().to_string();
        self.engine.reset();
        word
    }

    /// Finalizes the word at a boundary, applying auto-fix. If auto-fix is on and
    /// the composed word is not valid Vietnamese, returns a restore edit that
    /// replaces the rendering with the raw keystrokes (so `eĩt` becomes `exit`);
    /// otherwise returns `None`. Always clears the engine afterward, so the shell
    /// should call this once when it sees a word-boundary key, apply any returned
    /// edit, then let the boundary key through.
    pub fn commit(&mut self) -> Option<KeyResponse> {
        // Macro expansion (gõ tắt) takes precedence over auto-fix: if the typed keys
        // match a shortcut, replace the on-screen word with the expansion.
        if self.engine.is_composing() && !self.macros.is_empty() {
            let typed = self.engine.raw_string();
            if let Some(expansion) = self
                .macros
                .iter()
                .find(|m| m.shortcut.eq_ignore_ascii_case(&typed))
                .map(|m| m.expansion.clone())
            {
                let on_screen_len = self.engine.current_word().encode_utf16().count();
                self.engine.reset();
                self.forget_position(); // an expansion has no second reading
                return Some(KeyResponse {
                    handled: true,
                    backspaces: on_screen_len,
                    insert: expansion,
                });
            }
        }
        let restore = if self.engine.is_composing() {
            let rendered = self.engine.current_word().to_string();
            let raw = self.engine.raw_string();
            // A decision the user made about this exact word wins over every rule,
            // in both directions, and is the only thing that can force the
            // Vietnamese reading of a word auto-fix would otherwise restore.
            // Rules generalise; this word is the case where generalising was wrong.
            let wanted = match self.word_overrides.get(&raw.to_ascii_lowercase()) {
                Some(WordPreference::Raw) => Some(raw.clone()),
                Some(WordPreference::Vietnamese) => Some(rendered.clone()),
                // No decision recorded: fall back to the rules. Two independent
                // reasons to restore the raw keys —
                // - auto-fix: the rendering is not valid Vietnamese (`eĩt` → `exit`);
                // - English restore (opt-in): the raw keys are a common English
                //   word, even when the rendering IS valid Vietnamese (`ứa` → `was`).
                None => {
                    let invalid = self.auto_fix && is_invalid_vietnamese(&rendered);
                    let english = self.restore_english_words && english::is_common_english(&raw);
                    (invalid || english).then(|| raw.clone())
                }
            };
            // Only emit when it actually changes something. The backspace count is
            // the rendered word's full UTF-16 length because a restore replaces the
            // whole word — `tests/properties.rs` asserts exactly that.
            wanted
                .filter(|want| *want != rendered)
                .map(|want| KeyResponse {
                    handled: true,
                    backspaces: rendered.encode_utf16().count(),
                    insert: want,
                })
        } else {
            None
        };
        // Record what this boundary adds to the document behind the caret.
        //
        // A word that auto-fix restored to its raw keys **clears the whole
        // history** rather than simply not being pushed. It still occupies space
        // on screen, so leaving it out would break the one invariant the stack
        // rests on — that its entries are an unbroken account of the document
        // immediately behind the caret. Without this, `hồng`␣`work`␣ (where
        // auto-fix restored `work`) would leave `hồng` on top of the stack while
        // `work ` sat between it and the caret, and deleting back would re-open a
        // word five characters from where the engine thought it was.
        if !self.engine.is_composing() {
            // A boundary straight after another boundary — the `␣` of `hồng, `.
            // It gets an entry of its own for exactly the reason above: it is on
            // screen, so the account has to include it. Discarding the history
            // here instead is what put the original bug one comma away.
            self.push_behind(Behind::Boundary);
        } else if restore.is_none() {
            self.push_behind(Behind::Word(CommittedWord {
                raw: self.engine.raw_vec(),
                rendered: self.engine.current_word().to_string(),
            }));
        } else {
            self.committed.clear();
        }
        // Remember it for the correction hotkey too, and unlike the line above,
        // remember it **whether or not** it was restored: a word auto-fix already
        // rewrote is the one the user most often wants to argue with. The boundary
        // character is filled in by `note_boundary`, which the shell calls next.
        self.correctable = if self.engine.is_composing() {
            let raw = self.engine.raw_string();
            let rendered = self.engine.current_word().to_string();
            // Whatever the restore inserted is what the shell will put on screen;
            // with no restore, the rendering is already there.
            let on_screen = match &restore {
                Some(edit) => edit.insert.clone(),
                None => rendered.clone(),
            };
            // Nothing to correct when both readings are the same word.
            (raw != rendered).then_some(CorrectableWord {
                raw,
                rendered,
                on_screen,
                boundary: None,
            })
        } else {
            None
        };
        self.engine.reset();
        restore
    }

    /// Swaps the word just committed to its other reading and records that choice,
    /// returning the edit the shell must apply. `None` when there is nothing to
    /// correct.
    ///
    /// The edit reaches back **over the boundary character** into text already
    /// committed, which is further than anything else in GlowKey goes, and the
    /// blind model cannot verify that the caret is still there. That is why the
    /// memory is cleared by everything that could move it, and why this is
    /// one-shot: pressing the key twice must not toggle back and forth, because
    /// the second press would be recorded as a fresh decision and the list would
    /// learn whichever direction the user happened to stop on.
    pub fn correct_last_word(&mut self) -> Option<KeyResponse> {
        let word = self.correctable.take()?;
        // The word just changed identity on screen, so nothing about it is
        // re-composable any more. Clearing only `correctable` and leaving
        // the committed history behind is a guaranteed corruption: the following
        // Backspace re-composes the *old* rendering, and the next letter is then
        // diffed against a baseline that no longer matches the screen —
        // `was `⌃⇧W⌫`f` produced `wừa`. Three ordinary keystrokes.
        self.forget_position();
        if self.engine.is_composing() {
            // Mid-word: the caret is not just after that boundary any more.
            return None;
        }
        // No boundary means the host inserted nothing after the word, so the
        // caret's position is not something this can reason about.
        let boundary = word.boundary?;
        // Swap to whichever reading is not the one on screen. Comparing against
        // the raw keys rather than the rendering means a third string — an
        // ASCII-render restore, say — falls back to restoring what was typed,
        // which is always a defensible answer.
        let (replacement, prefer) = if word.on_screen == word.raw {
            (word.rendered.clone(), WordPreference::Vietnamese)
        } else {
            (word.raw.clone(), WordPreference::Raw)
        };
        self.set_word_override(&word.raw, prefer);
        // Delete the word and its boundary, then insert the other reading and put
        // the boundary back — one edit, one ordered post, so nothing can arrive
        // out of order (`docs/handoff.md` §5).
        let mut insert = replacement;
        insert.push(boundary);
        Some(KeyResponse {
            handled: true,
            backspaces: word.on_screen.encode_utf16().count() + boundary.len_utf16(),
            insert,
        })
    }

    /// The word a correction would act on and what it would become, for the
    /// on-screen confirmation. `None` when there is nothing to correct.
    #[must_use]
    pub fn correctable_word(&self) -> Option<(String, String)> {
        let word = self.correctable.as_ref()?;
        word.boundary?;
        let becomes = if word.on_screen == word.raw {
            word.rendered.clone()
        } else {
            word.raw.clone()
        };
        Some((word.on_screen.clone(), becomes))
    }

    /// On a Backspace that deletes a boundary character, restores the word in
    /// front of it into the composing buffer so the following keys keep editing
    /// it — Telex re-composition, e.g. `hồng`␣⌫`z` → `hông`. The caller passes
    /// the Backspace through either way, so the host deletes the boundary
    /// character; the answer says whether the engine is still in step.
    ///
    /// A no-op while composing or once the caller has deleted back past what the
    /// engine remembers.
    pub fn recompose_after_boundary_backspace(&mut self) -> BoundaryBackspace {
        if self.engine.is_composing() {
            // Mid-word backspace: a normal delete, not a boundary re-opening.
            // The words behind this one are untouched, so the history stays.
            self.start_new_word();
            return BoundaryBackspace::NotApplicable;
        }
        match self.committed.pop_back() {
            Some(Behind::Word(word)) => {
                self.engine.restore(word.raw, word.rendered);
                BoundaryBackspace::Reopened
            }
            // A bare boundary came off — `hồng, `⌫ leaves `hồng,` — and the word
            // is one more Backspace away. The stack behind it still describes the
            // document, so this is emphatically not a flush.
            Some(Behind::Boundary) => BoundaryBackspace::BoundaryRemoved,
            // Nothing remembered, or we have deleted back past the cap: the
            // engine cannot vouch for the caret. Forget the position here rather
            // than trusting the caller to flush — a `correctable` surviving this
            // Backspace would let ⌃⇧W post an edit that over-deletes by the
            // boundary character this keystroke just removed.
            None => {
                self.forget_position();
                BoundaryBackspace::NotApplicable
            }
        }
    }

    /// Mid-word Backspace: shrink the composition by one visible character so the
    /// keys that follow keep editing the same word (`hoongf`⌫`z` → `hôn`, because
    /// `z` still reaches the engine as the tone-removal key).
    ///
    /// Answers with a [`BackspaceOutcome`]: `InStep` (the host performs the
    /// delete, as it always has), `Repair` (the keystroke must be **suppressed**
    /// and this edit applied instead — it undoes a spell-check escape), or
    /// `Flush` (the engine cannot stay in step and the caller must flush).
    pub fn backspace_visible_char(&mut self) -> BackspaceOutcome {
        if !self.is_active() {
            return BackspaceOutcome::Flush;
        }
        self.engine.backspace_visible_char()
    }

    /// The user-interface language preference.
    #[must_use]
    pub fn language(&self) -> Language {
        self.language
    }

    /// Sets the user-interface language preference.
    pub fn set_language(&mut self, language: Language) {
        self.language = language;
    }

    /// Whether Quick Telex is on.
    #[must_use]
    pub fn quick_telex(&self) -> bool {
        self.engine.quick_telex()
    }

    /// Turns Quick Telex on or off.
    pub fn set_quick_telex(&mut self, on: bool) {
        self.engine.set_quick_telex(on);
        // The engine reset does not reach the committed history, and a word
        // remembered under the old setting would re-compose under the new one —
        // rewriting text already on screen.
        self.forget_position();
    }

    /// Whether the Telex bracket shortcuts are on.
    #[must_use]
    pub fn telex_brackets(&self) -> bool {
        self.engine.telex_brackets()
    }

    /// Turns the Telex bracket shortcuts on or off.
    pub fn set_telex_brackets(&mut self, on: bool) {
        self.engine.set_telex_brackets(on);
        // The engine reset does not reach the committed history, and a word
        // remembered under the old setting would re-compose under the new one —
        // rewriting text already on screen.
        self.forget_position();
    }

    /// Whether the mid-word spell check is on.
    #[must_use]
    pub fn strict_spell_check(&self) -> bool {
        self.engine.strict_spell_check()
    }

    /// Turns the mid-word spell check on or off.
    pub fn set_strict_spell_check(&mut self, on: bool) {
        self.engine.set_strict_spell_check(on);
        // The engine reset does not reach the committed history, and a word
        // remembered under the old setting would re-compose under the new one —
        // rewriting text already on screen.
        self.forget_position();
    }

    /// Flushes any in-progress word without changing mode or focus.
    ///
    /// The engine's edits ([`KeyResponse::backspaces`]) assume the current word's
    /// rendering is still the tail of the document. The shell **must** call this
    /// whenever that stops being true — composition commit, session deactivation,
    /// and any caret or selection move the engine did not cause (arrow keys, a
    /// mouse click, a host-side autocorrect). Skipping it lets a later keystroke
    /// delete text the engine never wrote.
    pub fn flush(&mut self) {
        self.engine.reset();
        self.forget_position();
        // A caret move / click lands us in unknown context; don't guess a sentence
        // start, so the next letter is not wrongly capitalized.
        self.pending_capital = false;
    }
}

/// Whether `word` is a non-empty string that is not a valid Vietnamese syllable —
/// the condition under which auto-fix restores the raw keystrokes. Uses `vi`'s
/// syllable validator; a plain ASCII word that never transformed is treated as
/// valid (nothing to fix) since it equals its raw input.
fn is_invalid_vietnamese(word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    // A pure-ASCII word is what the user typed verbatim — leave it alone.
    if word.is_ascii() {
        return false;
    }
    // A word starting with đ is deliberate, so keep it even when it is not a
    // syllable. Reaching a leading đ costs `dd` in Telex or `d9` in VNI, and no
    // English word begins with either, so there is nothing here to rescue — while
    // restoring the raw keys wrecks the Vietnamese chat abbreviations built this
    // way (`đc`, `đt`, `đk`, which would come back as `ddc`, `ddt`, `ddk`). English
    // words that merely *contain* the pair still restore, since their đ is not
    // leading: `address`→`ađress`, `odd`→`ođ`, `sudden`→`suđen`.
    if word.starts_with('đ') || word.starts_with('Đ') {
        return false;
    }
    !vi::validation::is_valid_syllable(word) || violates_stop_coda_tone(word)
}

/// Whether the syllable breaks Vietnamese's stop-coda tone rule.
///
/// A syllable closed by `c`, `ch`, `p` or `t` can only carry sắc or nặng — the
/// two "sharp" tones. Huyền, hỏi and ngã are impossible there. UniKey enforces
/// this in `lastWordIsNonVn` (`ukengine.cpp:2352`); the `vi` crate does not, and
/// happily calls `màc`, `hỏc`, `mãt` and `hòp` valid.
///
/// It matters in daily use because Telex's `f`, `r` and `x` are exactly those
/// three tones, so ordinary English words were being transformed and then not
/// rescued: `left`→`lèt`, `soft`→`sòt`, `gift`→`gìt`, `lift`→`lìt`. Auto-fix
/// left them alone because it had been told they were valid Vietnamese.
fn violates_stop_coda_tone(word: &str) -> bool {
    /// Vowels carrying huyền, hỏi or ngã — the tones a stop coda forbids.
    const FORBIDDEN_TONES: &str = "àèìòùỳằầềồờừÀÈÌÒÙỲẰẦỀỒỜỪ                                   ảẻỉỏủỷẳẩểổởửẢẺỈỎỦỶẲẨỂỔỞỬ                                   ãẽĩõũỹẵẫễỗỡữÃẼĨÕŨỸẴẪỄỖỠỮ";

    let lowered = word.to_lowercase();
    let stop_coda = lowered.ends_with("ch")
        || lowered.ends_with('c')
        || lowered.ends_with('p')
        || lowered.ends_with('t');
    stop_coda && word.chars().any(|ch| FORBIDDEN_TONES.contains(ch))
}

/// Computes the minimal edit turning `prev` into `next`: keep the common prefix,
/// delete the rest of `prev` (counted in UTF-16 code units), insert the rest of
/// `next`. This is the `backspaceCount` / `newCharCount` shape shipping engines use.
fn diff(prev: &str, next: &str) -> KeyResponse {
    // Longest common prefix in whole characters (never split a scalar).
    let common_bytes = prev
        .char_indices()
        .zip(next.char_indices())
        .take_while(|((_, a), (_, b))| a == b)
        .map(|((i, c), _)| i + c.len_utf8())
        .last()
        .unwrap_or(0);

    let deleted = &prev[common_bytes..];
    let inserted = &next[common_bytes..];

    KeyResponse {
        handled: true,
        backspaces: deleted.encode_utf16().count(),
        insert: inserted.to_string(),
    }
}
