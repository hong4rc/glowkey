//! User-interface language.
//!
//! Unikey ships a Vietnamese interface and so should GlowKey — its users are
//! Vietnamese, and an input method is the last place to make someone read a
//! second language. Strings are chosen at the call site by [`t`], which keeps
//! both forms side by side instead of in a key table that drifts out of step
//! with the code. There are few enough strings for that to be the simpler shape.
//!
//! The choice is process-global rather than threaded through every call: the
//! whole interface is one language at a time, and every caller is on the main
//! thread anyway.

use std::sync::atomic::{AtomicBool, Ordering};

use glowkey_engine::Language;
#[cfg(target_os = "macos")]
use objc2_foundation::{NSLocale, NSString};

static VIETNAMESE: AtomicBool = AtomicBool::new(false);

/// Applies the persisted preference, resolving [`Language::System`] against the
/// system's preferred languages.
pub fn set_language(language: Language) {
    let vietnamese = match language {
        Language::Vietnamese => true,
        Language::English => false,
        Language::System => system_prefers_vietnamese(),
    };
    VIETNAMESE.store(vietnamese, Ordering::Relaxed);
}

/// The string for the active interface language.
#[must_use]
pub fn t(english: &'static str, vietnamese: &'static str) -> &'static str {
    if VIETNAMESE.load(Ordering::Relaxed) {
        vietnamese
    } else {
        english
    }
}

/// Whether the system's most-preferred language is Vietnamese.
///
/// Only the *source* of the language tag is per-platform; the rule for reading
/// one is not, and lives in [`tag_is_vietnamese`] so both platforms answer
/// identically and it can be tested anywhere.
#[cfg(target_os = "macos")]
fn system_prefers_vietnamese() -> bool {
    let languages = NSLocale::preferredLanguages();
    let Some(first) = languages.iter().next() else {
        return false;
    };
    let tag: &NSString = &first;
    tag_is_vietnamese(&tag.to_string())
}

/// The Windows answer, from `GetUserPreferredUILanguages`.
///
/// Two calls: the first asks how much room the list needs, the second fills it.
/// The result is a double-NUL-terminated run of NUL-separated tags, most
/// preferred first — only the first is consulted, matching macOS.
#[cfg(target_os = "windows")]
fn system_prefers_vietnamese() -> bool {
    use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};

    let mut count = 0u32;
    let mut len = 0u32;
    // SAFETY: the documented size query — a null buffer with valid out-pointers.
    let ok = unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            std::ptr::null_mut(),
            &mut len,
        )
    };
    if ok == 0 || len == 0 {
        return false;
    }
    let mut buf = vec![0u16; len as usize];
    // SAFETY: `buf` is `len` units, which is what the query above asked for.
    let ok = unsafe {
        GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, buf.as_mut_ptr(), &mut len)
    };
    if ok == 0 {
        return false;
    }
    // The first tag, up to its NUL.
    let first: Vec<u16> = buf.into_iter().take_while(|&u| u != 0).collect();
    tag_is_vietnamese(&String::from_utf16_lossy(&first))
}

/// Whether a BCP-47 language tag names Vietnamese.
///
/// Matches on the language subtag, so `vi`, `vi-VN` and `vi-Hani-VN` all count
/// while `vic` — a different language — does not. Case-insensitive: macOS
/// reports `vi-VN` and Windows `vi-VN` too, but the standard does not require
/// either casing and a user-set tag can be anything.
fn tag_is_vietnamese(tag: &str) -> bool {
    let tag = tag.to_ascii_lowercase();
    tag == "vi" || tag.starts_with("vi-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vietnamese_tags_are_recognised() {
        for tag in ["vi", "vi-VN", "vi-Hani-VN", "VI", "vi-vn"] {
            assert!(tag_is_vietnamese(tag), "{tag} names Vietnamese");
        }
    }

    /// The subtag, not a prefix. `vic` is Virgin Islands Creole and several
    /// other real tags start with "vi"; matching them would put a Vietnamese
    /// interface in front of someone who did not ask for one.
    #[test]
    fn other_languages_are_not() {
        for tag in ["vic", "vie-x", "en", "en-US", "", "v"] {
            assert!(!tag_is_vietnamese(tag), "{tag} does not name Vietnamese");
        }
    }
}
