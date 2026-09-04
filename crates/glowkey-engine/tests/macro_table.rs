//! The macro table file: EVKey's `shortcut:expansion` lines, so a table curated
//! in Unikey or EVKey can be imported as-is, with a JSON fallback for tables the
//! line format cannot carry.

use glowkey_engine::{Macro, MacroConflict};

fn m(shortcut: &str, expansion: &str) -> Macro {
    Macro {
        shortcut: shortcut.into(),
        expansion: expansion.into(),
    }
}

#[test]
fn reads_the_evkey_line_format() {
    let table = Macro::parse_table("vn:Việt Nam\nhn:Hà Nội\n");
    assert_eq!(table, vec![m("vn", "Việt Nam"), m("hn", "Hà Nội")]);
}

#[test]
fn skips_blank_lines_comments_and_junk_instead_of_failing() {
    // One stray line must not cost the user the other entries.
    let table = Macro::parse_table("\n# a comment\nvn:Việt Nam\nnonsense\n:no shortcut\nhn:\nhn:Hà Nội\n");
    assert_eq!(table, vec![m("vn", "Việt Nam"), m("hn", "Hà Nội")]);
}

#[test]
fn an_expansion_may_contain_a_colon() {
    // Only the first colon separates, so times and ratios survive.
    assert_eq!(
        Macro::parse_table("t:12:30"),
        vec![m("t", "12:30")]
    );
}

#[test]
fn round_trips_through_the_line_format() {
    let table = vec![m("vn", "Việt Nam"), m("t", "12:30")];
    assert_eq!(Macro::parse_table(&Macro::format_table(&table)), table);
}

#[test]
fn falls_back_to_json_when_a_line_cannot_carry_the_macro() {
    // A multi-line expansion has no line-format representation, so the whole
    // table is written as JSON — and still parses back.
    let table = vec![m("addr", "12 Trần Phú\nHà Nội")];
    let text = Macro::format_table(&table);
    assert!(text.trim_start().starts_with('['), "expected JSON, got {text:?}");
    assert_eq!(Macro::parse_table(&text), table);
}

#[test]
fn an_empty_table_is_empty_both_ways() {
    assert_eq!(Macro::format_table(&[]), "");
    assert!(Macro::parse_table("").is_empty());
    assert!(Macro::parse_table("   \n\n").is_empty());
}

#[test]
fn round_trips_macros_the_line_format_would_mangle() {
    // Each of these is altered or dropped by the line reader, so format_table
    // must fall back to JSON rather than lose them: a `#` shortcut reads as a
    // comment, trimming eats a trailing space (ordinary in gõ tắt), and an empty
    // expansion is skipped even though add_macro accepts one.
    for table in [
        vec![m("#tag", "hashtag")],
        vec![m("vn", "Việt Nam ")],
        vec![m("sp", " leading")],
        vec![m("e", "")],
    ] {
        assert_eq!(
            Macro::parse_table(&Macro::format_table(&table)),
            table,
            "round trip lost {table:?}"
        );
    }
}

/// A session holding the two macros a user might already have.
fn session_with_two() -> glowkey_engine::Session {
    let mut s = glowkey_engine::Session::new(
        glowkey_engine::PlacementStyle::New,
        glowkey_engine::ExclusionList::new(),
    );
    s.add_macro("vn", "Việt Nam");
    s.add_macro("hn", "Hà Nội");
    s
}

#[test]
fn import_never_overwrites_an_existing_shortcut() {
    // add_macro is add-or-replace and answers true either way, so a merge built
    // on its return value silently destroyed the user's own macros and reported
    // them as added.
    let mut s = session_with_two();
    let (added, skipped) = s.import_macros(
        &[
            m("vn", "SOMETHING ELSE"),
            m("zz", "new one"),
            m("HN", "case-insensitive collision"),
        ],
        MacroConflict::Skip,
    );
    assert_eq!((added, skipped), (1, 2));
    let kept: Vec<_> = s
        .macros()
        .iter()
        .map(|x| (x.shortcut.as_str(), x.expansion.as_str()))
        .collect();
    assert!(kept.contains(&("vn", "Việt Nam")), "user's macro survives");
    assert!(kept.contains(&("hn", "Hà Nội")), "collision is case-insensitive");
    assert!(kept.contains(&("zz", "new one")), "the new one lands");
    assert_eq!(kept.len(), 3);
}

#[test]
fn import_counts_a_duplicate_inside_the_file_as_skipped() {
    let mut s = session_with_two();
    assert_eq!(
        s.import_macros(&[m("aa", "first"), m("aa", "second")], MacroConflict::Skip),
        (1, 1)
    );
    assert_eq!(
        s.macros().iter().find(|x| x.shortcut == "aa").unwrap().expansion,
        "first"
    );
}

#[test]
fn importing_nothing_changes_nothing() {
    let mut s = session_with_two();
    assert_eq!(s.import_macros(&[], MacroConflict::Skip), (0, 0));
    assert_eq!(s.macros().len(), 2);
}

#[test]
fn reads_a_real_unikey_export() {
    // What UniKey actually writes: a byte-order mark, then a `;`-prefixed header
    // naming the version, then the entries. Previously the header survived only
    // because it happens to contain no colon, and the mark would have corrupted
    // the first shortcut had it preceded an entry instead.
    let file = "\u{feff};DO NOT DELETE THIS LINE*** version=1 ***\nvn:Việt Nam\nhn:Hà Nội\n";
    assert_eq!(
        Macro::parse_table(file),
        vec![m("vn", "Việt Nam"), m("hn", "Hà Nội")]
    );
    assert!(!Macro::table_is_legacy_viqr(file));
}

#[test]
fn flags_an_old_viqr_encoded_unikey_table() {
    // Any version but 1 means the body is VIQR, not UTF-8, so importing it would
    // store `Vie^.t Nam` as literal text.
    let old = ";DO NOT DELETE THIS LINE*** version=0 ***\nvn:Vie^.t Nam\n";
    assert!(Macro::table_is_legacy_viqr(old));
    // An EVKey table has no header at all and is UTF-8 — it must not be caught.
    assert!(!Macro::table_is_legacy_viqr("vn:Việt Nam\n"));
    assert!(!Macro::table_is_legacy_viqr(""));
}

#[test]
fn an_expansions_own_spacing_is_preserved() {
    // UniKey does not trim, and a trailing space is ordinary in gõ tắt. The
    // shortcut is trimmed regardless: it is matched against typed keys, which
    // cannot contain a space.
    assert_eq!(
        Macro::parse_table(" vn :Việt Nam \n"),
        vec![m("vn", "Việt Nam ")]
    );
}

#[test]
fn macros_can_run_with_vietnamese_switched_off() {
    // UniKey's alwaysMacro. Off by default, and never in an excluded app.
    let mut s = session_with_two();
    s.set_frontmost_app("com.apple.TextEdit");
    s.toggle_mode(); // Vietnamese -> English
    assert!(!s.is_active());
    assert!(!s.macros_active(), "off by default");

    s.set_always_macro(true);
    assert!(s.macros_active());

    // Typing the shortcut still composes, verbatim, and the boundary expands it.
    for ch in "vn".chars() {
        s.process_key(ch);
    }
    let expansion = s.commit().expect("the macro should fire");
    assert_eq!(expansion.insert, "Việt Nam");
    assert_eq!(expansion.backspaces, 2, "the two typed letters come back off");
}

#[test]
fn always_macro_stays_out_of_excluded_apps() {
    // Excluded means hands off; a terminal silently expanding a shortcut would be
    // worse than the bug exclusions exist to prevent.
    let mut s = glowkey_engine::Session::new(
        glowkey_engine::PlacementStyle::New,
        glowkey_engine::ExclusionList::with_defaults(),
    );
    s.add_macro("vn", "Việt Nam");
    s.set_always_macro(true);
    s.toggle_mode();
    s.set_frontmost_app("com.apple.Terminal");
    assert!(!s.macros_active(), "Terminal is a shipped default exclusion");
    // The same session in an ordinary app does expand.
    s.set_frontmost_app("com.apple.TextEdit");
    assert!(s.macros_active());
}

#[test]
fn always_macro_does_nothing_without_macros() {
    let mut s = glowkey_engine::Session::new(
        glowkey_engine::PlacementStyle::New,
        glowkey_engine::ExclusionList::new(),
    );
    s.set_always_macro(true);
    s.toggle_mode();
    s.set_frontmost_app("com.apple.TextEdit");
    assert!(!s.macros_active(), "no macros means the path stays off");
}

/// Importing with `Replace` overwrites, which is the other half of asking the
/// user: the question is only honest if both answers are available.
#[test]
fn import_can_replace_when_the_user_asks_for_it() {
    let mut s = session_with_two();
    let (added, skipped) = s.import_macros(
        &[
            m("vn", "SOMETHING ELSE"),
            m("zz", "new one"),
            m("HN", "case-insensitive collision"),
        ],
        MacroConflict::Replace,
    );
    assert_eq!((added, skipped), (3, 0), "every row lands");
    let kept: Vec<_> = s
        .macros()
        .iter()
        .map(|x| (x.shortcut.as_str(), x.expansion.as_str()))
        .collect();
    assert!(kept.contains(&("vn", "SOMETHING ELSE")), "replaced");
    assert!(
        kept.contains(&("HN", "case-insensitive collision")),
        "replaced across a case difference"
    );
    assert!(kept.contains(&("zz", "new one")));
    assert_eq!(kept.len(), 3, "replacing must not also duplicate");
}

/// A repeat *inside the file* is still one macro, whichever answer was given.
/// Otherwise "Replace" would let a malformed file inflate the added count.
#[test]
fn replacing_still_collapses_a_duplicate_inside_the_file() {
    let mut s = session_with_two();
    assert_eq!(
        s.import_macros(
            &[m("aa", "first"), m("aa", "second")],
            MacroConflict::Replace
        ),
        (1, 1)
    );
    assert_eq!(
        s.macros()
            .iter()
            .find(|x| x.shortcut == "aa")
            .unwrap()
            .expansion,
        "first"
    );
}

/// The count the window shows before it asks. Counted without writing anything,
/// so the question can be put once for the whole file.
#[test]
fn conflicts_are_counted_before_anything_is_written() {
    let s = session_with_two();
    let imported = [
        m("vn", "collides"),
        m("HN", "collides, case-insensitively"),
        m("zz", "new"),
        m("zz", "repeated inside the file"),
        m("vn", "the same collision, listed twice"),
        m("  ", "blank shortcuts are rejected, not conflicting"),
    ];
    // Two: `vn` and `hn`. A repeat inside the file is not a conflict — it
    // collapses whatever the user answers — and `vn` listed twice is still one
    // decision, because a person reads this number.
    assert_eq!(s.macro_conflicts(&imported), 2);
    assert_eq!(s.macros().len(), 2, "counting must not modify the table");
}

/// What the Add button asks before it overwrites.
#[test]
fn an_existing_shortcut_is_reported_case_insensitively() {
    let s = session_with_two();
    assert!(s.has_macro("vn"));
    assert!(s.has_macro("VN"), "matching is case-insensitive");
    assert!(s.has_macro("  vn  "), "the field's whitespace is not a shortcut");
    assert!(!s.has_macro("zz"));
}
