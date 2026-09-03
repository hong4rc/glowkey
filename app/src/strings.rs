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

/// Whether the system's most-preferred language is Vietnamese. Matches on the
/// language subtag, so `vi`, `vi-VN` and `vi-Hani-VN` all count.
fn system_prefers_vietnamese() -> bool {
    let languages = NSLocale::preferredLanguages();
    let Some(first) = languages.iter().next() else {
        return false;
    };
    let tag: &NSString = &first;
    let tag = tag.to_string();
    tag == "vi" || tag.starts_with("vi-")
}
