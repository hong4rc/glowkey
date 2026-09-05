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
pub(crate) fn apply_case(lower: &str, raw: &[char]) -> String {
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

/// Whether `word` is a non-empty string that is not a valid Vietnamese syllable.
///
/// This is the engine's spell check: the mid-word strict check asks it on every
/// key, and the policy layer asks it at a word boundary to decide whether to
/// restore the raw keystrokes. Uses `vi`'s syllable validator plus the stop-coda
/// tone rule it lacks; a plain ASCII word that never transformed is treated as
/// valid (nothing to fix) since it equals its raw input.
pub fn is_invalid_vietnamese(word: &str) -> bool {
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
