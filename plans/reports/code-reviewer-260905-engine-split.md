# Code review — `feat/engine-split` (813056b…ae7cfb1)

Scope: `git diff main...HEAD` — 87 files, +4762/−3279. Read-only review.
Verified locally: `cargo test --workspace` (pass), `cargo clippy --workspace
--all-targets -- -D warnings` (clean), plus each step of the CI `engine` job.

Overall: the split is disciplined and the port trait is a genuine simplification —
`handle` has a fixed, documented order and `tests/platform.rs` proves every arm.
Two defects block: one breaks CI, one is a user-visible macOS regression that the
pre-split code carried an explicit comment guarding against.

---

## Critical

### C1. The CI `engine` job fails: `glowkey-session` tests assume `serde` is on
`crates/glowkey-session/tests/macro_table.rs:43` and `:63`

`cargo test -p glowkey-engine -p glowkey-session -p glowkey-input`
(`.github/workflows/ci.yml:24`) fails on this branch:

```
---- falls_back_to_json_when_a_line_cannot_carry_the_macro ----
expected JSON, got "addr:12 Trần Phú\nHà Nội\n"
---- round_trips_macros_the_line_format_would_mangle ----
assertion `left == right` failed: round trip lost [Macro { shortcut: "#tag", ... }]
  left: []  right: [Macro { shortcut: "#tag", expansion: "hashtag" }]
test result: FAILED. 18 passed; 2 failed
```

Cause: `serde` was a hard dependency of `glowkey-engine` on `main`; it is now an
optional feature of `glowkey-session`. `Macro::format_table`
(`crates/glowkey-session/src/macros.rs:139-150`) only emits JSON under
`#[cfg(feature = "serde")]`, and `parse_table` (`:84-88`) only reads it there. The
two tests assert the JSON path unconditionally. `cargo test --workspace` passes only
because the `glowkey` app enables `glowkey-session/serde` and feature unification
turns it on — which masks the failure locally and on the macOS/Windows jobs.

A `cargo test … --features serde` line has just been added at `ci.yml:25`; that adds
a second run, it does not fix the still-failing run at line 24.

Fix: gate the two serde-dependent tests, e.g.

```rust
#[test]
#[cfg(feature = "serde")]
fn falls_back_to_json_when_a_line_cannot_carry_the_macro() { … }
```

and in `round_trips_macros_the_line_format_would_mangle`, skip the `#tag` /
newline / empty-expansion cases when `serde` is off — those are exactly the tables
the doc comment says are lossy without a JSON writer.

### C2. macOS: notices now run while the session is mutably borrowed, and the
Personal Words window is repainted empty
`app/src/platform/macos/dispatch.rs:88-139` (via `crates/glowkey-input/src/platform.rs:115-135`)

`TapState::run` takes `self.session.try_borrow_mut()` at `dispatch.rs:225` and holds
it for the whole of `glowkey_input::handle`. Every `TapPort::notify` branch therefore
executes *inside* that borrow. `main` deliberately did the opposite:

```rust
// main:app/src/platform/macos/dispatch.rs:99-103
// Before the effects, every one of which may reach back into the session:
// `refresh_glyph` asks whether Vietnamese is active, and would read
// `false` off a failed borrow and paint the wrong glyph.
drop(session);
self.carry_out_effects(effects);
```

`request_indicator` was correctly deferred (`dispatch.rs:82-86`) for exactly this
reason. `Notice::PersonalWordsChanged` was not, and it has the same failure:

- `dispatch.rs:108` → `crate::prefs::personal_words_changed()` (`app/src/prefs/mod.rs:661`)
- → `PrefsController::refresh_words()` (`app/src/prefs/personal_words.rs:131`)
- → `self.refresh_list_counts()` (`app/src/prefs/tabs.rs:429-438`), which calls
  `state.exclusion_ids()`, `state.macros()`, `state.word_overrides()` — all
  `self.session.try_borrow()` with an `unwrap_or_default()` fallback
  (`app/src/platform/macos/settings.rs:65,309,368`)
- → `let words = self.state().word_overrides();` (`personal_words.rs:143`)

With the session already mutably borrowed, every one of those `try_borrow()`s
returns `Err` and yields the empty fallback. Observable result of pressing ⌃⇧W with
the Settings window open (it opens at launch by default): the Personal Words list is
cleared and rebuilt as *"Nothing yet…"*, `word_order` is overwritten with an empty
vector, and the Excluded-Apps / Macros / Personal-Words counters on the Settings tab
all reset to `0`. The word actually *was* recorded; the UI says it was not.

Fix: defer the reload the same way the glyph repaint is deferred —

```rust
struct Deferred { edits: Vec<KeyResponse>, replay: bool, refresh_glyph: bool,
                  personal_words_changed: bool }
…
Notice::PersonalWordsChanged => self.deferred.personal_words_changed = true,
```

and in `handle_key_down`, after the `refresh_glyph` block:

```rust
if deferred.personal_words_changed {
    crate::prefs::personal_words_changed();
}
```

---

## High

### H1. macOS: AppKit is entered from inside the session borrow (`hud::flash`)
`app/src/platform/macos/dispatch.rs:102,113,131`

`crate::hud::flash` (`app/src/hud.rs:129`) builds and shows an `NSWindow` from
`TapPort::notify`, i.e. inside `session.try_borrow_mut()`. On `main` this ran after
`drop(session)`. If AppKit pumps the run loop while the HUD is created and ordered
front, a key delivered to the tap re-enters `TapState::run`, the `try_borrow_mut()`
at `dispatch.rs:225` fails, and the key silently becomes `Decision::Passthrough` — an
untransformed keystroke, and a passthrough the engine does not know happened.
`crate::prefs::hotkey_recording_done` already carries a comment saying this
re-entrancy is real in this codebase (`app/src/prefs/mod.rs:671-672`).

Fix: defer the HUD text the same way as C2 (`deferred.hud: Option<String>`), or —
closer to `main`'s shape — have `run()` collect notices and replay them after the
borrow is released.

### H2. `ExclusionList::new()` / `from_ids()` silently disable tombstoning and the
terminal rule
`crates/glowkey-session/src/exclusion.rs:96-120,157-159,183-189`;
`crates/glowkey-session/src/builder.rs:20`

On `main`, `remove()` consulted the module-level `DEFAULT_EXCLUSIONS` table and
`is_terminal` was a free function, so both rules held no matter how the list was
built. They now read `self.defaults`, which is empty for `new()` and `from_ids()`.

The app is safe today — every construction goes through `Settings::exclusion_list()`
→ `from_saved(.., shipped())` (`app/src/prefs_model.rs:135-141`), and I found no
`ExclusionList::new()` / `from_ids(` outside tests and the builder. But
`SessionBuilder::default()` starts from `ExclusionList::new()`, and the published
`glowkey-session` README shows `Session::builder()` used without `.exclusions(…)`.
A consumer on that path who calls `toggle_app_exclusion` on a terminal gets
`ExclusionToggle::Enabled` (permanent) instead of `EnabledSessionOnly` — precisely
the protection the type's own docs (`exclusion.rs:25-28`, `session.rs:590-595`) say
must not be losable by accident.

Fix (either, both cheap):
- Document it on `new`/`from_ids`: "no defaults behind it, so `remove` never
  tombstones and `is_terminal` is always false". The test
  `a_list_without_defaults_never_tombstones` asserts it; no doc states it.
- And/or have `SessionBuilder` take `ExclusionDefaults`, so the no-defaults case is
  not the default.

### H3. `HotkeyPreset::Custom` changed on-disk shape; no fixture covers the shape
the currently shipped build writes
`crates/glowkey-input/src/hotkey.rs:44-58`; `app/tests/fixtures/settings-macos-custom-hotkey.json`

`main` wrote `{"macos_keycode": 40, "windows_vk": null}`; HEAD writes
`{"raw_code": 40}`. Reading is handled: `#[serde(default, alias = "keycode",
alias = "macos_keycode")]`, and `windows_vk` is ignored (serde ignores unknown fields
in struct variants absent `deny_unknown_fields`). Windows never recorded a code —
`main:app/src/platform/windows/settings_ui.rs:1729` always wrote `windows_vk: None` —
so nothing is lost there, and `hook.rs:420`'s hardcoded `resolve(preset, None)` is
exactly what `preset.windows_vk().map(i64::from)` always evaluated to. The change is
behaviour-neutral going forward.

Two gaps:
1. The only fixture spells the field `keycode` — the *pre-port* name. The shape a
   user upgrading from the current release has on disk is `macos_keycode` +
   `windows_vk`, and nothing tests it. Given that fixture's own doc comment ("If this
   fails, the schema change is wrong"), that shape deserves its own committed fixture.
2. Downgrade is lossy: a file written by this build, read by the shipped build, finds
   no `macos_keycode`, defaults to `None`, and the custom hotkey silently drops to
   layout-dependent character matching. Worth a changelog line.

---

## Medium

### M1. `ExclusionList`'s `PartialEq` now includes `defaults`
`crates/glowkey-session/src/exclusion.rs:81-91`

Two lists with identical `app_ids`/tombstones now compare unequal if built against
different tables. Nothing in the app compares `ExclusionList` values —
`normalize_exclusions` and `merge_settings` compare `Settings`
(`app/src/platform/windows/settings_ui.rs:1449`, `shell.rs:277-300`), and
`settings_from` re-derives the exclusion fields as `Vec<String>` — so this is latent,
not live. It is a public-API semantic change worth naming in
`crates/glowkey-session/CHANGELOG.md`.

### M2. `app_in_front` on macOS is a window-server round trip from inside the callback
`app/src/platform/macos/dispatch.rs:68-74`

`crate::app_info::frontmost()` is an `NSWorkspace` call whose own module doc says
"Every call here is a window-server or Launch Services round-trip, so none of it may
run from the tap callback (`docs/decisions/0008`)" (`app/src/app_info.rs:5-8`).
Unchanged from `main` (which called it from `carry_out`) and only on ⌃⇧E, so not a
regression — but the port trait now claims "None of them may block on anything
outside the process" (`crates/glowkey-input/src/platform.rs:26-27`), which this
implementation does not honour. Soften the trait doc or note the exception at the
call site.

### M3. `apply_settings` splits one borrow into two
`app/src/platform/windows/shell.rs:233-240`

`hook::replace_settings(&merged)` then `hook::with_session(|s| s.set_frontmost_app(…))`
are two independent `try_borrow_mut()`s, each silently a no-op on failure. `main` did
both under one borrow. On the main thread this should not happen, but the failure
mode is now "settings applied, frontmost app lost" rather than "nothing happened".
Consider folding the app restore into `replace_settings`.

---

## Low

- **L1.** `glowkey-input`'s `serde` feature does not enable `glowkey-session/serde`
  (`crates/glowkey-input/Cargo.toml:27`), unlike `glowkey-session`'s, which enables
  the engine's. It is the odd one out; the app works only because it names both
  explicitly (`app/Cargo.toml:24,27`).
- **L2.** The three READMEs' code examples are never compiled — no
  `#![doc = include_str!("../README.md")]` in any `lib.rs`; only the engine *example
  program* is run in CI (`ci.yml:29`). I checked every method the READMEs name
  (`Session::builder` `session.rs:160`, `is_active` `:218`, `process_key` `:232`,
  `backspace` `:559`, `commit` `:706`, `ExclusionDefaults::new`,
  `ExclusionList::from_saved`) — all exist and the snippets are correct today. Adding
  the `include_str!` doc attribute keeps them that way.
- **L3.** Publish hygiene: `Session` and `SessionBuilder` derive no `Debug`
  (C-COMMON-TRAITS); `Hotkey`, `HotkeyKey` and `ExclusionToggle` are not
  `#[non_exhaustive]` while their siblings are.
- **L4.** `settings_spec::hotkey_display`'s catch-all arm
  (`app/src/settings_spec.rs:502-509`) says an unknown preset is "shown as the default
  it will behave as". That is not guaranteed — `hotkey::resolve`
  (`crates/glowkey-input/src/hotkey.rs:151`) is exhaustive inside its own crate, so a
  new variant is a compile error there, not a fallback to `CtrlShiftSpace`. Reword.
- **L5.** `Effects::clear` (`crates/glowkey-input/src/decision.rs:59`) is public API
  with exactly one caller, in a test harness; `handle` builds a fresh `Effects` per
  key. Consider dropping it before publishing.
- **L6.** `crates/glowkey-input/tests/platform.rs:230` comments "The notices come
  before the injection, so a log reads cause before effect", but the assertion below
  only checks `injected.len() == 1`. The ordering claim is untested — the `Recorder`
  would need a single interleaved event log to prove it.

---

## Behaviour-parity checks — verified, no defect

- `handle`'s effect order matches `Effects`'s field order and both shells' previous
  `carry_out_effects`. Windows swapped `save`/`refresh` relative to `main`; both are
  flag-sets followed by `wake()`, so no observable difference.
- Windows `handle_key`: `HookPort` borrows `last_app` / `pending_save` /
  `pending_refresh` disjointly from `session` (`hook.rs:434-447`); `app_in_front`
  returns the cache, and `toggle_app_exclusion` sets `session.current_app` without
  touching `state.last_app`. Semantics unchanged from `main:hook.rs:carry_out`.
- Windows `resolve(preset, None)` (`hook.rs:420`) equals what
  `preset.windows_vk().map(i64::from)` always produced. No cross-platform keycode
  confusion is reachable today; the `raw_code` doc comment's warning about a second
  recorder is accurate and sufficient.
- macOS: saving after posting the edits (`dispatch.rs:150-181`) is a latency
  improvement, not a correctness change — `pending_save` is drained after the session
  borrow is released, and `save_settings` → `snapshot` re-borrows cleanly.
- `Settings`'s serde surface (field names, `#[serde(default …)]`, `Default`) is
  identical to `main:crates/glowkey-engine/src/config.rs`; `settings_from`
  (`app/src/session_adapter.rs:36-58`) is a total struct literal, so a new field
  cannot be silently dropped. `SessionBuilder` covers every knob
  `main:Session::from_settings` set that the session still owns.
- `ExclusionDefaults::new` folding terminals into `excluded` (`exclusion.rs:42-48`)
  does not defeat a tombstone: `from_saved` skips any id in `removed_defaults`
  regardless of which set it came from (`:136-140`); covered by
  `from_saved_respects_tombstones`.
- `#[non_exhaustive]` fallout: every out-of-crate `match` on `Decision`, `Notice` and
  `HotkeyPreset` has a wildcard, and each is either a deliberate "this platform has no
  surface for it" (`dispatch.rs:137`, `hook.rs:519`) or a test-harness `panic!`
  (`app/src/platform/macos/tests.rs:62,674`). None silently drops behaviour the
  pre-split code had.

---

## Recommended actions

1. Fix C1 (gate the two macro-table tests) — CI is red without it.
2. Fix C2 (defer `PersonalWordsChanged`) — user-visible macOS data-display bug.
3. Fix H1 (defer the HUD) — same class as C2, harder to observe but worse when it hits.
4. Address H2 (document or design out the defaults-less `ExclusionList`) before publishing.
5. Add the `macos_keycode` + `windows_vk` fixture (H3) and a changelog note on downgrade.
6. Sweep M1–M3 and the low items at leisure.

## Unresolved questions

- Was dropping `windows_vk` intended to be permanent, or should a Windows recorder
  land first so the field is not resurrected under a different name later?
- Is `ci.yml:25` (`cargo test … --features serde`, added while this review was in
  progress) meant to *replace* line 24 rather than supplement it? If serde-off is not
  a supported configuration for the session crate, say so in its manifest comment.
