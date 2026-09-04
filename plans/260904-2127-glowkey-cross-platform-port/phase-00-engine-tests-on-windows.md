---
phase: 0
title: "The engine's own tests pass on Windows"
status: complete
priority: P1
effort: "0.5d"
dependencies: []
---

# Phase 0: The engine's own tests pass on Windows

## Overview

Six engine tests fail on Windows. Not because Vietnamese behaviour is wrong — every
transformation test passes — but because the tests assert macOS bundle identifiers
against an exclusion default table that Phase 2 correctly made platform-conditional.
The premise check the whole port rests on cannot be run until they are fixed.

## The measurement, taken on a real Windows machine

```
cargo test -p glowkey-engine --no-fail-fast
164 tests: 158 passed, 6 failed
```

```text
config::tests::defaults_match_shipped_behavior
config::tests::exclusion_list_merges_defaults_and_respects_tombstones
config::tests::a_pre_port_custom_hotkey_loads_as_the_key_it_was_recorded_on
session::always_macro_stays_out_of_excluded_apps
session::switching_into_excluded_app_stops_transformation_immediately
session::terminal_hotkey_unexclusion_is_session_only
```

`cargo check --workspace` is green on Windows already. So is every test covering
Telex, VNI, Simple Telex, order-independent tones, Quick Telex, brackets, auto-fix,
the stop-coda rule, mid-word spell check and its escape, committed-word history,
`backspace_visible_char`, per-word overrides, macros and tombstones. **The port's
central assumption holds.** What does not hold is the test suite's portability.

Note the count: 164, not the 160 the issue quotes. Phase 2 added tests. Report the
number you actually observe rather than the number you expected.

## What is actually wrong

Phase 2 moved the shipped exclusion table behind `crate::exclusion_defaults`, so
`DEFAULT_EXCLUSIONS` is now per-target: `com.apple.Terminal` and
`com.mitchellh.ghostty` on macOS, `windowsterminal.exe`, `pwsh.exe`, `code.exe` and
friends on Windows. The tests were not moved with it. They still spell macOS
identities as literals:

```rust
assert!(s.exclusions.iter().any(|id| id == "com.apple.Terminal"));
assert!(list.is_excluded("com.mitchellh.ghostty"));
```

On Windows those identities are not defaults, so the assertions are false — and
correctly so.

The hotkey test fails for the same root cause wearing a different coat: it compares
a whole `Settings` value against `Settings::default()`, and the two `exclusions`
vectors differ by platform. The hotkey half of that test — `macos_keycode: Some(40)`
read through the alias, `windows_vk: None` — passes.

`exclusion_list_merges_defaults_and_respects_tombstones` is the one worth keeping
honest. It tests the *merge rule* (`saved ∪ (defaults − tombstones)`), which is
platform-neutral and genuinely valuable. Only its example data is macOS.

## Requirements

- Functional: `cargo test -p glowkey-engine` green on Windows **and** on macOS.
- Functional: every test keeps testing the rule it tested before. A test that passes
  on both platforms by asserting less is a regression wearing a fix's clothes.
- Non-functional: no `cfg(target_os)` added to the engine's non-test code. The
  engine's platform-freedom is a CI-enforced property and this phase must not spend it.

## Architecture

Three repair shapes, in order of preference. Prefer the first; use the second only
where the rule under test is genuinely about the shipped table.

**1. Test the rule with data the test owns.** The merge rule, the tombstone rule and
the session-suspension rule do not need a shipped identity to be exercised. Give them
literals of their own that are defaults on neither platform, and they become
platform-neutral by construction rather than by conditional.

**2. Ask the table rather than restate it.** Where the point *is* the shipped table —
`defaults_match_shipped_behavior` — assert against
`crate::exclusion_defaults::DEFAULT_EXCLUSIONS` instead of a literal, plus a
per-target assertion that the table names this platform's terminal at all. The test
then says "the shipped defaults protect terminals here", which is the thing actually
worth defending.

**3. Compare the fields that are under test.** For the hotkey round-trip, compare the
hotkey and the non-exclusion fields rather than the whole `Settings` value. The test's
name says what it is for; the exclusions vector is incidental to it.

For the two fixture files (`settings-macos-custom-hotkey.json`,
`settings-real-macos.json`) — leave them exactly as they are. They are real files
lifted off a real macOS installation and their value is that they are not synthetic.
A macOS settings file loading correctly on Windows is a *feature* of the schema, not
a problem to normalize away.

## Related Code Files

- Modify: `crates/glowkey-engine/src/config.rs` — the three `config::tests`
- Modify: `crates/glowkey-engine/tests/session.rs` — the three session tests
- Modify: `crates/glowkey-engine/tests/macro_table.rs` — `com.apple.Terminal` at line
  215 passes today (it sets a frontmost app rather than asserting a default), but hold
  it to the same standard while you are there
- Do not modify: `crates/glowkey-engine/tests/fixtures/*.json`
- Do not modify: `crates/glowkey-engine/src/exclusion.rs`,
  `crates/glowkey-engine/src/exclusion_defaults.rs` — the production tables are correct

## Implementation Steps

1. Run the suite on Windows and capture the six failures verbatim before touching
   anything, so the fix can be shown to address exactly them.
2. `session.rs`: replace shipped macOS identities with test-owned literals wherever
   the test is about exclusion *behaviour* rather than the shipped list.
   `terminal_hotkey_unexclusion_is_session_only` is the exception — it is about
   terminals specifically, so it needs this platform's terminal identity, which it
   should ask the defaults table for.
3. `config.rs`: rewrite `defaults_match_shipped_behavior` to assert the shipped table
   is non-empty and contains this target's terminal, sourced from `exclusion_defaults`.
4. `config.rs`: rewrite `exclusion_list_merges_defaults_and_respects_tombstones` to
   use a default drawn from the table plus a tombstone drawn from the table, so the
   merge rule is exercised without naming a platform.
5. `config.rs`: narrow `a_pre_port_custom_hotkey_loads_as_the_key_it_was_recorded_on`
   to the fields it is named after.
6. Run on Windows. Then confirm on macOS — this phase can break the macOS suite very
   easily and would not notice.
7. `cargo clippy -p glowkey-engine --all-targets -- -D warnings` on both.

## Success Criteria

- [ ] `cargo test -p glowkey-engine` on Windows: 164 passed, 0 failed
- [ ] `cargo test -p glowkey-engine` on macOS: unchanged, still fully green
- [ ] `cargo clippy -p glowkey-engine --all-targets -- -D warnings` silent on both
- [ ] No `cfg(target_os)` added outside `#[cfg(test)]` code
- [ ] The exclusion fixture files are byte-identical to before
- [ ] Each repaired test still fails when the rule it names is deliberately broken —
      check this by hand on at least the merge-rule test, because a test that cannot
      fail is the failure mode this phase is most exposed to

## Risk Assessment

**A test is "fixed" by weakening it.** The fastest repair for all six is to delete the
assertions, and it would be green everywhere. *Signal:* the diff removes more
assertions than it rewrites; a test body gets shorter. *Response:* step 7's
deliberate-break check. If breaking the merge rule does not turn the merge test red,
the test is decoration.

**The macOS suite breaks and nobody sees it.** This phase is executed on Windows and
the macOS CI job is the only thing watching the other half. *Signal:* CI's
`macos-latest` job goes red on the next push. *Response:* treat the CI matrix as part
of the phase, not as something downstream of it — do not land this without the macOS
job green.

**Scope creeps into the exclusion tables themselves.** Reading these tests invites
opinions about which Windows executables should ship as defaults. *Signal:* a diff
touching `exclusion_defaults.rs`. *Response:* that is a product decision with a real
user consequence (a wrongly-excluded app looks like GlowKey being broken), and it
belongs to Phase 6, where someone is typing into those applications. Not here.
