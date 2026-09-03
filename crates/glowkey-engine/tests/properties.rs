//! Generated-input proof of the engine's one load-bearing invariant.
//!
//! The blind model (`docs/handoff.md` §5) says GlowKey never reads the host
//! document: its only guarantee is **"rendered == the text tail at the caret."**
//! Everything the application does rests on that equality, and every typing bug
//! ever reported against GlowKey has been a violation of it.
//!
//! The hand-written suites pin known words. They cannot cover the keystroke
//! space, and the words nobody thought to write down are exactly where the next
//! auto-fix change will break. So this file states the rule as a property and
//! lets `proptest` search for a counterexample.
//!
//! The model here mirrors `app/src/tap.rs::decide` deliberately: a word
//! character goes to `Session::process_key` and the returned edit is applied; a
//! boundary character calls `Session::commit` and applies any auto-fix restore
//! before the boundary key lands. A property that modelled a path the tap never
//! takes would prove nothing.

use glowkey_engine::{
    ExclusionList, InputMethod, KeyResponse, PlacementStyle, Session, WordPreference,
};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;

/// A bundle id that is not in `DEFAULT_EXCLUSIONS`, so the session transforms.
/// `is_active()` fails closed on an unknown app, so this call is not optional.
const TEST_APP: &str = "com.example.PropertyTest";

/// Stands in for the Delete key in a generated sequence. Not a character the
/// engine ever receives — `tap.rs` routes Backspace by key code, before any
/// character lookup — so it is safe to borrow it as a marker here.
const BACKSPACE: char = '\u{8}';

/// The host document, split where the engine's knowledge ends.
///
/// `tail` is the word being composed — the "text tail at the caret" the whole
/// invariant is about. `committed` is everything before it, which the engine has
/// no memory of and must never touch. Keeping the two apart is what lets this
/// model catch an edit that reaches past the start of the word: in the real
/// application that character belongs to the user's document.
#[derive(Debug, Default)]
struct Screen {
    committed: String,
    tail: String,
}

impl Screen {
    /// Applies one edit exactly as `tap.rs::emit_edit` does: delete `backspaces`
    /// **UTF-16 code units** from the end, then insert.
    ///
    /// Returns `Err` when the edit would delete past the start of the word, which
    /// in the real application means eating a character of the user's existing
    /// document — the worst failure this code can produce, and the reason this is
    /// checked rather than saturated.
    fn apply(&mut self, response: &KeyResponse) -> Result<(), String> {
        let units: Vec<u16> = self.tail.encode_utf16().collect();
        if response.backspaces > units.len() {
            return Err(format!(
                "edit deletes {} UTF-16 units from a {}-unit word — this would eat \
                 the document to the left of the caret",
                response.backspaces,
                units.len()
            ));
        }
        let keep = units.len() - response.backspaces;
        self.tail = String::from_utf16(&units[..keep])
            .map_err(|_| "edit split a surrogate pair".to_string())?;
        self.tail.push_str(&response.insert);
        Ok(())
    }
}

/// Whether `ch` extends the current word, matching `tap.rs`'s `is_word_char`
/// closure. Kept in step with the tap by construction: the engine's own
/// `is_syllable_char` is the same rule, and both are method-aware.
fn is_word_char(session: &Session, ch: char) -> bool {
    ch.is_ascii_alphabetic()
        || (ch.is_ascii_digit() && session.input_method() == InputMethod::Vni)
        || (session.telex_brackets()
            && session.input_method() == InputMethod::Telex
            && matches!(ch, '[' | ']' | '{' | '}'))
}

/// Feeds one key through the same decision path the tap uses and checks every
/// invariant that applies to it. Returns a description of the first violation.
fn step(session: &mut Session, screen: &mut Screen, ch: char) -> Result<(), String> {
    if ch == BACKSPACE {
        return backspace_step(session, screen);
    }
    if is_word_char(session, ch) {
        let before = screen.tail.clone();
        let response = session.process_key(ch);
        if !response.handled {
            // The engine declined the key and reset, so the host inserts it
            // verbatim and the word is over — the same shape as a boundary.
            // Unreachable as configured (these sessions are always active, and
            // `is_word_char` here is the same rule as `Engine::is_syllable_char`),
            // but modelled correctly rather than plausibly: pushing onto `tail`
            // instead would leave the model disagreeing with an engine that has
            // just emptied itself, and the next key would then fail property 1 one
            // step late and in the wrong place.
            screen.committed.push_str(&screen.tail);
            screen.committed.push(ch);
            screen.tail.clear();
            return Ok(());
        }
        // Property 2 — an edit must never reach past the start of the word.
        screen
            .apply(&response)
            .map_err(|why| format!("key {ch:?} on tail {before:?}: {why}"))?;
        // Property 1 — the invariant itself.
        if screen.tail != session.current_word() {
            return Err(format!(
                "key {ch:?}: screen {:?} != engine {:?} (tail before {before:?}, \
                 edit bs={} ins={:?})",
                screen.tail,
                session.current_word(),
                response.backspaces,
                response.insert,
            ));
        }
        Ok(())
    } else {
        // A boundary key: commit, apply any restore edit, then the key lands.
        let rendered_before = session.current_word().to_string();
        let restore = session.commit();
        if let Some(response) = restore {
            // **The restore edit is checked exactly, not merely for sanity.**
            //
            // It has to replace the whole rendered word — that is what auto-fix
            // means (`eĩt` becomes `exit`) — so its backspace count is fully
            // determined: the rendering's UTF-16 length, no more and no less.
            // Checking only that it does not delete *too much* is not enough: an
            // edit that deletes too little leaves the difference stranded on
            // screen, and because `commit` clears the re-composition memory when
            // it restores (`lib.rs`, `last_committed = None`), nothing downstream
            // would ever notice. Deleting one unit too few turns `eĩt`␣ into
            // `ĩexit` and no other assertion in this file would fail.
            //
            // This matters beyond today: the ASCII-render restore planned in
            // `plans/260903-1637-unikey-phonotactics-and-restore/` rewrites this
            // exact decision, and this is the assertion that has to hold across
            // that change.
            let want = rendered_before.encode_utf16().count();
            if response.backspaces != want {
                return Err(format!(
                    "restore at boundary {ch:?}: deletes {} UTF-16 units but the \
                     rendered word {rendered_before:?} is {want} — it must replace \
                     the whole word",
                    response.backspaces
                ));
            }
            screen
                .apply(&response)
                .map_err(|why| format!("restore at boundary {ch:?}: {why}"))?;
            // After a full replacement the word on screen is exactly what the edit
            // inserted: the raw keystrokes, or a macro expansion.
            if screen.tail != response.insert {
                return Err(format!(
                    "restore at boundary {ch:?}: screen {:?} != inserted {:?}",
                    screen.tail, response.insert
                ));
            }
        }
        // Committing always ends the word, and committing again is inert — the
        // shell calls `commit` once per boundary, and a second call must not emit
        // a second edit for a word that is already finished.
        if !session.current_word().is_empty() {
            return Err(format!(
                "boundary {ch:?}: engine still composing {:?} after commit \
                 (was {rendered_before:?})",
                session.current_word()
            ));
        }
        if let Some(again) = session.commit() {
            return Err(format!(
                "boundary {ch:?}: committing twice emitted a second edit \
                 (bs={} ins={:?})",
                again.backspaces, again.insert
            ));
        }
        // The tap primes sentence capitalization from the boundary character
        // straight after the commit (`tap.rs`: `commit()` then `note_boundary`).
        // Without this the `auto_capitalize` option below is nearly inert — the
        // pending-capital flag would only ever come from the session's initial
        // state, so the sentence-restart path after `.`/`!`/`?` would never run.
        session.note_boundary(ch);
        // The word is finished and the boundary character is inserted after it;
        // the next word starts from an empty tail.
        screen.committed.push_str(&screen.tail);
        screen.committed.push(ch);
        screen.tail.clear();
        Ok(())
    }
}

/// The Backspace path, mirroring `tap.rs::decide`'s `KEY_CODE_DELETE` branch: the
/// **host** always performs the delete, and the engine only re-syncs to whatever
/// the screen will then show. Three cases in the same order the tap tries them.
///
/// This is the most delicate path in the project. Both re-composition and the
/// mid-word shrink make a promise about text the engine cannot see, and a wrong
/// answer desynchronises the diff baseline silently — the engine keeps composing
/// against characters that are not on screen.
fn backspace_step(session: &mut Session, screen: &mut Screen) -> Result<(), String> {
    let recomposed = session.recompose_after_boundary_backspace();
    let shrank = !recomposed && session.backspace_visible_char();
    if !recomposed && !shrank {
        session.flush();
    }

    if recomposed {
        // The engine reopened the word committed before this boundary. The host is
        // about to delete the boundary character, which puts the caret back at the
        // end of that word — so the engine's render must be exactly the text now
        // sitting there.
        let word = session.current_word().to_string();
        if !screen.tail.is_empty() {
            return Err(format!(
                "re-composed {word:?} while still composing {:?}",
                screen.tail
            ));
        }
        // Delete the boundary character the host removes.
        if screen.committed.pop().is_none() {
            return Err(format!("re-composed {word:?} with nothing on screen"));
        }
        if !screen.committed.ends_with(&word) {
            return Err(format!(
                "re-composed {word:?} but the text at the caret is {:?}",
                screen.committed
            ));
        }
        let keep = screen.committed.len() - word.len();
        screen.committed.truncate(keep);
        screen.tail = word;
        return Ok(());
    }

    // Not a re-composition: the host deletes one character from the caret.
    //
    // The model pops one `char` while `Screen::apply` counts UTF-16 code units,
    // and those agree only while every render is precomposed and in the basic
    // multilingual plane. Vietnamese NFC is, but that is an assumption about the
    // `vi` crate's output rather than something this file controls, so it is
    // asserted rather than trusted — a decomposed render (a base letter plus a
    // combining mark) would make the host's Backspace delete a mark where this
    // model deletes a whole character.
    let before_units = screen.tail.encode_utf16().count();
    let popped = screen.tail.pop();
    if let Some(popped) = popped {
        let removed = before_units - screen.tail.encode_utf16().count();
        if removed != 1 {
            return Err(format!(
                "render is not precomposed BMP: dropping {popped:?} removed \
                 {removed} UTF-16 units, so a host Backspace and this model \
                 disagree about what one keypress deletes"
            ));
        }
    } else if screen.committed.pop().is_none() {
        // Deleting from an empty document. Nothing to check; the engine flushed.
        return Ok(());
    }

    if shrank {
        // The engine promised it is still in step with the screen.
        if screen.tail != session.current_word() {
            return Err(format!(
                "after mid-word Backspace: screen {:?} != engine {:?}",
                screen.tail,
                session.current_word()
            ));
        }
    } else {
        // Flushed: the engine composes nothing, so whatever is left on screen is
        // now context the next word will be appended after.
        if !session.current_word().is_empty() {
            return Err(format!(
                "flushed but still composing {:?}",
                session.current_word()
            ));
        }
        screen.committed.push_str(&screen.tail);
        screen.tail.clear();
    }
    Ok(())
}

/// Keys weighted toward the ones that actually transform, so the search spends
/// its budget in the interesting part of the space rather than on `q` and `y`.
/// Telex tone keys (`f s r x j`), the diacritic doublers (`a e o d w`), the VNI
/// digits, the bracket shortcuts, and boundary characters all appear.
fn key() -> impl Strategy<Value = char> {
    prop_oneof![
        20 => prop::sample::select(vec!['a', 'e', 'o', 'u', 'i', 'y']),
        20 => prop::sample::select(vec!['f', 's', 'r', 'x', 'j', 'z', 'w', 'd']),
        10 => prop::sample::select(vec!['b', 'c', 'g', 'h', 'k', 'l', 'm', 'n', 'p', 't', 'v', 'q']),
        4 => prop::sample::select(vec!['1', '2', '3', '4', '5', '6', '7', '8', '9', '0']),
        3 => prop::sample::select(vec!['[', ']', '{', '}']),
        3 => prop::sample::select(vec![' ', '.', ',', '!', '-', '\'']),
        6 => Just(BACKSPACE),
        2 => prop::sample::select(vec!['A', 'E', 'O', 'D', 'H', 'N', 'W', 'F']),
    ]
}

/// The opt-in options, as one generated bundle. These four are the matrix the
/// hand-written suites cannot cover: each was added separately, each changes how
/// a render is derived, and nothing until now has typed a word with an arbitrary
/// combination of them switched on.
#[derive(Debug, Clone, Copy)]
struct Options {
    method: InputMethod,
    auto_fix: bool,
    restore_english: bool,
    quick_telex: bool,
    telex_brackets: bool,
    strict_spell_check: bool,
    auto_capitalize: bool,
    /// Whether a macro is defined. `commit` checks macros *before* auto-fix and
    /// emits a different edit shape when one matches (the on-screen length, then
    /// the expansion), so with no macros configured that branch is never entered
    /// and its backspace count goes unverified — the same gap the restore edit
    /// had. The shortcut is a plain two-letter word so ordinary generated
    /// sequences can actually hit it.
    macro_defined: bool,
}

impl Options {
    /// A session configured with these options, ready to type into.
    fn session(self) -> Session {
        let mut session = Session::new(PlacementStyle::New, ExclusionList::new());
        session.set_frontmost_app(TEST_APP);
        session.set_input_method(self.method);
        session.set_auto_fix(self.auto_fix);
        session.set_restore_english_words(self.restore_english);
        session.set_quick_telex(self.quick_telex);
        session.set_telex_brackets(self.telex_brackets);
        session.set_strict_spell_check(self.strict_spell_check);
        session.set_auto_capitalize(self.auto_capitalize);
        if self.macro_defined {
            // EVKey/UniKey's own example shortcut, so the expansion is realistic:
            // multi-word, and longer than the keys that trigger it.
            session.add_macro("vn", "Việt Nam");
        }
        session
    }
}

fn options() -> impl Strategy<Value = Options> {
    (
        prop::sample::select(vec![
            InputMethod::Telex,
            InputMethod::Vni,
            InputMethod::SimpleTelex,
        ]),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(
            |(
                method,
                auto_fix,
                restore_english,
                quick_telex,
                telex_brackets,
                strict_spell_check,
                auto_capitalize,
                macro_defined,
            )| Options {
                method,
                auto_fix,
                restore_english,
                quick_telex,
                telex_brackets,
                strict_spell_check,
                auto_capitalize,
                macro_defined,
            },
        )
}

fn keys() -> impl Strategy<Value = Vec<char>> {
    prop::collection::vec(key(), 1..14)
}

proptest! {
    // 4096 cases rather than the default 256. `options()` generates three input
    // methods and seven independent flags — 384 configurations — so the default
    // budget would sample each one less than once, and 4096 gives about ten.
    //
    // The explicit `failure_persistence` is not decoration. Proptest's default
    // looks for `lib.rs` or `main.rs` beside the test to decide where to write
    // its regressions file; from an integration test in `tests/` it finds
    // neither, prints "failed to find lib.rs or main.rs" on every run, and
    // silently keeps no record at all. A failure at case 3000 of 4096 would then
    // print its seed into a CI log that gets discarded, be unreproducible
    // locally, and never be pinned — exactly the flakiness this suite must not
    // have. Naming the path makes each counterexample a committed file that
    // re-runs first on every later invocation.
    #![proptest_config(ProptestConfig {
        cases: 4096,
        // Relative to the **crate root** — that is the working directory cargo
        // gives a test binary. A workspace-relative path here quietly creates a
        // nested `crates/glowkey-engine/crates/…` instead.
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/properties.txt",
        ))),
        ..ProptestConfig::default()
    })]

    /// **Property 1, and 2 and 5 alongside it.** For any keystroke sequence under
    /// any combination of options: applying each returned edit to the screen
    /// reproduces the engine's own rendering exactly, no edit ever deletes past
    /// the start of the word, and a boundary always ends the composition.
    ///
    /// This is the whole blind model. If it holds, the application is correct by
    /// construction; when it fails, the shrunk `keys` value is a minimal repro.
    #[test]
    fn the_diff_always_reproduces_the_render(options in options(), keys in keys()) {
        let mut session = options.session();
        let mut screen = Screen::default();
        for ch in keys {
            if let Err(why) = step(&mut session, &mut screen, ch) {
                prop_assert!(false, "{options:?}: {why}");
            }
        }
    }

    /// **Property 3.** No input panics and none diverges. Every ASCII printable
    /// character reaches the engine here, including the ones the weighted
    /// generator above rarely picks and the ones that are only word characters
    /// under some options (digits in VNI, brackets with the shortcuts on).
    ///
    /// The assertion is simply that this returns: a panic in the engine would
    /// unwind into CoreFoundation's C frames in the real application, which is
    /// why `tap.rs` wraps its callback in `catch_unwind` — but not panicking in
    /// the first place is the actual requirement.
    #[test]
    fn no_ascii_input_panics(
        options in options(),
        keys in prop::collection::vec(0x20u8..0x7f, 1..40),
    ) {
        let mut session = options.session();
        let mut screen = Screen::default();
        for byte in keys {
            // Errors are the invariant's business (property 1); here we only
            // require that the call returns at all.
            let _ = step(&mut session, &mut screen, byte as char);
        }
    }

    /// **Property 4.** `backspace_visible_char` either declines, or leaves the
    /// render equal to the previous render minus its last character.
    ///
    /// This is the contract `tap.rs` leans on for a mid-word Backspace: the host
    /// performs the delete, so a `true` answer is a promise that the engine now
    /// matches what the screen shows. A `true` that landed anywhere else would
    /// desynchronise the diff baseline silently — the engine would keep composing
    /// against text that is not there.
    #[test]
    fn mid_word_backspace_lands_exactly_one_character_back(
        options in options(),
        keys in keys(),
    ) {
        let mut session = options.session();
        let mut screen = Screen::default();
        for ch in keys {
            if step(&mut session, &mut screen, ch).is_err() {
                // Property 1 owns that failure; do not double-report it here.
                // The coupling is worth naming: if property 1 ever regresses, this
                // property goes vacuous rather than failing too, so a green result
                // here means nothing until property 1 is green.
                return Ok(());
            }
        }
        // Called without the `recompose_after_boundary_backspace` that always
        // precedes it in `tap.rs`, so this exercises a state the tap itself cannot
        // reach. That is deliberate — the contract should hold on its own — but it
        // means a failure here needs triage before it is believed.
        let before = session.current_word().to_string();
        let mut expected = before.clone();
        expected.pop();
        if session.backspace_visible_char() {
            prop_assert_eq!(
                session.current_word(),
                expected.as_str(),
                "{:?}: backspace_visible_char returned true from {:?} but landed on {:?},                  not {:?}",
                options,
                before,
                session.current_word(),
                expected
            );
        }
    }
}

/// The four render-shaping options, exhaustively, over words known to transform —
/// the deterministic companion to the generated suite above.
///
/// `proptest` samples its 384 configurations; this visits every combination of
/// the four options that change how a render is *derived* (input method, auto-fix,
/// English restore, Quick Telex, brackets) with the same fixed words, so a cell
/// that only breaks for a real Vietnamese word cannot hide behind a random seed.
/// The two options left pinned here — the mid-word spell check on, auto-capitalize
/// off — vary in the generated suite instead; the point of this test is
/// reproducibility, not a second full matrix.
#[test]
fn every_render_option_combination_holds_for_real_words() {
    // Telex-significant words, plus the English words auto-fix exists to rescue.
    const WORDS: &[&str] = &[
        "hoongf", "nguowif", "vieetj", "ddaaij", "exit", "address", "was", "left",
    ];
    let methods = [
        InputMethod::Telex,
        InputMethod::Vni,
        InputMethod::SimpleTelex,
    ];
    let mut checked = 0;
    for method in methods {
        for flags in 0u8..16 {
            let options = Options {
                method,
                auto_fix: flags & 1 != 0,
                restore_english: flags & 2 != 0,
                quick_telex: flags & 4 != 0,
                telex_brackets: flags & 8 != 0,
                strict_spell_check: true,
                auto_capitalize: false,
                macro_defined: false,
            };
            for word in WORDS {
                // Each word three ways: no per-word decision, pinned to the raw
                // keys, and pinned to the Vietnamese rendering. The override is a
                // *fourth* kind of edit `commit` can emit, and an edit the model
                // applies but never verifies is exactly how a suite ends up with
                // no teeth where it matters.
                for pinned in [
                    None,
                    Some(WordPreference::Raw),
                    Some(WordPreference::Vietnamese),
                ] {
                    let mut session = options.session();
                    if let Some(prefer) = pinned {
                        session.set_word_override(word, prefer);
                    }
                    let mut screen = Screen::default();
                    for ch in word.chars() {
                        if let Err(why) = step(&mut session, &mut screen, ch) {
                            panic!("{options:?} {pinned:?} typing {word:?}: {why}");
                        }
                    }
                    // Finish at a boundary too — the commit path is where auto-fix,
                    // the English restore and the override all actually fire.
                    if let Err(why) = step(&mut session, &mut screen, ' ') {
                        panic!("{options:?} {pinned:?} finishing {word:?}: {why}");
                    }
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 3 * 16 * WORDS.len() * 3, "matrix coverage changed");
}
