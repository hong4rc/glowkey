# Feature Comparison: UniKey engine behaviours → GlowKey (second pass)

Mode: `--compare`, then one finding implemented because it was a live defect.

First pass (`xia-260903-1447-unikey-source-comparison.md`) read the **options**:
`keycons.h`, the input-method key maps, `mactab.cpp`, the keymap syntax. This
pass reads `ukengine.cpp` itself — the *behaviours*, which are not exposed as
options and so were invisible to the first pass.

## Source manifest

| | |
|---|---|
| Repository | `github.com/hochanh/unikey-source`, commit `e3b8f3b` |
| Scope read | `x-unikey/src/ukengine/ukengine.cpp` (2380 lines), method by method |
| Local project | GlowKey, engine delegates transformation to the `vi` crate |

## The finding worth having

**UniKey enforces Vietnamese's stop-coda tone rule; the `vi` crate does not.**

`lastWordIsNonVn` (`ukengine.cpp:2352`):

```c
if ((cs == cs_c || cs == cs_ch || cs == cs_p || cs == cs_t) &&
    (tone == 2 || tone == 3 || tone == 4))
{
    return true;   // not Vietnamese
}
```

A syllable closed by `c`, `ch`, `p` or `t` can only carry sắc or nặng. Huyền,
hỏi and ngã are phonologically impossible there. Probed against `vi`:

| syllable | `vi::validation::is_valid_syllable` | actually Vietnamese |
|---|---|---|
| `các`, `học`, `sách`, `hợp`, `một` | true | yes |
| `màc`, `hỏc`, `mãt`, `hòp`, `cảt` | **true** | **no** |

**Why it mattered in daily use.** Telex spells huyền, hỏi and ngã as `f`, `r`
and `x` — three letters that appear constantly in English before a final `t`.
Measured on the engine before the fix:

| typed | rendered | auto-fix rescued it? |
|---|---|---|
| `left` | `lèt` | no |
| `soft` | `sòt` | no |
| `gift` | `gìt` | no |
| `lift` | `lìt` | no |
| `loft` | `lòt` | no |

Auto-fix stood aside because its predicate had been told those were valid
Vietnamese. The words are not rare and the failure is silent.

**Fixed.** `violates_stop_coda_tone` now backs both auto-fix and the mid-word
spell check — they share one predicate, so the rule reaches both. Tests pin the
five English words and, on the other side, that `việt`, `học`, `đất`, `nước`,
`đẹp`, `sách` and `quyết` are untouched: sắc and nặng on a stop coda are legal
and must stay.

## Behaviours examined and deliberately not taken

| UniKey behaviour | Where | Verdict |
|---|---|---|
| `m_singleMode` — a word that has escaped spell-checking renders raw for the rest of the word | set in `processDd`, read at `:2280` | **Already have it**, arrived at independently: our `escaped` flag is the same mechanism. Convergent design, nothing to port. |
| `restoreKeyStrokes` — put the raw keystrokes back | `:2183` | Only ever called from `processWordEnd`, so it is UniKey's auto-restore, not a user-facing escape key. We have the equivalent. |
| `checkEscapeVIQR` | `:1219` | VIQR only; VIQR stays out. |
| `getTonePosition(vs, terminated)` — placement depends on whether the syllable is finished | `:936` | `vi` owns placement and its modern/classic split already matches. No evidence of a gap. |
| `appendVowel` / `appendConsonnant` — UniKey's own spell tables | `:1347`, `:1505` | Reimplementing Vietnamese phonotactics wholesale is not worth it when `vi` covers all but the one rule above. Take rules, not the table. |
| `lastWordHasVnMark` | `:2366` | Supports UniKey's restore decisions; ours are structured differently and do not need it. |

## Recommendation

The stop-coda rule was the one live defect in this pass and is fixed. The rest of
`ukengine.cpp` is either already matched or deliberately out of scope.

The broader lesson is worth recording: `vi::validation::is_valid_syllable` is
**lenient**, and everything built on it inherits that. It accepts incomplete
prefixes (which is what makes mid-word checking viable at all) and it accepts
phonologically impossible codas. Any future feature that leans on it should be
probed against real words first rather than trusted.

## Unresolved questions

1. Are there other phonotactic rules `vi` misses? Only the stop-coda one was
   traced here because `lastWordIsNonVn` is short. `isValidCVC` (the table it
   calls) was not audited, and it may encode further constraints.
2. Should the stop-coda rule also apply under VNI? It does today — the predicate
   is method-agnostic, which is correct phonologically, but VNI users reach those
   tones by digit and so are far less likely to hit it by accident.
