# GlowKey typing rules

What GlowKey does to your keystrokes, as rules with examples. One table per
group. `⌫` is Backspace, `␣` is space.

This is the behaviour spec — the *what*. For the *why* and the code that owns
each rule, see `handoff.md`.

Defaults are marked **on** or **off**. Everything off is opt-in in Settings.

---

## 1. Typing a Vietnamese word (Telex, default)

| You type | You get | Rule |
| --- | --- | --- |
| `oo` | `ô` | A doubled vowel adds the circumflex. Same for `aa`→`â`, `ee`→`ê` |
| `aw` | `ă` | `w` adds the breve to `a`, the horn to `u`/`o` |
| `w` | `ư` | `w` alone is `ư` |
| `dd` | `đ` | |
| `cas` | `cá` | The five tone keys: `s` sắc, `f` huyền, `r` hỏi, `x` ngã, `j` nặng |
| `casz` | `ca` | `z` removes the tone. With no tone to remove it stays a letter: `caz`→`caz` |

**The tone key goes anywhere in the word.** All of these give `hồng`:

| `hoongf` | `hofong` | `hoonfg` | `hofngo` |
| --- | --- | --- | --- |

Each keystroke re-derives the whole word, so there is no "too late" to add a
tone or a diacritic. More examples: `nguyeenx`→`nguyễn`, `dduwowcj`→`được`,
`nguoiwf`→`người`, `quar`→`quả`, `khuyru`→`khuỷu`.

**Case is kept**: `Hoongf`→`Hồng`, `NGUYEENX`→`NGUYỄN`. The tone and diacritic
keys do not count towards it, so you can let go of Shift for them —
`HOONGf`→`HỒNG`, and `O` `A` `f`→`OÀ`.

---

## 2. Cancelling a transformation

Press the key again and you get the plain letters. This is the rejection
gesture — how you type a word that only looks like Telex.

| You type | You get | |
| --- | --- | --- |
| `oo` | `ô` | the transformation |
| `ooo` | `oo` | the third `o` cancels it, both letters stay literal |
| `aaa` | `aa` | |
| `ddd` | `dd` | |
| `cass` | `cas` | a repeated tone key drops the tone, the extra key stays |
| `hoongff` | `hôngf` | |
| `oooo` | `ooo` | after the cancel, each further press is one more letter |

**`oo` is a real Vietnamese sequence**, which is why the cancel exists — and it
takes tones, so a tone key typed after the cancel still reaches the vowel:

| You type | You get | |
| --- | --- | --- |
| `xooong` | `xoong` | a saucepan |
| `mooosc` | `moóc` | *xe moóc*, a trailer |
| `sooocs` | `soóc` | *quần soóc*, shorts |

**Backspace after a cancel undoes the cancel.** The delete takes back the
keystroke that rejected the diacritic, not just a character:

| You type | You get |
| --- | --- |
| `ooo`⌫ | `ô` |
| `ooo`⌫`f` | `ồ` — still composing, so the tone lands on the vowel |

---

## 3. Backspace while a word is open

The rule is **deletes remove a visible character**, not a keystroke. `hồng` is
four characters but six keystrokes; Backspace follows the screen.

| You type | You get | |
| --- | --- | --- |
| `hoongf`⌫ | `hồn` | the `g` goes, the tone stays |
| `hoongf`⌫`z` | `hôn` | still composing, so `z` still removes the tone |
| `hoongf` `a` ⌫ ⌫ `z` | `hôn` | asked about twice in live use; kept both times |
| `hoongf` `s` ⌫ ⌫ `z` | `hô` | `hống` is 4 characters, 7 keystrokes |

The two readings only ever disagree at a **tone key**, because that is the one
keystroke that produces no character of its own.

**When the engine cannot follow the screen it stops composing** and the delete
is an ordinary one: `viêt`⌫⌫ leaves `vi` as plain text. From there the next key
starts a fresh word.

---

## 4. Word boundaries and re-opening a word

A space, punctuation, or (in Telex) a digit ends the word.

| You type | You get | Rule |
| --- | --- | --- |
| `xin chaof` | `xin chào` | each word composes on its own |
| `hoongf4` | `hồng4` | a digit ends the word in Telex |
| `hồng`␣⌫`z` | `hông` | deleting the boundary re-opens the word |
| `hồng`␣`s`⌫⌫`z` | `hông` | it survives typing in between |
| `hồng,`␣⌫⌫`z` | `hông` | and a second boundary character |

GlowKey remembers the **last five** boundaries, so deleting back through
several words re-opens whichever one you land in. The memory is dropped
whenever the caret can move without GlowKey seeing it: arrow keys, Home/End,
Page keys, a mouse click, a shortcut, an app switch, or changing any setting
that affects rendering.

---

## 5. When Vietnamese gets out of the way

| Case | Behaviour |
| --- | --- |
| English words | Pass through untouched: `hello`, `caption`, `iPhone`, `JavaScript`, `macOS`, `PhD`, `GlowKey` |
| **Auto-fix** (**on**) | At a boundary, a word that is not valid Vietnamese is handed back as typed: `exit` stays `exit`, not `eĩt` |
| A word starting with `đ` | Never auto-fixed. `đc`, `đt`, `đk` survive; `address` and `odd` still restore, their `đ` is not leading |
| Excluded apps | Everything passes through. Terminals ship excluded |
| Vietnamese off | Plain passthrough. `Ctrl+Shift+Space` by default, or `Ctrl+Space` / `Ctrl+Shift+Z` |

**Why `exit` needs rescuing:** `x` is the ngã key, so left alone Telex reads it
as Vietnamese. Auto-fix is what makes English typing survive without switching
mode.

---

## 6. Rules the syllable validator lacks

Auto-fix asks "is this valid Vietnamese?". Three answers had to be added by
hand, because the library says yes to spellings Vietnamese cannot produce.

| Rule | Rejects | Why it matters |
| --- | --- | --- |
| A syllable closed by `c`, `ch`, `p`, `t` takes only sắc or nặng | `màc`, `hỏc`, `mãt`, `hòp` | `f`, `r`, `x` are exactly those three tones, so `left` came out `lèt`, `soft` `sòt`, `gift` `gìt` |
| The open diphthong `ưa` takes no coda — closed it is `ươ` | `ưam`, `ưát`, `ưáp` | `wasm` came out `ưám`, `wast`→`ưát`, `wasp`→`ưáp` |
| `nh` and `ch` close only a front vowel (a, ă, â, e, ê, i, y) | `ưnh`, `ônh`, `ơch` | Real words are unaffected: `oanh`, `uynh`, `hoạch`, `huênh` |

The `ia`/`ua` siblings of the second rule are **deliberately left out**: in
`quan`, `quát`, `gian`, `giam` the u/i belongs to the initial, not the vowel, so
the rule would reject real words. `ưa` needs no exception — there is no `qư-` or
`gư-` initial.

---

## 7. Options

### Input method

| Option | Rule | Example |
| --- | --- | --- |
| **Telex** (default) | as above | `hoongf`→`hồng` |
| **VNI** | digits carry the marks, and extend the word | `a6`→`â`, `o7`→`ơ`, `d9`→`đ`, `viet65`→`việt` |
| **Simple Telex** | Telex, except `w` never stands alone as `ư` | `w`→`w`, but `uw`→`ư` still works |

### Rendering

| Option | Rule | Example |
| --- | --- | --- |
| Tone placement (**new**) | modern convention | `hoaf`→`hoà`, `thuys`→`thuý` |
| Tone placement (old) | traditional convention | `hòa`, `thúy` |
| Auto-capitalize (off) | capitalizes the first letter of a sentence | |

### Typing shortcuts

| Option | Rule | Example |
| --- | --- | --- |
| **Quick Telex** (off) | a doubled consonant at the **start** of a syllable is its digraph | `ccao`→`chao`, `nnuowif`→`người`, `uu`→`ư` |
| **Telex brackets** (off) | `[`→ơ, `]`→ư, `{`→Ơ, `}`→Ư, and a tone after still lands | `[f`→`ờ`. Known limit: after a vowel the substitution leaks — `an[`→`anow` |
| **Macros** (off until you add one) | a shortcut expands at the boundary | `vn␣`→`Việt Nam␣` |
| Macros with Vietnamese off (off) | keys still accumulate so a macro can match, but nothing transforms. Never in an excluded app | |

Quick Telex applies to both Telex variants; the brackets are Telex-only.
Turning the brackets on stops `[` and `]` reaching the app at all, which is why
they are off by default.

### Corrections

| Option | Rule | Example |
| --- | --- | --- |
| **Auto-fix** (**on**) | see §5 | `exit`→`exit` |
| Restore common English words (off) | a committed word whose keys spell a common English word is handed back even when the Vietnamese is valid | `was`→`was`, not `ứa`. The cost: `cats`→`cats`, not `cát` |
| **Mid-word spell check** (off) | repairs at the keystroke instead of at the space: the moment a word becomes unspellable it shows your raw keys for the rest of the word | `exit` is fixed at the `x` |
| **Personal words** | one word pinned to English or Vietnamese beats every rule above, in both directions | `was`→`was` and `cats`→`cát` at the same time |

**`⌃⇧W` swaps the word you just typed** and remembers the choice as a personal
word. It is the answer to any word the rules get wrong.

**The mid-word spell check and Backspace:** while a word is escaped it shows
your keys, and deleting the key that broke it brings Vietnamese back —
`hoongf`→`hồng`, a mistyped `a` shows `hoongfa`, and ⌫ gives `hồng` again, still
composing. The repeat-key gesture of §2 stands aside from the check.

---

## 8. Fixed hotkeys

| Key | Does |
| --- | --- |
| `Ctrl+Shift+E` | Vietnamese on/off for the current app, and remembers it. In a terminal, only until restart |
| `Ctrl+Shift+W` | Swap the last word between English and Vietnamese, and remember it |

Both are fixed and cannot be recorded as the VN/EN toggle.

**On macOS these are written `⌃⇧E` and `⌃⇧W`**, and the app spells every hotkey
the way its platform does — `⌃⇧Space` there, `Ctrl+Shift+Space` here. Two of the
VN/EN choices are macOS-only: `⌥Space`, which Windows does not offer, and the
"Custom…" recorder that captures a combination you press.

---

## 9. Clipboard tools

Menu → remove tones, UPPERCASE, lowercase. They act on the **clipboard**, not a
selection — a background agent has no selection of its own. Non-text clipboards
are left alone. `café` is stripped to `cafe` too: nothing here knows the word is
French.
