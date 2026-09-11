//! UniKey's second spell-check option (`spellCheckEnabled`): refuse a diacritic
//! that would make the word impossible in Vietnamese, at the keystroke — as
//! distinct from auto-fix, which restores raw keys at the word boundary.
//!
//! The whole risk is false rejections: wrongly refusing a keystroke corrupts a
//! word the user typed correctly. The corpus below is the guard, and it was run
//! before the feature was built to prove the rule was viable at all.

use glowkey_engine::{is_invalid_vietnamese, BackspaceOutcome, Engine, PlacementStyle};

const CORPUS: &[(&str, &str)] = &[
    ("chaof", "chào"),
    ("vieejt", "việt"),
    ("nam", "nam"),
    ("hoongf", "hồng"),
    ("nguyeenx", "nguyễn"),
    ("ddaij", "đại"),
    ("hocj", "học"),
    ("sinh", "sinh"),
    ("truowngf", "trường"),
    ("nguowif", "người"),
    ("ddaats", "đất"),
    ("nuowcs", "nước"),
    ("ngayf", "ngày"),
    ("thangs", "tháng"),
    ("nawm", "năm"),
    ("tuooir", "tuổi"),
    ("ddepj", "đẹp"),
    ("gioir", "giỏi"),
    ("khoer", "khoẻ"),
    ("camr", "cảm"),
    ("own", "ơn"),
    ("looix", "lỗi"),
    ("bawts", "bắt"),
    ("ddaauf", "đầu"),
    ("quyeets", "quyết"),
    ("nghieepj", "nghiệp"),
    ("thuowngr", "thưởng"),
    ("cuoocj", "cuộc"),
    ("soongs", "sống"),
    ("tieengs", "tiếng"),
    ("nois", "nói"),
    ("bieets", "biết"),
    ("muoons", "muốn"),
    ("nhuwng", "nhưng"),
    ("dduowcj", "được"),
    ("khoong", "không"),
    ("nhieeuf", "nhiều"),
    ("theer", "thể"),
    ("nhuw", "như"),
    ("cuar", "của"),
    ("nhuwngx", "những"),
    ("veef", "về"),
    ("laf", "là"),
    ("mootj", "một"),
    ("cows", "cớ"),
    ("ddi", "đi"),
    ("laamf", "lầm"),
    ("xuoongs", "xuống"),
    ("truowcs", "trước"),
    ("giuwax", "giữa"),
    ("ngoaif", "ngoài"),
    // Non-ASCII words ending `-nh` and `-ch`, and the open `ưa`. Added
    // 2026-09-06: the corpus is named as the guard against a phonotactic rule
    // over-reaching, and it had no word of either shape — so the `nh`/`ch` rule
    // and the `ưa` rule could each have been arbitrarily wrong and this test
    // would still have passed.
    ("sachs", "sách"),
    ("anhs", "ánh"),
    ("tinhs", "tính"),
    ("hoachj", "hoạch"),
    ("khueechs", "khuếch"),
    ("leechj", "lệch"),
    ("giuwax", "giữa"),
];

fn typed(input: &str, strict: bool) -> String {
    let mut engine = Engine::new(PlacementStyle::New);
    engine.set_strict_spell_check(strict);
    for ch in input.chars() {
        engine.process_key(ch);
    }
    engine.current_word().to_string()
}

#[test]
fn no_false_rejection_across_real_vietnamese() {
    // Every word must type identically with the option on and off. A difference
    // means the check refused a legitimate keystroke.
    for (keys, expected) in CORPUS {
        assert_eq!(
            typed(keys, false),
            *expected,
            "corpus entry {keys} is wrong"
        );
        assert_eq!(
            typed(keys, true),
            *expected,
            "{keys} was corrupted by the mid-word spell check"
        );
    }
}

#[test]
fn off_by_default() {
    assert!(!Engine::new(PlacementStyle::New).strict_spell_check());
}

#[test]
fn the_repeat_key_escape_hatch_still_works() {
    // Pressing the diacritic key again is a deliberate rejection by the user, and
    // for every result Vietnamese can actually spell the check leaves it alone.
    // These three render pure ASCII, which is never refused.
    for (keys, expected) in [("cass", "cas"), ("aaa", "aa"), ("ddd", "dd")] {
        assert_eq!(typed(keys, true), expected, "{keys} with strict check on");
    }
}

/// A rejection that lands on something unspellable is still unspellable.
///
/// `hoongff` rejects the tone and leaves `hôngf`, which Vietnamese cannot spell.
/// The check used to exempt it, on the grounds that refusing a rejection undoes
/// what the user asked for. Changed 2026-09-04 from live use: with the check on,
/// the promise is that an impossible result shows you what you typed, and
/// `hôngf` is impossible like any other.
#[test]
fn a_rejection_that_is_still_unspellable_shows_the_raw_keys() {
    assert_eq!(typed("hoongff", true), "hoongff", "with the spell check on");
    // The gesture itself is untouched: with the check off — the default — the
    // rejection behaves exactly as it always has.
    assert_eq!(typed("hoongff", false), "hôngf", "with the spell check off");
}

#[test]
fn english_words_are_never_refused() {
    // A pure-ASCII render is what the user typed verbatim, so the check never
    // fires on it — English stays the business of auto-fix.
    for word in ["hello", "the", "code", "print", "value"] {
        assert_eq!(typed(word, true), typed(word, false), "{word}");
    }
}

/// Drives keys against a screen that already holds text, the way a document does.
fn screen_after(prefix: &str, keys: &str, strict: bool) -> String {
    let mut engine = Engine::new(PlacementStyle::New);
    engine.set_strict_spell_check(strict);
    let mut screen: Vec<u16> = prefix.encode_utf16().collect();
    for ch in keys.chars() {
        let r = engine.process_key(ch);
        if r.handled {
            for _ in 0..r.backspaces {
                screen.pop();
            }
            screen.extend(r.insert.encode_utf16());
        } else {
            screen.push(ch as u16);
        }
    }
    String::from_utf16(&screen).unwrap()
}

#[test]
fn never_touches_the_document_before_the_word() {
    // The refusal used to render twice and diff the second against the first,
    // while the screen still held neither — so the backspace count overshot and
    // ate one character to the LEFT of the word. It hit about a quarter of
    // English words; `aal` swallowed the preceding space.
    for keys in [
        "aal", "vieejtw", "nguowifw", "afire", "academic", "aardvark",
    ] {
        let out = screen_after("Xin chao ", keys, true);
        assert!(
            out.starts_with("Xin chao "),
            "{keys} destroyed the text before it: {out:?}"
        );
    }
}

#[test]
fn the_escape_does_not_outlive_the_word() {
    // Deleting an escaped word away used to leave the flag set, because
    // backspace_visible_char reported success on an emptied word so the shell
    // never flushed. Vietnamese then stayed dead for the next word.
    let mut engine = Engine::new(PlacementStyle::New);
    engine.set_strict_spell_check(true);
    for ch in "aal".chars() {
        engine.process_key(ch);
    }
    // Model the shell's three-case ladder rather than ignoring the answer. The
    // engine can decline to stay in step — deleting the only character of a word
    // that exists solely through a transformation (`â`⌫) is the ordinary case —
    // and the caller is then obliged to flush. A test that drops the return value
    // asserts against a state no shell would ever be in.
    for _ in 0..3 {
        if engine.backspace_visible_char() == BackspaceOutcome::Flush {
            engine.reset();
        }
    }
    for ch in "hoongf".chars() {
        engine.process_key(ch);
    }
    assert_eq!(engine.current_word(), "hồng");
}

#[test]
fn keeps_the_leading_d_bar_carve_out() {
    // Auto-fix exempts a leading đ so the chat abbreviations survive; the
    // mid-word check shares the predicate and must exempt them too.
    for (keys, expected) in [("ddc", "đc"), ("ddt", "đt"), ("ddk", "đk")] {
        assert_eq!(typed(keys, true), expected, "{keys} under strict check");
    }
}

/// Deleting the key that caused the escape undoes the escape.
///
/// Reported from live use: `hoongf` gives `hồng`, a mistyped `a` escapes the word
/// to `hoongfa`, and Backspace left `hoongf` — the raw keys, stuck verbatim for
/// the rest of the word's life, because the escape was a one-way latch. The way
/// out was missing: the check that refuses a word never re-ran when the word got
/// shorter.
#[test]
fn deleting_the_offending_key_restores_the_transformation() {
    let mut engine = Engine::new(PlacementStyle::New);
    engine.set_strict_spell_check(true);
    for ch in "hoongf".chars() {
        engine.process_key(ch);
    }
    assert_eq!(engine.current_word(), "hồng");

    // The mistake: the word can no longer be spelled, so it shows its raw keys.
    engine.process_key('a');
    assert_eq!(engine.current_word(), "hoongfa");

    // Backspace repairs it in one edit. The count covers the *whole* on-screen
    // word, because the shell suppresses the keystroke rather than letting the
    // host delete — mixing a native delete with a synthesized edit is the race
    // the full-suppression model exists to remove.
    match engine.backspace_visible_char() {
        BackspaceOutcome::Repair(edit) => {
            assert_eq!(edit.backspaces, "hoongfa".encode_utf16().count());
            assert_eq!(edit.insert, "hồng");
        }
        other => panic!("expected a repair, got {other:?}"),
    }
    assert_eq!(engine.current_word(), "hồng");
    assert!(engine.is_composing(), "and it is still being composed");

    // Still a live Vietnamese word: the next tone key applies rather than landing
    // as a literal, which is the whole point of getting the escape lifted.
    let r = engine.process_key('z');
    assert_eq!(
        engine.current_word(),
        "hông",
        "z removes the tone, so the huyền goes"
    );
    assert!(r.handled);
}

/// Keeping on deleting keeps transforming, rather than re-escaping.
#[test]
fn deleting_further_keeps_the_word_transformed() {
    let mut engine = Engine::new(PlacementStyle::New);
    engine.set_strict_spell_check(true);
    for ch in "hoongfa".chars() {
        engine.process_key(ch);
    }
    assert_eq!(engine.current_word(), "hoongfa");

    // The first delete repairs the word; after that the escape is gone and the
    // ordinary mid-word rule applies again — the render minus its last visible
    // character, which is the contract `docs/handoff.md` §4 records.
    for expected in ["hồng", "hồn", "hồ"] {
        match engine.backspace_visible_char() {
            BackspaceOutcome::Repair(edit) => assert_eq!(edit.insert, expected),
            BackspaceOutcome::InStep => {}
            BackspaceOutcome::Flush => panic!("unexpected flush before {expected:?}"),
        }
        assert_eq!(
            engine.current_word(),
            expected,
            "still transforming, never re-escaped"
        );
    }
}

/// A word that is *still* unspellable after the delete stays escaped — the exit
/// asks the same question the entry did, so it cannot let through something the
/// check would refuse.
#[test]
fn a_still_unspellable_word_stays_escaped() {
    let mut engine = Engine::new(PlacementStyle::New);
    engine.set_strict_spell_check(true);
    // `nguyeenxk` escapes, and dropping one key does not make it spellable.
    for ch in "nguyeenxkk".chars() {
        engine.process_key(ch);
    }
    let before = engine.current_word().to_string();
    assert_eq!(before, "nguyeenxkk", "escaped to its raw keys");
    match engine.backspace_visible_char() {
        BackspaceOutcome::InStep => assert_eq!(engine.current_word(), "nguyeenxk"),
        other => panic!("still-unspellable word must stay escaped, got {other:?}"),
    }
}

/// With the option off, none of this happens: the same sequence behaves exactly
/// as it always has, and the repair path is unreachable.
#[test]
fn with_the_spell_check_off_nothing_changes() {
    let mut engine = Engine::new(PlacementStyle::New);
    for ch in "hoongfa".chars() {
        engine.process_key(ch);
    }
    // Never escaped, so the render is the ordinary transformation.
    assert_eq!(engine.current_word(), "hồnga");
    assert_eq!(engine.backspace_visible_char(), BackspaceOutcome::InStep);
    assert_eq!(engine.current_word(), "hồng");
}

/// Deleting an escaped word away must clear the escape — guarded directly.
///
/// `the_escape_does_not_outlive_the_word` above used to be this guard, and the
/// un-escape-on-backspace fix silently took its teeth away: with the escape now
/// lifting on the *first* backspace, that word never reaches empty while still
/// escaped, so the empty-word clear became unreachable from it. Deleting
/// `escaped = false` from the `raw.is_empty()` branch left the whole engine
/// suite green.
///
/// The line is not dead in principle — `process_key_verbatim` escapes a word
/// with no spell check involved and no un-escape path — so this reaches empty
/// through that door and asserts the next word still transforms.
#[test]
fn deleting_a_verbatim_word_away_clears_the_escape() {
    let mut engine = Engine::new(PlacementStyle::New);
    // The always-macro path: keys compose verbatim so a shortcut can still match
    // at the boundary. A **single** key is the case that matters — with two, the
    // first backspace un-escapes (a one-key ASCII render is spellable) and the
    // word never reaches empty while still escaped, which is exactly how the
    // original guard lost its teeth.
    engine.process_key_verbatim('v');
    assert_eq!(engine.current_word(), "v");

    assert_eq!(
        engine.backspace_visible_char(),
        BackspaceOutcome::InStep,
        "the last key deletes normally"
    );
    assert!(!engine.is_composing(), "the word is now empty");

    // The escape must not outlive the word: the next word transforms normally.
    for ch in "hoongf".chars() {
        engine.process_key(ch);
    }
    assert_eq!(
        engine.current_word(),
        "hồng",
        "the escape leaked into the next word"
    );
}

/// **`hoongfc` escapes to raw keys; `hoongf4` does not.** Reported as an
/// inconsistency on 2026-09-06, and it is the two rules meeting rather than a
/// defect.
///
/// `c` extends the syllable, so the render is re-judged, fails, and the word is
/// handed back verbatim. `4` ends the syllable, so `hồng` had already committed
/// as valid Vietnamese and there is nothing for the check to refuse. The
/// boundary half is pinned in `telex.rs`; this is the escape half, and it needs
/// the option on because that is how the user who reported it was configured.
#[test]
fn a_digit_after_a_valid_word_is_not_escaped() {
    // The letter drags the whole word into invalidity, so it renders verbatim.
    assert_eq!(typed("hoongfc", true), "hoongfc");

    // The digit half is asserted in `telex.rs`, against reconstructed screen
    // text. It cannot be asserted here: `typed` returns `current_word`, which is
    // empty after a boundary either way, so it could not tell "the digit
    // committed the word cleanly" from "the digit ate it".
    assert_eq!(typed("hoongf", true), "hồng");

    // And with the check off the letter case is an ordinary render, which is the
    // difference the option exists to make.
    assert_eq!(typed("hoongfc", false), "hồngc");
}

/// **A re-opened word remembers that it was escaped.** Reported 2026-09-06.
///
/// Typed: `hoongfb` (the `b` escapes the word to its raw keys), space, `ss`,
/// then Backspace four times. The last one deletes the boundary, which re-opens
/// `hoongfb` behind the caret, and the one after it should lift the escape and
/// give `hồng` — exactly as it does when the word never committed.
///
/// It did not. `Engine::restore` put back `raw` and `rendered` and left
/// `escaped` false, so the re-opened word did not know it was being rendered
/// verbatim: the unescape could not fire, `backspace_visible_char` went looking
/// for a raw removal that re-renders to "the render minus its last character" —
/// impossible for an escaped word, whose render *is* its raw keys — and the
/// engine flushed instead.
///
/// Asserted through `restore` directly, which is the seam the session uses to
/// re-open a committed word.
#[test]
fn a_restored_escaped_word_can_still_unescape() {
    let mut engine = Engine::new(PlacementStyle::New);
    engine.set_strict_spell_check(true);
    for ch in "hoongfb".chars() {
        engine.process_key(ch);
    }
    // The escape is what puts the raw keys on screen.
    assert_eq!(engine.current_word(), "hoongfb");
    let raw = engine.raw_vec();
    let rendered = engine.current_word().to_string();

    // The boundary commits it, and a later Backspace re-opens it — what the
    // session does through `recompose_after_boundary_backspace`.
    engine.reset();
    engine.restore(raw, rendered);

    // Now the Backspace the user pressed. It must lift the escape, not flush.
    match engine.backspace_visible_char() {
        BackspaceOutcome::Repair(edit) => {
            assert_eq!(edit.insert, "hồng", "the escape must lift to the render");
        }
        other => panic!("expected the escape to lift, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Phonotactic rules the `vi` crate lacks.
//
// `vi::validation::is_valid_syllable` is lenient. Each rule below was added only
// after probing `vi` and watching it accept something Vietnamese cannot spell,
// and each is paired with real words it must never reject — which is the risk
// that matters, since every rule here *rejects* syllables. The corpus at the top
// of this file is the wider guard: these rules feed the same predicate, so a
// rule that over-reaches breaks it.
// ---------------------------------------------------------------------------

/// **`ưa` closed by a coda is impossible.** Reported 2026-09-06 as `wasm`
/// producing `ưám`.
///
/// `ưa` is the *open* form of the diphthong; closed by any coda it must be
/// written `ươ` (`ươm`, `vườn`, `mượn`), never `ưam`. That holds regardless of
/// tone, which is why `violates_stop_coda_tone` never caught `ưát`/`ưáp` — sắc
/// is legal on a stop coda.
///
/// In Telex `wasm` is `w`→ư, `a`, `s`→sắc, `m`: every key applied faithfully,
/// producing a syllable that cannot exist. `vi` called it valid, so auto-fix
/// declined to hand back the raw keys.
#[test]
fn an_open_diphthong_cannot_take_a_coda() {
    for word in [
        "ưam", "ưám", "ưàm", "ưat", "ưát", "ưáp", "ưac", "ưan", "ưang",
        // The tone sits anywhere on the nucleus and changes nothing. Every entry
        // here needs *this* rule: `ừat`/`ửac` were deliberately left out because
        // the older stop-coda tone rule already rejected them (huyền/hỏi on a
        // `t`/`c` coda), so they would have passed with this rule deleted.
        "ứam", "ừam", "ửam", "ữam", "ựat",
    ] {
        assert!(
            is_invalid_vietnamese(word),
            "{word} has ưa closed by a coda, which Vietnamese cannot spell"
        );
    }
}

/// The open form, and the closed form spelled correctly, must both survive.
#[test]
fn the_open_diphthong_itself_is_untouched() {
    for word in [
        // `ưa` word-final: the open form, where it is the only legal shape.
        "ưa",
        "mưa",
        "chứa",
        "giữa",
        "lửa",
        "cửa",
        "vừa",
        "sữa",
        // The same diphthong closed, spelled `ươ` as Vietnamese requires.
        "ươm",
        "vườn",
        "mượn",
        "hương",
        "thường",
        "nước",
        "được",
    ] {
        assert!(
            !is_invalid_vietnamese(word),
            "{word} is real Vietnamese and must not be rejected"
        );
    }
}

/// **`nh` and `ch` close only a front vowel.** Probed: `vi` accepts every one of
/// these.
///
/// The coda takes the vowel immediately before it — a, ă, â, e, ê, i or y. A
/// back or horned vowel there (`o`, `u`, `ư`, `ơ`) is not a Vietnamese rime.
#[test]
fn a_back_vowel_cannot_be_closed_by_nh_or_ch() {
    for word in [
        "ưnh", "ônh", "ơnh", "ơch", "ôch", "ưch", // Tones change nothing here either.
        "ónh", "ùnh", "ọch", "ộnh",
    ] {
        assert!(
            is_invalid_vietnamese(word),
            "{word} closes a back vowel with nh/ch, which is not a Vietnamese rime"
        );
    }
}

/// The all-ASCII spellings of the same impossible rimes are **deliberately**
/// left alone, and that is not this rule leaking.
///
/// `is_invalid_vietnamese` returns early for a pure-ASCII word: it is what the
/// user typed verbatim, so there is nothing to restore it *to*. `onh` and `uch`
/// reach the screen only by being typed literally, and handing them back
/// unchanged is already the right answer. The rules above matter for the
/// non-ASCII renders Telex actually produces.
///
/// Lifting this guard is 1637 phase 2 (ASCII-render restore); until then every
/// rule in this phase sits behind it, and pinning that here keeps the next
/// reader from filing the gap as a bug in the rule.
#[test]
fn the_ascii_guard_still_short_circuits_every_rule() {
    for word in ["onh", "unh", "uch", "och", "uam", "uat"] {
        assert!(
            !is_invalid_vietnamese(word),
            "{word} is pure ASCII — the verbatim guard must answer first"
        );
    }
}

/// Front vowels closed by `nh`/`ch` are ordinary Vietnamese, tones included.
#[test]
fn front_vowels_closed_by_nh_or_ch_are_untouched() {
    for word in [
        "anh", "inh", "ênh", "ach", "ich", "êch", "mách", "sách", "tinh", "xanh", "bênh", "lệch",
        // Toned front vowels: matching raw characters instead of tone-stripped
        // bases would reject all of these.
        "ánh", "ảnh", "ãnh", "ạnh", "ính", "ểnh",
        // A glide before the nucleus does not change which vowel the coda takes.
        "oanh", "uynh", "hoạch", "huênh", "quỳnh",
    ] {
        assert!(
            !is_invalid_vietnamese(word),
            "{word} is real Vietnamese and must not be rejected"
        );
    }
}

/// The `qu-`/`gi-` trap, pinned so a future sibling rule cannot break these.
///
/// `ia` and `ua` follow the same open-diphthong rule as `ưa`, but they are
/// **deferred** (2026-09-06): in `quan`, `quát`, `gian`, `giam` the u/i belongs
/// to the *initial*, not the nucleus, so a surface match rejects real words.
/// `ưa` has no such counterexample — no `qư-` or `gư-` initial exists — which is
/// why it ships alone. `uâ` and `oa` are different nuclei and are never in scope.
#[test]
fn the_deferred_siblings_counterexamples_stay_valid() {
    for word in [
        // qu- and gi- initials, the reason ia/ua are deferred.
        "quan", "quát", "quăn", "quát", "gian", "giam", "giát",
        // `uâ` is not `ua`; `oa` is not `ua`.
        "xuân", "tuân", "thuật", "toàn", "hoàn", // And the plain open forms.
        "của", "chia", "kia", "khuya",
    ] {
        assert!(
            !is_invalid_vietnamese(word),
            "{word} is real Vietnamese and must not be rejected"
        );
    }
}

/// **`ng` and `c` never close `i` or `y`.** Reported 2026-09-10 as `using`
/// producing `uíng`. Probed: `vi` accepts every one of these.
///
/// The other half of the rule above. Those rimes exist, but Vietnamese spells
/// them with the palatal coda — `inh`/`ich`, `uynh`/`uych` — so `ing`, `ic`,
/// `yng` and `yc` are not rimes at all. Sắc is legal on both codas, so the
/// stop-coda tone rule could not reach `uíng`, and the `ui` nucleus itself is
/// ordinary (`túi`, `múi`): only the coda makes it impossible.
#[test]
fn i_and_y_cannot_be_closed_by_ng_or_c() {
    for word in [
        "uíng", "íng", "íc", "ýng", "ýc", // Tones change nothing here either.
        "ìng", "ỉng", "ĩng", "ịc", "uỳng",
    ] {
        assert!(
            is_invalid_vietnamese(word),
            "{word} closes i/y with ng/c, which is not a Vietnamese rime"
        );
    }
}

/// The vowels `ng`/`c` do close are ordinary Vietnamese, and so is `i`/`y` under
/// the palatal coda it actually takes.
#[test]
fn other_vowels_closed_by_ng_or_c_are_untouched() {
    for word in [
        "ứng", "sáng", "xuống", "muốn", "nước", "cúc", "phúc", "vòng", "trắng", "vâng",
        // The second half of a diphthong is what the coda closes, not the `i`.
        "tiếng", "việc", "chiếc", "khiếng", // And the palatal spellings of the rejected rimes.
        "kính", "ích", "uýnh", "quýnh", // A word-final `i`/`y` has no coda at all.
        "túi", "múi", "mý",
    ] {
        assert!(
            !is_invalid_vietnamese(word),
            "{word} is real Vietnamese and must not be rejected"
        );
    }
}

// ---------------------------------------------------------------------------
// The rime inventory.
//
// The three shape rules above — `ưa` closed by a coda, `nh`/`ch` on a back
// vowel, `ng`/`c` on `i`/`y` — are no longer written by hand. Each had been
// added after a user reported the English word it mangled, each was true, and
// none was the general statement, so the next impossible rime always got
// through. `engine.rs`'s `RIMES` is the closed set they were approximating; the
// tests above still pass because the table subsumes them, and the tests below
// are what a table can be asked that a pile of rules cannot.
// ---------------------------------------------------------------------------

/// **Every prefix of a real word must survive the mid-word check.** This is the
/// test the rime table exists to be safe against.
///
/// A membership test over finished rimes would be catastrophic here: `biết`
/// passes through `biế`, and open `iê` is not a legal rime — closed, it needs a
/// coda. The check would refuse the keystroke, escape the word, and `biết` would
/// be untypeable. `chuyển`/`uyê` and `mượn`/`ưo` are the same shape, and `ưo`
/// adds the second half of the problem: Telex delivers the horn on a *later*
/// keystroke than the vowel, so a half-typed rime differs from the finished one
/// in the modifier as well as in length.
///
/// Asserting on/off equality rather than a fixed string is deliberate — the
/// claim is "the check changed nothing", which is exactly what a false rejection
/// would break.
#[test]
fn every_prefix_of_a_real_word_survives_the_mid_word_check() {
    for keys in [
        // The nuclei that are illegal until their coda arrives.
        "bieets", "chuyeenr", "tieengs", "vieejt", "muwown", "dduwowcj", "nguowif", "thuowngr",
        "xuoongs", "khuyeenr", "quyeets", "nghieepj",
        // Modifier-late orders: the horn or circumflex lands after the vowel.
        "tuanwa", "muaw", "tuaan", "chuaanr", "hoongf", "nguyeenx",
        // The rare rimes read out of the wordlist tail.
        "thuowr", "khuyur", "quyst", "huychj", "gieecs", "khuaang", "ngoamj", "tueenhf",
    ] {
        for n in 1..=keys.chars().count() {
            let prefix: String = keys.chars().take(n).collect();
            assert_eq!(
                typed(&prefix, true),
                typed(&prefix, false),
                "the check refused a prefix of {keys}: {prefix}"
            );
        }
    }
}

/// **The rimes `vi` accepts and Vietnamese does not.** Probed: every one of
/// these passes `vi::validation::is_valid_syllable`.
///
/// The first three were reported one at a time and each got its own rule; the
/// rest were still live after all three shipped, which is what moved the check
/// to a table. `vi` cannot express any of them: it validates the nucleus and the
/// coda independently and strips every diacritic before looking, so `i` + `ng`
/// and `ư` + `a` + `m` are each half-true and jointly impossible.
#[test]
fn the_rime_table_rejects_what_vi_accepts() {
    for word in [
        // Reported, and previously each its own rule.
        "ưám", "ưnh", "uíng", // wasm, the nh/ch rule, using
        // Reported later, and still live once those three had shipped.
        "pởe", "ưét", "tưo", "lă", "peón", "buín", // power, west, two, law, person, business
        // The gaps the docs listed as known and unclosed.
        "ớng", "ớc", "ôo", "ơo",
    ] {
        assert!(
            is_invalid_vietnamese(word),
            "{word} is not a Vietnamese rime and must be refused"
        );
    }
}

/// The rare rimes read by hand out of the wordlist tail are real, and typable.
///
/// The table was derived by frequency, and a frequency cut would have dropped
/// every one of these — `giếc` is a fish, `hừm` is a grunt. They are the reason
/// the cut was reviewed rather than trusted, and they are pinned here because
/// nothing else in the suite would notice them going missing.
#[test]
fn the_rare_rimes_kept_from_the_tail_are_typable() {
    for (keys, expected) in [
        ("thuowr", "thuở"),
        ("khuyur", "khuỷu"),
        ("quyst", "quýt"),
        ("khuaang", "khuâng"),
        ("huychj", "huỵch"),
        ("gieecs", "giếc"),
        ("ngoamj", "ngoạm"),
        ("tueenhf", "tuềnh"),
        ("xoengr", "xoẻng"),
        ("huwmf", "hừm"),
    ] {
        assert_eq!(typed(keys, true), expected, "{keys} with the check on");
        assert!(
            !is_invalid_vietnamese(expected),
            "{expected} must also survive the boundary"
        );
    }
}

/// A finished word is judged strictly, and that is what `law` turns on.
///
/// Mid-word the table accepts any prefix, so a bare `ă` passes — `ăng` exists.
/// At the boundary the word is finished and `ă` has to stand on its own, which
/// it cannot: `ă` never appears without a coda. Judging both alike would have
/// left `law` as `lă`, and the two halves of `Word` are what keep them apart.
#[test]
fn a_finished_word_is_judged_more_strictly_than_a_half_typed_one() {
    // Half-typed: the check leaves them alone, because a coda could still come.
    for keys in ["law", "saw", "raw"] {
        assert_eq!(typed(keys, true), typed(keys, false), "{keys} mid-word");
    }
    // Finished: the same rimes are not words.
    for word in ["lă", "să", "ră", "iê", "uyê", "ươ"] {
        assert!(
            is_invalid_vietnamese(word),
            "{word} is not a finished syllable"
        );
    }
    // And the finished spellings that *are* words still pass.
    for word in ["lăng", "sẵn", "riêng", "chuyên", "mượn"] {
        assert!(!is_invalid_vietnamese(word), "{word} is real Vietnamese");
    }
}

/// **A `uâ` word typed horn-first now composes.** This used to be the escape
/// latch swallowing `tuanwa`, and the rime table fixed it without being aimed at
/// it.
///
/// `tuanwa` reaches `tuân` by putting the horn on `u` before the final `a`
/// arrives, so the intermediate render is `tưan`. The old open-diphthong rule
/// refused that — `ưa` closed by a coda — and because the escape is a latch, the
/// `a` that would have repaired the word never applied: the whole word stayed
/// raw. The table judges a half-typed word with its vowel modifiers ignored,
/// because Telex delivers a horn one keystroke late, so `tưan` reads as `uan`
/// and is never refused in the first place.
///
/// The latch itself is untouched — nothing here lifts an escape on a forward
/// key. What changed is that this word no longer trips it. `docs/typing-rules.md`
/// and `plans/260906-2159-.../phase-08-lift-the-escape-latch.md` recorded this
/// as a known consequence; the class it belonged to is narrower now, and the
/// filed item is about the latch rather than about this word.
#[test]
fn a_horn_first_ua_word_composes_with_the_check_on() {
    assert_eq!(typed("tuanwa", true), "tuân", "no longer escapes");
    assert_eq!(
        typed("tuanwa", false),
        "tuân",
        "and matches the check being off"
    );

    // The ordinary spelling is untouched, which is what bounds the blast radius.
    for (keys, expected) in [
        ("tuaan", "tuân"),
        ("xuaan", "xuân"),
        ("chuaanr", "chuẩn"),
        ("luaanj", "luận"),
        ("thuaanf", "thuần"),
    ] {
        assert_eq!(typed(keys, true), expected, "{keys} with the check on");
    }
}

/// Some onsets cannot be followed by the `o` glide, and `vi` accepts them all.
///
/// `b`, `m`, `ph` and `v` are made with the lips and Vietnamese does not put the
/// rounded glide after them; `c` and `k` cannot carry it either, because /k/
/// before the glide is spelled `qu`. The rime table catches none of this — `oa`
/// and `oe` are perfectly ordinary rimes — so only the pairing rule reaches
/// them. It is the `-ore` family of English words that pays for it: `more` →
/// `moẻ`, where `r` is hỏi. Reported 2026-09-11.
#[test]
fn the_onsets_that_cannot_carry_the_o_glide() {
    for word in [
        "moẻ", "moé", "boẻ", "voè", "phoà", "moằn", "moá", "coẻ", "koà",
    ] {
        assert!(
            is_invalid_vietnamese(word),
            "{word} puts the o glide after a labial onset, which Vietnamese does not"
        );
    }
}

/// Every other onset takes the glide, and the listed ones take everything else.
///
/// `boong` and `moóc` are the words at risk from a rule matching `o` too
/// eagerly: their `oo` is a nucleus, not a glide. `muốn` and `vuông` are the
/// same trap one vowel over — `uô` is a nucleus after a labial, and always was.
#[test]
fn the_o_glide_is_untouched_after_every_other_onset() {
    for word in [
        "hoà", "khoẻ", "loà", "ngoè", "toè", "xoà", "choè", "doạ", "goá", "hoạch", "quỳnh", "soạn",
        "noãn", "quà", "quê",
        // The listed onsets with a nucleus that merely starts with `o` or `u`.
        "boong", "moóc", "muốn", "vuông", "mượn", "bưởi", "mèo", "vào", "cuốn", "cõi",
    ] {
        assert!(
            !is_invalid_vietnamese(word),
            "{word} is real Vietnamese and must not be rejected"
        );
    }
}
