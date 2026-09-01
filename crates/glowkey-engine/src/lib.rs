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

use vi::methods::IncrementalBuffer;
use vi::processor::AccentStyle;

pub mod exclusion;

pub use exclusion::ExclusionList;

/// Tone-mark placement convention.
///
/// New style is the modern default (`hoà`, `thuý`); old style is the traditional
/// convention (`hòa`, `thúy`). Mirrors [`AccentStyle`] but keeps `vi` out of the
/// shell's type surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlacementStyle {
    /// Modern orthography — the software default.
    #[default]
    New,
    /// Traditional orthography.
    Old,
}

impl From<PlacementStyle> for AccentStyle {
    fn from(style: PlacementStyle) -> Self {
        match style {
            PlacementStyle::New => AccentStyle::New,
            PlacementStyle::Old => AccentStyle::Old,
        }
    }
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
    /// Raw keystrokes of the word being typed, in their original case.
    raw: Vec<char>,
    /// The text currently on screen for this word — the diff baseline.
    rendered: String,
}

impl Engine {
    /// Creates an engine with the given placement style.
    #[must_use]
    pub fn new(style: PlacementStyle) -> Self {
        Self {
            style,
            raw: Vec::new(),
            rendered: String::new(),
        }
    }

    /// Changes the placement style. Takes effect on the next word.
    pub fn set_style(&mut self, style: PlacementStyle) {
        self.style = style;
        // Any in-progress word keeps its style; flush so the next word uses the new one.
        self.reset();
    }

    /// Clears all in-progress state. Call on focus change, app switch, or after a
    /// word boundary — a stale word must never bleed into a new field.
    pub fn reset(&mut self) {
        self.raw.clear();
        self.rendered.clear();
    }

    /// Whether a word is currently being composed.
    #[must_use]
    pub fn is_composing(&self) -> bool {
        !self.raw.is_empty()
    }

    /// Feeds one typed character to the engine.
    ///
    /// A character that can extend a Vietnamese syllable (an ASCII letter) is added
    /// to the word and the resulting edit is returned. Anything else is a word
    /// boundary: the engine flushes and reports the key as unhandled so the host
    /// inserts it verbatim.
    pub fn process_key(&mut self, ch: char) -> KeyResponse {
        if !is_syllable_char(ch) {
            // Word boundary (space, digit, punctuation). Commit and hand the key back.
            self.reset();
            return KeyResponse::passthrough();
        }
        self.raw.push(ch);
        self.rerender()
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
        let mut response = self.rerender();
        // Always consumed: even an empty word means we just deleted our last char.
        response.handled = true;
        response
    }

    /// Re-derives the rendered word from the raw key log and returns the edit that
    /// turns the previous rendering into the new one.
    fn rerender(&mut self) -> KeyResponse {
        let next = render(&self.raw, self.style);
        let response = diff(&self.rendered, &next);
        self.rendered = next;
        response
    }
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
fn render(raw: &[char], style: PlacementStyle) -> String {
    let lowered: String = raw.iter().map(|c| c.to_ascii_lowercase()).collect();
    let mut buffer = IncrementalBuffer::new_with_style(&vi::TELEX, style.into());
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

/// True when `ch` can be part of a Vietnamese syllable typed in Telex — i.e. an
/// ASCII letter. Everything else (space, digit, punctuation) ends the word.
fn is_syllable_char(ch: char) -> bool {
    ch.is_ascii_alphabetic()
}

/// Whether the session currently transforms input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
    /// Bundle identifier of the frontmost application, set by the shell on focus
    /// change. `None` before the first application is known.
    current_bundle_id: Option<String>,
}

impl Session {
    /// Creates a session with the given placement style and ignore list.
    #[must_use]
    pub fn new(style: PlacementStyle, exclusions: ExclusionList) -> Self {
        Self {
            engine: Engine::new(style),
            mode: InputMode::default(),
            exclusions,
            current_bundle_id: None,
        }
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
        if self.is_active() {
            self.engine.process_key(ch)
        } else {
            self.engine.reset();
            KeyResponse::passthrough()
        }
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
        self.current_bundle_id = Some(bundle_id.into());
        self.engine.reset();
    }

    /// Toggles VN/EN mode and flushes the current word. Has no effect on whether an
    /// excluded application transforms — exclusion still wins.
    pub fn toggle_mode(&mut self) -> InputMode {
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
        self.engine.set_style(style);
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
    }
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
