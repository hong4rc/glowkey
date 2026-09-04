---
phase: 2
title: "Platform-neutral hotkeys and app identity"
status: complete
priority: P1
effort: "2d"
dependencies: [1]
---

# Phase 2: Platform-neutral hotkeys and app identity

## Overview

Two macOS values have leaked into portable surfaces: a macOS virtual key code inside
`HotkeyPreset::Custom`, which is **persisted to settings.json**, and bundle
identifiers as the application-identity key. Fix both without silently reinterpreting
any settings file a user already has.

## Requirements

- Functional: an existing macOS `settings.json` loads and behaves exactly as before.
- Functional: a Windows build never matches a macOS keycode against a Windows key.
- Functional: exclusions, tombstones and session suspension behave unchanged.
- Non-functional: the engine gains no platform dependency.

## Architecture

### The hotkey

Today, in the engine and on disk:

```rust
HotkeyPreset::Custom { control: bool, shift: bool, option: bool,
                       keycode: i64,      // ← a macOS virtual key code
                       key_char: char }
```

`keycode` is the matcher; `key_char` is only for display. **The four named presets
are already neutral** — `CtrlShiftSpace`, `CtrlSpace`, `OptionSpace`, `CtrlShiftZ`
are described by modifiers plus a semantic key, so they need nothing.

Only `Custom` is the problem. Recommendation: keep one schema, make the platform
value explicitly platform-scoped.

```rust
HotkeyPreset::Custom {
    control: bool, shift: bool, option: bool,
    key_char: char,             // display, and the cross-platform fallback matcher
    macos_keycode: Option<i64>, // #[serde(default)] — what today's field becomes
    windows_vk: Option<u16>,    // #[serde(default)]
}
```

- Reading an old file: `keycode` deserializes into `macos_keycode` via
  `#[serde(alias = "keycode")]`. Nothing changes for an existing macOS user.
- A Windows build sees `windows_vk: None` and falls back to matching `key_char`,
  which is correct-but-layout-dependent, and records that it did so.
- Recording a custom hotkey on either platform fills in that platform's field.

This is the minimum abstraction. It is deliberately **not** a universal keycode
table: the request forbids one, and two platforms do not justify inventing a third
keyboard model.

### Application identity

`ExclusionList` is keyed on `String` and is already portable; only the *values* are
macOS. Introduce a thin newtype and per-platform default tables.

```rust
pub struct AppId(String);   // opaque, platform-defined, compared case-insensitively
```

- macOS: bundle identifier — unchanged, so existing files keep matching.
- Windows: lowercased executable file name (`windowsterminal.exe`), with the full
  path available for disambiguation. Not AUMID: it is absent for many apps and
  unstable across installs.
- Linux: deferred to Phase 8.

`DEFAULT_EXCLUSIONS` (14), `TERMINAL_EXCLUSIONS` (9) and `CHROMIUM_BUNDLE_PREFIXES`
(7) become per-platform constant tables selected at compile time. The **tombstone**
mechanism in `ExclusionList` must keep working across this: a user who removed a
shipped default must not have it reappear.

## Related Code Files

- Modify: `crates/glowkey-engine/src/lib.rs` (`HotkeyPreset`), `src/config.rs`
  (serde), `src/exclusion.rs` (identity type + platform tables)
- Create: `crates/glowkey-engine/src/exclusion_defaults/{macos,windows}.rs`
- Modify: `crates/glowkey-input/src/hotkey.rs` — matching over the neutral key
- Modify: `crates/glowkey-engine/tests/exclusion*.rs`, `config` round-trip tests

## Implementation Steps

1. Add the serde alias and the two optional fields; keep `Custom`'s public shape
   otherwise stable.
2. Write a round-trip test using a **real settings.json captured from today's build**
   and assert the loaded hotkey is byte-identical in behaviour.
3. Split the three exclusion constant tables per platform behind `cfg`, in the
   *engine's data module only* — this is data selection, not logic branching, and is
   the one place the engine may see a platform (document why in the module header).
4. Build the Windows terminal/editor table: Windows Terminal, conhost, PowerShell,
   pwsh, cmd, WSL, Alacritty, WezTerm, mintty; VS Code, the JetBrains suite, Visual
   Studio, Sublime, Neovim/Vim hosts.
5. Confirm tombstones survive: add a test that removes a shipped default, reloads,
   and asserts it stays removed.

## Success Criteria

- [x] A settings.json written by today's macOS build loads with identical behaviour
- [x] `HotkeyPreset::Custom` recorded on macOS still matches on macOS
- [ ] A Windows build with a macOS-recorded custom hotkey falls back to `key_char`
      and logs that it did, rather than matching a wrong key — the fallback and
      the log line exist and are tested (`hotkey::Hotkey::is_char_fallback`,
      `crates/glowkey-input/tests/hotkey.rs`); there is no Windows build to run
      it against until Phase 5
- [x] Tombstone behaviour proven by test across the table split
- [x] `cargo test -p glowkey-engine` green on the Linux target's test suite in CI

## Risk Assessment

**Silent settings reinterpretation is the worst outcome here** — worse than a hard
failure, because the user's hotkey would simply do something else. *Signal:* the
captured-settings round-trip test fails. *Response:* stop; the schema change is
wrong. Never "fix" the test by regenerating the fixture.

**Getting the Windows terminal table wrong reintroduces the exact bug the ignore
list exists to prevent** — synthesized backspaces mangling text in a terminal.
*Signal:* Phase 6 finds Vietnamese firing in Windows Terminal. *Response:* the table
is data, so this is cheap to fix, but it must be on the Phase 6 checklist explicitly.

**`key_char` fallback is layout-dependent.** A custom hotkey recorded on one layout
may not match on another. *Signal:* known and accepted for the cross-platform
fallback path only. *Response:* record it in the limitations list; the fix is to
re-record the hotkey on that platform, and the UI should say so.
