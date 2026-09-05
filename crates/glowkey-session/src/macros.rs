//! Text-expansion macros and the UniKey table format.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A text-expansion macro (Unikey's "gõ tắt"): typing `shortcut` then a boundary
/// replaces it with `expansion`. E.g. `vn` → `Việt Nam`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Macro {
    /// The typed keys that trigger the expansion (matched case-insensitively).
    pub shortcut: String,
    /// The text inserted in place of the shortcut.
    pub expansion: String,
}

/// The longest expansion a macro may carry, in characters.
///
/// Gõ tắt expands an abbreviation into a phrase; the longest real entries in
/// shipped UniKey and EVKey tables are a sentence. The cap exists because the
/// expansion is inserted a character at a time on the keystroke path and an
/// imported table is the one file GlowKey reads that it did not write — a
/// downloaded table with a megabyte on one line would otherwise be accepted and
/// then typed.
pub const MAX_EXPANSION_CHARS: usize = 4096;

/// Whether a shortcut and expansion may be stored as a macro.
///
/// One rule, used by every path that can introduce one — typed into the Macros
/// window, parsed from a table, or merged by an import — because they had drifted
/// apart and disagreed about the same table.
///
/// Rejects:
/// - an empty shortcut, which nothing could ever type;
/// - a shortcut containing whitespace: it is matched against the keys typed
///   before a boundary, and a boundary is where matching stops;
/// - **control characters** in either field. This is the untrusted-input rule.
///   An expansion is inserted as text, never as key codes, so a control
///   character cannot press Enter or Ctrl — but a carriage return or a NUL in
///   the middle of one still corrupts the list on screen and the table on the
///   way back out, and no real gõ tắt entry contains one;
/// - an expansion longer than [`MAX_EXPANSION_CHARS`].
///
/// An **empty expansion** is *not* rejected here. The line format cannot carry
/// one and skips it, while the JSON fallback round-trips it deliberately; which
/// of those is right is a product question, and answering it silently in a
/// validator would change what an existing settings file means.
#[must_use]
pub fn is_storable(shortcut: &str, expansion: &str) -> bool {
    !shortcut.is_empty()
        && !shortcut.chars().any(char::is_whitespace)
        && !shortcut.chars().any(char::is_control)
        && !expansion.chars().any(char::is_control)
        && expansion.chars().count() <= MAX_EXPANSION_CHARS
}

/// What an import should do when a shortcut it carries already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroConflict {
    /// Leave the existing expansion alone and count the row as skipped.
    Skip,
    /// Overwrite the existing expansion.
    Replace,
}

/// The `version` a UniKey macro-table header declares for a UTF-8 body. Anything
/// else means the body is VIQR (`UKMACRO_VERSION_UTF8` in UniKey's `mactab.cpp`).
pub(crate) const UNIKEY_MACRO_VERSION_UTF8: i32 = 1;

/// Whether a line is UniKey's macro-table header.
pub(crate) fn is_unikey_header(line: &str) -> bool {
    unikey_header_version(line).is_some()
}

/// The version declared by a UniKey macro-table header line, if it is one.
/// The header is written as `;DO NOT DELETE THIS LINE*** version=1 ***`, with
/// the leading `;` only on Windows.
pub(crate) fn unikey_header_version(line: &str) -> Option<i32> {
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
            // file that is plainly not in that format. Without `serde` there is
            // no JSON reader, and a JSON table is likewise nothing.
            #[cfg(feature = "serde")]
            return serde_json::from_str(trimmed).unwrap_or_default();
            #[cfg(not(feature = "serde"))]
            return Vec::new();
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
                (!expansion.is_empty() && is_storable(shortcut, expansion)).then(|| Self {
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
        #[cfg(feature = "serde")]
        if !line_safe {
            return serde_json::to_string_pretty(macros).unwrap_or_default();
        }
        // Without `serde` there is no JSON writer; the line format is the best
        // that can be done, and the caller opted out of the lossless path.
        #[cfg(not(feature = "serde"))]
        let _ = line_safe;
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
