//! The session: mode, exclusions, corrections and macros over the engine (leaves this crate in a later phase).

use super::*;

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

/// How many entries of the document behind the caret to remember. Deleting back
/// further than this leaves the engine unable to vouch for where the caret is,
/// so it flushes instead of guessing.
pub(crate) const COMMITTED_HISTORY: usize = 5;

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
    /// Capitalize the first letter of each sentence.
    auto_capitalize: bool,
    /// True when the next typed letter starts a sentence (document start, or after
    /// `.`/`!`/`?`). Consumed by the first letter of the following word.
    pending_capital: bool,
    /// Text-expansion macros (shortcut → expansion).
    macros: Vec<Macro>,
    /// Opt-in: at a boundary, restore a committed word to its raw keys when those
    /// keys form a common English word — even if the rendering is valid
    /// Vietnamese (`was`→`ứa`). Off by default: it inverts the ambiguity for
    /// Vietnamese words typed with a trailing tone key (`cats`→`cát`).
    restore_english_words: bool,
    /// UniKey's `alwaysMacro`: expand macros even while Vietnamese is off.
    always_macro: bool,
    /// The word just committed, for the correction hotkey. One-shot: cleared by
    /// the correction itself and by anything that could move the caret.
    correctable: Option<CorrectableWord>,
    /// Per-word decisions, indexed for lookup at the word boundary. The persisted
    /// form is a `Vec<WordOverride>` in `Settings` — stable and diffable, like
    /// `macros`; this map is the index over it, rebuilt on load.
    word_overrides: HashMap<String, WordPreference>,
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
            auto_capitalize: false,
            pending_capital: true,
            macros: Vec::new(),
            restore_english_words: false,
            always_macro: false,
            correctable: None,
            word_overrides: HashMap::new(),
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

    /// Replaces the macro table wholesale — how a persisted list is put back.
    pub fn set_macros(&mut self, macros: Vec<Macro>) {
        self.macros = macros;
    }

    /// Replaces the personal word list wholesale — how a persisted list is put
    /// back. Keys are compared case-insensitively, as `set_word_override` does.
    pub fn set_word_overrides(&mut self, overrides: &[WordOverride]) {
        self.word_overrides = overrides
            .iter()
            .map(|o| (o.keys.to_ascii_lowercase(), o.prefer))
            .collect();
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
pub(crate) fn is_invalid_vietnamese(word: &str) -> bool {
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
pub(crate) fn violates_stop_coda_tone(word: &str) -> bool {
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
pub(crate) fn diff(prev: &str, next: &str) -> KeyResponse {
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
