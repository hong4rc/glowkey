---
phase: 2
title: "Unbundle product preferences"
status: pending
priority: P1
effort: "6h"
dependencies: [1]
---

# Phase 2: Unbundle product preferences

## Overview
Move what only GlowKey needs out of the engine: the `Settings` file model,
`Language`, the launch flags, and the hotkey preset with its platform key codes.
The engine stops knowing there is a settings window or a login item.

## Requirements
- Functional: `settings.json` written by today's build loads and round-trips
  byte-for-byte (modulo key order, which serde_json preserves as declared).
- Non-functional: after this phase the engine's only `serde` users are
  `Macro`/`WordOverride`, which leave in phase 3 (then `serde` goes entirely);
  no `cfg(target_os)`, no `macos_keycode`, no `windows_vk`.

## Architecture
- `app/src/prefs_model.rs` (new; shared by both shells): `Settings` moves here
  verbatim with its serde derives and defaults; `Language` moves here;
  `open_settings_at_launch` and `welcome_shown` leave `Session` and live only
  in `Settings` (the shells already read them from the file at startup).
- `HotkeyPreset` moves to `glowkey-input::hotkey` (it is a hotkey), keeping
  serde under the input crate's `serde` feature; `macos_keycode`/`windows_vk`
  collapse into the existing `raw_code: Option<i64>` idea already present on
  `KeyEvent::with_raw_code`, with a serde alias so old files load.
- `Session::from_settings` / `Session::snapshot` become an **Adapter** in the
  app: `app/src/session_adapter.rs` with `fn session_from(settings: &Settings)
  -> Session` and `fn settings_from(session: &Session, prefs: &Settings) ->
  Settings` (the second keeps the product-only fields from `prefs`).
- `settings_spec::Toggle::settings_field` and the Windows draft editing keep
  working against the moved `Settings` (path change only).

## Related Code Files
- Create: `app/src/prefs_model.rs`, `app/src/session_adapter.rs`
- Modify: `crates/glowkey-engine/src/{lib,session,config,language,hotkey}.rs`
  (delete `config.rs`, `language.rs`, `hotkey.rs` after the move),
  `crates/glowkey-input/src/hotkey.rs`, `app/src/settings_store.rs`,
  `app/src/settings_spec.rs`, `app/src/platform/**` import paths,
  `app/src/prefs/**` import paths
- Delete: `crates/glowkey-engine/src/config.rs`

## Implementation Steps
1. Fixture test first: copy the user's real `settings.json` shapes (Windows
   and macOS) into `app/tests/fixtures/`, test load → save → equal.
2. Move `Settings` and `Language`; fix imports; tests green.
3. Move `HotkeyPreset` into `glowkey-input` with the alias; tests green.
4. Remove `open_settings_at_launch` / `welcome_shown` from `Session`; the
   shells read them from `Settings`.
5. Replace `Session::from_settings`/`snapshot` with the adapter; delete the
   engine's `Settings` dependency; make `serde` optional in the engine (removed
   outright in phase 3).
<!-- Updated: Validation Session 1 - serde leaves the engine with macros in phase 3 -->
6. Gates on three targets; fixture test green.

## Success Criteria
- [ ] `grep -r "Settings\|Language\|macos_keycode\|windows_vk\|open_settings_at_launch\|welcome_shown" crates/glowkey-engine/src` is empty.
- [ ] `cargo build -p glowkey-engine --no-default-features` succeeds.
- [ ] Fixture round-trip test passes on both saved-file shapes.
- [ ] All tests green; clippy on three targets.

## Risk Assessment
- The Windows `merge_settings` and the macOS `TapState` both touch `Settings`
  fields by name; a move is a path change, but a field rename would break the
  file. Rule: no field renames in this phase.
- `HotkeyPreset` serde: `#[serde(alias = "keycode")]` already exists for
  `macos_keycode`; add aliases for both old names onto `raw_code`. Test with
  a Custom preset fixture from each platform.
