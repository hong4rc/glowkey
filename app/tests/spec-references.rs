//! The typing spec says which test pins each rule. This checks it is telling
//! the truth.
//!
//! `docs/typing-rules.md` is written so a behaviour change cannot pass quietly:
//! every group of rules names the tests that would fail if the behaviour moved.
//! That only holds while the names are real. A test renamed or deleted would
//! otherwise leave the document pointing at nothing — still reading like a
//! guarantee, no longer being one — and nothing in a green suite would say so.
//!
//! So this test parses the document's "Pinned by" blocks and requires that
//! every test it names exists, in the file it is attributed to. It is a
//! documentation test in the literal sense: it fails on the day someone renames
//! a test without touching the spec, which is exactly when a reader would
//! otherwise be misled.
//!
//! Lives in the top-level crate because it is about the repository rather than
//! any one layer of it.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// The repository root, from this crate's manifest.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the app crate sits one level under the repository root")
        .to_path_buf()
}

/// Every `fn name(` in a file — test functions and their helpers alike, which is
/// the right granularity here: the spec cites what you can pass to `cargo test`.
fn function_names(source: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    for (index, _) in source.match_indices("fn ") {
        // Only a declaration at the start of a line (after indentation), so
        // `impl Fn` bounds and the word inside a comment do not count.
        let before = &source[..index];
        if !before
            .chars()
            .rev()
            .take_while(|c| *c != '\n')
            .all(char::is_whitespace)
        {
            continue;
        }
        let rest = &source[index + 3..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
            .collect();
        if !name.is_empty() && rest[name.len()..].starts_with('(') {
            names.insert(name);
        }
    }
    names
}

/// Which files define which functions, across every integration test in the
/// workspace.
fn test_functions(root: &Path) -> HashMap<String, HashSet<String>> {
    let mut out: HashMap<String, HashSet<String>> = HashMap::new();
    let crates = fs::read_dir(root.join("crates")).expect("the crates directory");
    for entry in crates.flatten() {
        let tests = entry.path().join("tests");
        if !tests.is_dir() {
            continue;
        }
        for file in fs::read_dir(&tests).expect("a tests directory").flatten() {
            let path = file.path();
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            let source = fs::read_to_string(&path).expect("a readable test file");
            // The path as the document writes it, from the repository root.
            let relative = path
                .strip_prefix(root)
                .expect("under the root")
                .to_string_lossy()
                .replace('\\', "/");
            for name in function_names(&source) {
                out.entry(name).or_default().insert(relative.clone());
            }
        }
    }
    out
}

/// The backtick-quoted tokens of a "Pinned by" block, in order: either a path
/// to a test file, or a name that must live in the last path seen.
fn citations(spec: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut current = String::new();
    for line in spec.lines() {
        // Blocks are markdown quotes; anything else resets nothing, since a
        // block's file path can carry across its own wrapped lines only.
        let Some(body) = line.strip_prefix('>') else {
            continue;
        };
        for token in body.split('`').skip(1).step_by(2) {
            if token.ends_with(".rs") {
                current = token.to_string();
            } else if token.len() >= 6
                && token
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                out.push((current.clone(), token.to_string()));
            }
        }
    }
    out
}

#[test]
fn every_test_the_typing_spec_names_exists_where_it_says() {
    let root = repo_root();
    let spec = fs::read_to_string(root.join("docs/typing-rules.md")).expect("the typing spec");
    let defined = test_functions(&root);
    let cited = citations(&spec);

    assert!(
        cited.len() > 60,
        "only {} citations parsed out of the spec — the \"Pinned by\" format has \
         changed and this check is no longer reading it",
        cited.len()
    );

    let mut problems = Vec::new();
    for (file, name) in &cited {
        match defined.get(name) {
            None => problems.push(format!(
                "{name}: named by the spec, defined by no test in the workspace"
            )),
            Some(files) if !files.contains(file) => {
                let mut actual: Vec<&str> = files.iter().map(String::as_str).collect();
                actual.sort_unstable();
                problems.push(format!(
                    "{name}: the spec attributes it to {file}, it lives in {}",
                    actual.join(", ")
                ));
            }
            Some(_) => {}
        }
    }

    assert!(
        problems.is_empty(),
        "docs/typing-rules.md cites tests that do not match the suite. Either the \
         rule moved and the spec needs updating, or a test was renamed and the \
         spec's reference needs following:\n  {}",
        problems.join("\n  ")
    );
}
