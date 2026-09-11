//! The engine: raw keystrokes in, a minimal edit out.

use super::*;

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
    /// Nothing composed, or the delete empties the word. The caller flushes and
    /// lets the delete happen normally.
    Flush,
    /// The engine is in step with what the host's delete will leave behind. The
    /// caller passes the keystroke through, as it always has.
    InStep,
    /// The engine could not simply shrink by a character, so it rewrote the word.
    /// Two things reach this: the mid-word spell check had refused the word and
    /// deleting this character makes it spellable again, so the transformation
    /// comes back; or the last visible character was not the work of a single key
    /// and the last *keystroke* was undone instead (`ooo`⌫ → `ô`).
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
    /// A key the engine did not consume: the host should insert it itself.
    /// `handled` is false and there is nothing to delete or insert.
    #[must_use]
    pub fn passthrough() -> Self {
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

    /// The raw keystrokes of the word being composed, exactly as typed —
    /// every key, including the ones that only carried a gesture.
    ///
    /// This is the diagnostic and macro-matching view. What auto-fix puts back on
    /// screen is [`typed_word`](Self::typed_word), which is the same keys minus
    /// the presses that were an instruction rather than a letter.
    #[must_use]
    pub fn raw_string(&self) -> String {
        self.raw.iter().collect()
    }

    /// The word the user typed, as auto-fix must put it back: the raw keys with
    /// the **repeat-key rejections** removed.
    ///
    /// A third press of a doubling key takes the diacritic back and stands for
    /// nothing itself — `ooo` is two letters on screen, not three. Restoring the
    /// raw log verbatim spelled that instruction out as a letter: `chooose` is
    /// how `choose` is typed once the `oo` has to be stopped from becoming `ô`,
    /// and auto-fix handed back the very `chooose` the user had just worked to
    /// avoid. Reported 2026-09-11.
    ///
    /// Tone and diacritic keys are **not** removed, and that is the distinction
    /// this draws: `x` in `exit` is a key the user pressed for a letter of an
    /// English word, so a restore owes it back. The rejected third press is the
    /// only key that asks the engine to undo something rather than to add
    /// something, so it is the only one dropped.
    ///
    /// An escaped word is its own raw keys already — nothing was cancelled —
    /// so it answers with them.
    #[must_use]
    pub fn typed_word(&self) -> String {
        if self.escaped {
            return self.raw_string();
        }
        let mut out = String::with_capacity(self.raw.len());
        let mut run = 0usize;
        for (i, ch) in self.raw.iter().enumerate() {
            let lower = ch.to_ascii_lowercase();
            run = if i > 0 && self.raw[i - 1].to_ascii_lowercase() == lower {
                run + 1
            } else {
                1
            };
            // Exactly the third press. The fourth onward are letters again — the
            // rejection has already happened and `render` puts them on screen
            // (see [`is_cancelled_repeat`]) — so `oooo` is four keys and four
            // characters minus the one instruction: `ooo`.
            if run == 3 && is_cancelled_repeat(lower, self.method) {
                continue;
            }
            out.push(*ch);
        }
        out
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
    ///
    /// **The escape state comes back too, and is derived rather than passed.** A
    /// word the mid-word spell check refused is rendered as its raw keys, so its
    /// stored rendering is not what those keys naturally produce — and that
    /// difference is the only thing that can cause it. Restoring without it left
    /// the re-opened word believing it had never been escaped, which broke the
    /// Backspace after it in a way that looked like a defect in re-composition:
    /// the unescape could not fire, so `backspace_visible_char` went looking for
    /// a raw removal that re-renders to "the render minus its last character" —
    /// impossible for a word whose render *is* its raw keys — and the engine
    /// flushed instead of lifting the escape.
    ///
    /// Reported as `hoongfb`␣`ss`⌫⌫⌫⌫ not returning to `hồng` (2026-09-06), where
    /// the identical keystrokes without the intervening boundary always had.
    /// Derived instead of added to the signature because the answer is already
    /// in the arguments, and because `restore` is published API.
    pub fn restore(&mut self, raw: Vec<char>, rendered: String) {
        self.raw = raw;
        self.escaped = self.render_keys(&self.raw) != rendered;
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
    /// the state that produced the earlier text. Returns `KeyResponse::passthrough`
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
    /// When no single removal reproduces the target, the last visible character
    /// is not the work of one key — it exists only through a transformation — so
    /// the last *keystroke* is undone instead and the word is rewritten as a
    /// [`BackspaceOutcome::Repair`]: `ooo`⌫ is `ô`, the state the first two keys
    /// produced, not the bare `o` that deleting a character would leave.
    ///
    /// Returns [`BackspaceOutcome::Flush`] when neither works — when undoing that
    /// keystroke would empty the word or would not shorten it by exactly one
    /// character (`oo`⌫, `viê`⌫). The caller then flushes and stops composing.
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
        // No removal reproduces the screen minus a character, which means the
        // last visible character is not the work of one key: it exists only
        // because of a transformation. Undo the last *keystroke* instead and
        // rewrite the word.
        //
        // `ooo`⌫ is the case that named this. The third `o` rejects the
        // circumflex, so the screen reads `oo` for three keys; deleting a
        // character used to flush, leaving a bare `o` with the second one
        // stranded as a literal. Popping the `o` puts the word back in the state
        // the first two keys produced — `ô` — and keeps it composing.
        //
        // Only accepted when it lands exactly one visible character back, which
        // is the one thing a Backspace must do. Undoing a key that shortens the
        // word by nothing would answer the user's delete with an edit that
        // leaves the screen the same length: `viêt`⌫⌫ is the case that pins it —
        // dropping the second `e` of `viee` renders `vie`, still three
        // characters, so the delete would appear to do nothing. Flushing is the
        // honest answer there, and it is what the engine has always done.
        //
        // Only reachable unescaped: while escaped the render *is* the raw keys,
        // so dropping the last one always reproduces the target and the loop
        // above has already returned.
        let mut candidate = self.raw.clone();
        candidate.pop();
        if candidate.is_empty() {
            // Nothing left to compose. Let the host's delete stand.
            return BackspaceOutcome::Flush;
        }
        let undone = self.render_keys(&candidate);
        if undone.chars().count() + 1 != on_screen.chars().count() {
            return BackspaceOutcome::Flush;
        }
        self.raw = candidate;
        self.rendered = undone;
        BackspaceOutcome::Repair(KeyResponse {
            handled: true,
            backspaces: on_screen.encode_utf16().count(),
            insert: self.rendered.clone(),
        })
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
        !cannot_become_vietnamese(&candidate)
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
pub(crate) fn expand_quick_telex(raw: &[char]) -> Vec<char> {
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
pub(crate) fn expand_telex_brackets(raw: &[char]) -> Vec<char> {
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
pub(crate) fn render(
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
    let definition = match method {
        InputMethod::Telex => &vi::TELEX,
        InputMethod::Vni => &vi::VNI,
        InputMethod::SimpleTelex => &SIMPLE_TELEX,
    };
    let mut buffer = IncrementalBuffer::new_with_style(definition, style.into());
    // The keys `vi` was given, and the letters withheld from it with where they
    // belong in the rendering (counted in characters).
    let mut fed = String::new();
    let mut withheld: Vec<(usize, char)> = Vec::new();
    // The keys that put a character on screen, in order — the word's own
    // letters, with the diacritic and tone keys left out. See [`apply_case`].
    let mut letters: Vec<char> = Vec::new();
    let mut run = 0usize;
    for (i, ch) in raw.iter().enumerate() {
        let lower = ch.to_ascii_lowercase();
        run = if i > 0 && raw[i - 1].to_ascii_lowercase() == lower {
            run + 1
        } else {
            1
        };
        if run > 3 && is_cancelled_repeat(lower, method) {
            withheld.push((buffer.view().chars().count(), *ch));
            // Withheld from `vi`, but it is a letter of the word.
            letters.push(*ch);
            continue;
        }
        let before = buffer.view().chars().count();
        buffer.push(lower);
        fed.push(lower);
        // A key that lengthened the rendering spelled a letter; one that did not
        // was consumed as a mark.
        if buffer.view().chars().count() > before {
            letters.push(*ch);
        }
    }
    let out = buffer.view();

    // No Vietnamese transformation occurred: emit the keys exactly as typed so all
    // original case survives. This is the common case for English words.
    if out == fed {
        return raw.iter().collect();
    }

    let rendered = apply_case(out, if letters.is_empty() { raw } else { &letters });
    if withheld.is_empty() {
        return rendered;
    }
    // Back in at the positions they were typed at. In typing order, so each
    // insertion shifts the ones after it by one.
    let mut chars: Vec<char> = rendered.chars().collect();
    for (inserted, (at, ch)) in withheld.iter().enumerate() {
        let index = (at + inserted).min(chars.len());
        chars.insert(index, *ch);
    }
    chars.into_iter().collect()
}

/// Whether this key is one the user has already cancelled, so a further press
/// of it is a letter rather than another try at the diacritic.
///
/// Only reached from the fourth press onward. A doubled key carries a diacritic
/// (`oo`→ô, `aa`→â, `ee`→ê, `dd`→đ) and the third press takes it back, leaving
/// the two letters as typed — `oo` is a real Vietnamese sequence (`xoong`,
/// `boong`, `moóc`), which is why that gesture exists. What the fourth press
/// must not do is start over.
///
/// `vi` gets that right for three of the four keys and wrong for `o`, and the
/// reason is a gap in its syllable validator. Its rule is: re-apply the
/// modification, then keep the result only if `is_valid_syllable` accepts it.
/// `âa` and `êe` are refused, so the key falls through to being a literal — but
/// **`ôo` is accepted**, though no Vietnamese syllable has `ô` followed by a
/// bare `o`. So `oooo` came out `ôo` — two characters for four keys — and
/// because composing continues, every key after it built on an impossible
/// syllable: `oooof` gave `ồo`, `oooong` gave `ôong`. Neither auto-fix nor the
/// spell check rescues those, since both ask the same validator that let it in.
/// Reported 2026-09-09.
///
/// Withholding the key from `vi` rather than post-processing its output is what
/// keeps the rest of the word working: the tone in `mooosc`→`moóc` and
/// `xooongf`→`xoòng` is applied by `vi` to a syllable it still sees whole.
///
/// VNI is excluded because it has no doubling rule at all — there a diacritic is
/// a digit, and four `o`s are four `o`s.
fn is_cancelled_repeat(lowered_key: char, method: InputMethod) -> bool {
    /// The keys whose doubling carries a diacritic in the Telex family.
    const DOUBLED_KEYS: [char; 4] = ['a', 'e', 'o', 'd'];
    method.is_telex_family() && DOUBLED_KEYS.contains(&lowered_key)
}

/// Re-applies the typed case pattern to a transformed lowercase rendering.
///
/// **Judged on the keys that spelled a letter, not on every key typed.** A tone
/// or diacritic key is not a letter of the word, and people let go of Shift for
/// it: `O` `A` `f` is how an all-caps `ÒA` is typed, and counting that `f` as
/// part of the pattern made the word Title case instead — `Òa`, with the `A`
/// demoted. Reported 2026-09-09. The same slip hit `HOONGf` (→ `Hồng`) and every
/// all-caps VNI word, where the marks are digits and so never uppercase
/// (`VIET65` → `Việt`).
///
/// `keys` is always non-empty here (an empty or untransformed word takes the
/// verbatim path in [`render`]) and holds the keys as typed, so their case is
/// the user's.
pub(crate) fn apply_case(lower: &str, keys: &[char]) -> String {
    if keys.iter().all(|c| c.is_ascii_uppercase()) {
        return lower.to_uppercase();
    }
    if keys[0].is_ascii_uppercase() {
        // Title-case: uppercase the first character of the rendering.
        let mut chars = lower.chars();
        return match chars.next() {
            Some(first) => first.to_uppercase().chain(chars).collect(),
            None => String::new(),
        };
    }
    lower.to_string()
}

/// Whether `word` is a non-empty string that is not a valid Vietnamese syllable.
///
/// This is the engine's spell check: the mid-word strict check asks it on every
/// key, and the policy layer asks it at a word boundary to decide whether to
/// restore the raw keystrokes.
///
/// Uses `vi`'s syllable validator plus the two things it lacks: the rime
/// inventory ([`violates_rime_inventory`]) and the stop-coda tone rule
/// ([`violates_stop_coda_tone`]), which the tone-stripped table cannot see. A
/// plain ASCII word that never transformed is treated as valid (nothing to fix)
/// since it equals its raw input, which also means every rule below it is
/// unreachable for an all-ASCII spelling.
pub fn is_invalid_vietnamese(word: &str) -> bool {
    judge(word, Word::Finished)
}

/// The shared body of [`is_invalid_vietnamese`] and [`cannot_become_vietnamese`].
fn judge(word: &str, shape: Word) -> bool {
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
    !vi::validation::is_valid_syllable(word)
        || violates_stop_coda_tone(word)
        || violates_glide_onset(word)
        || violates_rime_inventory(word, shape)
}

/// Whether the syllable puts the `o` glide behind an onset that cannot carry it.
///
/// The glide is the `o` of `hoa`, `khoe`, `toe` — a rounded /w/ between the
/// onset and the nucleus. Two families of onset never take it, and the rime
/// table cannot see either, because the rimes themselves are perfectly ordinary
/// and only the *pairing* is impossible. `vi` accepts the pairing too.
///
/// - **The labials `b`, `m`, `ph`, `v`.** A rounded glide after a consonant made
///   with the same lips is what Vietnamese does not do: there is no `moa`,
///   `moe`, `boe`, `voa`.
/// - **`c` and `k`.** Orthographic rather than phonotactic: /k/ before the glide
///   is spelled `qu`, so `qua` and `quê` are how those syllables are written and
///   `coa`/`coe`/`koa` never occur.
///
/// Every other onset does take it — `hoà`, `khoẻ`, `loà`, `ngoè`, `toè`, `xoà`,
/// `choè`, `doạ`, `goá`, `soạn`, `noãn` — so the list is closed and short.
///
/// The English words this rescues are the `-ore`/`-oe` family, where Telex reads
/// the `r` as hỏi and leaves the vowel behind it: `more`→`moẻ`, `bore`→`boẻ`,
/// `core`→`coẻ` — none of which auto-fix could see, since a rime of `oe` is real.
/// Reported 2026-09-11.
///
/// Only marked renders reach this: a plain `boa` or `voan` is ASCII, and
/// [`judge`] has already returned for anything the user typed verbatim. That is
/// what keeps the handful of French loans spelled this way out of its reach.
pub(crate) fn violates_glide_onset(word: &str) -> bool {
    /// Onsets that cannot carry the glide. Longest first, so `ph` is found
    /// before a bare `p` would be — `p` alone is not one of them.
    const ONSETS: [&str; 6] = ["ph", "b", "m", "v", "c", "k"];
    /// The glide spellings, with the vowel's own modifier kept by
    /// [`strip_tone_marks`] so `oă` is distinguishable from `oa`.
    const GLIDES: [&str; 3] = ["oa", "oă", "oe"];

    let stripped = strip_tone_marks(&word.to_lowercase());
    let Some(onset) = ONSETS.iter().find(|onset| stripped.starts_with(**onset)) else {
        return false;
    };
    let rime = &stripped[onset.len()..];
    GLIDES.iter().any(|glide| rime.starts_with(glide))
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

/// Strips tone marks while keeping the vowel's own modifier — the horn on `ư`
/// and `ơ`, the breve on `ă`, the circumflex on `â`/`ê`/`ô`.
///
/// [`remove_tones`](crate::remove_tones) cannot be used for this. It flattens
/// all the way to ASCII (`ư`→`u`, `ơ`→`o`), which collapses exactly the
/// distinctions the phonotactic rules below turn on: `ưa` would become `ua` and
/// `ơch` would become `och`, so a rule about one would silently judge the other.
fn strip_tone_marks(lowered: &str) -> String {
    debug_assert!(
        !lowered.chars().any(char::is_uppercase),
        "callers lowercase first; an uppercase toned vowel would pass through unmapped"
    );
    /// Toned forms for each base vowel, which keeps its modifier.
    const TONED: [(&str, char); 12] = [
        ("àáảãạ", 'a'),
        ("ằắẳẵặ", 'ă'),
        ("ầấẩẫậ", 'â'),
        ("èéẻẽẹ", 'e'),
        ("ềếểễệ", 'ê'),
        ("ìíỉĩị", 'i'),
        ("òóỏõọ", 'o'),
        ("ồốổỗộ", 'ô'),
        ("ờớởỡợ", 'ơ'),
        ("ùúủũụ", 'u'),
        ("ừứửữự", 'ư'),
        ("ỳýỷỹỵ", 'y'),
    ];

    lowered
        .chars()
        .map(|ch| {
            if ch.is_ascii() {
                return ch;
            }
            TONED
                .iter()
                .find(|(forms, _)| forms.contains(ch))
                .map_or(ch, |(_, base)| *base)
        })
        .collect()
}

/// Vietnamese's rime inventory: every legal nucleus-plus-coda, tone stripped.
///
/// A syllable is an onset, a rime and a tone. `vi`'s validator checks the onset
/// and the tone but is lenient about the rime, accepting `uing`, `ưam`, `ơch`
/// and `pơe` — spellings the language does not have. This table is the closed
/// set it is missing, so an impossible rime is caught by *absence* rather than
/// by a rule written after someone reports it.
///
/// **Derived, not remembered.** The entries were extracted from the 74k-word
/// Viet74K list by stripping tones and onsets and counting what was left; the
/// 148 rimes above a frequency cut, plus 22 rare ones read by hand out of the
/// tail because they are real (`thuở`, `khuỷu`, `quýt`, `bâng khuâng`, `giếc`,
/// `ngoạm`, `huỵch`, `tuềnh`, `xoẻng`, `hừm`). Transliterated loanwords were
/// left out — `ing` and `ic` reach the list only through `ping` and `acid`,
/// which is exactly why `using` and `basic` used to survive as Vietnamese.
///
/// Sorted, because [`rime_is_possible`] binary-searches it.
const RIMES: [&str; 170] = [
    "a", "ac", "ach", "ai", "am", "an", "ang", "anh", "ao", "ap", "at", "au", "ay", "e", "ec",
    "em", "en", "eng", "eo", "ep", "et", "i", "ia", "ich", "im", "in", "inh", "ip", "it", "iu",
    "iêc", "iêm", "iên", "iêng", "iêp", "iêt", "iêu", "o", "oa", "oac", "oach", "oai", "oam",
    "oan", "oang", "oanh", "oao", "oap", "oat", "oay", "oc", "oe", "oem", "oen", "oeng", "oeo",
    "oet", "oi", "om", "on", "ong", "ooc", "oong", "op", "ot", "oăc", "oăm", "oăn", "oăng", "oăt",
    "u", "ua", "uc", "ui", "um", "un", "ung", "up", "ut", "uy", "uya", "uych", "uyn", "uynh",
    "uyp", "uyt", "uyu", "uyên", "uyêt", "uân", "uâng", "uât", "uây", "uê", "uêch", "uênh", "uêu",
    "uôc", "uôi", "uôm", "uôn", "uông", "uôt", "uơ", "y", "ych", "ynh", "yp", "yt", "yu", "yêm",
    "yên", "yêng", "yêt", "yêu", "âc", "âm", "ân", "âng", "âp", "ât", "âu", "ây", "ê", "êc", "êch",
    "êm", "ên", "êng", "ênh", "êp", "êt", "êu", "ô", "ôc", "ôi", "ôm", "ôn", "ông", "ôp", "ôt",
    "ăc", "ăm", "ăn", "ăng", "ăp", "ăt", "ơ", "ơi", "ơm", "ơn", "ơp", "ơt", "ư", "ưa", "ưc", "ưi",
    "ưm", "ưn", "ưng", "ưt", "ưu", "ươc", "ươi", "ươm", "ươn", "ương", "ươp", "ươt", "ươu",
];

/// Whether the word being judged is finished or still being typed.
///
/// The distinction is load-bearing and is why this is not a `bool`. A half-typed
/// word's rime is a *prefix* of the finished one — `biết` passes through `biế`,
/// `chuyển` through `chuyể` — and open `iê` and `uyê` are not legal rimes, since
/// closed they need a coda. Judging a half-typed word by membership alone would
/// refuse the keystroke and escape the word, making `biết` untypeable with the
/// mid-word check on.
///
/// A half-typed rime can also differ from the finished one in the *modifier*,
/// not just in length, because Telex delivers the horn, breve and circumflex on
/// a later keystroke than the vowel they land on: `mượn` is typed `muwown` and
/// goes through `mưo` before the second `w` turns that `o` into `ơ`. So the
/// half-typed comparison ignores modifiers, and only the finished word has to
/// spell its vowels exactly.
///
/// The reverse error is milder but real: judging a *finished* word by prefix
/// would accept a bare `ă` because `ăng` exists, and `law` would stay `lă`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Word {
    /// A word at its boundary. Its rime must be in the table outright.
    Finished,
    /// A word still being typed. Its rime may be any prefix of a table entry.
    HalfTyped,
}

/// The vowel under its modifier: the horn off `ư`/`ơ`, the breve off `ă`, the
/// circumflex off `â`/`ê`/`ô`. Tone marks are already gone by here.
///
/// Not [`remove_tones`](crate::remove_tones), which also flattens `đ`, and not
/// [`strip_tone_marks`], which deliberately *keeps* the modifier this drops.
fn base_vowel(ch: char) -> char {
    match ch {
        'ă' | 'â' => 'a',
        'ê' => 'e',
        'ô' | 'ơ' => 'o',
        'ư' => 'u',
        other => other,
    }
}

/// Whether `rime` is a prefix of `entry` once both have their vowel modifiers
/// removed — the half-typed comparison, for the reason [`Word`] gives.
fn could_grow_into(entry: &str, rime: &str) -> bool {
    let mut entry = entry.chars().map(base_vowel);
    rime.chars()
        .map(base_vowel)
        .all(|want| entry.next() == Some(want))
}

/// Whether `rime` is in the inventory, or — for a half-typed word — could still
/// grow into something that is.
fn rime_is_possible(rime: &str, word: Word) -> bool {
    match word {
        // The table is sorted, so an exact match sits at the partition point.
        Word::Finished => {
            let at = RIMES.partition_point(|entry| *entry < rime);
            RIMES.get(at) == Some(&rime)
        }
        // Modifier-blind, so the sort order does not apply and every entry is
        // tried. 170 short strings that almost all fail on the first character;
        // the engine budget is per keystroke and this does not register in it.
        Word::HalfTyped => RIMES.iter().any(|entry| could_grow_into(entry, rime)),
    }
}

/// Onsets, longest first so `ngh` is found before `ng` and `ng` before `n`.
///
/// Order is the whole contract here: matching `n` first would leave `gh` as the
/// rime of `nghe` and reject an ordinary word.
const ONSETS: [&str; 27] = [
    "ngh", "ng", "nh", "ch", "gh", "gi", "kh", "ph", "th", "tr", "qu", "b", "c", "d", "đ", "g",
    "h", "k", "l", "m", "n", "p", "r", "s", "t", "v", "x",
];

/// Splits the onset off a tone-stripped, lowercased syllable, leaving the rime.
///
/// `gi` and `qu` are onsets only when a vowel follows: in `gì` the `i` *is* the
/// nucleus, so the word has to fall through to the bare `g`. The same guard
/// keeps a two-letter word from being consumed whole and leaving no rime at all.
fn split_rime(syllable: &str) -> &str {
    /// The letters that can open a rime; `gi`/`qu` need one of these after them.
    const NUCLEI: &str = "aăâeêioôơuưy";

    for onset in ONSETS {
        if let Some(rest) = syllable.strip_prefix(onset) {
            if rest.is_empty() {
                continue;
            }
            if matches!(onset, "gi" | "qu") && !rest.starts_with(|c| NUCLEI.contains(c)) {
                continue;
            }
            return rest;
        }
    }
    syllable
}

/// Whether the syllable's rime is absent from Vietnamese's inventory.
///
/// This one rule replaced three written by hand — `ưa` closed by a coda, `nh`/`ch`
/// on a back vowel, `ng`/`c` on `i`/`y` — each of which had been added only after
/// a user reported the English word it mangled (`wasm`→`ưám`, `using`→`uíng`,
/// `business`→`buín`). Each was true and none was the general statement, so the
/// next impossible rime always got through: `power`→`pởe`, `west`→`ưét`,
/// `two`→`tưo`, `law`→`lă`, `person`→`peón` were all still live when this landed.
///
/// The stop-coda **tone** rule is not subsumed and still runs alongside this: the
/// table is tone-stripped, so `màc` reduces to the perfectly ordinary rime `ac`
/// and only a rule that can see the tone will catch it.
pub(crate) fn violates_rime_inventory(word: &str, shape: Word) -> bool {
    let stripped = strip_tone_marks(&word.to_lowercase());
    let rime = split_rime(&stripped);
    // A word that is all onset has no rime to judge; leave it to `vi`.
    !rime.is_empty() && !rime_is_possible(rime, shape)
}

/// Whether a word still being typed can no longer become Vietnamese.
///
/// The mid-word spell check's question. It differs from
/// [`is_invalid_vietnamese`] only in judging the rime by prefix, for the reason
/// [`Word::HalfTyped`] gives; every other rule is shared, because an entry rule
/// and an exit rule that have to agree are best written once.
pub(crate) fn cannot_become_vietnamese(word: &str) -> bool {
    judge(word, Word::HalfTyped)
}

/// Computes the minimal edit turning `prev` into `next`: keep the common prefix,
/// delete the rest of `prev` (counted in UTF-16 code units), insert the rest of
/// `next`. This is the `backspaceCount` / `newCharCount` shape shipping engines
/// use, and the shape every [`KeyResponse`] the engine returns has.
pub fn diff(prev: &str, next: &str) -> KeyResponse {
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
