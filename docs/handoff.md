# GlowKey — Session Handoff & Status

Purpose: give a fresh session everything needed to continue GlowKey — the goal,
how it works, what's built, what's broken, and how to build/test/diagnose. Read
this first, then the `decisions/` records for depth (`docs/checkpoint.md` is a
superseded historical note).

---

## 1. What GlowKey is

A **Vietnamese input method for macOS**, in the style of **EVKey/Unikey**. It is a
background menu-bar agent (no Dock icon) that installs a **`CGEventTap`** to *wrap*
the active keyboard layout: the user keeps their Colemak/US layout, and GlowKey
adds Vietnamese on top. It is **not** an InputMethodKit input method and uses **no
marked text / composition** — it writes straight to the document by suppressing
keys and re-emitting synthesized events.

- **All-Rust** via the `objc2` ecosystem (matches the sibling `marau` project).
- Core feature the user cares most about: the **per-app ignore list** (Vietnamese
  off in terminals/editors, on elsewhere), toggled per app and remembered.
- Typing methods: **Telex** (default) and **VNI**.

## 2. Goal (current)

Feature parity with the **useful** parts of Unikey/EVKey, correct typing in
normal text fields, and a polished native-macOS UX. Legacy encodings (TCVN3,
VNI-Windows), VIQR, and clipboard-encoding conversion are **intentionally
omitted** — every modern macOS app is Unicode NFC, so they add no value.

## 3. Architecture

Cargo workspace:
- **`crates/glowkey-engine`** — platform-free Vietnamese logic. Knows nothing
  about macOS; unit-tested on any OS.
- **`app/`** — the macOS binary `GlowKey` (objc2 shell).

### Engine (`crates/glowkey-engine/src/`)
- `lib.rs`: `Engine` keeps the **raw keystroke log** for the current word and
  **re-derives** the whole rendering each key via the `vi` crate (`vi::TELEX` /
  `vi::VNI`), producing a **`KeyResponse { handled, backspaces, insert }`** diff
  (backspaces in **UTF-16 code units**). `Session` wraps `Engine` + all state:
  mode, exclusions, style, auto-fix, auto-capitalize, input method, toggle-hotkey
  preset, macros, open-settings-at-launch, and the recomposition memory.
- `config.rs`: `Settings` (serde JSON) — the persisted subset. Tolerant of missing
  and unknown keys.
- `exclusion.rs`: `ExclusionList` + `DEFAULT_EXCLUSIONS` (terminals/editors).

### Shell (`app/src/`)
- `tap/` — the `CGEventTap`, split eight ways (2026-09-03; it had reached 2255
  lines): `mod.rs` (state, run, the C callback, circuit breaker), `decide.rs`
  (the pure decision + its `CGEvent`-driven tests live in `tests.rs`),
  `keys.rs` (reading an event, recognising the hotkeys), `emit.rs` (everything
  that writes to the document, plus the omnibox guard call site), `settings.rs`
  (the `*_and_save` accessor wall the UI calls), `health.rs` (the tap health
  monitor, §6.6), `permission.rs` (the startup gate). **Full-suppression model**: GlowKey suppresses
  **every** letter it handles and re-emits the diff from a **single tagged
  `CGEventSource`** via `CGEventPost(SessionEventTap)`. This is the crux of
  correctness (see §5). Tags its own events and skips them (feedback-loop guard);
  a latching **circuit breaker** caps runaways. `decide()` is a pure function of
  event + session and is unit-tested with real `CGEvent`s.
- `menu_bar.rs` — `NSStatusItem`, live **VI/EN glyph**, menu (per-app toggle,
  mode, auto-fix, launch-at-login, reset, reveal log, Settings, About, Quit).
- `prefs/` — Settings window, an `NSTabView` of four panes (General / Typing /
  Corrections / Apps & macros), plus the separate **Excluded Apps** and
  **Macros** windows. Split six ways (2026-09-03, from 1423 lines): `mod.rs`
  (the `define_class!` controller and its actions), `tabs.rs` (the four panes),
  `excluded.rs`, `macros_window.rs` (including the import/export bodies, moved
  out of the class where 141 lines of file dialog sat among forty four-line
  toggles), `personal_words.rs` (the per-word ambiguity decisions, §6.3),
  `widgets.rs` (the shared row/label/stack helpers). The panes exist
  because a single column had grown past 800 points — each tab builds its own
  stack via `tab_stack`, and the tab title carries the grouping that section
  headers used to.
- `about_window.rs`, `welcome.rs` (the one-time guide, §6.7), `hud.rs` (toggle
  flash), `login_item.rs` (SMAppService),
  `app_info.rs` (frontmost app), `settings_store.rs` (file I/O), `log.rs`.

## 4. Features implemented (all committed, test-covered where headless-possible)

- Order-independent Telex tone marks (`hoongf`/`hofong`/`hoonfg` → `hồng`),
  immediate diacritics (`oo`→`ô`).
- **VNI** input method (`viet65`→`việt`) — Settings picker.
- **Per-app exclusions**, independent + remembered; ⌃⇧E to toggle the current app.
- **Auto-fix**: at a boundary, restore raw keys when the result isn't valid
  Vietnamese (`exit`, not `eĩt`). Validity is `vi::validation::is_valid_syllable`
  **plus** the stop-coda tone rule the `vi` crate lacks: a syllable closed by
  `c`, `ch`, `p` or `t` can carry only sắc or nặng, never huyền, hỏi or ngã
  (UniKey's `lastWordIsNonVn`, `ukengine.cpp:2352`). `vi` calls `màc`, `hỏc`,
  `mãt` and `hòp` valid; they are not. This is not academic — Telex's `f`, `r`
  and `x` are exactly those three tones, so before the rule was added `left` came
  out `lèt`, `soft` `sòt`, `gift` `gìt` and `lift` `lìt`, and auto-fix would not
  rescue them because it had been told they were valid Vietnamese. One exemption: a word starting with **đ** is
  kept as-is. A leading đ costs `dd` (Telex) or `d9` (VNI) and no English word
  begins with either, so it is always deliberate — this is what keeps the everyday
  abbreviations `đc`, `đt`, `đk` from being handed back as `ddc`, `ddt`, `ddk`.
  Words that merely *contain* the pair still restore (`address`, `odd`, `sudden`),
  since their đ is not leading.
- **Re-composition**: `hồng`␣⌫`z` → `hông` (deleting the boundary re-opens the word).
- **Personal word list + ⌃⇧W** (2026-09-03): a per-word answer to the
  English/Telex ambiguity, which §6.3 had recorded as unresolvable. A word pinned
  to either reading beats auto-fix, the curated English list and the global
  switch, in both directions, and loses only to a macro — so `was`→`was` and
  `cats`→`cát` hold at the same time, which no setting of the global switch can
  do. **⌃⇧W** right after a word swaps it and records the choice; the list is
  managed in Settings → Corrections → Personal Words…. The hotkey is fixed, like
  ⌃⇧E and unlike the VN/EN toggle, and the recorder refuses both.
  `tests/word_overrides.rs`, and the correction is modelled in
  `tests/properties.rs`.
- **Deleting the mistake undoes a spell-check escape** (2026-09-04): when the
  mid-word spell check has refused a word and is rendering it verbatim, a
  Backspace that leaves something spellable brings the transformation back —
  `hoongf` gives `hồng`, a mistyped `a` escapes it to `hoongfa`, and ⌫ restores
  `hồng` still composing. The escape used to be a one-way latch, cleared only
  when the word emptied or hit a boundary, so the word stayed literal for the
  rest of its life. The exit asks the same question the entry did
  (`Engine::can_unescape`), so the two rules cannot drift apart. **The repair is
  emitted, not passed through**: the tap suppresses the Backspace and sends one
  edit covering the whole on-screen word, because letting the host delete and
  then posting a repair mixes a native keystroke with a synthesized one — the
  race §5 exists to remove. `BackspaceOutcome` is the three-way answer that makes
  the caller decide explicitly; a `bool` could not.

  **Deletes stay visible-character deletes**, questioned twice in live use and
  reaffirmed both times (2026-09-04). `hoongf` `a` ⌫ ⌫ `z` gives `hôn`, not `hông`:
  the second ⌫ removes the visible `g`, not the tone key `f`. The two diverge
  only at a tone key — `hồng` is four characters and six keystrokes — and
  keystroke-undo would have meant a second Backspace mode that exists only after
  a repair, or reversing the contract above for every word.

  The second report was `hoongf` `s` ⌫ ⌫ `z`, wanting `hông`. Same root: `s` is
  a tone key, so `hống` is four characters and seven keystrokes, and deleting
  characters gives `hốn` → `hố` where deleting keystrokes would give `hoong` →
  `hông`. Worth knowing if it comes up a third time — the disagreement is always
  a tone key, never anything else, because that is the only place a keystroke
  produces no character of its own.
- **Mid-word backspace stays composed**: `hoongf`⌫`z` → `hôn`. The host does the
  delete, so the engine has to land on exactly what the screen shows — the render
  minus its last character (`hồn`), which means dropping the raw `g` and keeping
  the tone key `f`. `Engine::backspace_visible_char` searches the raw log from the
  end for the one removal that re-renders to that target; it returns false when
  nothing matches and the tap flushes. Note the older `Engine::backspace` pops the
  last raw *key* instead (`hồng`→`hông`) — wrong for this path, and now unused by
  the app.
- **Caret-navigation flush**: arrows/Home/End/Page flush the diff baseline.
- **Auto-capitalize** first letter of each sentence (opt-in).
- **Configurable toggle hotkey** (⌃⇧Space / ⌃Space / ⌥Space / ⌃⇧Z) **plus a
  recorder**: the "Custom…" segment in Settings arms a recorder that captures the
  next ⌃/⌥ combo (`HotkeyPreset::Custom`). Safety: while armed, only ⌃/⌥ combos
  are intercepted — plain typing and every ⌘ shortcut pass through; Esc, any
  mouse click, or switching apps cancels; ⌃⇧E and ⌃⇧W are rejected (reserved for the
  per-app toggle).
- **Chromium omnibox guard**: before emitting backspaces in a Chromium browser,
  one AX check (focused element is an `AXTextField` with non-empty
  `AXSelectedText`) detects the omnibox's inline-autocomplete trailing selection
  and clears it with a forward-delete (`app/src/ax.rs`). Best-effort — see §6.1.
- **Exclusion tombstones**: `removed_default_exclusions` in settings; at load the
  effective list is `saved ∪ (defaults − tombstones)`, so new shipped defaults
  reach old settings files without resurrecting deliberate removals.
- **Session-only terminal un-exclusion**: ⌃⇧E in a known terminal
  (`TERMINAL_EXCLUSIONS`) enables Vietnamese only until restart (HUD shows
  "VI ⚠"); permanent removal only via the Excluded Apps window.
- **Restore common English words** (opt-in, Settings → Typing): a committed word
  whose raw keys are a common English word (embedded list, `english.rs`) is
  restored even when the render is valid Vietnamese (`was`→`was`, not `ứa`).
  Off by default — it inverts the ambiguity for `cats`→`cát`, `car`→`cả`.
- **Simple Telex** (`UkSimpleTelex`), a third input method. Telex with one
  change: `w` only ever adds a horn to `u`/`o` or a breve to `a`, so it never
  stands alone as `ư` — the behaviour people either rely on or trip over, which
  is why UniKey ships both. Implemented as our own `phf` `Definition` fed to `vi`
  (it takes any `Definition`, so a custom method costs eleven lines), pinned to
  phf 0.11 to match `vi`'s. Quick Telex applies to both Telex variants — its
  digraphs are plain letters — while the bracket shortcuts stay Telex-only,
  because UniKey's Simple Telex mapping deliberately drops them.
  `tests/simple_telex.rs`.
- **Clipboard tools** (UniKey's "Công cụ"), in the menu: remove tones,
  UPPERCASE, lowercase. They act on the clipboard rather than a selection —
  a background agent has no selection of its own. `engine::remove_tones` maps
  every toned vowel and `đ` back to its base letter and leaves everything else
  alone; a borrowed word like `café` is stripped too, since `é` is an ordinary
  Vietnamese tone form and nothing here knows the word is French. Non-text
  clipboards are left untouched. `tests/remove_tones.rs`.
- **Macros while Vietnamese is off** (`alwaysMacro`, opt-in, Settings → Apps &
  macros). The keys compose verbatim so a shortcut can still match at the
  boundary, reusing the same `escaped` path the spell check sets rather than
  adding a second verbatim mode. Never in an excluded app — excluded means hands
  off, and a terminal silently expanding `vn` would be worse than the bug
  exclusions prevent — and inert with no macros defined, so English typing keeps
  its untouched passthrough unless all three conditions hold.
- **Telex bracket shortcuts** (opt-in, Settings → Typing): UniKey's `[`→ơ,
  `]`→ư, `{`→Ơ, `}`→Ư. Each bracket is rewritten to the Telex *keys* that spell
  the vowel (`[`→`ow`) rather than to the character, so a tone key typed after it
  still lands (`[f`→ờ); inserting a precomposed `ơ` would leave `vi` with
  something it cannot modify. Telex only, and applied after Quick Telex, which
  inspects the first two raw keys. Off by default because turning it on stops `[`
  and `]` reaching the app at all, including where they are bare-key commands —
  `Engine::is_syllable_char` and the tap's `is_word_char` both widen to accept
  them only while it is on. The injected keys carry the surrounding case (two or
  more capitals means Caps Lock, one means Title case), since Caps Lock does not
  shift `[`. **Known limit:** a bracket after a vowel leaks its substitution
  keys — `an[` gives `anow` — because `vi` then applies no transformation and
  returns the expanded keys verbatim. Every real Vietnamese use is unaffected
  (ơ and ư follow a consonant or open the syllable), and
  `tests/telex_brackets.rs` pins thirteen real words. Feeding `vi` a precomposed
  `ơ` instead is worse: it strips the horn (`tơ`→`to`).
- **Mid-word spell check** (opt-in, Settings → Typing): UniKey's
  `spellCheckEnabled`, which it keeps separate from `autoNonVnRestore`. Auto-fix
  repairs at the space; this repairs at the keystroke — `exit` becomes `exit`
  at the `x` rather than after the boundary. When a render turns non-ASCII and
  fails `vi::validation::is_valid_syllable`, the word is **escaped**: it renders
  its raw keys verbatim until the next boundary. Escaping the whole word rather
  than dropping the one key is forced by the design — the engine re-derives
  everything from the raw log each keystroke, so a dropped key is re-applied by
  the next one. Judged on the **render**, never the raw keys: raw `nguow` is not
  a syllable but its render `ngươ` is an ordinary step of typing `người`.
  One carve-out: a repeated diacritic key is the user's own rejection gesture
  (`hoongff`→`hôngf`), so the check stands aside. `tests/midword_spell_check.rs`
  carries a 51-word corpus asserting identical output with the option on and off.
- **Macros (gõ tắt)**: `vn `→`Việt Nam `, managed in the Macros window, with
  **import and export** — EVKey's `shortcut:expansion` line format, so a table
  curated in Unikey or EVKey imports as-is. Import merges and reports
  `(added, skipped)`; an existing shortcut is skipped, never overwritten. Tables
  the line format cannot carry (a colon in a shortcut, a newline in either field)
  are written as JSON and still parse back.
  Real UniKey exports are handled too: the byte-order mark is stripped, the
  `;DO NOT DELETE THIS LINE*** version=N ***` header is recognised, and a version
  other than 1 means a VIQR body, which the importer refuses with an explanation
  rather than storing `Vie^.t Nam` as literal text. Neither field is trimmed
  (UniKey does not), except the shortcut, which is matched against typed keys and
  so cannot hold a space. `Macro::parse_table` /
  `Macro::format_table`, and the merge rule in `Session::import_macros` — note
  that `add_macro` is add-*or-replace* and answers `true` either way, so a merge
  built on its return value destroys the user's own macros. Tested in
  `tests/macro_table.rs`.
- **Vietnamese interface** (Unikey's "Vietnamese interface"): Settings → General
  → Language, one of System / Tiếng Việt / English, default System (resolved
  against `NSLocale::preferredLanguages`). Strings are picked at the call site by
  `strings::t(english, vietnamese)` — no key table to drift. The language is
  resolved in `tap::run` **before** the permission gate, since that alert is the
  first thing a new user sees. Changing it closes and rebuilds the windows
  (`prefs_window::rebuild_windows`), so it applies without a restart.
- **Quick Telex** (opt-in, Settings → Typing; an EVKey / later-UniKey idea, not
  in the 2015 UniKey source): a doubled consonant at the
  **start** of a syllable types its digraph — `cc`→`ch`, `gg`→`gi`, `kk`→`kh`,
  `nn`→`ng`, `pp`→`ph`, `qq`→`qu`, `tt`→`th`, `uu`→`ư`. Syllable-initial only,
  which is where these are legal Vietnamese onsets and which leaves English
  alone: `letter`, `happy`, `accept` all have their doubles mid-word. `uu`
  expands to the Telex keys `uw` rather than to `ư`, so **the whole feature is
  Telex-only** — under VNI those keys would put a literal `w` on screen that
  auto-fix cannot repair (the result is plain ASCII, so it counts as typed
  verbatim). Case follows the trigger: both keys shifted means caps lock and the
  whole digraph uppercases (`CCAO`→`CHAO`); one shifted is Title case
  (`Ccao`→`Chao`). Off by default; `tests/quick_telex.rs`.
- **Launch always in Vietnamese** (mode is session-only, never persisted).
- **Open Settings on launch** (toggle), **Launch at login**, **VI/EN glyph + HUD**,
  **Reveal Log in Finder**, **About** window, **Reset input**.
- **Persistent logging** → `~/Library/Logs/GlowKey/glowkey.log` (see §7).

## 5. Key decisions (the "why")

- **Full suppression, single source.** Mixing native passthrough with synthesized
  backspaces races (a native char and a later synthetic backspace arrive out of
  order in multiprocess apps → `aa`→`aâ`, `hoongf`→`hoồng`). Suppressing every
  letter and emitting all edits from one `CGEventPost` FIFO removes the race by
  construction. This is how EVKey/OpenKey work. See `decisions/` + `tap.rs` header.
  The **boundary key is part of this invariant**: an auto-fix restore suppresses
  the space/punctuation that triggered it and replays it from GlowKey's own source
  (`Decision::EmitThenReplayKey`, flags preserved so ⇧1 stays `!`). Passing it
  through natively instead lost the race — the host applied it before the posted
  backspaces, which then ate it: `ddc`␣→`đddc`, `work`␣→`ưwork`, space swallowed.
- **Mode is session-only.** Persisting the global VN/EN toggle let one accidental
  ⌃⇧Space at quit make the app launch disabled ("aa not work"). Now it always
  launches Vietnamese; only exclusions/auto-fix/style/method/macros/hotkey persist.
- **Blind model.** The engine has no cursor/selection/host-text read-back; its one
  invariant is "rendered == the text tail at the caret." Everything that can move
  the caret (shortcuts, mouse, arrows, app switch) calls `flush()`.

## 6. KNOWN ISSUES / STATUS (updated 2026-09-02, second session)

1. **Chrome/Edge omnibox** — MITIGATION SHIPPED (best-effort, not a proof), needs
   live verification. The guard (`tap.rs::emit_edit` + `ax.rs`): when an edit
   with backspaces is about to land in a Chromium browser AND the focused element
   is an `AXTextField` with non-empty `AXSelectedText`, post one forward-delete
   to clear the inline-autocomplete selection first. Normal fields (empty
   selection) and non-text-field surfaces (web content, contenteditable) are
   untouched. Known residual: the AX read races Chrome's async renderer path, so
   a stale answer can occasionally skip or misfire the guard — it converts a
   deterministic bug into a rare timing one. Adds up to 2–3 AX IPC round-trips
   (50 ms cap, typ. sub-ms) per *transforming* keystroke in Chromium apps only,
   and querying AX makes Chromium keep its accessibility tree on. Log line
   "OMNIBOX trailing selection detected" marks each fire; "AX guard unavailable"
   (once per run) marks a dead guard.
2. **Terminals** — HARDENED. ⌃⇧E in a known terminal (`TERMINAL_EXCLUSIONS`) now
   un-excludes for the session only (HUD "VI ⚠"); restart re-excludes. Shipped
   defaults merge into old settings files at load (tombstones in
   `removed_default_exclusions`), so `org.alacritty` etc. self-heal. Permanent
   removal is still possible, but only via the Excluded Apps window.
3. **English/Telex ambiguity** — RESOLVED per word (2026-09-03), still inherent
   in principle. The same keystrokes are legitimate Vietnamese and legitimate
   English, and no blind rule decides which: `was` is `ứa`, `cats` is `cát`.

   Two blunt instruments existed and both remain. The opt-in "Restore common
   English words" (curated list, `english.rs`) is global, and its trade-off is
   wide — with it ON, syllables typed with a trailing tone key that collide with
   listed words become untypeable in that key order (á→as, í→is, ú→us, ò→of,
   ỏ→or, mã→max, sĩ→six, thú→thus, cả→car, hải→hair, tả→tar, cát→cats, sét→sets),
   which is why it ships OFF. And pressing a diacritic key again rejects the mark
   (`cass`→`cas`, `hoongff`→`hôngf`), the gesture Unikey uses, pinned by
   `repeating_the_diacritic_key_rejects_it` in `tests/telex.rs` — but nothing
   remembered it, so the same word had to be rejected again every time.

   **The answer is now per word.** `Settings.word_overrides` pins a word to
   either reading; an override beats auto-fix, the curated list and the global
   switch, in both directions, and loses only to a macro. With it, `was`→`was`
   and `cats`→`cát` are both true at once, which no setting of the global switch
   can achieve. Managed in Settings → Corrections → **Personal Words…**, and
   learned in one keystroke: **⌃⇧W** right after a word swaps it to the other
   reading and records the choice. `tests/word_overrides.rs`.

   Three things about ⌃⇧W are load-bearing and each is a bug that shipped once:
   it forgets the word afterwards (a corrected word must not re-compose, or the
   next Backspace reopens the old rendering and the letter after that eats a
   character); it refuses when the boundary key inserted nothing at the caret
   (Escape, function keys, keypad Enter, Help, ⌦ — and Tab and Return, which move
   the caret outright); and the decision is written to disk from
   `handle_key_down`, because `decide` is deliberately free of disk side effects
   and the feature otherwise forgot everything at quit.
4. **All GUI is unverifiable headless** (unchanged) — new controls to eyeball:
   English-restore checkbox, the 5-segment hotkey picker ("Custom…" arms the
   recorder; the caption row shows "Current: …"), "VI ⚠" HUD variant.
5. **Accessibility re-grant after rebuild or move** — the ad-hoc signature changes
   on every build, and copying the bundle elsewhere (to `/Applications`, say)
   makes a new one as far as the system is concerned, so the grant is dropped and
   the app waits at the permission gate. The gate is now **visible**: it shows an
   alert naming the exact fix and offering "Open System Settings", re-shows it
   after that button, and dismisses itself the moment the switch is flipped —
   before, an `LSUIElement` agent with no icon and no window simply looked dead
   (`tap.rs::wait_for_accessibility`, plus the log line "STARTUP waiting for the
   Accessibility permission"). Verified on screen 2026-09-03.

   Two details there are load-bearing. **`NSAlert::layout()` must be called before
   `window()`** — NSAlert lays itself out inside `runModal`, which a raw modal
   session never calls, and without it the panel renders its un-laid-out
   template: 260 points wide with both strings truncated, a placeholder "Do not
   show this message again" checkbox, and a spare untitled button between the two
   real ones. And **the system's own prompt (`AXIsProcessTrustedWithOptions`)
   fires from the alert's button, not at launch** — it is what registers the app
   in the Accessibility list, so it cannot be dropped, but calling it at startup
   put two dialogs on screen at once.

6. **Accessibility revoked while running** — FIXED, needs live verification.
   The permission used to be checked once at startup and never again, so
   revoking it killed the tap silently: the process stayed alive, the menu bar
   kept showing **VI**, the log said nothing, and re-granting did not help
   because nothing re-entered the gate. There was no lag and no loop — nothing
   polled at all — which is exactly why the failure was silence. Now
   `tap/health.rs` polls `CGEventTapIsEnabled` every two seconds and branches on
   the cause: disabled-but-trusted is re-enabled in place and logged with a
   count; revoked shows a **⚠** glyph plus a menu line offering "Open System
   Settings…", and when trust returns the tap is **rebuilt** (re-enabling the old
   port does nothing — it was created under a grant that no longer exists). The
   `TapDisabled*` callback branch is logged and counted too, where it used to
   re-enable blind. See `docs/decisions/0007`. **Unverified:** on some macOS
   versions revoking terminates the process outright, which would make the
   recovery path unreachable and harmless; that reproduction is step 1 of
   `docs/manual-verification.md` §9.

7. **First-run discoverability** — FIXED, needs an eyeball. A one-time
   `NSAlert` after the first successful grant names ⌃⇧Space, ⌃⇧E and the default
   terminal exclusions; `welcome_shown` in settings keeps it to once, and the
   menu's **Quick Guide…** reopens it. Shown only from the path where the gate
   succeeded, never from inside it — two dialogs at once is the bug §6.5
   records.

8. **Signing and distribution** — `build-app.sh` now signs with a stable
   self-signed identity when one exists and says so, ad-hoc otherwise; it also
   stamps the version from `app/Cargo.toml` and prints the designated
   requirement, so the cdhash churn behind the re-grant problem is visible at
   build time. `scripts/make-dmg.sh` + `.github/workflows/release.yml` turn a
   `v*` tag into a disk image. Not notarized (no paid Apple account, by choice),
   so a downloaded copy needs `xattr -dr com.apple.quarantine` once. See
   `docs/decisions/0006`.

## 7. Diagnosing from the log (do this first for any reported typing bug)

`~/Library/Logs/GlowKey/glowkey.log` records every handled key:
```
#42 +3.4s KEY Some('o') code=41 app=com.mitchellh.ghostty mode=Vietnamese active=true | Emit bs=1 ins="ô" | raw="hoo" rendered="hô"
#43 +3.4s EMIT took=180µs
```
The KEY line shows the **app**, whether Vietnamese was **active**, the
**decision**, the **emitted diff**, and the engine's **raw/rendered**. It is
written *before* the decision is carried out, so the lines an action writes for
itself (`TOGGLE`, `OMNIBOX`, `RUNAWAY`) follow the KEY line that caused them, and
so the KEY line survives even if the emit path dies. The emits are almost always
correct — a reported bug is usually (a) wrong app active (terminal/omnibox) or
(b) host-side delivery. `GLOWKEY_DEBUG=1` also echoes to stderr.

`EMIT took=` follows every emit and measures **the emit alone**, which is where
the only millisecond-scale cost in the path lives: the Chromium omnibox guard's
accessibility round-trip (§6.1), capped at 50 ms. It deliberately excludes the
rest of the keystroke, because a hotkey writes a settings file and the first HUD
flash creates a window — folding those in would make a slow number sometimes
mean "saved settings" and send the next person debugging a latency report at the
wrong subsystem.

The engine is not a suspect either way: 2 µs per keystroke in release, 9 µs in
the test profile, pinned by `crates/glowkey-engine/tests/latency.rs` and measured
per word by `cargo bench -p glowkey-engine`. So a large `EMIT took=` means the AX
guard or `CGEventPost`, never Vietnamese logic.

**Not yet measured on screen:** the actual `EMIT took=` figures in a live
Chromium window versus a plain text field. That difference is the AX guard's real
cost, which §6.1 still describes only as "typ. sub-ms" — an estimate, not a
measurement. The field is in place; reading it needs a granted build and someone
typing in Chrome.

## 8. Build / test / run

```bash
cargo test --workspace         # 134 tests, all green; the headless proof
cargo clippy --workspace --all-targets   # must be 0 warnings
cargo bench -p glowkey-engine  # keystroke latency numbers (criterion)
bash scripts/release-install.sh          # build GlowKey.app → /Applications → launch
bash scripts/dev-run.sh                  # build+run "GlowKey Dev" w/ GLOWKEY_DEBUG=1
bash scripts/build-app.sh [release|dev] [release|debug]   # bundle only, no install
bash scripts/make-dmg.sh                 # package build/GlowKey.app as a .dmg
```

Manual checks that no test can reach — every Settings control, the HUD variants,
the omnibox in each browser, permission revocation — are scripted in
[`manual-verification.md`](manual-verification.md). It has **never been run end
to end**; treat unticked sections as unverified.

**Two app identities.** The shipped app is `GlowKey` / `io.glowkey.GlowKey`; the
dev loop builds `GlowKey Dev` / `io.glowkey.GlowKey.dev` (own display name, own
executable name). They are separate apps to macOS, so each holds its own
Accessibility entry — iterating on the dev build no longer invalidates the grant
of the app you actually type with. They share settings and the log, and must
never run at once (two taps process every keystroke twice); both wrapper scripts
stop both variants first.

- **Accessibility permission** required (System Settings → Privacy → Accessibility).
  The grant is tied to the ad-hoc **signature**, not just the path: an unchanged
  rebuild keeps the same cdhash and the same grant, but any code change produces a
  new one and needs a fresh grant. The app says so on screen (§6.5) and starts
  itself once the switch is flipped. A stable self-signed certificate would end
  the re-granting; not set up. This bites `release-install.sh` only —
  **`dev-run.sh` needs no grant at all**: it `exec`s the binary from the shell, so
  macOS makes the *terminal* the responsible process and the tap inherits the
  terminal's grant. Verified both ways — the same bundle launched with `open`
  waits for a permission of its own.
- Does not work in secure/password fields (macOS withholds those events).
- The project lives at `~/project/ai/glowkey` inside the `ai/` container of ~25
  repos (`ai/` is NOT a repo; `ls` prints empty in the sandbox — use glob/find).

## 9. objc2 / Rust gotchas (bit us before)

- **`setReleasedWhenClosed(false)`** is required (and is `unsafe`) or a window
  can't reopen after the user closes it — macOS frees it on close.
- Clippy flags **unnecessary `unsafe`** on objc2 methods that are actually safe
  (setters, `labelWithString`, `activateFileViewerSelectingURLs`, …); only
  `msg_send!` and some class constructors need `unsafe`.
- **VNI digits must extend the word**, not end it — `Engine::is_syllable_char` and
  the tap's word-char test are method-aware (letters always; digits only in VNI).
- **Telex-safe test words**: avoid `w f j s x z r`, double vowels, and `dd` — they
  transform. (`next`→`nẽt`, `good`→`gôd`.) Use `hi man big cat top van go`.
- `cp`/`rm` may be interactive-aliased in this shell; use `cat >` / `command cp`.

## 10. Where the records are

- Plans: `plans/260901-1919-...` (UI/ignore/auto-fix), `plans/260902-1230-...`
  (remaining fixes + deferred omnibox), `plans/260902-1425-...` (Unikey/EVKey copy).
- Decisions: `docs/decisions/0001`–`0005` (all-Rust objc2; CGEventTap wrap;
  omnibox AX guard; terminal exclusion hardening; opt-in English restore).
  UI design: `docs/ui-design.md`. Checkpoint: superseded pointer only.

## 11. Suggested next steps for a new session

1. Re-grant Accessibility (the 2026-09-02 rebuild dropped it — §6.5), then verify
   by eye: the omnibox guard in Chrome (`hoongf`→`hồng` in the address bar), the
   "VI ⚠" HUD on ⌃⇧E in Ghostty, the new Settings controls, hotkey recording.
2. If the omnibox guard proves itself, consider extending it beyond Chromium
   (Safari's address bar has the same autocomplete pattern) — kept narrow first.
3. Everything in §6 is otherwise shipped; plan record:
   `plans/260902-1515-fix-known-issues/plan.md`.
