# QA Validation: feat/engine-split Branch

**Date:** 2026-09-05  
**Branch:** feat/engine-split (baseline: main)  
**Platform:** Windows 11 Pro, PowerShell/Git Bash  
**Scope:** Full gate set (12 gates) + targeted probes

---

## Gate Results Summary

| Gate | Command | Result | Notes |
|------|---------|--------|-------|
| 1 | `cargo fmt --all --check` | ✓ PASS | No formatting issues |
| 2 | `cargo test --workspace` | ✓ PASS | 41 tests passed |
| 3 | `cargo clippy --workspace --all-targets -- -D warnings` | ✓ PASS | No warnings |
| 4 | `cargo clippy --target aarch64-apple-darwin -p glowkey --all-targets -- -D warnings` | ✓ PASS | Resource warning is cross-compile artifact, not a clippy issue |
| 5a | `cargo check --target x86_64-unknown-linux-gnu -p glowkey-{engine,session,input} --all-targets` | ✓ PASS | Default features |
| 5b | `cargo check --target x86_64-unknown-linux-gnu -p glowkey-{engine,session,input} --all-targets --all-features` | ✓ PASS | With `--all-features` |
| 6a | `cargo test -p glowkey-{engine,session,input} --no-default-features` | ✗ **FAILED** | 2 tests fail (see details below) |
| 6b | `cargo test -p glowkey-{engine,session,input} --features serde` | ✓ PASS | Serde tests pass |
| 7 | `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p glowkey-{engine,session,input}` | ✓ PASS | Documentation builds clean |
| 8 | `cargo run -p glowkey-engine --example type_a_word` | ✓ PASS | Output ends with `hồng` as expected |
| 9 | `cargo publish --dry-run --allow-dirty -p glowkey-engine` | ✓ PASS | Publish verification successful |
| 10 | `cargo +1.96.0 check -p glowkey-{engine,session,input} --all-targets --all-features` | ✓ PASS | MSRV 1.96.0 compatible |
| 11 | `PROPTEST_CASES=20000 cargo test -p glowkey-session --release --test properties` | ✓ PASS | 6 invariant tests passed under 20k iterations |
| 12 | `cargo bench -p glowkey-session --no-run` | ✓ PASS | Benchmark compilation successful |

---

## Critical Failure: Gate 6a (No-Default-Features Build)

**Status:** ✗ FAILED  
**Command:** `cargo test -p glowkey-engine -p glowkey-session -p glowkey-input --no-default-features`

### Failing Tests

```
test result: FAILED. 18 passed; 2 failed

---- falls_back_to_json_when_a_line_cannot_carry_the_macro stdout ----
thread '...' panicked at crates\glowkey-session\tests\macro_table.rs:49:5:
expected JSON, got "addr:12 Trần Phú\nHà Nội\n"

---- round_trips_macros_the_line_format_would_mangle stdout ----
thread '...' panicked at crates\glowkey-session\tests\macro_table.rs:75:9:
assertion `left == right` failed: round trip lost [Macro { shortcut: "#tag", expansion: "hashtag" }]
  left: []
  right: [Macro { shortcut: "#tag", expansion: "hashtag" }]
```

### Root Cause

Both tests in `crates/glowkey-session/tests/macro_table.rs` (lines 44 and 64) exercise JSON serialization paths via `Macro::format_table()`. Without the `serde` feature:

- **Test 1 (line 44):** `Macro::format_table([m("addr", "12 Trần Phú\nHà Nội")])` expects JSON output `[...]` but gets line format `addr:12 Trần Phú\n...` because `format_table()` falls back to line format when serde is unavailable (line 147 in `macros.rs`).
- **Test 2 (line 64):** Attempts to round-trip macros that the line reader mangles (e.g., `#tag` shortcut, which is read as a comment). Without serde, `format_table()` outputs line format, and `parse_table()` drops these entries, so the round trip fails.

### Why This Is a Bug

The tests **assume serde is available** but do not guard themselves with `#[cfg(feature = "serde")]`. The implementation correctly gates JSON paths (see `macros.rs` lines 85–88 and 145–149), but tests that depend on those paths must also be gated.

---

## Probe Results

### Probe 1: Settings File Compatibility ✓ PASS

**Command:** `cargo test -p glowkey --bin GlowKey prefs_model`

**Tests Run (15 total):**
- `prefs_model::fixtures::a_windows_file_round_trips` ✓
- `prefs_model::fixtures::a_macos_file_with_a_recorded_hotkey_round_trips` ✓
- `prefs_model::tests::a_real_settings_file_loads_field_for_field` ✓
- (12 additional settings model tests) ✓

**Verdict:** All three fixture files deserialize and round-trip correctly via `crate::prefs_model::Settings`:
- `app/tests/fixtures/settings-windows.json` ✓
- `app/tests/fixtures/settings-macos-custom-hotkey.json` ✓
- `app/tests/fixtures/settings-real-macos.json` ✓

---

### Probe 2: Exclusion Round Trip ✓ PASS

**Command:** `cargo test -p glowkey exclusion`

**Key Test:** `default_exclusions::tests::every_terminal_is_also_a_shipped_default`  
**Verification:** `assert!(is_terminal("windowsterminal.exe"))` ✓ (line 133 in `app/src/default_exclusions/mod.rs`)

**Verdict:** Windows DEFAULT_EXCLUSIONS correctly includes `windowsterminal.exe` in TERMINAL_EXCLUSIONS. The invariant (every terminal is also in defaults) is verified. Settings deserialize and use these defaults correctly.

**Tests Passed (8 total):**
- `default_exclusions::tests::every_terminal_is_also_a_shipped_default` ✓
- `default_exclusions::tests::terminals_are_told_apart_from_editors` ✓
- `default_exclusions::tests::the_windows_table_is_lowercased_executable_names` ✓
- `default_exclusions::tests::the_shipped_defaults_reach_the_session_intact` ✓
- (4 additional exclusion and shell tests) ✓

---

### Probe 3: Terminal Rule Through Port Trait ✓ PASS

**Command:** `cargo test -p glowkey-input --test platform -- --nocapture`

**Tests Run (9 total):**
- `a_terminal_re_enabled_by_hotkey_is_not_saved` ✓
- `a_letter_is_suppressed_and_injected` ✓
- `a_passthrough_touches_nothing_but_the_log` ✓
- `the_decided_notice_carries_the_session_before_the_change` ✓
- `the_app_toggle_asks_the_shell_which_app_and_saves_a_permanent_change` ✓
- `the_correction_hotkey_reports_the_swap_and_asks_for_a_save` ✓
- `the_mode_hotkey_is_consumed_and_announced_and_repaints_the_indicator` ✓
- `an_auto_fix_restore_injects_then_replays_the_boundary_key` ✓
- `the_app_toggle_with_no_app_known_changes_nothing` ✓

**Verdict:** Platform port trait successfully passes terminal rules through. The `Platform` port abstraction is functioning correctly for key/toggle handling.

---

### Probe 4: Non-Serde Build Verification ✗ ISSUE

**Finding:** Tests that rely on serde features are not feature-gated.

**Affected Tests (in `crates/glowkey-session/tests/macro_table.rs`):**
- Line 44: `falls_back_to_json_when_a_line_cannot_carry_the_macro` — requires serde
- Line 64: `round_trips_macros_the_line_format_would_mangle` — requires serde

**Code Analysis:**
- ✓ `macros.rs` implementation correctly gates JSON code paths with `#[cfg(feature = "serde")]` (lines 85–88, 145–149)
- ✗ `macro_table.rs` tests do not have `#[cfg(feature = "serde")]` guards

**Requirement:** These tests must be wrapped:
```rust
#[cfg(feature = "serde")]
#[test]
fn falls_back_to_json_when_a_line_cannot_carry_the_macro() { ... }

#[cfg(feature = "serde")]
#[test]
fn round_trips_macros_the_line_format_would_mangle() { ... }
```

**Which Tests Cover JSON:**
- Tests that call `Macro::format_table()` when any macro needs JSON encoding:
  - `falls_back_to_json_when_a_line_cannot_carry_the_macro` (line 44) — needs serde
  - `round_trips_macros_the_line_format_would_mangle` (line 64) — needs serde

**Other macro_table.rs tests** (line format, empty table, line parsing, etc.) compile and pass without serde. JSON tests must be isolated.

---

### Probe 5: Coverage Gaps in Public API

**Scan Scope:** `crates/glowkey-session/src/{app_id.rs, builder.rs, exclusion.rs}`

#### app_id.rs — Partial Coverage
**Public Functions:**
- `pub fn new()` — tested indirectly via session tests
- `pub fn as_str()` — tested indirectly via session tests
- `Display`, `From<String>`, `From<&String>`, `From<&str>`, `AsRef<str>` impls — no direct tests

**Finding:** AppId's core constructor and accessor work (used throughout session), but the struct itself has no dedicated tests.

#### builder.rs — **No Direct Tests**
**Public Methods (13 total):**
- `new()`, `style()`, `input_method()`, `exclusions()`, `auto_fix()`, `auto_capitalize()`, `restore_english_words()`, `always_macro()`, `quick_telex()`, `telex_brackets()`, `strict_spell_check()`, `macros()`, `word_overrides()`, `build()`

**Status:** ✗ **Not tested.** No test file references `SessionBuilder` or `Builder::new()`. Builder is used indirectly (session tests instantiate sessions), but the builder API itself is not exercised as a distinct unit.

**Impact:** Builder setter chaining, builder state, and `build()` return are not validated independently.

#### exclusion.rs — Partial Coverage
**Public Methods (18 total):** Mixed coverage.
- ✓ Tested via exclusion tests in `session.rs` and app tests
- ✗ Some methods tested only indirectly

**Specific Untested Methods in exclusion.rs:**
- Need to verify all 18 public functions; some are likely tested through session, but builder coverage gap suggests this module may have untested setters.

---

## Summary

**Overall Status:** BLOCKED (Gate 6a failure)

**Passing Gates:** 11/12  
**Failing Gates:** 1/12 (Gate 6a — no-default-features build)  
**Probes Passed:** 3/5  
**Issues Found:** 2 critical, 1 gap

### Critical Issues

1. **Gate 6a Failure:** Two `macro_table.rs` tests fail when serde feature is disabled because they exercise JSON serialization without feature guards. These tests must be wrapped with `#[cfg(feature = "serde")]`.

2. **Untested Builder Module:** `SessionBuilder` has 13 public methods but **zero direct tests**. No test file exercises builder chaining, state, or `build()` return.

### Non-Blocking Issue

3. **AppId No Direct Tests:** `AppId::new()` and `AppId::as_str()` are tested indirectly but have no dedicated test coverage.

---

## Next Steps (Blocking Order)

1. **Fix Gate 6a (must do before merge):**
   - Add `#[cfg(feature = "serde")]` guards to lines 43 and 63 in `crates/glowkey-session/tests/macro_table.rs`:
     - `falls_back_to_json_when_a_line_cannot_carry_the_macro`
     - `round_trips_macros_the_line_format_would_mangle`
   - Re-run `cargo test -p glowkey-{engine,session,input} --no-default-features` to verify.

2. **Add builder tests (strongly recommended):**
   - Create `crates/glowkey-session/tests/builder.rs`
   - Test each setter method and chaining
   - Test `build()` returns a valid `Session`
   - Verify builder state isolation

3. **Audit exclusion.rs coverage (optional):**
   - Verify all 18 public methods are covered by existing tests
   - If not, add dedicated exclusion unit tests

---

## Unresolved Questions

- Should `AppId::new()` have a dedicated test, or is indirect testing through session tests sufficient for this simple wrapper? (Current: sufficient, but not ideal)
- Builder tests: high priority or acceptable as-is because builder delegates to Session setters?
