---
title: "Engine split: three library crates, injected exclusion defaults, and a Platform port"
date: 2026-09-05
summary: glowkey-engine is the pure core again; glowkey-session holds the typing policy with the shipped app tables injected by the app; glowkey-input names the shell port as a trait. Review caught the macOS notices running inside the session borrow.
---

# Engine split: three library crates, injected exclusion defaults, and a Platform port

## What happened

The user asked for the engine to be something someone else could take, "refactor
with design pattern". The plan (`plans/260905-1333-engine-split-and-layering/`)
was validated first: seven decisions, the load-bearing ones being that
`Session`, macros and word overrides are policy rather than core, and that the
preferences file belongs to the app. Then all five phases were cooked in one
session on `feat/engine-split`.

Phase 1 split one 2200-line `lib.rs` into modules with an identical public API.
Phase 2 moved `Settings`, `Language` and the launch flags into
`app/src/prefs_model.rs`, with the three settings fixtures moving alongside so
the file stays byte-compatible; `HotkeyPreset` moved to `glowkey-input` with one
`raw_code` and serde aliases for the two old field names. Phase 3 created
`glowkey-session`: `Session`, `SessionBuilder`, `ExclusionList` with an injected
`ExclusionDefaults`, `AppId`, macros and overrides. The shipped application
tables left the library crates for `app/src/default_exclusions/`; the session
crate has no bundle identifier or `.exe` in it and re-exports the engine so a
consumer names one crate. Phase 4 added `trait Platform` and `handle` to
`glowkey-input`, with `HookPort` on Windows and `TapPort` on macOS. Phase 5 did
the crates.io metadata, READMEs, changelogs, an example, `#[non_exhaustive]`,
and CI jobs for docs, MSRV, publish dry run and semver-checks.

## What went wrong, and what the review caught

Windows path handling in the patch scripts: `glob` returns backslashes, so a
dictionary keyed by forward-slash paths missed every globbed file. Nothing was
written because the scripts write only at the end, which is the habit that
paid for itself three times today.

The Bash tool rejects heredocs containing an apostrophe, even inside quoted
delimiters. Every multi-line patch went through a Python file written with the
Write tool instead.

The macOS shell needed a shape the plan did not anticipate. `handle` holds the
session mutably while the port runs, and the macOS tests call `decide` with real
`CGEvent`s on a developer's machine. If the port posted edits directly, the
tests would type into the machine. So `TapPort` queues edits and the replay,
and `handle_key_down` posts them after `handle` returns.

The reviewer then found the same borrow in a place the queue had not reached:
`Notice::PersonalWordsChanged` called straight into the Personal Words window,
whose refresh reads the session with `try_borrow().unwrap_or_default()`. Inside
the borrow every read fails, the list rebuilds empty and the counters show 0.
`main` had a `drop(session)` with a comment saying exactly why, and the port
moved the effects past it. The HUD flash had the same exposure (AppKit may pump
the run loop; a re-entrant callback would see a failed borrow and pass the key
through). Both now go through `Deferred`. The lesson is the one the old comment
already stated: on macOS nothing that can reach the session runs while the
policy holds it, and a port must be designed around that borrow, not merely
compile against it.

The tester found the CI library job red: two macro-table tests exercise the JSON
fallback, which now exists only under the `serde` feature, and the job runs the
default feature set. `cargo test --workspace` hid it because the app turns the
feature on for everyone. Both tests are gated; CI now tests both feature sets.

The tester also reported the builder as untested. It has two unit tests in its
own module; the grep covered `tests/` only. Noted, not acted on.

A phase 2 miss surfaced in phase 4: the macOS tests still named `macos_keycode`
in one place, because the phase 2 script's replacement matched a different
indentation. The macOS `--tests` compile check was not in the gate set then; it
is now.

## Decisions

Decision 0012: four layers, each depending only on the one below; the patterns
adopted and rejected. Recorded deviations: the engine keeps `serde` as an
optional feature; `is_invalid_vietnamese`, `diff` and `KeyResponse::passthrough`
are public on the engine; `Platform` has six methods, not five; `Decision`,
`InputMethod`, `PlacementStyle`, `Key`, `HotkeyPreset` and `Notice` are
`#[non_exhaustive]`, while `BackspaceOutcome` and `BoundaryBackspace` are not,
because the ladder matches them exhaustively on purpose.

## Next steps

- macOS runtime: the whole port has been compile-checked only. ⌃⇧W with the
  Personal Words window open is the specific check the review adds.
- The session and input crates cannot be packaged until the engine is on
  crates.io; the CI comment says where to add them.
- `SessionBuilder::default()` starts with no exclusion defaults, so a consumer
  who skips `.exclusions(..)` gets no terminal rule. Documented on the builder;
  worth a louder README note before the first tag.
- Still unanswered from earlier today: porting the checkbox-in-control-column,
  count units and rhythm constants to the macOS renderer.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
