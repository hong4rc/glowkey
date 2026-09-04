# Phases 1–3 — the macOS refactor

**Date:** 2026-09-04 · **Branch:** `main` · **Scope:** plan
`260904-2127-glowkey-cross-platform-port`, phases 1, 2 and 3 only. No Windows code.

## Outcome

The decision ladder is out of the macOS tap and into `crates/glowkey-input`, the
macOS tap is seated on it, and the two macOS values that had leaked into portable
surfaces are per-platform without disturbing anyone's settings file.

All 194 original tests pass. 47 new ones join them: **241 total**. Clippy is
silent on the workspace with `-D warnings`, `cargo fmt` reports no drift, and
`cargo check` passes for `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu`
for every crate including the app stub.

## Commits

| Commit | What |
|---|---|
| `5d63123` | `cargo fmt -p glowkey-engine -- --check` was already failing on `main` — fixed before starting, pushed separately |
| `654f678` | Phase 1 — `crates/glowkey-input` |
| `f16c798` | Phase 2 — hotkey schema and per-platform application tables |
| `51e1248` | Phase 3a — the directory move, **and nothing else** |
| `f8d620f` | Phase 3b — the rewiring |
| `924373a` | Plan status sync-back |

## The non-negotiables, each answered

**The ladder ported as a unit, tests first.** The 30 policy tests were written
against an empty `decide` that returned `Passthrough` and run: **28 failed**, the
two greens being the two that legitimately expect a passthrough (an excluded app,
and ⌃⇧W in an excluded app). Only then was the ladder filled in. The ordering is
unchanged and is documented step by step at the top of `ladder.rs` as the
specification it is. Both named sequences pass off-platform:
`hoongf, ⌫⌫z → hông` and `hoongf vieet s⌫⌫⌫⌫⌫⌫⌫z → hồngz`. The five Backspace
cases each have a test named for the case.

Phase 1 and its tests are one commit, because the crate is not reviewable in
halves — but they were not written in one pass, and the red run is recorded above
and in the commit message.

**The settings fixture.** `crates/glowkey-engine/tests/fixtures/` holds two files
captured **before** any schema change: the real `settings.json` off this machine's
installation, and one written by the pre-change build carrying
`HotkeyPreset::Custom { keycode: 40 }`. Both are asserted field by field. The
alias was then deliberately removed to confirm the fixture test actually fails
(`macos_keycode: None` vs `Some(40)`) and restored. The fixture is never
regenerated; the test says so in place.

**glowkey-input has no operating system in it.** No `cfg(target_os)`, no
`unsafe` (`#![deny(unsafe_code)]`), one dependency (`glowkey-engine`). Both
cross-target checks pass. The CI Linux job now runs `fmt --check`, `clippy -D
warnings` and `test` for it alongside the engine.

**The move is its own commit.** `51e1248` is a pure `git mv` plus five import
paths; git recorded all eight files as renames.

## Deviations from the plan, and why

**`Ctx` has one field, not two.** The frontmost application turned out not to
belong there — the session already knows it, and asking twice is how two answers
start to disagree. Recorded in the type's doc comment.

**`decide` gained a fourth parameter, `&mut Effects`.** The ladder used to write
to the log, flash the indicator and repaint the menu bar itself. None of that is
policy and all of it is an operating system, so it is reported as plain data and
the platform performs it in field order the instant `decide` returns — which
keeps every log line in the order it has always appeared in. A callback would
have been the alternative, and the plan rules callbacks out for good reason.

**Hotkey recording is not in the ladder.** It produces a value shaped by the
platform (the key code it reported), so `hotkey::capture` decides what the
keystroke *means* and the platform builds the preset. It still runs in the exact
position the recording branch always occupied — the platform calls it before
`decide` — so the ordering is untouched.

**`decide.rs` became `dispatch.rs` rather than folding into `mod.rs`.** The plan
had its remains move into `mod.rs`; that would have made `mod.rs` ~670 lines for
no gain. `dispatch.rs` is the same file minus the ladder.

**The tap's 34 `CGEvent` tests were not thinned.** The plan expected them to
shrink. They are the cross-check the plan itself asks for — "a ported test
passes while the tap-level equivalent fails" only detects a bad adapter if both
exist — and "all 194 tests stay green" was a non-negotiable. One test changed:
the preset matcher now drives a real `CGEvent` through the adapter into the
neutral matcher, so it proves the translation rather than re-proving the matching.

**`AppId` was not introduced.** Phase 2's architecture proposes an opaque newtype
compared *case-insensitively*. That is a real change to how macOS exclusions
match, and Phase 2's own requirements say exclusions must behave unchanged. It
buys macOS nothing and is only needed when Windows exists. Deferred to Phase 4,
where the need is real. The per-platform tables — which Phase 2's success
criteria do require — are done.

## The one behavioural difference

The session is now borrowed once for the length of the ladder and released before
any effect runs. Previously `crate::prefs::personal_words_changed()` was called
from *inside* the mutable borrow, so the Personal Words window refreshed itself
from `word_overrides()`, which returns an empty list on a failed borrow — it
blanked. Doing the borrow once fixes that. It is visible only if that window is
open while ⌃⇧W is pressed, and it is a repair.

Related, and unreachable: a failed `try_borrow_mut` now yields `Passthrough` for
the VN/EN toggle hotkey where it previously yielded `Consume`. The callback is
single-threaded and holds no borrow across the call, so neither path can be
reached; it is recorded here rather than special-cased in code.

## Still owed

1. **A human typing Vietnamese into the release build, for one minute.** The
   bundle builds and signs (`scripts/build-app.sh` → `build/GlowKey.app`,
   `io.glowkey.GlowKey`, universal, signed). 160 of the 241 tests are engine
   tests that never touch this code; the real coverage here is the 34 tap tests
   plus a person. The plan calls this mandatory before Phase 4 and it is not
   done.
2. **`docs/manual-verification.md` §2, §4, §5 read against the ladder.** Done by
   inspection while porting — every comment moved with its step — but not
   re-walked end to end at the keyboard.

## Unresolved questions

- **Should the tap's 34 `CGEvent` tests eventually thin out?** Kept for now as
  the adapter cross-check. Worth revisiting once Windows has its own adapter and
  the pattern is established.
- **Linux borrows the macOS exclusion table** (`exclusion_defaults/mod.rs`)
  because Phase 8 has not decided what an application identity is there. It keeps
  the Linux CI job testing data a real platform ships. Confirm that is the
  intended placeholder.
- **The Windows application tables were written from a Mac.** Every name is a
  hypothesis; the module says so. Phase 6 must check them at a real machine —
  a wrong terminal name puts back the exact bug the ignore list exists to prevent.
- **Incidental fmt.** `cargo fmt --all` swept ~18 lines of pre-existing
  whitespace drift in `about_window.rs` and `menu_bar.rs` into the Phase 2
  commit. Harmless, but it is noise in that diff.
