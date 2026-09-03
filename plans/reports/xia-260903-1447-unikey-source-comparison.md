# Feature Comparison: UniKey engine → GlowKey

Mode: `--compare`. The argument gave a source and the word "feature" with no mode,
and there is already an implemented parity plan built from memory — so the useful
output is ground truth against that plan, not another implementation plan.

## Source manifest

| | |
|---|---|
| Repository | `github.com/hochanh/unikey-source` (mirror of the UniKey CVS tree) |
| Commit | `e3b8f3b53bdaa700945a62e99129df625509cd38`, 2015-01-25 |
| Scope read | `x-unikey/src/ukengine/` (engine), `x-unikey/src/ukinterface/`, `x-unikey/doc/keymap-syntax` |
| **License** | **GNU LGPL v2** (`x-unikey/COPYING`, per-file headers) |
| Local project | GlowKey, **MIT** (`LICENSE`, both crate manifests) |

Source content was read as data only; nothing from it was executed.

## The licensing finding comes first, because it sets the mode

UniKey's engine is **LGPL v2**; GlowKey is **MIT**. LGPL code cannot be
relicensed as MIT, and `--copy` or `--port`-by-translation of `ukengine.cpp`
would make GlowKey a derivative work. What *is* free to use is the factual
layer: which key performs which action, what a file format looks like, what an
option means. Those are facts and interfaces, not expression.

**Every recommendation below is "reimplement from the observed specification",
never "translate the source".** GlowKey's engine already delegates Vietnamese
transformation to the `vi` crate, so nothing here needs UniKey's algorithm
anyway — only its behavioural contract.

## Head-to-head: the authoritative option set

`struct _UnikeyOptions` (`ukengine/keycons.h:40`) is the whole surface. Nine
fields:

| UniKey option | Meaning | GlowKey |
|---|---|---|
| `freeMarking` | tone key in any order | **has** — order-independent |
| `modernStyle` | `hoà` vs `hòa` | **has** — `PlacementStyle` |
| `macroEnabled` | gõ tắt | **has** |
| `spellCheckEnabled` | reject impossible diacritics *while typing* | **missing** (see below) |
| `autoNonVnRestore` | restore raw keys at word end when not Vietnamese | **has** — auto-fix |
| `useUnicodeClipboard` | clipboard workaround for legacy encodings | omit — correct |
| `useIME` | Win32 only | not applicable |
| `strictSpellCheck` | — | **declared in the struct and never read anywhere in the tree** |
| `alwaysMacro` | — | plumbed through `ukinterface/unikey.cpp:87` into shared memory, **never acted on by the engine** |

Two of the nine are dead. Worth recording so nobody chases them.

Input methods (`keycons.h:37`): `UkTelex`, `UkVni`, `UkViqr`, `UkMsVi`,
`UkUsrIM`, `UkSimpleTelex`. GlowKey has Telex and VNI; VIQR and the Microsoft
layout stay out by standing decision.

## What is genuinely worth taking

### 1. Bracket shortcuts in full Telex — small, real, verified missing

`TelexMethodMapping` (`inputproc.cpp:99`) is Telex plus four entries GlowKey has
no equivalent for:

```
'[' → ơ    ']' → ư    '{' → Ơ    '}' → Ư
```

Verified against our engine: `[`, `]`, `t[`, `d]` all render empty. And the tap
would not deliver them anyway — `is_word_char` accepts only ASCII letters, plus
digits in VNI, so a bracket is treated as a word boundary and flushes the word.

Cost: a mapping in the engine plus widening the tap's word-character test for
Telex. Contained, and it is the kind of shortcut a fast typist actually uses.

### 2. Simple Telex — nearly free once brackets exist

`SimpleTelexMethodMapping` (`inputproc.cpp:119`) differs from full Telex in
exactly two ways: `W` becomes Hook-All rather than the special Telex-W, and the
four bracket entries are absent. A third input-method option, defined entirely
as a diff against one we would already have.

### 3. Spell check is two independent options in UniKey, one in GlowKey

`ukengine.cpp:2280` gates the spell-check path on `spellCheckEnabled`;
`ukengine.cpp:2292` restores raw keys only when `autoNonVnRestore` is also on.
GlowKey's single "Auto-fix" checkbox is `autoNonVnRestore`. The other half —
refusing to place a diacritic that cannot occur in Vietnamese, at the moment it
is typed rather than at the word boundary — GlowKey does not do at all.

This is the most *behaviourally* significant gap on the list, and the most
invasive: it means validating mid-word, not only at commit.

### 4. User-defined input method — the real generalisation

`UkUsrIM` plus `doc/keymap-syntax`: a plain text file of `<key> = <action>`
lines, `;` for comments, against a fixed table of roughly twenty-five named
actions (`Roof-A`, `Hook-UO`, `Bowl`, `Tone1`…`Tone5`, `D-mark`, `Telex-W`, and
literal character actions). Telex and VNI are just two instances of it.

Largest item here, and the only one that would change GlowKey's engine shape.
Not recommended until something concrete asks for it.

## Corrections this run forces on already-shipped work

The parity plan was written from memory this morning and three phases were
implemented against it. Ground truth contradicts it in two places.

### Quick Telex is not a UniKey feature in this source

`quickTelex`, `QuickTelex` and "quick telex" appear **nowhere** in the tree, and
`inputproc.cpp` has no digraph expansion. The plan, the code comments, the
handoff and the Settings caption all attribute Quick Telex to "Unikey's Quick
Telex option". Against this 2015 CVS tree that attribution is unsupported — the
option exists in later UniKey releases and in EVKey, but it is not evidenced
here.

The feature itself is fine: it is implemented, tested, opt-in and off by
default. Only the provenance claim needs softening.

### The macro importer shipped today does not actually read a real UniKey file

`mactab.cpp` is the authority, and the format has three properties our parser
does not know about:

| Property | UniKey (`mactab.cpp`) | Our parser | Consequence |
|---|---|---|---|
| Header line | `DO NOT DELETE THIS LINE*** version=N ***`, written first (`:161`) | not recognised | survives **by accident** — the line has no colon, so it is skipped |
| Byte-order mark | Windows writes `\xEF\xBB\xBF` before the header (`:161`) | not stripped | harmless today; corrupts the first shortcut in any file whose BOM precedes a `key:text` line |
| Body encoding | version ≠ UTF-8 means the body is **VIQR**, e.g. `Vie^.t Nam` (`:196`) | assumes UTF-8 | an older table imports as literal VIQR text, silently |
| Field trimming | none — key is bytes before the first colon, text is everything after (`:314`) | trims both | a leading or trailing space in an imported expansion is silently altered |
| Split rule | first colon only (`strchr`, `:314`) | first colon only | **matches** |

The split rule and the general shape were right. The BOM, the version/VIQR
signal, and the trimming are real defects against a real UniKey export, and all
three are small fixes in `Macro::parse_table`.

## Decision matrix

| Decision | UniKey's way | Our way | Recommendation |
|---|---|---|---|
| Reuse of engine code | LGPL v2 C++ | MIT Rust over the `vi` crate | **Never copy.** Reimplement from behaviour only |
| Transformation algorithm | own state machine | `vi` crate | Keep ours — no reason to import theirs |
| Bracket Telex shortcuts | `[ ] { }` in the key map | absent, and blocked at the tap | **Adopt** — smallest useful win |
| Simple Telex | third method | absent | Adopt with the brackets, or not at all |
| Spell check granularity | two options | one | Adopt the split only if mid-word rejection is actually wanted |
| User-defined key map | `<key> = <action>` file | fixed Telex/VNI | Defer — largest change, no demand yet |
| Macro file compatibility | header + BOM + VIQR versions | UTF-8 lines, trimmed | **Fix** — we claim UniKey compatibility today and do not have it |
| Legacy encodings, VIQR, `UkMsVi`, clipboard | present | omitted | Keep omitting — confirmed correct |

## Risk score

**Low overall**, with one exception.

- Licensing: **medium** if anyone treats this as a porting exercise; **low** under
  the reimplement-only rule above.
- Macro import fixes: low — contained in one engine function with tests already
  around it.
- Brackets and Simple Telex: low — additive, opt-in by method selection.
- Spell-check split: **medium** — mid-word validation touches the hot path that
  produced today's `đddc` and `ưwork` defects.
- User-defined key map: high effort, low urgency.

## Recommendation

Do the macro-format fixes and the attribution correction now: we shipped a
UniKey-compatibility claim today that this reading shows we do not meet, and
both are small. Then the bracket shortcuts, with Simple Telex alongside them if
they land cleanly. Leave the spell-check split and the user-defined key map
alone until something concrete asks for them.

## Unresolved questions

1. Do you actually use `[`/`]` for ơ/ư? It changes whether item 1 is worth the
   tap-level change to word-character handling.
2. Is mid-word rejection of impossible diacritics wanted, or is restoring at the
   word boundary enough? UniKey ships both; GlowKey has the second.
3. Should the macro importer *convert* an old VIQR-encoded table, or refuse it
   with an explanation? Converting means implementing VIQR decoding, which the
   standing decision otherwise excludes.
