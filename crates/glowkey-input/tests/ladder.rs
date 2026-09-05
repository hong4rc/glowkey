//! The decision ladder, driven directly.
//!
//! These are the macOS tap's policy tests, brought across unchanged in substance
//! and freed of `CGEvent`. What they pin is the **order** of `decide`'s steps:
//! every one of them was written from a bug someone hit while typing Vietnamese
//! into a real application, and the order in which the ladder asks its questions
//! is the record of those bugs. A reordering that looks harmless in a diff puts
//! one of them back, and nothing else in the codebase would notice.
//!
//! The tap keeps its own copies of these (`app/src/platform/macos/tests.rs`),
//! driving real `CGEvent`s through the adapter. That duplication is deliberate:
//! a policy test passing here while the tap-level equivalent fails is exactly how
//! a bad adapter shows itself.

use glowkey_input::{
    decide, hotkey, Ctx, Decision, Effects, HotkeyPreset, Key, KeyEvent, Modifiers,
};
use glowkey_session::{
    ExclusionDefaults, ExclusionList, ExclusionToggle, InputMode, KeyResponse, PlacementStyle,
    Session, WordPreference,
};

/// A session plus the little the platform would otherwise own: which key code it
/// recorded for a custom hotkey, and somewhere to put the effects.
struct Tap {
    session: Session,
    /// The toggle preset. The shell keeps it with its preferences, not in the
    /// session, so the harness does too.
    preset: HotkeyPreset,
    /// The virtual key code this pretend platform recorded for a custom hotkey.
    recorded_code: Option<i64>,
    effects: Effects,
}

impl Tap {
    /// A session in an application that is not excluded, so transformation is on.
    fn active() -> Self {
        let mut tap = Self::bare();
        tap.session.set_frontmost_app("com.apple.TextEdit");
        tap
    }

    /// A session with nothing set — the caller picks the frontmost application.
    fn bare() -> Self {
        Self {
            // The shipped exclusion defaults, as a fresh settings file has them.
            session: Session::new(
                PlacementStyle::default(),
                ExclusionList::with_defaults(shipped_defaults()),
            ),
            preset: HotkeyPreset::default(),
            recorded_code: None,
            effects: Effects::default(),
        }
    }

    fn ctx(&self) -> Ctx {
        Ctx {
            toggle_hotkey: hotkey::resolve(self.preset, self.recorded_code),
        }
    }

    fn decide(&mut self, event: &KeyEvent) -> Decision {
        let ctx = self.ctx();
        self.effects.clear();
        decide(&mut self.session, event, &ctx, &mut self.effects)
    }
}

/// Applies an edit to the model document exactly as the platform would: N UTF-16
/// code units off the end, then the insertion.
fn apply(screen: &mut String, r: &KeyResponse) {
    let units: Vec<u16> = screen.encode_utf16().collect();
    let keep = units.len().saturating_sub(r.backspaces);
    *screen = String::from_utf16(&units[..keep]).unwrap();
    screen.push_str(&r.insert);
}

/// Types `input` through the real ladder and returns what the document would show.
fn type_through(tap: &mut Tap, input: &str) -> String {
    let mut screen = String::new();
    for ch in input.chars() {
        let event = KeyEvent::character(ch);
        match tap.decide(&event) {
            Decision::Passthrough => screen.push(ch),
            Decision::Consume | Decision::ToggleApp => {}
            Decision::Emit(r) => apply(&mut screen, &r),
            Decision::EmitThenReplayKey(r) => {
                apply(&mut screen, &r);
                screen.push(ch); // the boundary key still types
            }
            other => panic!("a decision this harness does not apply: {other:?}"),
        }
    }
    screen
}

/// The same, where `⌫` in the input stands for the Backspace key.
///
/// The character-only [`type_through`] cannot express a Backspace, which is why
/// every field report involving one used to be checked against a hand-written
/// model in a scratch binary instead. A model that shares the author's
/// assumptions cannot contradict them; this drives the code the app runs.
fn type_with_deletes(tap: &mut Tap, input: &str) -> String {
    let mut screen = String::new();
    for ch in input.chars() {
        let is_delete = ch == '⌫';
        let event = if is_delete {
            KeyEvent::key(Key::Backspace)
        } else {
            KeyEvent::character(ch)
        };
        match tap.decide(&event) {
            // For Backspace, a passthrough means the *host* performs the deletion.
            Decision::Passthrough => {
                if is_delete {
                    screen.pop();
                } else {
                    screen.push(ch);
                }
            }
            Decision::Consume | Decision::ToggleApp => {}
            Decision::Emit(r) => apply(&mut screen, &r),
            Decision::EmitThenReplayKey(r) => {
                apply(&mut screen, &r);
                screen.push(ch);
            }
            other => panic!("a decision this harness does not apply: {other:?}"),
        }
    }
    screen
}

/// A shipped default exclusion.
///
/// The session does not know what an application is called; the shell hands it
/// the shipped tables, so this harness hands it invented ones. A test that
/// writes `com.apple.Terminal` into an assertion is a macOS test wearing a
/// portable test's clothes: three of these were, and they failed the first time
/// the suite ran on Windows.
fn a_default_exclusion() -> &'static str {
    "example.terminal"
}

/// A shipped default that is a **terminal**, so the session-only un-exclusion
/// rule applies to it.
fn a_terminal_default() -> &'static str {
    "example.terminal"
}

/// The tables a shell would ship: the one terminal above.
fn shipped_defaults() -> ExclusionDefaults {
    ExclusionDefaults::new([a_default_exclusion()], [a_terminal_default()])
}

fn ctrl_shift() -> Modifiers {
    Modifiers {
        control: true,
        shift: true,
        option: false,
        command: false,
    }
}

// ── Step 5: letters are suppressed and re-emitted ───────────────────────────

#[test]
fn free_tone_placement() {
    // The headline: the tone key in any position → hồng.
    assert_eq!(type_through(&mut Tap::active(), "hoongf"), "hồng");
    assert_eq!(type_through(&mut Tap::active(), "hofong"), "hồng");
    assert_eq!(type_through(&mut Tap::active(), "hoonfg"), "hồng");
    // Multi-transform word (w horns uo→ươ, f tones).
    assert_eq!(type_through(&mut Tap::active(), "nguoiwf"), "người");
    assert_eq!(type_through(&mut Tap::active(), "hofngo"), "hồng");
}

#[test]
fn words_and_english() {
    assert_eq!(type_through(&mut Tap::active(), "nguyeenx"), "nguyễn");
    assert_eq!(type_through(&mut Tap::active(), "dduwowcj"), "được");
    assert_eq!(type_through(&mut Tap::active(), "Hoongf"), "Hồng");
    // English passes through untouched (fast path).
    assert_eq!(type_through(&mut Tap::active(), "hello"), "hello");
}

// ── Step 6: the boundary key, and the replay after a restore ────────────────

#[test]
fn boundary_commits_the_word() {
    // Space is a boundary: the word is already on screen, space passes through.
    assert_eq!(type_through(&mut Tap::active(), "hoongf "), "hồng ");
    // Without deleting the space, a following key starts a NEW word — z is
    // literal, not a modifier of the previous word.
    assert_eq!(type_through(&mut Tap::active(), "hoongf z"), "hồng z");
}

/// The two shapes reported from the field. An auto-fix restore at a boundary must
/// leave the raw keys followed by the boundary key, in that order. While the
/// boundary key was passed through natively instead of replayed, the host applied
/// it before the posted backspaces and the edit ate it: `ddc`␣ came out `đddc` and
/// `work`␣ came out `ưwork`, both with the space swallowed.
#[test]
fn an_auto_fix_restore_keeps_the_boundary_key() {
    assert_eq!(type_through(&mut Tap::active(), "work "), "work ");
    // A leading đ is exempt from auto-fix, so this one commits with no restore
    // — the boundary key must survive that path too.
    assert_eq!(type_through(&mut Tap::active(), "ddc "), "đc ");
}

/// Pins the mechanism, not just the result: the boundary key that triggers an
/// auto-fix restore must be suppressed and replayed, never left to race the edit
/// as a plain passthrough.
#[test]
fn an_auto_fix_boundary_replays_the_key_rather_than_passing_it_through() {
    let mut tap = Tap::active();
    for ch in "work".chars() {
        tap.decide(&KeyEvent::character(ch));
    }
    match tap.decide(&KeyEvent::character(' ')) {
        Decision::EmitThenReplayKey(_) => {}
        other => panic!("boundary key must be replayed, got {other:?}"),
    }
}

// ── Step 4: a caret move flushes ────────────────────────────────────────────

#[test]
fn a_caret_move_flushes_the_engine() {
    // An arrow key mid-word must flush (so a stale baseline cannot corrupt later
    // edits) and pass through — never emit an edit.
    let mut tap = Tap::active();
    for ch in "hoo".chars() {
        tap.decide(&KeyEvent::character(ch));
    }
    assert!(tap.session.is_composing());

    assert!(matches!(
        tap.decide(&KeyEvent::key(Key::CaretMove)),
        Decision::Passthrough
    ));
    assert!(
        !tap.session.is_composing(),
        "a caret move must flush the composing word"
    );
}

// ── Step 2: the shortcut filter flushes and passes through ──────────────────

#[test]
fn a_shortcut_flushes_the_engine() {
    // ⌘A (select-all) changes the selection; the engine must flush so the next
    // keystroke is not diffed against a stale baseline (the select-all → hoồng
    // bug). A ⌘-shortcut passes through and clears composing state.
    let mut tap = Tap::active();
    assert_eq!(type_through(&mut tap, "hoong"), "hông");
    assert!(tap.session.is_composing());

    let select_all = KeyEvent::character('a').with_mods(Modifiers {
        command: true,
        ..Modifiers::none()
    });
    assert!(matches!(tap.decide(&select_all), Decision::Passthrough));
    assert!(!tap.session.is_composing());
}

// ── The exclusion gate ──────────────────────────────────────────────────────

#[test]
fn an_excluded_app_passes_everything_through() {
    let mut tap = Tap::bare();
    // A shipped default, asked of the table rather than spelled out: the
    // identities are per-target (bundle identifiers on macOS, executable names
    // on Windows), so naming one makes this a single-platform test.
    tap.session.set_frontmost_app(a_default_exclusion());
    assert_eq!(type_through(&mut tap, "hoongf"), "hoongf");
}

// ── Step 1: hotkeys, ahead of the shortcut filter ───────────────────────────

#[test]
fn the_toggle_hotkey_switches_mode_and_is_consumed() {
    let mut tap = Tap::active();
    // Vietnamese by default: transforms.
    assert_eq!(type_through(&mut tap, "hoongf"), "hồng");

    // ⌃⇧Space toggles to English — and is consumed (types nothing).
    let toggle = KeyEvent::key(Key::Space).with_mods(ctrl_shift());
    assert!(matches!(tap.decide(&toggle), Decision::Consume));
    assert_eq!(tap.effects.mode_toggled, Some(InputMode::English));
    assert!(tap.effects.refresh_glyph);
    assert_eq!(tap.session.mode(), InputMode::English);

    // Now the same keys pass through untransformed.
    assert_eq!(type_through(&mut tap, "hoongf"), "hoongf");

    // Toggle back to Vietnamese.
    assert!(matches!(tap.decide(&toggle), Decision::Consume));
    assert_eq!(type_through(&mut tap, "hoongf"), "hồng");
}

#[test]
fn the_app_toggle_hotkey_asks_the_platform_to_toggle() {
    // ⌃⇧E toggles the current app's ignore-list membership and consumes the key.
    let mut tap = Tap::active(); // frontmost = TextEdit, not excluded
    assert_eq!(type_through(&mut tap, "hoongf"), "hồng");

    let app_toggle = KeyEvent::character('e').with_mods(ctrl_shift());
    assert!(matches!(tap.decide(&app_toggle), Decision::ToggleApp));
    // Applying the toggle (as the platform does) excludes TextEdit.
    assert!(tap
        .session
        .toggle_app_exclusion("com.apple.TextEdit")
        .excluded());
    assert_eq!(type_through(&mut tap, "hoongf"), "hoongf");
}

/// ⌃⇧W corrects the word just typed and emits the swap.
///
/// The engine half is covered in `crates/glowkey-engine/tests/word_overrides.rs`;
/// what this pins is that the ladder recognises the combination, reaches it
/// **before** the shortcut filter — which would flush and destroy the very memory
/// it needs — and returns an edit rather than passing a stray `W` into the
/// document.
#[test]
fn the_correction_hotkey_beats_the_shortcut_filter() {
    let mut tap = Tap::active();
    // Type `was` and a space: `ứa ` is on screen.
    for ch in "was ".chars() {
        tap.decide(&KeyEvent::character(ch));
    }

    let correct = KeyEvent::character('w').with_mods(ctrl_shift());
    match tap.decide(&correct) {
        Decision::Emit(edit) => {
            assert!(edit.backspaces > 0, "the on-screen word must be replaced");
            assert!(
                edit.insert.starts_with("was"),
                "expected the raw keys back, got {:?}",
                edit.insert
            );
        }
        other => panic!("expected an edit, got {other:?}"),
    }
    // The platform is told to reload the editor and to write the file.
    assert!(tap.effects.personal_words_changed);
    assert!(tap.effects.save_settings);
    assert!(tap.effects.corrected.is_some());
    // And the decision was recorded, so the next `was` needs no keystroke.
    assert_eq!(tap.session.word_override("was"), Some(WordPreference::Raw));
}

/// With nothing to correct the key is consumed, not passed through: a stray `W`
/// appearing in the document would be worse than a keystroke that did nothing.
#[test]
fn the_correction_hotkey_with_nothing_to_correct_is_consumed() {
    let mut tap = Tap::active();
    let correct = KeyEvent::character('w').with_mods(ctrl_shift());
    assert!(matches!(tap.decide(&correct), Decision::Consume));
    assert!(!tap.effects.save_settings);
}

/// Never in an excluded application: excluded means hands off, and this edit
/// rewrites text that is already on screen.
#[test]
fn the_correction_hotkey_is_inert_in_an_excluded_app() {
    let mut tap = Tap::bare();
    tap.session.set_frontmost_app(a_default_exclusion());
    let correct = KeyEvent::character('w').with_mods(ctrl_shift());
    assert!(matches!(tap.decide(&correct), Decision::Passthrough));
}

// ── Step 3: the five-case Backspace ladder ──────────────────────────────────
//
// One test per case, named for the case, because the order of these five is the
// part of the ladder that has cost the most.

/// Case 1 — deleting the boundary right after a committed word re-composes it, so
/// the next keys keep editing it (hồng␣⌫z → hông).
#[test]
fn backspace_case_1_deleting_a_boundary_reopens_the_word() {
    let mut tap = Tap::active();
    assert_eq!(type_with_deletes(&mut tap, "hoongf ⌫z"), "hông");
}

/// Case 2 — deleting a boundary with no word in front of it (the `␣` of `hồng, `)
/// removes nothing the engine composed, and the word is one more Backspace away.
///
/// `hồng`␣⌫`z` worked, but `hồng``,`␣⌫⌫`z` gave `hồngz`: the space after the comma
/// committed nothing, and a commit with nothing composing used to throw the whole
/// history away. `, ` and `. ` are the two commonest pairs in prose, so the fix is
/// not an edge case.
#[test]
fn backspace_case_2_a_bare_boundary_is_one_more_delete() {
    let mut tap = Tap::active();
    assert_eq!(type_with_deletes(&mut tap, "hoongf, ⌫⌫z"), "hông");

    let mut tap = Tap::active();
    assert_eq!(type_with_deletes(&mut tap, "hoongf. ⌫⌫z"), "hông");

    // A run of them: each boundary is one entry and one delete.
    let mut tap = Tap::active();
    assert_eq!(type_with_deletes(&mut tap, "hoongf,,, ⌫⌫⌫⌫z"), "hông");
}

/// Case 3 — mid-word, shrink the composition by one visible character and stay
/// composed, so the next key is still a Telex key rather than a literal
/// (hoongf⌫z → hôn, not hồnz).
#[test]
fn backspace_case_3_mid_word_shrinks_and_stays_composed() {
    let mut tap = Tap::active();
    assert_eq!(type_through(&mut tap, "hoongf"), "hồng");
    assert!(tap.session.is_composing());

    // Backspace passes through (the host deletes the last visible character,
    // hồng → hồn) and the engine shrinks with it.
    assert!(matches!(
        tap.decide(&KeyEvent::key(Key::Backspace)),
        Decision::Passthrough
    ));
    assert!(tap.session.is_composing());
    let (raw, rendered, _, _) = tap.session.debug_state();
    assert_eq!((raw.as_str(), rendered.as_str()), ("hoonf", "hồn"));

    // z now edits the re-composed word rather than typing a literal.
    let mut tap = Tap::active();
    assert_eq!(type_with_deletes(&mut tap, "hoongf⌫z"), "hôn");
}

/// Case 3 again, on a word that was never escaped, so the common path is untouched
/// for everyone with strict spell check off — the default.
#[test]
fn backspace_case_3_an_ordinary_mid_word_delete_passes_through() {
    let mut tap = Tap::active();
    for ch in "hoongf".chars() {
        tap.decide(&KeyEvent::character(ch));
    }
    assert!(matches!(
        tap.decide(&KeyEvent::key(Key::Backspace)),
        Decision::Passthrough
    ));
    assert_eq!(tap.session.current_word(), "hồn");
}

/// Case 4 — undoing a spell-check escape rewrites the word in one edit and
/// swallows the keystroke (`hoongfa`⌫ → `hồng`).
///
/// Reported from live use: `hoongf` `a` ⌫ left `hoongf` on screen instead of
/// restoring `hồng`. The repair cannot be a passthrough plus a posted edit — that
/// mixes a native keystroke with a synthesized one, which is the race the
/// full-suppression model exists to remove — so the ladder must own the whole thing.
#[test]
fn backspace_case_4_undoing_an_escape_emits_instead_of_passing_through() {
    let mut tap = Tap::active();
    tap.session.set_strict_spell_check(true);

    for ch in "hoongf".chars() {
        tap.decide(&KeyEvent::character(ch));
    }
    // The mistake escapes the word to its raw keys.
    tap.decide(&KeyEvent::character('a'));
    assert_eq!(tap.session.current_word(), "hoongfa");

    match tap.decide(&KeyEvent::key(Key::Backspace)) {
        Decision::Emit(edit) => {
            assert_eq!(
                edit.backspaces,
                "hoongfa".encode_utf16().count(),
                "the edit replaces the whole on-screen word, including the character \
                 the user asked to delete — the key is suppressed, so nothing else \
                 removes it"
            );
            assert_eq!(edit.insert, "hồng");
        }
        other => panic!("expected the repair to be emitted, got {other:?}"),
    }
}

/// Case 5 — if the engine cannot stay in step, flush and stop composing.
///
/// Deleting `viêt` back to `vi` has no single raw-key removal that produces it —
/// `viee` minus an `e` renders `viê`, not `vi` — so the engine cannot stay in step
/// and flushes. After that it no longer knows how many characters of that word
/// remain on screen, so it cannot tell when the caret reaches the boundary, and the
/// history must go with it. Re-opening a word on a guess is the failure this whole
/// feature is built to avoid.
#[test]
fn backspace_case_5_losing_track_mid_word_ends_the_chain() {
    let mut tap = Tap::active();
    assert_eq!(
        type_with_deletes(&mut tap, "hoongf vieet s⌫⌫⌫⌫⌫⌫⌫z"),
        "hồngz",
        "the flush inside viêt cleared the history, so z starts a fresh word"
    );
}

// ── Re-composition across a boundary, the whole sequence ────────────────────

#[test]
fn recompose_after_deleting_the_space() {
    // hồng, Space, Backspace (delete the space), then z (Telex tone-clear) must
    // re-compose the previous word: hồng + z → hông.
    let mut tap = Tap::active();
    let mut screen = String::new();

    for ch in "hoongf".chars() {
        match tap.decide(&KeyEvent::character(ch)) {
            Decision::Passthrough => screen.push(ch),
            Decision::Emit(r) => apply(&mut screen, &r),
            other => panic!("unexpected {other:?} for {ch}"),
        }
    }
    assert_eq!(screen, "hồng");

    // Space — boundary commits the (valid) word and passes through.
    match tap.decide(&KeyEvent::character(' ')) {
        Decision::Passthrough => screen.push(' '),
        Decision::EmitThenReplayKey(r) => {
            apply(&mut screen, &r);
            screen.push(' ');
        }
        other => panic!("unexpected {other:?} for space"),
    }
    assert_eq!(screen, "hồng ");

    // Backspace — passes through (host deletes the space); engine re-composes.
    match tap.decide(&KeyEvent::key(Key::Backspace)) {
        Decision::Passthrough => {
            screen.pop();
        }
        other => panic!("backspace should pass through, got {other:?}"),
    }
    assert_eq!(screen, "hồng");

    // z — now edits the re-composed word: hồng → hông.
    match tap.decide(&KeyEvent::character('z')) {
        Decision::Emit(r) => apply(&mut screen, &r),
        Decision::Passthrough => screen.push('z'),
        other => panic!("unexpected {other:?} for z"),
    }
    assert_eq!(screen, "hông");
}

/// The sequences reported from live use.
///
/// Reported as producing `hồngz` — the tone key landing as a literal after a word
/// the engine had stopped composing.
#[test]
fn reported_delete_sequences_land_where_they_should() {
    // Mistyped vowel: the word escapes to its raw keys, and deleting the
    // offending key brings the transformation back.
    let mut tap = Tap::active();
    tap.session.set_strict_spell_check(true);
    assert_eq!(type_with_deletes(&mut tap, "hoongfa⌫"), "hồng");

    // Mistyped tone key. `hống` is spellable, so nothing escapes; the deletes are
    // ordinary visible-character deletes and `z` removes the tone.
    let mut tap = Tap::active();
    tap.session.set_strict_spell_check(true);
    assert_eq!(type_with_deletes(&mut tap, "hoongfs⌫⌫z"), "hô");

    // The same with the spell check off — the default — must be identical here,
    // because nothing escapes either way.
    let mut tap = Tap::active();
    assert_eq!(type_with_deletes(&mut tap, "hoongfs⌫⌫z"), "hô");

    // And the plain case the contract is written around.
    let mut tap = Tap::active();
    assert_eq!(type_with_deletes(&mut tap, "hoongf⌫z"), "hôn");
}

/// Deleting back to a word re-opens it, however you got there.
///
/// Reported three times in one morning, each time as a different-looking bug.
/// Read off the log, the sequence has a **space** in it — the shorthand
/// `hoongf s(del)(del)z` is seven keystrokes, not six:
///
/// ```text
/// 'f'  Emit bs=3 ins="ồng"   raw="hoongf" rendered="hồng"
/// ' '  Passthrough           raw=""       rendered=""      <- commits the word
/// 's'  Emit bs=0 ins="s"     raw="s"      rendered="s"     <- a new word starts
/// ⌫    Passthrough                                          <- deletes the s
/// ⌫    Passthrough                                          <- deletes the space
/// 'z'  Emit bs=0 ins="z"     raw="z"      rendered="z"     <- z was its own word
/// ```
///
/// The committed word was destroyed by the first keystroke after the boundary, so
/// re-opening only ever worked if the Backspace was *immediate*. Now the history
/// survives keys that are later deleted.
#[test]
fn deleting_back_to_a_word_reopens_it() {
    let mut tap = Tap::active();
    assert_eq!(type_with_deletes(&mut tap, "hoongf s⌫⌫z"), "hông");

    // The immediate case, which must not regress.
    let mut tap = Tap::active();
    assert_eq!(type_with_deletes(&mut tap, "hoongf ⌫z"), "hông");

    // A longer intervening word: four deletes to clear `abc` and the space.
    let mut tap = Tap::active();
    assert_eq!(type_with_deletes(&mut tap, "hoongf abc⌫⌫⌫⌫z"), "hông");
}

/// Two words back. The history is a stack, so each word empties in turn and the
/// next Backspace re-opens the one before it — no special case for depth.
#[test]
fn deleting_back_through_two_words_reopens_the_right_one() {
    let mut tap = Tap::active();
    assert_eq!(
        type_with_deletes(&mut tap, "hoongf man s⌫⌫⌫⌫⌫⌫z"),
        "hông",
        "the first word re-opened after two words were deleted away, and z removed its tone"
    );
}

/// The cap is exactly five, and both sides of it are pinned.
///
/// Each intervening word costs two deletes: one removes the boundary and re-opens
/// the word before it, the next empties that word.
#[test]
fn the_history_cap_is_five_entries() {
    // Five entries — `hồng` and four `a`s — so the oldest is still there and nine
    // deletes re-open it.
    let mut tap = Tap::active();
    assert_eq!(
        type_with_deletes(&mut tap, "hoongf a a a a ⌫⌫⌫⌫⌫⌫⌫⌫⌫z"),
        "hông",
        "within the cap the first word must still re-open"
    );

    // Six entries: `hồng` falls off the front. Deleting back to it finds an empty
    // stack, so the engine stops vouching for the caret and `z` starts a new word.
    let mut tap = Tap::active();
    assert_eq!(
        type_with_deletes(&mut tap, "hoongf a a a a a ⌫⌫⌫⌫⌫⌫⌫⌫⌫⌫⌫z"),
        "hồngz",
        "past the cap nothing re-opens"
    );
}

/// Bare boundaries sit *between* words in the stack, and deleting through them
/// still lands on the right word.
///
/// This is the case a single trailing-boundary count could not represent: after
/// `hồng, man ` the two boundaries behind `hồng` are no longer the trailing ones,
/// so a count kept only for the tail would have forgotten them and re-opened
/// `hồng` while `,` still sat at the caret.
#[test]
fn deleting_back_through_a_bare_boundary_reopens_the_word_before_it() {
    let mut tap = Tap::active();
    // `hồng, man ` — six deletes: the space (re-opening `man`), `man` itself, the
    // space after the comma, then the comma (re-opening `hồng`).
    assert_eq!(type_with_deletes(&mut tap, "hoongf, man ⌫⌫⌫⌫⌫⌫z"), "hông");
}

/// Anything that moves the caret where the engine cannot see it clears the whole
/// history. Re-opening a word on a guess is how a blind editor corrupts a
/// document, so this is the property the feature rests on.
#[test]
fn a_caret_move_clears_the_whole_history() {
    // A flush stands for every one of them: mouse-down, arrow keys, ⌘ shortcuts.
    let mut tap = Tap::active();
    for ch in "hoongf ".chars() {
        tap.decide(&KeyEvent::character(ch));
    }
    tap.session.flush();
    tap.decide(&KeyEvent::key(Key::Backspace));
    match tap.decide(&KeyEvent::character('z')) {
        Decision::Emit(edit) => assert_eq!(
            edit.insert, "z",
            "z must start a fresh word, not edit a word the engine lost track of"
        ),
        other => panic!("expected a fresh word, got {other:?}"),
    }

    // An app switch is the case with no event to flush on — a call popup, a
    // finished build — so it must clear the history by itself.
    let mut tap = Tap::active();
    for ch in "hoongf ".chars() {
        tap.decide(&KeyEvent::character(ch));
    }
    tap.session.set_frontmost_app("com.tinyspeck.slackmacgap");
    assert_eq!(type_with_deletes(&mut tap, "⌫z"), "z");
}

/// A word auto-fix restored is not re-composable, and it clears the history rather
/// than merely staying out of it: it still occupies space on screen, so leaving it
/// out would break the invariant that the stack is an unbroken run of words
/// immediately behind the caret.
#[test]
fn a_restored_word_breaks_the_chain() {
    let mut tap = Tap::active();
    // `work ` is restored by auto-fix, so nothing behind it stays re-openable.
    assert_eq!(type_with_deletes(&mut tap, "hoongf work ⌫z"), "hồng workz");
}

// ── The macros exception to the exclusion gate ──────────────────────────────

/// Vietnamese off but macros on: the keys must still reach the engine, because
/// UniKey's always-macro expands a shortcut regardless of mode.
#[test]
fn always_macro_keeps_feeding_the_engine_with_vietnamese_off() {
    let mut session = Session::new(
        PlacementStyle::default(),
        ExclusionList::with_defaults(shipped_defaults()),
    );
    session.set_always_macro(true);
    session.set_macros(vec![glowkey_session::Macro {
        shortcut: "vn".into(),
        expansion: "Việt Nam".into(),
    }]);
    let mut tap = Tap {
        session,
        preset: HotkeyPreset::default(),
        recorded_code: None,
        effects: Effects::default(),
    };
    tap.session.set_frontmost_app("com.apple.TextEdit");
    tap.session.toggle_mode(); // → English
    assert_eq!(tap.session.mode(), InputMode::English);
    assert_eq!(type_through(&mut tap, "vn "), "Việt Nam ");
}

// ── A session-only terminal toggle ──────────────────────────────────────────

#[test]
fn a_terminal_enabled_by_hotkey_is_live_but_still_persisted_as_excluded() {
    // A shipped *terminal*, because the session-only rule is specifically the
    // accidental-un-exclusion protection for terminals.
    let terminal = a_terminal_default();
    let mut tap = Tap::bare();
    tap.session.set_frontmost_app(terminal);
    assert_eq!(type_through(&mut tap, "hoongf"), "hoongf"); // excluded by default

    let outcome = tap.session.toggle_app_exclusion(terminal);
    assert_eq!(outcome, ExclusionToggle::EnabledSessionOnly);
    assert_eq!(type_through(&mut tap, "hoongf"), "hồng"); // live for the session
    assert!(
        tap.session.exclusions().ids().any(|id| id == terminal),
        "the persisted exclusion must survive a session-only toggle"
    );
}

// ── A recorded custom hotkey matches by the code the platform recorded ──────

#[test]
fn a_recorded_custom_hotkey_toggles_and_the_old_preset_stops() {
    let mut tap = Tap::active();
    // ⌃⌥K, recorded on a platform that calls that key code 40.
    tap.preset = HotkeyPreset::Custom {
        control: true,
        shift: false,
        option: true,
        key_char: 'K',
        raw_code: Some(40),
    };
    tap.recorded_code = Some(40);

    let combo = KeyEvent::character('k')
        .with_mods(Modifiers {
            control: true,
            shift: false,
            option: true,
            command: false,
        })
        .with_raw_code(40);
    assert!(matches!(tap.decide(&combo), Decision::Consume));
    assert_eq!(tap.session.mode(), InputMode::English);

    // The old default no longer toggles.
    let old = KeyEvent::key(Key::Space).with_mods(ctrl_shift());
    tap.decide(&old);
    assert_eq!(
        tap.session.mode(),
        InputMode::English,
        "the replaced preset must not toggle anymore"
    );
}
