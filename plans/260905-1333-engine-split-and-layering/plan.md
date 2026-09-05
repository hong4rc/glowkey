---
title: "Engine split and layering: a Vietnamese engine anyone can take, a product built on it"
description: "Carve glowkey-engine into a reusable text-transform core, a session/policy layer with injected defaults, and explicit platform ports; move GlowKey's own preferences out of the engine; make the core publishable."
status: pending
priority: P1
effort: "3-4 days"
tags: [glowkey, architecture, engine, crates, refactor, api]
created: 2026-09-05
blockedBy: []
blocks: [260904-2127-glowkey-cross-platform-port]
---

# Engine split and layering

The user's goal: someone who wants a Vietnamese input engine should be able to
take it without taking GlowKey. Today they cannot, because the engine crate
carries the product.

## What the scout found

`crates/glowkey-engine/src/lib.rs` is 2,209 lines and three layers in one file:

| Layer | What it is | Who wants it | Where it sits now |
|---|---|---|---|
| **Core** | `Engine`: Telex / VNI / Simple Telex via the `vi` crate, tone placement, backspace and re-composition, Quick Telex, bracket shortcuts, mid-word spell check, `remove_tones`. Pure functions over characters. | Anyone building a Vietnamese IME, editor, or bot | `lib.rs` 588–1140, `InputMethod`, `PlacementStyle`, `KeyResponse`, `BackspaceOutcome` |
| **Policy** | `Session`: VN/EN mode, per-app exclusion, auto-fix at the boundary, restore-English, personal word overrides, macros (gõ tắt), auto-capitalize, correction of the last word. Stateful orchestration of the core. | Anyone building a *keyboard-level* Vietnamese tool | `lib.rs` 1146–2209, `exclusion.rs`, `Macro`, `WordOverride` |
| **Product** | GlowKey's preferences file and its shell's flags: `Settings` (the whole JSON), `Language` (UI language), `open_settings_at_launch`, `welcome_shown`, `HotkeyPreset` with `macos_keycode` / `windows_vk`, `DEFAULT_EXCLUSIONS` naming `.exe`s and bundle ids | Only GlowKey | `config.rs`, `lib.rs` 38–220, `exclusion.rs::DEFAULT_EXCLUSIONS`, `Session` fields |

`glowkey-input` (the decision ladder, `decide(&mut Session, &KeyEvent, &Ctx,
&mut Effects) -> Decision`) is already a clean port: platform-free, returns a
decision and effects for the shell to carry out. It depends on `Session` and
`HotkeyPreset`. `app/` implements the shells; `settings_store.rs` already owns
file I/O and says "the engine owns the data", which is the coupling to undo.

Tests: 12 integration files. Seven use only `Engine` (telex, quick telex,
brackets, simple telex, spell check, remove_tones); five use `Session`; one
touches `Settings`. The split follows the tests.

## Design

Layering, bottom up, each a crate with one reason to change:

```
glowkey-engine   core: Engine, InputMethod, PlacementStyle, KeyResponse, remove_tones
      ▲
glowkey-session  policy: Session, InputMode, ExclusionList (defaults injected),
                 auto-fix, correction, Macro (data + expansion), WordOverride/
                 WordPreference, AppId newtype
      ▲
glowkey-input    ports: KeyEvent/Key/Modifiers in, Decision/Effects out, hotkey matching,
                 `Platform` trait for what a shell must provide
      ▲
app              GlowKey: Settings (prefs file), Language, launch flags, HotkeyPreset
                 platform codes, DEFAULT_EXCLUSIONS, tray/menu/windows, settings_spec
```

Patterns, named because the user asked, and only where the code already wants
them:

- **Facade** — `Engine` and `Session` stay the two entry points; the split does
  not add a third. Internals become modules behind them.
- **Strategy** — input methods are `vi::methods::Definition` values selected by
  `InputMethod`; made explicit as `Engine::with_method(&Definition)` so a
  consumer can add a method without forking.
- **Builder** — `Session::builder().style(..).exclusions(..).auto_fix(..)
  .build()` replaces `Session::from_settings(&Settings)`; the product's
  `Settings → Session` mapping moves to the app as an **Adapter**.
- **Dependency injection** — `ExclusionList::with_defaults(iter)`; the shipped
  `.exe`/bundle-id table moves to the app. The session never names an app.
- **Ports and adapters** — `glowkey-input` gains `trait Platform { fn
  inject(&mut self, emit: &Emit); fn app_in_front(&self) -> Option<&str>; fn
  request_save(&mut self); fn request_indicator(&mut self); }`; the macOS
  `dispatch.rs` and Windows `hook.rs` `carry_out_effects` become its two
  adapters. This is where Linux (port plan Phase 8) plugs in.
- **Newtype** — `AppId(String)` for the frontmost application identity, which is
  a bundle id on macOS and a process name on Windows; today it is `String`
  named `bundle_id` on both.

Not adopted: Observer (nothing subscribes), Command (effects are already a
value), a plugin registry (one product).

## Phases

| # | Phase | Status | Depends on |
|---|---|---|---|
| 1 | [Carve the engine into modules, no behaviour change](./phase-01-start.md) | completed | — |
| 2 | [Unbundle product preferences](./phase-02-unbundle-product-preferences.md) | completed | 1 |
| 3 | [Session crate and injected defaults](./phase-03-session-crate-and-injected-defaults.md) | completed | 2 |
| 4 | [Ports for platforms](./phase-04-ports-for-platforms.md) | pending | 3 |
| 5 | [Publish readiness](./phase-05-publish-readiness.md) | pending | 1 |

## Acceptance criteria

1. `glowkey-engine` has no `serde` at all, no `Settings`, no `Language`, no
   `HotkeyPreset`, no `Macro`, no `WordOverride`, no `.exe` or bundle id
   anywhere, no `cfg(target_os)`; `cargo test -p glowkey-engine` green on Linux CI; a
   consumer example `examples/type_a_word.rs` compiles with the crate alone.
2. `glowkey-session` builds a `Session` through a builder with injected
   exclusion defaults; every existing session test passes unchanged in
   behaviour (moved, not rewritten).
3. `glowkey-input` exposes `Platform`; both shells implement it; the ladder
   tests (767 lines) pass unchanged.
4. `app` owns `Settings`, `Language`, launch flags, hotkey platform codes and
   `DEFAULT_EXCLUSIONS`; the settings file format is byte-compatible (a file
   saved by today's build loads and round-trips).
5. Workspace: 285+ tests green; clippy `-D warnings` on Windows, macOS
   (`aarch64-apple-darwin` check) and Linux (`glowkey-engine`, `glowkey-session`,
   `glowkey-input`); `cargo doc --no-deps` warning-free with
   `#![warn(missing_docs)]` on the three library crates.
6. Publish readiness for `glowkey-engine`: `repository`, `keywords`,
   `categories`, `readme`, `rust-version` set; `cargo semver-checks` in CI
   from the first tagged version; a CHANGELOG; `#[non_exhaustive]` on public
   enums a consumer matches on (`InputMethod`, `KeyResponse` variants).

## Non-goals

- No change to Vietnamese behaviour. Every existing test is a regression test.
- No new input method, no new feature.
- Not publishing to crates.io in this plan; making it publishable is.
- The UI (`settings_spec.rs`, renderers) is untouched except for the type
  moves in phase 2.

## Risks

- **Settings file compatibility.** `Settings` moves crates; its serde shape
  must not change. Signal: a copied `settings.json` fails to load or
  round-trips differently. Response: a fixture test on a real file from
  `%APPDATA%` and `~/Library/Application Support` before any move.
- **Test churn.** Moving `Session` tests to a new crate changes paths only;
  if a test needs rewriting, the split is wrong there. Response: stop, adjust
  the boundary.
- **Blast radius in `app/`.** ~80 call sites import `glowkey_engine::`.
  Response: phase 2 and 3 land as separate commits with `pub use` shims kept
  one phase, removed in the next.
- **`vi` crate as a dependency of the public core.** MIT, active upstream;
  the consumer inherits it. Acceptable; documented in the engine README.
- Port plan Phase 8 (Linux) builds on phase 4 here; the port plan is marked
  blocked by this one.

## Validation Log

### Validation Session 1 (2026-09-05 13:40)

### Verification Results
- Claims checked: 22 (Fact Checker, Flow Tracer, Scope Auditor, Contract Verifier)
- Verified: 22 | Failed: 0 | Unverified: 0
- Tier: Full
- Evidence: `Engine` at `lib.rs:588`, `Session` at `lib.rs:1166`;
  `Session::from_settings` 3 callers, `.snapshot()` 5, `set_frontmost_app` 13
  references at 8 sites (menu_bar.rs, macos/mod.rs x3, macos/tests.rs x4,
  windows/hook.rs, windows/shell.rs x2); `HotkeyPreset` 66 references,
  `Settings` 123, `DEFAULT_EXCLUSIONS` 17; `carry_out_effects` present in both
  `macos/dispatch.rs:109` and `windows/hook.rs`; `KeyEvent.raw_code: i64`
  exists; the `keycode` serde alias at `lib.rs:187`; the engine has no
  `pub(crate) fn` (the session uses only public API, so the split is clean);
  no `rust-version` anywhere yet; the Linux CI job covers engine and input only.

### Decisions (all as recommended)
1. Session in its own crate `glowkey-session`.
2. **Macros and word overrides move to the session crate**, not the core.
   Changes the design diagram, criterion 1, the phase 1 module table
   (`macros.rs`, `overrides.rs` marked as leaving), and phase 3 scope (move
   them plus `tests/macro_table.rs`, `tests/word_overrides.rs`). The engine
   then has no `serde` at all.
3. `Settings`, `Language`, launch flags: a module in `app/`.
4. `Platform` trait, five methods; phase 4 stays.
5. `HotkeyPreset` to `glowkey-input` with one `raw_code` and serde aliases.
6. `AppId` newtype at the 13 references.
7. Semver-checks and MSRV in CI now, semver `continue-on-error` until a tag.

### Whole-Plan Consistency Sweep
Re-read `plan.md` and all five phase files after propagation. Every mention of
`Macro`/`WordOverride` placement now says session; criterion 1 and the phase 1
module table agree; `from_settings`, `snapshot`, `DEFAULT_EXCLUSIONS`,
`AppId`, `Platform` are used consistently. No unresolved contradictions.

## Rollback

Each phase is one PR-sized commit set on its own branch; revert the branch.
Phase 1 alone is safe to keep in any case.
