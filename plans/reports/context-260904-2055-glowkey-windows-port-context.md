# GlowKey — Technical Context Export for a Windows Port Review

**Repository:** `~/project/ai/glowkey` · branch `main` · HEAD `44a38fa`
**Exported:** 2026-09-04. Describes what the code does *today*. No proposed architecture.
**Verified against source.** `cargo test --workspace` → **190 tests, 0 failures** (run during this export).

GlowKey is a Vietnamese input method for macOS in the style of UniKey/EVKey. It is
**not** an InputMethodKit input method and uses **no marked text / composition**: it
installs a `CGEventTap`, suppresses keys it handles, and re-emits synthesized events
straight into the focused document. Written entirely in Rust (`objc2` for the macOS
frameworks). Its distinguishing feature is a per-application ignore list.

---

## 1. Project structure

Cargo workspace, two members (`Cargo.toml`):

```
crates/glowkey-engine/   Platform-free Vietnamese logic + settings + ignore list. 2 851 lines src.
app/                     macOS shell (objc2). 4 274 lines src (incl. 898 lines of tap tests).
scripts/                 build-app.sh, dev-run.sh, make-dmg.sh, release-install.sh,
                         setup-signing.sh, make-icon.sh, uninstall.sh
justfile                 Task runner (install / dev / test / lint / bench / dmg / log / stop).
docs/                    handoff.md (START HERE), decisions/0001–0008, manual-verification.md, ui-design.md
plans/                   11 dated plan directories + journals + reports
.github/workflows/       ci.yml (Linux engine job + macOS workspace job), release.yml (v* tag → dmg)
rust-toolchain.toml      Pinned channel 1.98.0 (vi 0.8 requires 1.96+)
```

### macOS app entry points

| File | Role |
|---|---|
| `app/src/main.rs` | `fn main()` → `tap::run()`. Every module is `#[cfg(target_os = "macos")]`; non-macOS builds a stub so CI can test the engine on Linux. |
| `app/src/tap/mod.rs:362` | `pub fn run()` — the real entry: loads settings, resolves UI language, blocks on the Accessibility gate, creates `TapState`, leaks a `TapContext`, creates the tap, installs the health timer, installs the menu bar, shows welcome/Settings, runs `NSApplication::run()`. |
| `app/Resources/Info.plist` | `LSUIElement=true` (background agent, no Dock icon), `io.glowkey.GlowKey`, `LSMinimumSystemVersion 13.0`. |
| `app/build.rs` | Stamps the git commit into the binary (`GLOWKEY_COMMIT`) for the About window. |

Two app identities: shipped `GlowKey` / `io.glowkey.GlowKey`; dev loop builds
`GlowKey Dev` / `io.glowkey.GlowKey.dev` so iterating never invalidates the shipped
app's Accessibility grant. **Never run both** — two taps process every keystroke twice.

### Input method / keyboard handling code (`app/src/tap/`, split 8 ways)

| File | Lines | Contents |
|---|---|---|
| `mod.rs` | 445 | `TapState`, `TapContext`, `GLOWKEY_TAG`, the C callback `tap_callback` / `tap_dispatch`, the runaway circuit breaker constants, `run()`. |
| `decide.rs` | 450 | `Decision` enum and `TapState::decide` — **the pure decision function**; `handle_key_down`, `carry_out`, `key_log_line`, `capture_hotkey`. |
| `emit.rs` | 203 | Everything impure: `emit_edit`, `emit`, `post_key`, `post_key_with_flags`, `post_string`, `circuit_ok`, `is_own_event`, `frontmost_bundle_id`, `own_bundle_id`, `CHROMIUM_BUNDLE_PREFIXES`. |
| `keys.rs` | 146 | Pure event reading: key codes, `unicode_char`, `integer_field`, `is_caret_move`, `is_shortcut`, `is_ctrl_shift`, `is_toggle_hotkey`, `is_app_toggle_hotkey`, `is_correction_hotkey`, `modifier_names`. |
| `health.rs` | 345 | `create_tap`, `install_health_timer`, `check_tap_health`, `flush_after_gap`, `tap_is_dead`. |
| `permission.rs` | 200 | `accessibility_trusted`, `wait_for_accessibility` (modal-session alert), `open_accessibility_settings`. |
| `settings.rs` | 375 | The `*_and_save` accessor wall the UI calls (≈43 methods). No logic — borrow session, mutate, snapshot, write file. |
| `tests.rs` | 898 | 34 tests driving `decide` with **real `CGEvent`s**, no Accessibility grant needed. |

Also `app/src/ax.rs` (142) — the only Accessibility read-back, for the Chromium omnibox guard.

### Core typing/IME engine (`crates/glowkey-engine/src/`)

| File | Lines | Contents |
|---|---|---|
| `lib.rs` | 2 098 | `Engine`, `Session`, `KeyResponse`, `BackspaceOutcome`, `BoundaryBackspace`, `Behind`/`CommittedWord`, `CorrectableWord`, `Macro`, `WordOverride`/`WordPreference`, `InputMethod`, `PlacementStyle`, `HotkeyPreset`, `Language`, `InputMode`, `ExclusionToggle`, `render`, `diff`, `apply_case`, `expand_quick_telex`, `expand_telex_brackets`, `SIMPLE_TELEX`, `is_invalid_vietnamese`, `violates_stop_coda_tone`, `remove_tones`. |
| `config.rs` | 309 | `Settings` (serde, 17 fields, all `#[serde(default)]`), `from_json`/`to_json`, `exclusion_list()`, 9 unit tests. |
| `exclusion.rs` | 312 | `ExclusionList` (3 `BTreeSet`s), `DEFAULT_EXCLUSIONS` (14 ids), `TERMINAL_EXCLUSIONS` (9 ids), `is_terminal`, 8 unit tests. |
| `english.rs` | 132 | `is_common_english` + a curated static `WORDS` list, 3 unit tests. |

### Settings / UI (`app/src/`)

| File | Lines | Role |
|---|---|---|
| `menu_bar.rs` | 488 | `NSStatusItem`, VI/EN/⚠ glyph, menu (per-app toggle, mode, auto-fix, launch-at-login, clipboard tools, reset, reveal log, Settings, About, Quick Guide, Quit). Registers the `NSWorkspaceDidActivateApplicationNotification` observer (`menu_bar.rs:457`) that feeds `TapState::set_frontmost_app`. |
| `prefs/mod.rs` | 615 | `define_class!` controller + actions. Public: `show`, `personal_words_changed`, `hotkey_recording_done`. |
| `prefs/tabs.rs` | 457 | The four `NSTabView` panes: General / Typing / Corrections / Apps & macros. |
| `prefs/excluded.rs` | 139 | Excluded Apps window. |
| `prefs/macros_window.rs` | 324 | Macros window incl. import/export file dialogs. |
| `prefs/personal_words.rs` | 192 | Per-word override editor. |
| `prefs/widgets.rs` | 124 | Shared row/label/stack helpers. |
| `about_window.rs` 119, `welcome.rs` 74, `hud.rs` 138 | About; one-time quick guide; borderless toggle-flash panel. |
| `settings_store.rs` | 61 | The **only** settings I/O: `~/Library/Application Support/GlowKey/settings.json`, atomic write (tmp + rename) with one `.bak`. |
| `strings.rs` | 51 | `t(english, vietnamese)` — call-site localization, no key table. Language resolved against `NSLocale::preferredLanguages`. |
| `log.rs` | 259 | Per-key logging to `~/Library/Logs/GlowKey/glowkey.log`, 5 MB rotation with one kept generation. **⚠ Uncommitted working-tree change** (see §8). |
| `login_item.rs` | 33 | `SMAppService` launch-at-login. |
| `app_info.rs` | 16 | `NSWorkspace::frontmostApplication` → `(display name, bundle id)`. |

### Tests

| Location | Tests | Focus |
|---|---|---|
| `crates/glowkey-engine/src/*` (`#[cfg(test)]`) | 23 | Settings JSON tolerance, exclusion list + tombstones, english list hygiene. |
| `crates/glowkey-engine/tests/` (13 files) | 133 | Integration, per feature (see §5). |
| `crates/glowkey-engine/benches/keystroke.rs` | criterion | Per-word keystroke latency. |
| `app/src/tap/tests.rs` | 34 | The decision function against real `CGEvent`s. |
| **Total** | **190** | All green. |

---

## 2. Architecture

### How a key event flows through the system

```
physical key
  → system keyboard layout (Colemak/US/…) maps it
  → CGEventTap callback  [tap/mod.rs::tap_callback → tap_dispatch]
      · catch_unwind wrapper (a panic must not unwind into CoreFoundation)
      · TapDisabledByTimeout / ByUserInput → CGEvent::tap_enable(port,true), log+count, state.flush(), pass through
      · Left/RightMouseDown                → state.flush(), pass through
      · not KeyDown                        → pass through
      · is_own_event(event)                → pass through   (source user-data == GLOWKEY_TAG)
      · record last_key_at = now
  → TapState::handle_key_down             [tap/decide.rs:42]
      · refresh_frontmost_at_word_start()  — ONE NSWorkspace query, first keystroke of the run only
      · decision = self.decide(event)      — pure
      · if a hotkey recording just finished → save_settings()
      · if pending_save (⌃⇧W decision)      → save_settings()
      · log the KEY line (BEFORE carrying out)
      · carry_out(event, decision)
  → TapState::decide                       [tap/decide.rs:176]
      1. recording_hotkey?          → capture_hotkey
      2. is_toggle_hotkey(preset)?  → session.toggle_mode(), hud, glyph, Consume
      3. is_app_toggle_hotkey (⌃⇧E)?→ ToggleApp
      4. is_correction_hotkey(⌃⇧W)? → session.correct_last_word() → Emit | Consume
      5. is_shortcut(⌘/⌃/⌥)?        → session.flush(), Passthrough
      6. !is_active && !macros_active → Passthrough
      7. keycode == 51 (Delete)     → see "Backspace" below
      8. is_caret_move (arrows/Home/End/Page) → session.flush(), Passthrough
      9. word char (letter | VNI digit | bracket-when-enabled)
             → session.process_key(ch) → Emit(response)      ← EVERY letter, incl. plain append
     10. any other char (boundary)
             → session.commit(); session.note_boundary(ch)
             → Some(restore) ? EmitThenReplayKey(restore) : Passthrough
     11. no char                    → Passthrough
  → carry_out                              [tap/decide.rs:72]
      Passthrough        → return the original event pointer
      Consume            → return null (suppressed)
      ToggleApp          → app_info::frontmost(), toggle, save, hud::flash, refresh_glyph, consume
      Emit(r)            → emit_edit(&r), consume
      EmitThenReplayKey(r) → emit_edit(&r), then post the boundary keycode+flags
                             from GlowKey's OWN source (down+up), consume
  → TapState::emit_edit                    [tap/emit.rs:72]
      · circuit_ok()  — >60 emits/1000 ms latches DISABLED for the run
      · omnibox guard: if r.backspaces>0 AND frontmost is Chromium AND
        ax::focused_text_field_has_selection() → post ⌦ (keycode 117) first
      · emit(): r.backspaces × (Delete down+up), then post_string(insert)
      · log "EMIT took=…µs"
  → CGEvent::post(SessionEventTap, …)      — one ordered FIFO, single tagged source
  → host application document
```

The **full-suppression model** is the load-bearing invariant (`docs/handoff.md` §5,
`tap/mod.rs` module docs). Every letter GlowKey handles is suppressed and re-emitted
from one tagged `CGEventSource` through one `CGEventPost` queue. Mixing native
passthrough with synthesized backspaces races in multiprocess apps (Chrome/Edge):
`aa`→`aâ`, `hoongf`→`hoồng`. The boundary key is part of it —
`Decision::EmitThenReplayKey` re-posts it rather than letting it through, because
passing it through natively lost the race (`ddc`␣→`đddc`, `work`␣→`ưwork`).

**Nothing in the keystroke path may block** (`docs/decisions/0008`). The callback sits
in the system's *synchronous* delivery path for every key on the machine; a
window-server round-trip there freezes the whole Mac and macOS logs
`TAP disabled by timeout`. So the keystroke path makes **zero** window-server calls:
frontmost app arrives by notification, with one bootstrap query on the first keystroke.
The single deliberate exception is the Chromium omnibox AX guard (50 ms cap, only on
transforming keystrokes in Chromium apps).

### Where Vietnamese composition / Telex logic lives

Entirely in `crates/glowkey-engine/src/lib.rs`, all inside `Engine` and the free
functions it calls. `Engine` keeps the **raw keystroke log** for the current word and
**re-derives the entire rendering on every keystroke** through the `vi` crate, then
returns a minimal `(backspaces, insert)` diff.

```rust
// lib.rs:1020
fn render(raw, style, method, quick_telex, telex_brackets) -> String
    → expand_quick_telex(raw)            if quick_telex && method.is_telex_family()
    → expand_telex_brackets(raw)         if telex_brackets && method == Telex
    → lowercase everything
    → vi::methods::IncrementalBuffer::new_with_style(definition, style.into())
         definition = &vi::TELEX | &vi::VNI | &SIMPLE_TELEX (our own phf map, lib.rs:896)
    → if out == lowered { return raw verbatim }   // English/mixed-case path: iPhone, macOS
    → apply_case(out, raw)                        // ALL-CAPS and Title-case exactly
```

Case is re-applied afterwards because `vi` mishandles whole-word uppercase
(`NGUYEENX` places the tone on the wrong vowel).

`diff(prev, next)` (`lib.rs:2080`) computes the edit: longest common prefix in whole
chars, delete the rest of `prev` counted in **UTF-16 code units**, insert the rest of
`next`. UTF-16 because that is `NSRange`/`NSTextInputClient`'s unit.

Validity, for auto-fix and the mid-word spell check:
- `is_invalid_vietnamese` (`lib.rs:2033`) — empty → valid; pure-ASCII → valid (typed
  verbatim); leading `đ`/`Đ` → valid (deliberate; keeps `đc`, `đt`, `đk`);
  otherwise `!vi::validation::is_valid_syllable(word) || violates_stop_coda_tone(word)`.
- `violates_stop_coda_tone` (`lib.rs:2065`) — our own rule the `vi` crate lacks: a
  syllable closed by `c`/`ch`/`p`/`t` can carry only sắc or nặng. UniKey's
  `lastWordIsNonVn` (`ukengine.cpp:2352`). Without it `left`→`lèt`, `soft`→`sòt`,
  `gift`→`gìt`, `lift`→`lìt` and auto-fix would not rescue them.

### Where state is stored

**In-memory, per process** — `Session` (`lib.rs:1111`) owns everything:

| Field | Meaning |
|---|---|
| `engine: Engine` | `style`, `method`, `raw: Vec<char>`, `rendered: String`, `quick_telex`, `telex_brackets`, `strict_spell_check`, `escaped: bool` |
| `mode: InputMode` | Vietnamese / English. **Session-only, never persisted** (an accidental ⌃⇧Space at quit used to make the app launch disabled). |
| `exclusions: ExclusionList` | `bundle_ids` + `removed_defaults` (persisted) + `session_removed` (not) |
| `current_bundle_id: Option<String>` | `None` → **fails closed**, nothing transforms |
| `committed: VecDeque<Behind>` | Re-composition stack, cap `COMMITTED_HISTORY = 5` |
| `correctable: Option<CorrectableWord>` | One-shot memory for ⌃⇧W |
| `word_overrides: HashMap<String, WordPreference>` | Index over the persisted `Vec<WordOverride>` |
| `macros: Vec<Macro>` | gõ tắt table |
| `pending_capital: bool` | Sentence-start flag for auto-capitalize |
| the rest | `auto_fix`, `auto_capitalize`, `restore_english_words`, `always_macro`, `toggle_hotkey`, `language`, `open_settings_at_launch`, `welcome_shown`, `style` |

**On disk** — `Settings` (`config.rs`) → `~/Library/Application Support/GlowKey/settings.json`.
17 fields, every one `#[serde(default)]`; `from_json` falls back to *full* defaults on
any parse error, which is why `WordOverride::prefer` has a `lenient_preference`
deserializer (one mistyped verdict used to discard the whole file, and the next UI
change wrote defaults over it). `save()` keeps one `.json.bak`.

`Session::from_settings` / `Session::snapshot` are the round trip. `Settings` and its
serialization live in the engine so they stay platform-free; the app crate owns only
the path and the I/O.

**Shell-side state** — `TapState` (`tap/mod.rs:96`): `RefCell<Session>`,
`last_bundle_id`, the tagged `CGEventSource`, `recent_emits` (circuit breaker),
`recording_hotkey`, `pending_save`, `last_key_at`. `TapContext` adds the tap port, the
run-loop source, the event mask, and the health counters. Both are `Box::into_raw`'d
and **leaked** for the process lifetime; the callback runs on the main run-loop thread,
so `RefCell`/`Cell` suffice (no cross-thread access). Every borrow uses `try_borrow`
and degrades to passthrough on failure.

### How word boundaries and Backspace are handled

**Boundary** — any char that is not a syllable char (`Engine::is_syllable_char`:
ASCII letter always; digit only in VNI; `[]{}"` only when `telex_brackets` and Telex).
The tap's `is_word_char` closure (`decide.rs:334`) mirrors it exactly. On a boundary:

1. `Session::commit()` (`lib.rs:1736`) — precedence, highest first:
   **macro** (exact case-insensitive match on raw keys) → **word override** →
   **auto-fix** (`is_invalid_vietnamese`) *or* **English restore** (opt-in) → nothing.
   The restore's `backspaces` is the rendered word's full UTF-16 length.
2. Record the step behind the caret: `Behind::Boundary` if nothing was composing,
   `Behind::Word{raw, rendered}` if composing and nothing was restored, else
   **clear the whole stack** (a restored word occupies screen space, so leaving it out
   would break the "unbroken account of the document" invariant).
3. Record `correctable` (whether or not restored — a restored word is the one users
   most want to argue with), with `boundary: None`.
4. `engine.reset()`.
5. Tap calls `Session::note_boundary(ch)` — sets `pending_capital` on `.!?`; if
   `ch.is_control()` calls `forget_position()` (Escape, function keys, keypad Enter,
   Help, ⌦, **Tab, Return** insert nothing at the caret, and Tab/Return move it
   outright); otherwise fills in `correctable.boundary`.

**Backspace** (`decide.rs:275`, keycode 51) — five cases, in order:

```
session.recompose_after_boundary_backspace()      [lib.rs:1913]
  composing?            → start_new_word(); NotApplicable → fall through
  pop_back == Word      → engine.restore(raw, rendered); Reopened        → Passthrough
  pop_back == Boundary  → BoundaryRemoved                                → Passthrough
  pop_back == None      → forget_position(); NotApplicable → fall through
session.backspace_visible_char()                  [lib.rs:1949 → 795]
  Repair(edit)  → Decision::Emit(edit)   // SUPPRESS the key, rewrite the whole word
  InStep        → Passthrough            // host performs the delete
  Flush         → session.flush(); Passthrough
```

`Engine::backspace_visible_char` is the crux: the host performs the delete, so the
engine must land on exactly what the screen will show — the render minus its last
character. It searches the raw log **from the end** for the one removal that
re-renders to that target. `hồng`⌫ → `hồn` means dropping raw `g` and *keeping* the
tone key `f`. Returns `Flush` when no single removal matches.

`Repair` exists for the spell-check escape: when the escape can be lifted
(`Engine::can_unescape`, the exact complement of the entry rule), the tap suppresses
the Backspace and emits one edit covering the whole on-screen word — letting the host
delete and then posting a repair would mix a native keystroke with a synthesized one,
the race the whole design removes.

**Deletes are visible-character deletes, not keystroke-undo.** Questioned twice in
live use and reaffirmed both times (`docs/handoff.md` §4). `hoongf` `a` ⌫⌫ `z` gives
`hôn`, not `hông`. The two only diverge at a tone key, which is the one place a
keystroke produces no character of its own.

**The older `Engine::backspace`** (`lib.rs:755`) pops the last raw *key*
(`hồng`→`hông`). Wrong for this path and **unused by the app** — only
`tests/telex.rs:111` calls it. Dead code in practice.

**Everything that can move the caret unseen calls `flush()`**: caret keys, mouse-down,
⌘/⌃/⌥ shortcuts, app switch, mode/exclusion toggles, input-method change, style
change, the three render options, tap death/disable, and deleting back past the stack.
`Session::flush` = `engine.reset()` + `forget_position()` + `pending_capital = false`.
`forget_position()` clears `committed` **and** `correctable` together, deliberately —
a `correctable` surviving a caret move would let one keystroke rewrite text elsewhere.

### How text is inserted / replaced in applications

No marked text, no `NSTextInputClient`, no host-text read-back. The engine emits
`KeyResponse { handled, backspaces: usize /* UTF-16 */, insert: String /* NFC */ }`
and `tap/emit.rs` renders it as synthetic events:

- `backspaces` × `CGEvent::new_keyboard_event(source, 51, down/up)` → `CGEvent::post`
- `insert` → one key-down carrying `CGEvent::keyboard_set_unicode_string` (keycode 0)
  plus a matching key-up to keep the pair balanced
- all from the one `CGEventSourceStateID::Private` source tagged
  `user_data = 0x474C4F57` ("GLOW"), posted at `CGEventTapLocation::SessionEventTap`

Session-level posting is required so multiprocess apps (Chrome's text field lives in a
renderer) route them to the focused element. The tap recognizes its own output by
reading the event's source user-data (`is_own_event`) and skips it, which is what
prevents a feedback loop; a latching circuit breaker caps a runaway at 60 emits/second.

### macOS-specific APIs / frameworks involved

| Framework / API | Where | Purpose |
|---|---|---|
| `CGEventTapCreate` / `CGEvent::tap_enable` / `CGEventTapIsEnabled` | `tap/health.rs` | Install, re-enable, poll the tap |
| `CGEventSource` (Private state, `set_user_data`, `user_data`) | `tap/mod.rs`, `tap/emit.rs` | Tagged source + self-identification |
| `CGEvent::new_keyboard_event`, `set_flags`, `post`, `keyboard_set_unicode_string`, `keyboard_get_unicode_string`, `integer_value_field`, `flags`, `new_source_from_event` | `tap/emit.rs`, `tap/keys.rs` | Emit and read events |
| `CGEventFlags`, `CGEventField`, `CGEventType`, `CGEventTapLocation/Options/Placement` | `tap/*` | Event model |
| `CFRunLoop`, `CFRunLoopSource`, `CFMachPort`, `kCFRunLoopCommonModes`, `CFRunLoopTimer` | `tap/mod.rs`, `tap/health.rs` | Run loop + 2 s health timer |
| `AXIsProcessTrusted(WithOptions)` | `tap/permission.rs` | The Accessibility gate |
| `AXUIElementCreateSystemWide`, `AXUIElementCopyAttributeValue`, `AXUIElementSetMessagingTimeout`, `CFString*`, `CFEqual`, `CFRelease` (raw `#[link(name="ApplicationServices")] extern "C"`) | `app/src/ax.rs` | Omnibox guard: `AXFocusedUIElement` → `AXRole`/`AXSelectedText` |
| `NSWorkspace` (`frontmostApplication`, `NSWorkspaceDidActivateApplicationNotification`, `activateFileViewerSelectingURLs`, `openURL`) | `app_info.rs`, `menu_bar.rs`, `tap/emit.rs`, `tap/permission.rs` | Frontmost app; reveal log; open System Settings |
| `NSStatusBar` / `NSStatusItem` / `NSMenu` / `NSMenuItem` | `menu_bar.rs` | Menu bar |
| `NSApplication`, `NSAlert` (+ modal *session*, and `layout()` before `window()`), `NSWindow`, `NSPanel`, `NSTabView`, `NSStackView`, `NSTextField`, `NSSegmentedControl`, `NSButton`, `NSLayoutConstraint`, `NSOpenPanel`/`NSSavePanel`, `NSFont`, `NSColor` | `prefs/*`, `about_window.rs`, `welcome.rs`, `hud.rs`, `tap/permission.rs` | All UI |
| `NSPasteboard` | `menu_bar.rs` | Clipboard tools (remove tones / UPPER / lower) |
| `SMAppService` (macOS 13+) | `login_item.rs` | Launch at login |
| `NSLocale::preferredLanguages` | `strings.rs` | Resolve `Language::System` |
| `NSBundle::mainBundle().bundleIdentifier()` | `tap/emit.rs` | Own bundle id |
| `objc2::define_class!`, `MainThreadMarker`, `Retained`, `CFRetained` | `menu_bar.rs`, `prefs/mod.rs`, `hud.rs` | Objective-C class definition and memory management |

---

## 3. Engine vs platform coupling

**This is the single best thing about the codebase for a port: the split already exists
and is enforced by CI.** `.github/workflows/ci.yml` runs `cargo fmt`, `cargo clippy -D
warnings` and `cargo test` for `-p glowkey-engine` **on `ubuntu-latest`**, explicitly
"to guard against macOS-specific code leaking into the engine crate."

### Truly platform-independent (reusable verbatim)

All of `crates/glowkey-engine` — 2 851 lines, 156 of the 190 tests. Its complete
dependency tree is `vi 0.8` (→ `log`, `nom`, `phf`, `smallvec`), `serde`, `serde_json`,
`phf`; dev-only `proptest`, `criterion`. **Zero platform crates, zero `cfg(target_os)`,
zero `unsafe`.** Concretely:

- All Telex/VNI/Simple-Telex transformation, Quick Telex, bracket shortcuts
- The raw-log + re-derive model, `diff`, `apply_case`, `remove_tones`
- Auto-fix, the stop-coda tone rule, the English word list, per-word overrides
- The mid-word spell check and its escape/unescape symmetry
- The committed-word history and re-composition
- `backspace_visible_char` and the three-way `BackspaceOutcome`
- Macro table parse/format/import (including UniKey/EVKey line format and its BOM +
  version header)
- `Settings` + all its serde tolerance; `ExclusionList` + tombstones + session suspension
- Auto-capitalize, `note_boundary`, the correction hotkey logic
- Every enum the UI binds to: `InputMethod`, `PlacementStyle`, `Language`, `InputMode`,
  `HotkeyPreset`, `WordPreference`, `ExclusionToggle`

### macOS-specific

All of `app/` (4 274 lines). Divided by *how* macOS-specific it is:

1. **Irreducibly macOS** — `tap/mod.rs`, `tap/emit.rs`, `tap/health.rs`,
   `tap/permission.rs`, `ax.rs`, `login_item.rs`, `app_info.rs`. Event tap creation,
   synthetic event posting, run-loop integration, the Accessibility permission model,
   the AX read-back, `SMAppService`.
2. **Platform-shaped but logically portable** — `tap/decide.rs` and `tap/keys.rs`.
   The *ordering and policy* in `decide` (hotkeys before shortcut filter, the five-case
   Backspace ladder, boundary handling, full suppression) is a platform-neutral state
   machine; only its inputs (`CGEvent`, `CGEventFlags`, macOS virtual key codes) and
   outputs (`CGEventPost`) are macOS. Likewise `keys.rs` is pure logic over macOS key
   codes and flag masks.
3. **Thin plumbing** — `settings_store.rs` (path + atomic write), `log.rs` (path +
   rotation), `strings.rs` (only `NSLocale` is macOS; `t()` is portable),
   `tap/settings.rs` (43 methods of `borrow → mutate → snapshot → save`).
4. **UI** — `menu_bar.rs`, all of `prefs/`, `about_window.rs`, `welcome.rs`, `hud.rs`.
   Pure AppKit; no logic worth extracting beyond the field list each pane binds to.

### What would need to be extracted for a cross-platform engine

Almost nothing from the engine — it is already there. The extraction work is in
`app/`, and it is about lifting the platform-neutral *policy* out of `decide.rs`:

- **The decision state machine.** `Decision` (5 variants) and the ordered ladder in
  `TapState::decide` are shared behaviour expressed over macOS types. Extracting it
  needs a platform-neutral key event (character, a semantic key identity for
  Backspace/arrows/Escape/Space/E/W/Z, and four modifier booleans) plus a
  platform-neutral emit sink.
- **Key identity.** `keys.rs` hard-codes macOS virtual key codes (51 Delete, 117
  forward-delete, 53 Escape, 49 Space, 14 E, 13 W, 6 Z, 123–126/115/116/119/121 caret
  moves). These need to become an enum with per-platform mapping tables.
- **`HotkeyPreset::Custom.keycode: i64`** — documented as "the macOS virtual key code
  the tap matches on", and it is **persisted in settings.json**. This is the one place
  a platform value has already leaked into the engine's public, serialized surface.
- **Settings path + log path** — trivial, one function each.
- **`Settings` field ↔ UI binding lists** — the four panes and the `*_and_save` wall
  enumerate every option; a second UI re-enumerates them.
- **The exclusion identity model.** `ExclusionList` is keyed on strings and is
  therefore already portable, but the *values* are macOS bundle identifiers
  (`DEFAULT_EXCLUSIONS`, `TERMINAL_EXCLUSIONS`, `CHROMIUM_BUNDLE_PREFIXES`). The type
  is fine; the 14 + 9 + 7 constants are macOS data.

### Swift / macOS types currently leaking into core logic

There is **no Swift anywhere** in this project (decision 0001: all-Rust via `objc2`).
Leaks of macOS *concepts* into the engine, exhaustively:

| Leak | Location | Severity |
|---|---|---|
| `HotkeyPreset::Custom { keycode: i64, key_char: char }` — a macOS virtual key code, serialized to disk | `lib.rs:157`, `config.rs:40` | **Real.** The only platform value in the engine's persisted API. A Windows build reading the same file would match the wrong key. |
| `backspaces` counted in **UTF-16 code units** ("the unit `NSRange` and `NSTextInputClient` use") | `lib.rs:511`, and every `encode_utf16().count()` | Cosmetic in naming only — UTF-16 is also the right unit for Win32 `WM_CHAR`/`SendInput`, so this is a lucky alignment rather than a leak to fix. |
| Bundle-identifier strings as the app-identity key | `exclusion.rs:165,185`, `emit.rs:28` | Type is portable; the data is macOS. Windows identifies apps by executable path / AUMID. |
| Doc comments referencing `NSRange`, `NSTextInputClient`, InputMethodKit, "the shell" | throughout `lib.rs` | Cosmetic. |
| `PlacementStyle` mirrors `vi`'s `AccentStyle` deliberately "to keep `vi` out of the shell's type surface" | `lib.rs:36,115` | Not a leak — this is the pattern working as intended. |

Nothing else. No `objc2` type, no `cfg(target_os)`, no `unsafe` reaches the engine.

---

## 4. Current behavior

### Telex / VNI features

- **Order-independent tone marks**: `hoongf` / `hofong` / `hoonfg` all → `hồng`.
  Immediate diacritics: `oo`→`ô`.
- **Three input methods** (`InputMethod`): `Telex` (default), `Vni` (`viet65`→`việt`;
  digits extend the word), `SimpleTelex` (UniKey's `UkSimpleTelex` — `w` only ever adds
  a horn to `u`/`o` or a breve to `a`, never stands alone as `ư`; our own 11-entry
  `phf` `Definition` at `lib.rs:896`).
- **Placement style**: `New` (default, `hoà`/`thuý`) or `Old` (`hòa`/`thúy`).
- **Quick Telex** (opt-in, Telex family only): doubled consonant at the **syllable
  start** expands to its digraph — `cc`→`ch`, `gg`→`gi`, `kk`→`kh`, `nn`→`ng`,
  `pp`→`ph`, `qq`→`qu`, `tt`→`th`, `uu`→`uw`(→`ư`). Syllable-initial only, which keeps
  `letter`/`happy`/`accept` untouched. Case follows the trigger: both shifted → all
  caps (`CCAO`→`CHAO`), one shifted → Title (`Ccao`→`Chao`). Telex-only because the
  expansions are Telex *keys*.
- **Telex bracket shortcuts** (opt-in, Telex only): `[`→ơ, `]`→ư, `{`→Ơ, `}`→Ư. Each
  bracket is rewritten to the Telex *keys* (`[`→`ow`) so a tone key after it still
  lands (`[f`→`ờ`); a precomposed `ơ` would leave `vi` unable to modify it. Applied
  *after* Quick Telex. Turning it on stops `[`/`]` reaching the app at all.
- **Case handling**: if `vi` applied no transformation the raw keys are emitted
  verbatim, so `iPhone`/`JavaScript`/`macOS` keep exact case. Transformed words get
  ALL-CAPS or Title-case exactly; other interior case is best-effort.
- **Auto-capitalize** (opt-in): first letter of each sentence, primed by `.`/`!`/`?`,
  consumed by the next word's first letter. Handles a word starting with a bracket
  (`[`→`{`).
- **Clipboard tools** (menu, UniKey's "Công cụ"): remove tones (`remove_tones`),
  UPPERCASE, lowercase. They act on the clipboard, not a selection — a background
  agent has no selection.
- **Deliberately omitted**: legacy encodings (TCVN3, VNI-Windows), VIQR, clipboard
  encoding conversion. Every modern macOS app is Unicode NFC.

### Spell checking / invalid-word recovery

Two independent mechanisms, as in UniKey (`autoNonVnRestore` vs `spellCheckEnabled`):

1. **Auto-fix** (`auto_fix`, **on by default**) — repairs **at the boundary**. If the
   render is invalid Vietnamese, restore the raw keys: `eĩt`→`exit`. Validity is
   `vi::validation::is_valid_syllable` **plus** our stop-coda tone rule. Exemptions:
   pure-ASCII renders and words *starting* with `đ` (`đc`/`đt`/`đk` survive; `address`,
   `odd`, `sudden` still restore because their `đ` is not leading).
2. **Mid-word spell check** (`strict_spell_check`, **off by default**) — repairs **at
   the keystroke**. When a render turns non-ASCII and fails validity, the word is
   **escaped**: it renders its raw keys verbatim until the next boundary. Escaping the
   whole word (not the one key) is forced by the re-derive design — a dropped key
   would be re-applied by the next one. Judged on the **render**, never the raw keys
   (raw `nguow` is not a syllable but its render `ngươ` is a normal step of `người`).
   `tests/midword_spell_check.rs` carries a 51-word corpus asserting identical output
   with the option on and off.
   **The escape is reversible** (2026-09-04): a Backspace that leaves something
   spellable brings the transformation back, via `BackspaceOutcome::Repair`. Exit asks
   the same question entry did (`can_unescape` is written as the exact complement of
   `last_key_made_it_impossible`) so the two cannot drift.
   **No repeat-key carve-out** — it existed (`hoongff`→`hôngf` was exempt) and was
   removed 2026-09-04 at the owner's direction from live use. The rejection gesture
   itself still works with the check off, which is the default.

### Word history

`Session::committed: VecDeque<Behind>`, cap 5 (`COMMITTED_HISTORY`).

- `Behind::Word(CommittedWord { raw, rendered })` — a word and the boundary that
  committed it. `Behind::Boundary` — a boundary with no word before it (the `␣` of
  `hồng, `; `, ` and `. ` are the commonest pairs in prose, and having no entry for
  them is what left the original bug one comma away).
- **The stack order *is* the caret position.** The document is
  `[entry₁][entry₂]…[composing]`, every entry accounts for exactly one boundary
  character on screen, so what the caret stands behind is always the top — which is
  why no offset is stored. Overflow drops from the **front**, so what remains is still
  an unbroken run ending at the caret.
- Push on commit; pop on a Backspace with nothing composing; **clear entirely** on
  anything that can move the caret unseen. Two limits are deliberate and test-pinned:
  an auto-fix-restored word **clears** the stack rather than merely staying out of it
  (it occupies screen space), and a mid-word Backspace the engine cannot follow flushes.
- Deleting back past the stack calls `forget_position()` **in the engine** rather than
  trusting the caller, so ⌃⇧W cannot post an edit that puts back a boundary character
  the Backspace just removed.

`correctable: Option<CorrectableWord { raw, rendered, on_screen, boundary }>` is a
**separate, one-shot** memory for ⌃⇧W. It is set *whether or not* auto-fix restored the
word (the opposite of `committed`), because a restored word is the one users argue
with. `on_screen` is stored rather than derived, so the backspace count can never
disagree with the screen.

### Backspace / undo behavior

Covered in §2. Summary of user-visible contracts:

| Sequence | Result | Why |
|---|---|---|
| `hoongf`⌫`z` | `hôn` | Mid-word: shrink one **visible** char, stay composed, `z` is still a Telex key |
| `hồng`␣⌫`z` | `hông` | Boundary delete re-opens the committed word |
| `hồng`␣`s`⌫⌫`z` | `hông` | Survives typing in between (the 5-entry stack) |
| `hồng,`␣⌫⌫`z` | `hông` | `Behind::Boundary` handles the double boundary |
| `hoongf``a`⌫ | `hồng` (still composing) | `Repair`: escape lifted, whole word rewritten in one edit |
| `hoongf``a`⌫⌫`z` | `hôn` | Visible-char deletes, **not** keystroke-undo. Reaffirmed twice. |
| ⌫ past the stack | flush | Engine stops vouching for the caret |

### English-word detection

Three layers, in precedence order (highest wins), all resolved in `Session::commit`:

1. **Macro** — an exact case-insensitive match on the raw keys beats everything.
2. **Per-word override** (`Settings.word_overrides` → `word_overrides: HashMap`) —
   pins a word to `Raw` or `Vietnamese`. Beats auto-fix, the curated list, and the
   global switch **in both directions**, so `was`→`was` and `cats`→`cát` hold at the
   same time, which no setting of the global switch can achieve. Keyed on lowercased
   raw keys. Learned in one keystroke with **⌃⇧W** (`correct_last_word`), managed in
   Settings → Corrections → Personal Words.
3. **Rules** — auto-fix (invalid render) OR **"Restore common English words"**
   (`restore_english_words`, opt-in, off by default): a committed word whose raw keys
   match `english.rs`'s curated static list is restored even when the render is *valid*
   Vietnamese (`was`→`was`, not `ứa`).

Plus the implicit layer: `render` returns raw keys verbatim whenever `vi` applied no
transformation, which is the common case for English.

**The ambiguity is inherent and unresolved in principle** (`docs/handoff.md` §6.3): the
same keystrokes are legitimate Vietnamese and legitimate English. The global switch's
cost, with it ON, is that these become untypeable in that key order:
á→`as`, í→`is`, ú→`us`, ò→`of`, ỏ→`or`, mã→`max`, sĩ→`six`, thú→`thus`, cả→`car`,
hải→`hair`, tả→`tar`, cát→`cats`, sét→`sets`. That is why it ships off.

Three things about ⌃⇧W are load-bearing and each shipped as a bug once:
it forgets the word afterwards (`forget_position()` — otherwise the next Backspace
re-composes the *old* rendering and the letter after that eats a character:
`was `⌃⇧W⌫`f` produced `wừa`); it refuses when the boundary key inserted nothing at
the caret; and the decision is written to disk from `handle_key_down` via
`pending_save`, because `decide` is deliberately free of disk side effects (otherwise
every taught word was lost at quit).

### Special handling for Chrome, Spotlight, Terminal, etc.

- **Chromium browsers** (`CHROMIUM_BUNDLE_PREFIXES`: Chrome, Edge, Chromium, Brave,
  Vivaldi, Opera, Arc). The omnibox's inline autocomplete keeps a **trailing
  selection**, which the first synthetic Backspace deletes instead of a character
  (`hoongf`→`hoồng`). Guard, at `emit.rs:97`: when an edit with `backspaces > 0` is
  about to land in a Chromium app **and** `ax::focused_text_field_has_selection()`
  (focused element is `AXRole == AXTextField` with non-empty `AXSelectedText`), post one
  forward-delete (⌦) first. Normal fields (empty selection) and non-text-field surfaces
  (web content, contenteditable) are untouched; ⌦ is a no-op at end-of-text, GlowKey's
  normal caret position. 2–3 AX IPC round-trips, 50 ms cap, typical sub-ms.
  Logs "OMNIBOX trailing selection detected" per fire and "AX guard unavailable" once
  per run for a dead guard. **Best-effort, not a proof** — see §8.
- **Terminals** (`TERMINAL_EXCLUSIONS`, 9 ids: Apple Terminal, iTerm2, Warp
  Stable/Preview, kitty, WezTerm, Ghostty, Alacritty, Hyper). A PTY ignores synthetic
  backspaces, so Vietnamese in a terminal always produces garbage. All 9 are also
  shipped defaults (test-asserted). ⌃⇧E in a known terminal un-excludes it **for the
  session only** (`suspend_for_session`; HUD shows "VI ⚠"); a restart re-excludes.
  Permanent removal only via the Excluded Apps window.
- **Editors**, shipped-excluded by default: Xcode, VS Code, IntelliJ, PyCharm,
  WebStorm. (14 `DEFAULT_EXCLUSIONS` total = the 9 terminals + these 5.)
- **Exclusion tombstones**: `removed_default_exclusions` in settings. At load the
  effective list is `saved ∪ (defaults − tombstones)`, so a new release's defaults
  reach old settings files without resurrecting deliberate removals. A tombstone
  survives a remove/add pair (test-pinned).
- **Spotlight**: no special handling. Not in the default exclusions and not named
  anywhere in the source.
- **Password / secure input fields**: macOS withholds those events from all event taps.
  Inherent to the architecture; nothing to handle.
- **Fails closed on an unknown app**: `Session::is_active()` returns `false` while
  `current_bundle_id` is `None`. For a tool whose primary feature is *not* transforming
  in excluded apps, an unknown app must not transform.

### Configuration / options

Persisted (`~/Library/Application Support/GlowKey/settings.json`, 17 fields):

| Field | Default | Surface |
|---|---|---|
| `exclusions` | 14 defaults | Excluded Apps window; ⌃⇧E; menu |
| `removed_default_exclusions` | `[]` | (internal tombstones) |
| `auto_fix` | `true` | menu + Typing pane |
| `style` | `New` | Typing pane |
| `input_method` | `Telex` | Typing pane |
| `auto_capitalize` | `false` | Typing pane |
| `toggle_hotkey` | `CtrlShiftSpace` | General pane, 5-segment picker |
| `macros` | `[]` | Macros window (+ import/export) |
| `restore_english_words` | `false` | Typing pane |
| `open_settings_at_launch` | `true` | General pane |
| `language` | `System` | General pane (System / Tiếng Việt / English) |
| `quick_telex` | `false` | Typing pane |
| `telex_brackets` | `false` | Typing pane |
| `strict_spell_check` | `false` | Typing pane |
| `always_macro` | `false` | Apps & macros pane |
| `welcome_shown` | `false` | (internal) |
| `word_overrides` | `[]` | Personal Words window; ⌃⇧W |

**Not persisted**: `InputMode` (VN/EN). Always launches Vietnamese — persisting it let
one accidental ⌃⇧Space at quit make the app launch disabled.

Hotkeys: **⌃⇧Space** (VN/EN, configurable — presets ⌃⇧Space / ⌃Space / ⌥Space / ⌃⇧Z
plus a **recorder** for any ⌃/⌥ combo); **⌃⇧E** (per-app toggle, fixed);
**⌃⇧W** (correct last word, fixed). The recorder rejects ⌃⇧E and ⌃⇧W, never captures a
⌘ combo, and is cancelled by Escape / any mouse click / an app switch — while armed,
plain typing and every ⌘ shortcut pass straight through, so a forgotten recorder cannot
lock the keyboard.

Other: launch at login (`SMAppService`), macro import/export in EVKey's
`shortcut:expansion` line format (UniKey BOM + `version=N` header recognized; a
non-1 version means a VIQR body and is **refused** with an explanation), reset input,
reveal log in Finder, About, Quick Guide.

Environment: `GLOWKEY_DEBUG=1` echoes emits to stderr.

---

## 5. Tests

### Organization

- **Engine unit tests** (`#[cfg(test)] mod tests` in `config.rs`, `exclusion.rs`,
  `english.rs`) — 23. Settings JSON tolerance, exclusion semantics, word-list hygiene.
- **Engine integration tests** (`crates/glowkey-engine/tests/`, 13 files) — 133. One
  file per feature. All drive `Session` through its public API and model the screen as
  a `String` the emitted edits are applied to.
- **Tap decision tests** (`app/src/tap/tests.rs`) — 34. Construct **real `CGEvent`s**
  and call `TapState::decide` / `handle_key_down`. Runs headless with no Accessibility
  grant, because `decide` is pure (no event synthesis, no workspace query, no disk).
- **Benchmark** (`benches/keystroke.rs`, criterion, `harness = false`).
- **Manual script** (`docs/manual-verification.md`) for everything no test can reach.

Run: `cargo test --workspace` (190 tests), `just test-hard`
(`PROPTEST_CASES=60000 cargo test -p glowkey-engine --release --test properties`),
`cargo clippy --workspace --all-targets` (must be silent — a warning is a failure),
`cargo bench -p glowkey-engine`.

### Test files and counts

| File | Tests | Covers |
|---|---|---|
| `tests/properties.rs` | 6 | **The invariant suite.** See below. |
| `tests/word_overrides.rs` | 24 | Per-word overrides + ⌃⇧W correction |
| `tests/auto_fix.rs` | 17 | Auto-fix, English restore, auto-capitalize, macros, the `đ` carve-out, stop-coda |
| `tests/session.rs` | 17 | Mode/exclusion precedence, focus change, terminal session-only, history |
| `tests/macro_table.rs` | 16 | EVKey/UniKey line format, JSON fallback, import merge, VIQR refusal, always-macro |
| `tests/telex.rs` | 14 | Tone orders, case, boundaries, backspace, old/new style, VNI |
| `tests/midword_spell_check.rs` | 13 | 51-word corpus, escape/unescape, off-by-default byte-identity |
| `tests/telex_brackets.rs` | 9 | Brackets, tones after, case, VNI exclusion, Quick Telex composition |
| `tests/quick_telex.rs` | 7 | Digraphs, case, VNI exclusion, English inner doubles |
| `tests/simple_telex.rs` | 5 | `w` semantics, parity with Telex, bracket exclusion |
| `tests/remove_tones.rs` | 4 | All vowel families, `đ`, case, five tones |
| `tests/latency.rs` | 1 | 250 µs/keystroke ceiling |
| `app/src/tap/tests.rs` | 34 | The decision function against real `CGEvent`s |

### Important existing test cases

**`tests/properties.rs`** is the load-bearing suite. It models the host document as
`Screen { committed: String, tail: String }` — split exactly where the engine's
knowledge ends — and applies each edit as `emit_edit` does (delete N **UTF-16 code
units** from the end, then insert). `Screen::apply` returns `Err` when an edit would
delete **past the start of the word**, which in the real app means eating a character
of the user's existing document. The model mirrors `tap.rs::decide` deliberately
("a property that modelled a path the tap never takes would prove nothing"), including
markers for Backspace (`\u{8}`) and the correction hotkey (`\u{17}`).

Properties: `the_diff_always_reproduces_the_render`, `no_ascii_input_panics`,
`mid_word_backspace_lands_exactly_one_character_back`,
`every_render_option_combination_holds_for_real_words`,
`the_correction_never_reaches_past_the_word_it_owns`,
`committing_twice_emits_nothing_the_second_time`. Failures persist to
`proptest-regressions/`, committed so a CI failure reproduces locally.

**`tests/latency.rs`** — 10 000 keystrokes of `hoongf` with a commit each boundary,
asserting ≤ **250 µs/keystroke**. Measured 2026-09-03 on Apple Silicon: **2 µs**
release, **9 µs** in the unoptimized profile CI runs. The ceiling is ~28× the debug
figure and ~125× release: deliberately loose, to catch an order-of-magnitude regression
(a dictionary lookup or regex compile entering the per-key path) without flaking on a
shared runner.

**`app/src/tap/tests.rs`** — notable named cases:
`auto_fix_boundary_replays_the_key_rather_than_passing_it_through`,
`backspace_that_unescapes_a_word_emits_instead_of_passing_through`,
`reported_delete_sequences_land_where_they_should`,
`deleting_back_through_two_words_reopens_the_right_one`,
`a_second_boundary_in_a_row_keeps_the_chain`, `the_history_cap_is_five_entries`,
`a_restored_word_breaks_the_chain`, `a_caret_move_clears_the_whole_history`,
`losing_track_mid_word_ends_the_chain`, `terminal_toggle_via_hotkey_is_session_only`,
`hotkey_recording_escape_cancels`, `chromium_browsers_are_classified_by_prefix`.

### Edge cases already covered

- Empty / corrupt / partial / unknown-key settings JSON → defaults, never a crash
- A malformed `word_overrides` entry must not discard the rest of the file
- Legacy `default_mode` key ignored
- Tombstone survival through remove/add pairs; defaults merging into old files
- Mid-word exclusion change must not corrupt the document
- Cross-field leak prevention (`reset`)
- Interior capitals surviving when no transformation occurred
- Caps-lock vs Title-case for Quick Telex and bracket injection
- An expansion containing a colon; a shortcut containing a colon (→ JSON fallback);
  trailing spaces preserved (UniKey does not trim); duplicate-inside-file counted as
  skipped; import never overwrites
- Boundary keys that insert nothing (Escape, Tab, Return, ⌦, keypad Enter, function
  keys) leaving nothing to correct
- `escape_after_a_word_cannot_eat_the_preceding_space`
- The correction never deleting more than the word and its boundary
- Committing twice emitting nothing the second time
- Off-by-default options asserted **byte-identical** to the option being absent

### Known failing / fragile cases

**No test currently fails.** The fragility is in what tests cannot reach:

- **All GUI is unverifiable headless** (`docs/handoff.md` §6.4). Every Settings
  control, the HUD variants ("VI ⚠"), the hotkey recorder's on-screen state, the
  permission alert, the welcome panel.
- **`docs/manual-verification.md` has never been run end to end.** Treat unticked
  sections as unverified. This is the project's own stated assessment.
- **The omnibox guard has no test that proves it in Chrome** — only
  `chromium_browsers_are_classified_by_prefix` (the prefix match). The AX read itself
  and the ⌦ timing are unverified on screen.
- **The default 4 096 proptest cases have twice passed over a real corruption**
  (justfile comment on `test-hard`). The suite is trustworthy only at
  `PROPTEST_CASES=60000` for changes to the diff or restore paths.
- **`EMIT took=` in a live Chromium window vs a plain field is still unmeasured.**
  §6.1's "typ. sub-ms" for the AX guard is an estimate, not a measurement.
- **Tap-death recovery is unverified live**: on some macOS versions revoking the grant
  terminates the process outright, which would make the recovery path unreachable.

---

## 6. Dependencies

### Engine (`crates/glowkey-engine`) — all platform-free

| Crate | Version | Role | Note |
|---|---|---|---|
| `vi` | 0.8 | Vietnamese transformation (`TELEX`, `VNI`, `IncrementalBuffer`, `validation::is_valid_syllable`, `AccentStyle`, `Action`/`ToneMark`/`LetterModification`, `methods::Definition`) | MIT (LICENSE file is verbatim MIT; crates.io shows "non-standard" only because it declares `license-file`). Notice must ship — see `THIRD-PARTY-NOTICES.md`. Requires Rust 1.96+. |
| `serde` | 1 (`derive`) | `Settings`, `Macro`, `WordOverride`, all enums | |
| `serde_json` | 1 | Settings + macro-table JSON | |
| `phf` | 0.11 (`macros`) | `SIMPLE_TELEX` definition | **Pinned to 0.11 to match `vi`'s** — `vi` takes any `Definition`, which is a `phf::Map`. |
| `proptest` | 1 | dev-only, `tests/properties.rs` | |
| `criterion` | 0.5 (`default-features=false`, `cargo_bench_support`) | dev-only, `benches/keystroke.rs`, `harness = false` | |

Transitive: `log`, `nom 8`, `smallvec` (via `vi`); `siphasher`; `itoa`, `memchr`, `zmij`.
**No platform crate anywhere in the engine's tree.**

### macOS-only (`app/`, all under `[target.'cfg(target_os = "macos")'.dependencies]`)

`objc2 0.6`; `objc2-foundation 0.3`; `objc2-app-kit 0.3` (large feature list — 40+
classes); `objc2-core-graphics 0.3` (`CGEvent`, `CGEventTypes`, `CGEventSource`,
`CGError`); `objc2-core-foundation 0.3` (`CFRunLoop`, `CFMachPort`, `CFBase`,
`CFDictionary`, `CFNumber`, `CFDate`); `objc2-service-management 0.3`.

Plus a raw `#[link(name = "ApplicationServices", kind = "framework")] extern "C"` block
in `app/src/ax.rs` for the six AX/CF functions `objc2` does not wrap.

`app/Cargo.toml` declares `glowkey-engine` unconditionally, so the workspace builds on
non-macOS with a stub `main`.

### Build tools and versions

- **Rust 1.98.0**, pinned in `rust-toolchain.toml` with `rustfmt` + `clippy`
  ("pin an explicit version so rustup auto-installs it without disturbing the machine's
  global default").
- **Release profile** (workspace root): `codegen-units = 1`, `lto = true`,
  `opt-level = "s"`, `strip = true` — "an input method sits on the keystroke hot path,
  so favour a lean binary."
- **`just`** — the task front door; `scripts/` holds the shell implementation
  ("assembling a macOS bundle is genuinely shell work — lipo, codesign, PlistBuddy,
  hdiutil").
- **Apple toolchain**: `lipo` (universal binary), `codesign`, `PlistBuddy`, `hdiutil`,
  `xattr`, `otool` (CI privacy guard), `iconutil` (`make-icon.sh`).
- **macOS 13+** (`LSMinimumSystemVersion`; `SMAppService` requires it).
- **Signing**: self-signed "GlowKey Developer" identity when present, ad-hoc otherwise.
  **Not notarized** (no paid Apple Developer account, by choice) — a downloaded copy
  needs `xattr -dr com.apple.quarantine` once.
- **CI** (`ci.yml`): `ubuntu-latest` job = fmt + clippy `-D warnings` + test for the
  engine alone (the cross-platform guard); `macos-latest` job = clippy + build + test
  for the workspace, then a **privacy guard** that fails the build if the binary links
  `Network.framework`, `CFNetwork` or `libcurl`.
- **Release** (`release.yml`): a `v*` tag must match `app/Cargo.toml`'s version, then
  builds the universal app and attaches a `.dmg`.

---

## 7. Windows port assessment

### What can be reused directly

**The entire `glowkey-engine` crate — 2 851 lines and 156 of 190 tests — compiles and
passes on Windows unchanged.** This is not an estimate: CI already proves it on Linux
with `-D warnings`, its dependency tree contains no platform crate, and it has no
`unsafe` and no `cfg(target_os)`. That is the whole of the Vietnamese behaviour: three
input methods, Quick Telex, brackets, auto-fix + the stop-coda rule, the mid-word spell
check and its reversible escape, the committed-word history, `backspace_visible_char`,
per-word overrides, the English list, macros with UniKey/EVKey table compatibility,
`Settings` with its serde tolerance, `ExclusionList` with tombstones and session
suspension, auto-capitalize, `remove_tones`.

Also directly reusable, with only the path changed: `settings_store.rs` (61 lines) and
`log.rs`'s rotation logic; and `strings.rs`'s `t()` pattern (only
`system_prefers_vietnamese` is macOS).

### What should become a shared engine

Three things currently in `app/` are platform-neutral *policy* wearing macOS clothes.
A Windows shell that re-implements them from scratch will re-introduce bugs this
codebase has already paid for:

1. **The decision ladder** — `TapState::decide` (`decide.rs:176`). The ordering is
   load-bearing and every step of it is a fixed bug: hotkeys before the shortcut
   filter (a flush destroys the memory ⌃⇧W needs); the five-case Backspace ladder with
   its **exhaustive, catch-all-free** matches (a `bool` hiding the difference between
   `BoundaryRemoved` and "nothing remembered" is what made `hoongf, ⌫⌫z` produce
   `hồngz`); full suppression of every letter including plain appends; the boundary key
   replayed rather than passed through. Its inputs and outputs are macOS; its logic is
   not.
2. **Key identity** — `keys.rs`'s pure predicates (`is_caret_move`, `is_shortcut`,
   `is_ctrl_shift`, `is_toggle_hotkey`, `is_app_toggle_hotkey`,
   `is_correction_hotkey`) over a set of key codes and four modifier booleans.
3. **`HotkeyPreset`** — already in the engine, but `Custom.keycode: i64` is a macOS
   virtual key code and is **persisted**. It needs a platform-neutral key identity
   before two platforms can share a settings file, or an explicit decision that they
   do not.

The natural extraction is a neutral input event (character + semantic key + four
modifier flags) and a neutral emit sink, with the ladder written once over those.

### What must be rewritten for Windows

| macOS mechanism | Windows equivalent to build |
|---|---|
| `CGEventTapCreate` + run-loop source | A low-level keyboard hook — `SetWindowsHookEx(WH_KEYBOARD_LL)` — or a TSF text service. Both differ fundamentally from a tap in threading and suppression semantics. |
| `CGEventPost(SessionEventTap)` + tagged `CGEventSource` | `SendInput` with `KEYEVENTF_UNICODE`, and a self-identification scheme. `SendInput`'s `dwExtraInfo` is the natural analogue of the source user-data tag — the feedback-loop guard depends on having one. |
| `CGEvent::keyboard_set_unicode_string` | `SendInput` `KEYEVENTF_UNICODE` (UTF-16 — the engine's `backspaces` unit already matches). |
| Accessibility permission + `AXIsProcessTrusted` + the modal gate + the health poll | No equivalent permission. But UIPI/UAC integrity levels mean a non-elevated hook cannot inject into an elevated window, which is a *different* silent-failure mode needing its own detection and its own honest indicator. |
| `NSWorkspace.frontmostApplication` + `NSWorkspaceDidActivateApplicationNotification` | `GetForegroundWindow` + `GetWindowThreadProcessId` + `QueryFullProcessImageName`, and `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` for the notification. **The notification path is not optional** — it is what keeps the keystroke path free of blocking calls. |
| Bundle identifiers (`DEFAULT_EXCLUSIONS`, `TERMINAL_EXCLUSIONS`, `CHROMIUM_BUNDLE_PREFIXES`) | Executable names / paths / AUMIDs. All three constant tables need Windows equivalents (Windows Terminal, conhost, PowerShell, WSL, cmd, Alacritty, WezTerm; VS Code, the JetBrains suite, Visual Studio). |
| The AX omnibox guard (`ax.rs`) | UI Automation (`IUIAutomation` → `TextPattern`/`ValuePattern`) for the same trailing-selection question, if Chrome on Windows exhibits it. **Unverified there.** |
| `NSStatusItem` + `NSMenu` | Shell tray icon (`Shell_NotifyIcon`) + a popup menu. |
| AppKit Settings / About / Welcome / HUD windows (≈1 900 lines) | A full UI rewrite in whatever Windows toolkit is chosen. |
| `SMAppService` | `Run` registry key, Startup folder, or a Task Scheduler entry. |
| `NSPasteboard` | `OpenClipboard`/`GetClipboardData` (`CF_UNICODETEXT`). |
| `NSLocale::preferredLanguages` | `GetUserPreferredUILanguages`. |
| `~/Library/Application Support` / `~/Library/Logs` | `%APPDATA%` / `%LOCALAPPDATA%`. |
| Bundle assembly (`build-app.sh`, `make-dmg.sh`, codesign, notarization stance) | MSI/MSIX or a portable zip; Authenticode signing; SmartScreen reputation is the analogue of the notarization problem. |
| `SMAppService`, `LSUIElement` | A tray-only process with no console window. |

### Recommended Windows integration approach

Judged only from what this code does today, `SetWindowsHookEx(WH_KEYBOARD_LL)` is the
closer structural match to the existing design, and TSF is the closer match to the
platform's intent. The trade-off is concrete:

- **Low-level hook** preserves the architecture exactly: layout-agnostic wrapping, no
  input-source switching, full suppression, one ordered injection queue, and the blind
  model. Every invariant in `decide.rs` transfers. It shares the tap's weaknesses too —
  the callback is on the critical input path (so `decisions/0008`'s "nothing may block"
  rule applies verbatim, and Windows enforces it with `LowLevelHooksTimeout`), it
  cannot reach elevated windows (UIPI), and it will fail in some protected input
  contexts, as the tap fails in secure fields.
- **TSF** is the sanctioned path, works with elevated and store applications, and gives
  real composition — but composition is precisely what this engine does *not* model.
  The blind diff model, `backspace_visible_char`'s "land on what the screen will show",
  and the full-suppression race fix all exist *because* there is no marked text. A TSF
  port would keep the transformation logic and discard much of the delivery logic.

Either way, three things from the macOS work are worth carrying over regardless of
mechanism: the **single ordered injection queue with self-tagging** (`dwExtraInfo`),
the **zero-blocking-calls rule** in the hook callback, and the **honest indicator** —
`docs/decisions/0007` exists because a tray icon claiming "VI" over a dead hook is a
defect, not a limitation.

### Architectural risks

1. **The blind model is the deepest assumption in the codebase.** "rendered == the text
   tail at the caret", maintained purely by flushing on everything that could move the
   caret. Windows has caret-moving events macOS does not (and vice versa); every one
   the port misses is a corruption that eats the user's text, not a cosmetic bug. The
   flush call sites are the highest-risk surface in the port.
2. **Injection ordering.** The full-suppression model exists *only* because native
   passthrough raced synthesized backspaces in multiprocess apps. Windows' `SendInput`
   ordering guarantees relative to native input, and Chrome's renderer path on Windows,
   need to be established empirically before assuming the same fix works — and before
   assuming a *different* fix is needed.
3. **The persisted `HotkeyPreset::Custom.keycode` is a macOS virtual key code.** If
   both platforms are ever to read one settings file, this is a schema break waiting to
   happen. Decide it deliberately, early.
4. **UTF-16 backspace counting is a lucky alignment, not a design.** It happens to
   match `SendInput`'s unit. Worth stating explicitly so nobody "fixes" it to chars.
5. **The exclusion list's *values* are macOS identifiers.** The type is portable; the
   14 + 9 + 7 constants are not. Getting the Windows terminal set wrong reintroduces
   the exact bug the ignore list exists to prevent.
6. **A hook callback that blocks freezes Windows input the way it froze the Mac.**
   `decisions/0008` was written from a real incident (five `TAP disabled by timeout`
   lines in one user's log, median emit 58 µs against a maximum of 22.4 ms). Windows
   has the same shape of failure with its own timeout. Anything the port adds to the
   keystroke path must be measured, not assumed.
7. **UIPI / elevated windows** is a Windows-only silent-failure class with no macOS
   analogue in this codebase. There is no existing detection or indicator design to
   copy.
8. **The UI is not portable at all** and is roughly 1 900 lines. The engine gives the
   port its behaviour for free; the UI gives it none of its surface for free.
9. **Test coverage is heavily engine-side.** 156 of 190 tests move to Windows
   unchanged. The 34 that prove the *delivery* logic are `CGEvent`-based and do not —
   so the port starts with the platform-neutral logic well proven and the platform
   integration proven not at all. That is exactly the ratio the macOS side started
   with, and §5's "known fragile" list is what came of it.

---

## 8. Current known issues

Sourced from `docs/handoff.md` §6 and verified against the code. Items relevant to
porting are marked.

### Uncommitted working-tree change

`git status` shows **`M app/src/log.rs`** (+184 / −23) plus untracked brand assets
(`GlowKey_Assets/`, `GlowKey_Assets.zip`, `GlowKey_Brand_Guidelines.pdf`). The `log.rs`
change replaces truncation with **rotation** (one kept generation `glowkey.log.1`,
bounded at 2 × 5 MB) and tracks size **in memory** rather than by `stat`. Its own doc
comment records the bug it fixes: the size check happened once, when the process opened
the file, so "a single long run grew without bound". `stat` per keystroke is explicitly
not an option (`decisions/0008`). **A reviewer should know this is in the tree and not
in a commit.**

### Shipped but unverified on screen

- **§6.1 Chromium omnibox guard** — mitigation shipped, **best-effort, not a proof**,
  needs live verification. Known residual: the AX read races Chrome's async renderer
  path, so a stale answer can occasionally skip or misfire the guard. *It converts a
  deterministic bug into a rare timing one.* Also: querying AX makes Chromium keep its
  accessibility tree on. **Port-relevant** — the same class of race would exist in any
  UI Automation equivalent.
- **§6.6 Accessibility revoked while running** — fixed, needs live verification.
  Unverified: on some macOS versions revoking terminates the process outright, which
  would make the recovery path unreachable (and harmless).
- **§6.9 System-wide input freeze while toggling the permission** — fixed, needs live
  verification. **This is the one unverified item that can hurt the whole machine**,
  and `docs/handoff.md` §11 names verifying it as step 1 for a new session.
- **§6.5 permission gate** and **§6.7 first-run welcome** — verified on screen
  2026-09-03 / need an eyeball respectively.
- **§6.4 all GUI is unverifiable headless.**

### Deliberate limitations and hacks

- **Telex brackets leak after a vowel**: `an[` gives `anow`, because `vi` then applies
  no transformation and returns the expanded keys verbatim. Documented as a **known
  limit**. Every real Vietnamese use is unaffected (ơ/ư follow a consonant or open the
  syllable) and 13 real words are pinned. Feeding `vi` a precomposed `ơ` is worse — it
  strips the horn (`tơ`→`to`). **Port-relevant**: an artifact of the `vi` crate, so it
  travels with the engine.
- **The stop-coda tone rule is our own addition**, not `vi`'s. `vi` calls `màc`, `hỏc`,
  `mãt`, `hòp` valid; they are not. If `vi` ever adds the rule, `violates_stop_coda_tone`
  becomes redundant and possibly double-counting.
- **`Engine::backspace` is dead in the app path** (`lib.rs:755`) — only
  `tests/telex.rs:111` calls it, and its doc comment says it is "wrong for this path,
  and now unused by the app". A port that reaches for the obvious-looking `backspace`
  instead of `backspace_visible_char` will get `hồng`→`hông` where the screen shows
  `hồn`.
- **`ax.rs` stores the system AX element as a raw `usize` address**, never released
  (process lifetime), main-thread use only. Deliberate, documented.
- **`TapState` / `TapContext` are `Box::into_raw`'d and leaked**; the menu bar item and
  controller are `mem::forget`'d. Deliberate — they must outlive the run loop.
- **`try_borrow` everywhere, degrading to passthrough.** Correct for a callback that
  must never panic, but it means a re-entrancy bug shows up as a silently dropped
  keystroke rather than a crash.
- **Escape-the-whole-word** (rather than dropping one key) in the mid-word spell check
  is *forced* by the re-derive design, not chosen.
- **⌃⇧W is swallowed even when there is nothing to correct** — a stated trade-off of
  the hotkey being fixed rather than configurable.
- **Session-only terminal un-exclusion** and **exclusion tombstones** are both
  compatibility workarounds for the fact that an accidental hotkey press must not
  permanently disarm a protection.
- **The 5-entry history cap** is explicitly "about bounding how far a wrong assumption
  about the caret could reach", not about memory.

### Environment / build friction

- **§6.5 Accessibility re-grant after rebuild or move.** The grant follows the code
  signature; ad-hoc signing changes it on every build, and copying the bundle makes a
  new one. A stable self-signed identity fixes it (`decisions/0006`). `dev-run.sh`
  needs no grant at all because `exec`ing from the shell makes the *terminal* the
  responsible process. **No Windows analogue** — this whole class of friction disappears.
- **§6.8 not notarized** (by choice) — a downloaded copy needs
  `xattr -dr com.apple.quarantine` once. The Windows analogue is SmartScreen reputation.
- **Two app identities must never run at once** — two taps process every keystroke
  twice. Both wrapper scripts kill both variants first.
- `cp`/`rm` may be interactive-aliased in the developer's shell; use `cat >` /
  `command cp`. `ls` prints empty in the sandbox for the `ai/` container — use glob/find.
- **Telex-safe test words**: avoid `w f j s x z r`, double vowels, and `dd` in test
  fixtures — they transform (`next`→`nẽt`, `good`→`gôd`). Use `hi man big cat top van go`.

### Privacy posture (a constraint, not an issue)

No network, no analytics, no accounts; CI **fails the build** if the binary links a
networking framework. The log **does** record typed word content, locally, capped, and
deletable. `PRIVACY.md` documents it. **A Windows port inherits this as a requirement**,
including the CI guard's intent.

---

## Unresolved questions for the reviewer

1. Should the Windows port share the settings file / schema with macOS? If yes,
   `HotkeyPreset::Custom.keycode` needs a platform-neutral key identity first.
2. Does Chrome on Windows exhibit the omnibox trailing-selection behaviour at all? The
   guard's existence on macOS should not be assumed to imply a need there.
3. Low-level hook or TSF? The answer determines how much of `decide.rs`'s logic is
   reusable policy and how much is discarded — the blind diff model is inseparable from
   the no-marked-text choice.
4. Is `app/src/log.rs`'s uncommitted rotation change intended to land before the port
   work starts?
