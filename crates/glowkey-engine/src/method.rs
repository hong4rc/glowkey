//! Input methods and tone placement: the strategy the engine composes with.

use super::*;

/// Tone-mark placement convention.
///
/// New style is the modern default (`hoà`, `thuý`); old style is the traditional
/// convention (`hòa`, `thúy`). Mirrors [`AccentStyle`] but keeps `vi` out of the
/// shell's type surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum PlacementStyle {
    /// Modern orthography — the software default.
    #[default]
    New,
    /// Traditional orthography.
    Old,
}

impl InputMethod {
    /// Whether this is one of the Telex variants. Quick Telex applies to both,
    /// since its digraphs are plain letters; the bracket shortcuts do not,
    /// because UniKey's Simple Telex mapping deliberately drops them.
    #[must_use]
    pub fn is_telex_family(self) -> bool {
        matches!(self, Self::Telex | Self::SimpleTelex)
    }
}

impl From<PlacementStyle> for AccentStyle {
    fn from(style: PlacementStyle) -> Self {
        match style {
            PlacementStyle::New => AccentStyle::New,
            PlacementStyle::Old => AccentStyle::Old,
        }
    }
}

/// The keyboard input method for Vietnamese, as in Unikey/EVKey. Telex uses letter
/// keys (`aa`→â, `f`→huyền); VNI uses digits (`a6`→â, `2`→huyền).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum InputMethod {
    /// Telex — the software default.
    #[default]
    Telex,
    /// VNI — tone and diacritic digits.
    Vni,
    /// Simple Telex — UniKey's `UkSimpleTelex`. Telex with one difference: `w`
    /// only ever adds a horn or a breve to a vowel already typed, so it never
    /// stands alone as `ư`.
    SimpleTelex,
}

/// UniKey's Simple Telex (`SimpleTelexMethodMapping`, `inputproc.cpp:119`).
///
/// Identical to Telex but for `w`, which UniKey maps to Hook-All rather than to
/// its special Telex-W: it adds a horn to `u`/`o` or a breve to `a`, and does
/// nothing on its own. Full Telex additionally lets a bare `w` stand for `ư`,
/// which is the behaviour people either rely on or trip over — hence the
/// separate method.
///
/// Spelled out as its own definition rather than patched at the key level: `vi`
/// takes a whole `Definition`, and copying ten unchanged entries is clearer than
/// intercepting one key on the way past.
pub(crate) static SIMPLE_TELEX: vi::methods::Definition = phf::phf_map! {
    's' => &[Action::AddTonemark(ToneMark::Acute)],
    'f' => &[Action::AddTonemark(ToneMark::Grave)],
    'r' => &[Action::AddTonemark(ToneMark::HookAbove)],
    'x' => &[Action::AddTonemark(ToneMark::Tilde)],
    'j' => &[Action::AddTonemark(ToneMark::Underdot)],
    'a' => &[Action::ModifyLetterOnCharacterFamily(LetterModification::Circumflex, 'a')],
    'e' => &[Action::ModifyLetterOnCharacterFamily(LetterModification::Circumflex, 'e')],
    'o' => &[Action::ModifyLetterOnCharacterFamily(LetterModification::Circumflex, 'o')],
    'w' => &[Action::ModifyLetter(LetterModification::Horn), Action::ModifyLetter(LetterModification::Breve)],
    'd' => &[Action::ModifyLetter(LetterModification::Dyet)],
    'z' => &[Action::RemoveToneMark],
};
