//! Per-word user decisions between raw keys and Vietnamese (leaves this crate in a later phase).

use super::*;

/// Which reading of one set of typed keys the user wants.
///
/// The English/Telex ambiguity is not resolvable by rule — the same keystrokes
/// are legitimate Vietnamese and legitimate English (`docs/handoff.md` §6.3), so
/// `cats` is both `cats` and `cát` and no amount of cleverness decides which.
/// This is the user answering, one word at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WordPreference {
    /// Keep the keys as typed: `cats` stays `cats`.
    ///
    /// The aliases exist because this list is meant to be hand-editable, and
    /// `"raw"` is what a person writes.
    #[default]
    #[serde(alias = "raw", alias = "typed")]
    Raw,
    /// Keep the Vietnamese rendering: `cats` becomes `cát`.
    #[serde(alias = "vietnamese", alias = "vi")]
    Vietnamese,
}

/// One word the user has decided about.
///
/// Keyed on the **raw keys**, lowercased, because the raw keys are what the
/// ambiguity is about: one key sequence, two readings. `cats` is the question;
/// `cats` and `cát` are the two answers. Lowercased to match
/// `english::is_common_english`, so a capitalised word at a sentence start obeys
/// the same decision as the same word mid-sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordOverride {
    /// The typed keys, lowercase.
    #[serde(default)]
    pub keys: String,
    /// Which reading to keep. An unrecognised or missing value reads as "keep
    /// what was typed", the safe answer.
    ///
    /// Lenient on purpose, because hand-editing this list is a documented
    /// workflow and the file is parsed with `unwrap_or_default`: one mistyped
    /// verdict used to discard **every** setting in it — exclusions, macros,
    /// hotkey, the lot — and the next change from the UI then wrote the defaults
    /// back over them. Losing a curated exclusion list to a typo in an unrelated
    /// field is not a trade anyone would accept, so this field refuses to be the
    /// thing that fails the document.
    #[serde(default, deserialize_with = "lenient_preference")]
    pub prefer: WordPreference,
}

/// Reads a word preference, treating anything unrecognised as the default.
///
/// See [`WordOverride::prefer`] for why this cannot be allowed to fail.
pub(crate) fn lenient_preference<'de, D>(deserializer: D) -> Result<WordPreference, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(deserializer).unwrap_or_default();
    Ok(match raw.to_ascii_lowercase().as_str() {
        "vietnamese" | "vi" => WordPreference::Vietnamese,
        _ => WordPreference::Raw,
    })
}
