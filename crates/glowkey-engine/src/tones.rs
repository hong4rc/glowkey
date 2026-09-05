//! Removing tone marks from text.

/// Strips Vietnamese diacritics from text, leaving plain ASCII letters —
/// UniKey's "bỏ dấu" tool (`m_removeTone`).
///
/// `đ`/`Đ` become `d`/`D`; every toned or modified vowel falls back to its base
/// letter. Everything else, including text that was never Vietnamese, passes
/// through untouched. Useful for filenames, URLs and search boxes.
#[must_use]
pub fn remove_tones(text: &str) -> String {
    /// Base letter for each Vietnamese vowel form, lowercase. Uppercase is
    /// handled by casing the result, so only one table is needed.
    const BASES: [(&str, char); 12] = [
        ("aàáảãạăằắẳẵặâầấẩẫậ", 'a'),
        ("eèéẻẽẹêềếểễệ", 'e'),
        ("iìíỉĩị", 'i'),
        ("oòóỏõọôồốổỗộơờớởỡợ", 'o'),
        ("uùúủũụưừứửữự", 'u'),
        ("yỳýỷỹỵ", 'y'),
        ("dđ", 'd'),
        ("AÀÁẢÃẠĂẰẮẲẴẶÂẦẤẨẪẬ", 'A'),
        ("EÈÉẺẼẸÊỀẾỂỄỆ", 'E'),
        ("IÌÍỈĨỊ", 'I'),
        ("OÒÓỎÕỌÔỒỐỔỖỘƠỜỚỞỠỢ", 'O'),
        ("UÙÚỦŨỤƯỪỨỬỮỰ", 'U'),
    ];

    text.chars()
        .map(|ch| {
            if ch.is_ascii() {
                return ch;
            }
            for (forms, base) in BASES {
                if forms.contains(ch) {
                    return base;
                }
            }
            // The two remaining uppercase families, kept out of the table so the
            // lines stay readable.
            match ch {
                'Ỳ' | 'Ý' | 'Ỷ' | 'Ỹ' | 'Ỵ' => 'Y',
                'Đ' => 'D',
                other => other,
            }
        })
        .collect()
}
