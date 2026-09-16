# GlowKey typing rules

What GlowKey does to your keystrokes, as rules with examples. One table per
group. `⌫` is Backspace, `␣` is space.

**Every group names the tests that pin it.** That is the point of the layout: a
rule here is not a description of what the code happens to do, it is a claim
some test will fail over. Change a rule and the named test goes red; add a rule
and it belongs in a table with a test beside it. Paths are relative to the repo
root, and a name after `::` is what `cargo test <name>` runs.

This is the behaviour spec — the *what*. For the *why* and the code that owns
each rule, see `handoff.md`.

Defaults are marked **on** or **off**. Everything off is opt-in in Settings.

---

## 1. Typing a Vietnamese word (Telex, default)

| You type | You get | Rule |
| --- | --- | --- |
| `oo` | `ô` | A doubled vowel adds the circumflex. Same for `aa`→`â`, `ee`→`ê` |
| `aw` | `ă` | `w` adds the breve to `a`, the horn to `u`/`o` (`ow`→`ơ`, `uw`→`ư`) |
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

> Pinned by `crates/glowkey-engine/tests/telex.rs`::`each_telex_key_does_its_own_job`,
> `free_tone_placement_all_orders`, `immediate_circumflex`,
> `hard_nuclei_and_onsets`, `uppercase_and_mixed_case`,
> `a_lowercase_mark_key_keeps_an_all_caps_word_in_caps`,
> `an_all_caps_vni_word_stays_in_caps`; and
> `crates/glowkey-input/tests/ladder.rs`::`free_tone_placement`, `words_and_english`
> for the same rules through the whole keyboard path.

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

**A restore hands back the word, not the cancel.** The cancelling press is an
instruction and puts nothing on screen, so auto-fix (§5) leaves it out of what it
restores — `choose` is typed `chooose`, and handing back the third `o` would undo
the gesture the user had just made. Every other key comes back, tone keys
included: the `x` of `exit` is a letter of the English word.

| You type | You get | |
| --- | --- | --- |
| `chooose`␣ | `choose` | the render `choóe` is not Vietnamese, so auto-fix restores |
| `oooo` | `ooo` | the presses after the cancel are letters and stay |

> Pinned by `crates/glowkey-engine/tests/telex.rs`::`repeating_the_diacritic_key_rejects_it`,
> `a_cancelled_repeat_stays_cancelled`, `a_tone_after_a_cancelled_repeat_still_lands`,
> `mid_word_backspace_after_a_rejected_diacritic_restores_the_diacritic`,
> `the_typed_word_drops_only_the_rejection_keystroke`,
> `the_typed_word_keeps_every_key_under_vni`;
> `crates/glowkey-session/tests/auto_fix.rs`::`a_restore_does_not_hand_back_the_rejection_keystroke`,
> `the_rejection_gesture_still_types_its_vietnamese_words`; and
> `crates/glowkey-input/tests/ladder.rs`::`backspace_after_a_rejected_diacritic_restores_it`.
> The `moóc` rows have a test of their own because the first attempt at the
> `oooo` rule broke them.

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

> Pinned by `crates/glowkey-engine/tests/telex.rs`::`mid_word_backspace_drops_a_visible_char_and_keeps_composing`,
> `mid_word_backspace_reports_failure_when_it_cannot_stay_in_step`;
> `crates/glowkey-input/tests/ladder.rs`::`backspace_case_3_mid_word_shrinks_and_stays_composed`,
> `backspace_case_3_an_ordinary_mid_word_delete_passes_through`,
> `backspace_case_5_losing_track_mid_word_ends_the_chain`,
> `reported_delete_sequences_land_where_they_should` (the two rows reported from
> live use); and the contract itself as a property,
> `crates/glowkey-session/tests/properties.rs`::`mid_word_backspace_lands_exactly_one_character_back`.

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

> Pinned by `crates/glowkey-engine/tests/telex.rs`::`word_boundary_passes_through`,
> `a_digit_ends_the_word_where_a_letter_extends_it`;
> `crates/glowkey-input/tests/ladder.rs`::`boundary_commits_the_word`,
> `backspace_case_1_deleting_a_boundary_reopens_the_word`,
> `backspace_case_2_a_bare_boundary_is_one_more_delete`,
> `deleting_back_to_a_word_reopens_it`,
> `deleting_back_through_two_words_reopens_the_right_one`,
> `deleting_back_through_a_bare_boundary_reopens_the_word_before_it`,
> `the_history_cap_is_five_entries`, `a_caret_move_clears_the_whole_history`,
> `a_restored_word_breaks_the_chain`; and
> `crates/glowkey-session/tests/session.rs`::`changing_a_typing_option_forgets_the_re_composition_memory`,
> `focus_change_flushes_in_progress_word`.

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

> Pinned by `crates/glowkey-engine/tests/telex.rs`::`interior_capitals_survive_when_not_transformed`;
> `crates/glowkey-session/tests/auto_fix.rs`::`restores_invalid_english_word`,
> `keeps_valid_vietnamese`, `auto_fix_off_leaves_telex_result`,
> `plain_english_without_transform_is_untouched`,
> `batch_of_real_words_not_restored`, `keeps_abbreviations_that_start_with_d_bar`,
> `still_restores_english_words_whose_d_bar_is_not_leading`,
> `an_invalid_syllable_restores_at_the_boundary_not_before`; and
> `crates/glowkey-session/tests/session.rs`::`excluded_app_never_transforms`,
> `english_mode_passes_through_in_a_normal_app`,
> `exclusion_beats_the_mode_toggle`, `per_app_exclusion_is_independent`,
> `terminal_hotkey_unexclusion_is_session_only`,
> `switching_into_excluded_app_stops_transformation_immediately`.

---

## 6. Rules the syllable validator lacks

Auto-fix asks "is this valid Vietnamese?". The `vi` crate answers yes to
spellings Vietnamese cannot produce, so two things are checked on top of it.

### The rime inventory

`vi` validates a syllable's three parts **independently** — initial consonant,
vowel cluster, final consonant — and strips every diacritic before looking at the
vowel. So `i` is a vowel and `ng` is a final, each true on its own, and `uíng`
comes out valid. The same blindness passes `ưa` closed by a coda, `ơng`, `ơch`
and `pởe`.

The fix is the set `vi` has no way to express: **the 170 rimes Vietnamese
actually has** (`engine.rs`::`RIMES`). A rime outside it is caught by *absence*,
so the rule does not have to be written again for each new word that trips it.

| | |
| --- | --- |
| **Derived from** | the 74k-word Viet74K list — tones and onsets stripped, what is left counted |
| **Kept** | 148 rimes above the frequency cut, plus 22 read out of the tail by hand because they are real: `thuở`, `khuỷu`, `quýt`, `bâng khuâng`, `giếc`, `ngoạm`, `huỵch`, `tuềnh`, `xoẻng`, `hừm` |
| **Left out** | transliterated loanwords — `ing` and `ic` reach the list only through `ping` and `acid`, which is exactly why `using` and `basic` used to survive as Vietnamese |
| **Rejects** | `uíng`, `ưám`, `ưnh`, `ơch`, `pởe`, `ưét`, `tưo`, `lă`, `peón`, `buín` — `using`, `wasm`, `power`, `west`, `two`, `law`, `person`, `business` |

It replaced three rules that had been written one per bug report — `ưa` closed by
a coda, `nh`/`ch` on a back vowel, `ng`/`c` on `i`/`y`. Each was true; none was
the general statement, so the next impossible rime always got through.

**A half-typed word is judged differently, and this is load-bearing.** `biết`
passes through `biế`, and open `iê` is not a legal rime — closed, it needs a
coda. Judging it by membership would refuse the keystroke and make `biết`
untypeable with the mid-word check on. Mid-word the rime may therefore be any
prefix of a table entry, **with vowel modifiers ignored**, because Telex delivers
a horn one keystroke after the vowel it lands on: `mượn` is typed `muwown` and
passes through `mưo`. At the boundary the word is finished and must match
outright, which is what restores `law` rather than leaving it `lă`.

Fixed in passing: a `uâ` word typed horn-first (`tuanwa`) used to trip the escape
latch through the old `ưa` rule and stay raw. It composes now.

### The stop-coda tone rule

Not subsumed by the table, which is tone-stripped: `màc` reduces to the perfectly
ordinary rime `ac`, and only a rule that can see the tone catches it. A syllable
closed by `c`, `ch`, `p` or `t` takes only sắc or nặng — and `f`, `r`, `x` are
exactly the three forbidden tones, so `left` came out `lèt`, `soft` `sòt`, `gift`
`gìt`.

### The onsets that cannot carry the `o` glide

Not a rime rule either, because the rimes are ordinary and only the **pairing**
is impossible. The glide is the `o` of `hoa`, `khoe`, `toe`, and two families of
onset never take it: the labials `b`, `m`, `ph`, `v` — a rounded glide after a
consonant made with the same lips is what Vietnamese does not do — and `c`/`k`,
where /k/ before the glide is spelled `qu` (`qua`, `quê`). Every other onset does
take it: `hoà`, `loà`, `ngoè`, `xoà`, `choè`, `doạ`, `goá`, `soạn`, `noãn`.

It is the English `-ore` family that pays for the gap: `r` is hỏi and the `e`
lands behind it, so `more`→`moẻ`, `bore`→`boẻ`, `core`→`coẻ` — all accepted by
`vi` and by the rime table. The French loans spelled this way (`boa`, `voan`) are
out of reach on purpose: typed plainly they are ASCII, and the verbatim guard
answers first.

### What is still not fixed

A render that is **pure ASCII** is left alone: it equals the keys the user typed,
so there is nothing to restore it to. `business` still commits as `buiness` and
`message` as `mesage`, because `ss` collapses to `s` and the result never stops
being ASCII. That is the ASCII-render restore, filed separately; every rule in
this section sits behind that guard. (Typing `busin` alone *does* restore, since
`uín` is a render the table can judge.)

> Pinned by `crates/glowkey-session/tests/auto_fix.rs`::`restores_english_words_broken_by_the_stop_coda_tone_rule`,
> `the_stop_coda_rule_leaves_legal_vietnamese_alone`,
> `restores_words_whose_render_closes_an_open_diphthong`,
> `keeps_the_open_diphthong_and_its_closed_spelling`,
> `restores_english_words_whose_render_is_not_a_vietnamese_rime`,
> `keeps_the_rimes_a_velar_coda_can_close`; and
> `restores_words_whose_render_puts_the_o_glide_behind_a_closed_onset`,
> `keeps_the_o_glide_after_the_onsets_that_carry_it`;
> `crates/glowkey-engine/tests/midword_spell_check.rs`::`the_rime_table_rejects_what_vi_accepts`,
> `the_onsets_that_cannot_carry_the_o_glide`,
> `the_o_glide_is_untouched_after_every_other_onset`,
> `every_prefix_of_a_real_word_survives_the_mid_word_check`,
> `the_rare_rimes_kept_from_the_tail_are_typable`,
> `a_finished_word_is_judged_more_strictly_than_a_half_typed_one`,
> `a_horn_first_ua_word_composes_with_the_check_on`,
> `an_open_diphthong_cannot_take_a_coda`,
> `the_open_diphthong_itself_is_untouched`,
> `a_back_vowel_cannot_be_closed_by_nh_or_ch`,
> `front_vowels_closed_by_nh_or_ch_are_untouched`,
> `i_and_y_cannot_be_closed_by_ng_or_c`,
> `other_vowels_closed_by_ng_or_c_are_untouched`,
> `the_ascii_guard_still_short_circuits_every_rule`,
> `the_deferred_siblings_counterexamples_stay_valid`,
> `no_false_rejection_across_real_vietnamese` (a 51-word corpus, asserted
> identical with the check on and off).

---

## 7. Options

### Input method

| Option | Rule | Example |
| --- | --- | --- |
| **Telex** (default) | as above | `hoongf`→`hồng` |
| **VNI** | digits carry the marks, and extend the word | `a6`→`â`, `o7`→`ơ`, `d9`→`đ`, `viet65`→`việt` |
| **Simple Telex** | Telex, except `w` never stands alone as `ư` | `w`→`w`, but `uw`→`ư` still works |

> Pinned by `crates/glowkey-engine/tests/telex.rs`::`vni_input_method`,
> `an_all_caps_vni_word_stays_in_caps`; and
> `crates/glowkey-engine/tests/simple_telex.rs`::`w_no_longer_stands_alone_for_u_horn`,
> `w_still_adds_the_horn_and_the_breve`, `everything_else_matches_full_telex`.

### Rendering

| Option | Rule | Example |
| --- | --- | --- |
| Tone placement (**new**) | modern convention | `hoaf`→`hoà`, `thuys`→`thuý` |
| Tone placement (old) | traditional convention | `hòa`, `thúy` |
| Auto-capitalize (off) | capitalizes the first letter of a sentence | |

> Pinned by `crates/glowkey-engine/tests/telex.rs`::`old_style_placement_differs`,
> `a_lowercase_mark_key_keeps_an_all_caps_word_in_caps` (which asserts both
> styles); and `crates/glowkey-session/tests/auto_fix.rs`::`auto_capitalize_sentence_start`,
> `crates/glowkey-session/tests/session.rs`::`auto_capitalize_handles_a_word_starting_with_a_bracket`.

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

> Pinned by `crates/glowkey-engine/tests/quick_telex.rs` (all seven, including
> `off_by_default_and_byte_identical_when_off` and
> `english_words_with_inner_doubles_are_untouched`);
> `crates/glowkey-engine/tests/telex_brackets.rs` (all nine, including
> `real_vietnamese_words_round_trip`);
> `crates/glowkey-engine/tests/simple_telex.rs`::`quick_telex_applies_to_both_telex_variants`,
> `brackets_stay_telex_only`;
> `crates/glowkey-session/tests/auto_fix.rs`::`macro_expansion`;
> `crates/glowkey-session/tests/macro_table.rs` (the import/export format); and
> `crates/glowkey-input/tests/ladder.rs`::`always_macro_keeps_feeding_the_engine_with_vietnamese_off`.

### Corrections

| Option | Rule | Example |
| --- | --- | --- |
| **Auto-fix** (**on**) | see §5 | `exit`→`exit` |
| Restore common English words (off) | a committed word whose keys spell a common English word is handed back even when the Vietnamese is valid | `was`→`was`, not `ứa`. The cost: `cats`→`cats`, not `cát` |
| **Mid-word spell check** (off) | repairs at the keystroke instead of at the space: the moment a word becomes unspellable it shows your raw keys for the rest of the word. It is auto-fix made earlier, so auto-fix off switches it off too — the tick is remembered, not cleared | `exit` is fixed at the `x`; with auto-fix off, `aal`→`âl` |
| **Personal words** | one word pinned to English or Vietnamese beats every rule above, in both directions | `was`→`was` and `cats`→`cát` at the same time |

**`Ctrl+Shift+W` swaps the word you just typed** and remembers the choice as a
personal word. It is the answer to any word the rules get wrong.

**The mid-word spell check and Backspace:** while a word is escaped it shows
your keys, and deleting the key that broke it brings Vietnamese back —
`hoongf`→`hồng`, a mistyped `a` shows `hoongfa`, and ⌫ gives `hồng` again, still
composing. The repeat-key gesture of §2 stands aside from the check.

> Pinned by `crates/glowkey-session/tests/auto_fix.rs`::`english_restore_fixes_valid_vietnamese_collisions`,
> `english_restore_never_touches_vietnamese_words`,
> `english_restore_off_by_default_keeps_vietnamese_reading`,
> `english_restore_works_independently_of_auto_fix`;
> `crates/glowkey-engine/tests/midword_spell_check.rs`::`off_by_default`,
> `with_the_spell_check_off_nothing_changes`,
> `the_repeat_key_escape_hatch_still_works`,
> `deleting_the_offending_key_restores_the_transformation`,
> `a_still_unspellable_word_stays_escaped`,
> `the_escape_does_not_outlive_the_word`,
> `never_touches_the_document_before_the_word`;
> `crates/glowkey-session/tests/word_overrides.rs`::`the_two_words_a_global_switch_cannot_both_get_right`,
> `an_override_beats_auto_fix_even_when_the_result_is_invalid`,
> `a_macro_still_wins_over_an_override` (the precedence order),
> `correcting_a_word_swaps_it_and_remembers_the_choice`,
> `the_correction_never_deletes_more_than_the_word_and_its_boundary`; and
> `crates/glowkey-input/tests/ladder.rs`::`backspace_case_4_undoing_an_escape_emits_instead_of_passing_through`.

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

> Pinned by `crates/glowkey-input/tests/hotkey.rs`::`the_fixed_hotkeys_need_exactly_control_and_shift`,
> `the_fixed_hotkeys_are_refused` (they cannot be recorded),
> `the_presets_match_only_their_own_combination`, `command_never_matches`,
> `escape_cancels_the_recording`; and
> `crates/glowkey-input/tests/ladder.rs`::`the_toggle_hotkey_switches_mode_and_is_consumed`,
> `the_app_toggle_hotkey_asks_the_platform_to_toggle`,
> `the_correction_hotkey_beats_the_shortcut_filter`,
> `the_correction_hotkey_is_inert_in_an_excluded_app`,
> `a_terminal_enabled_by_hotkey_is_live_but_still_persisted_as_excluded`,
> `a_recorded_custom_hotkey_toggles_and_the_old_preset_stops`.

---

## 9. Clipboard tools

Menu → remove tones, UPPERCASE, lowercase. They act on the **clipboard**, not a
selection — a background agent has no selection of its own. Non-text clipboards
are left alone. `café` is stripped to `cafe` too: nothing here knows the word is
French.

> Pinned by `crates/glowkey-engine/tests/remove_tones.rs`::`strips_every_vowel_family_and_d_bar`,
> `preserves_case`, `leaves_everything_else_alone`,
> `covers_all_five_tones_on_one_vowel`.

---

## What is not pinned here

Two kinds of behaviour in this document rest on something other than a test in
the lists above, and saying so is better than implying a green suite covers
everything:

- **The clipboard's UPPERCASE and lowercase** (§9) are the platform's own case
  mapping over the clipboard text, and have no test of their own.
- **Anything that needs a live desktop** — that the hook sees a keystroke at
  all, that injected text lands in a particular application, that the Chromium
  address bar behaves. Those are checklists, not tests:
  `manual-verification.md` (macOS) and `manual-verification-windows.md`
  (Windows).

Everything else in §1–§8 fails a named test if it changes.
