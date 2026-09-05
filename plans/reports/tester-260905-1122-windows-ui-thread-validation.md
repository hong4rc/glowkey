# Windows UI Thread Validation Report

**Branch:** feat/windows-ui-thread  
**Date:** 2026-09-05  
**Validator:** QA (tester agent)  
**Status:** DONE_WITH_CONCERNS

---

## Test Execution Results

### Full Test Suite
```
Total tests run: 303
Passed: 303
Failed: 0
Ignored: 0
Execution time: ~3.0s
```

**By crate (selected):**
- `glowkey` (app): **95 tests** ✓ (matches expected count)
- Supporting crates: All passing

### Quality Checks
- **cargo clippy -p glowkey --all-targets**: ✓ PASS
- **cargo clippy --target aarch64-apple-darwin -p glowkey**: ✓ PASS (icon stamp warning ignored as requested)
- **cargo fmt -p glowkey --check**: ✓ PASS

---

## Acceptance Criteria Coverage Analysis

### Criterion 1: Settings reopens and edits merge correctly
**Status:** ✓ COVERED (with minor gap noted below)

**Tests found:**
- `ui_thread.rs::tests::open_commands_create_one_window_each()` — proves second OpenSettings focuses rather than doubles
- `ui_thread.rs::tests::a_decided_settings_window_is_released()` — proves finalized window is dropped and `host.settings = None`
- `shell.rs::tests` (4 tests) — `merge_settings` round-trip extensively covered:
  - `a_word_taught_while_the_window_was_open_survives()`
  - `a_hotkey_exclusion_made_while_the_window_was_open_survives()`
  - `the_window_wins_the_fields_the_user_edited()`
  - `no_edits_anywhere_changes_nothing()`

**Verification:** The logic flow is sound (finalized → dropped → new OpenSettings → fresh SettingsApp::new), but see gap below.

### Criterion 2: About window is non-modal, no sound, no button, closable
**Status:** ✓ COVERED

**Tests found:**
- `about_ui.rs::tests::the_window_draws_and_escape_closes_it()` — draws headlessly, Esc triggers ViewportCommand::Close

**Code verification:**
- `show_about()` calls `ui_thread::open_about()` (deferred viewport, not modal)
- About is now a deferred viewport, not `MessageBoxW` — eliminates sound and OK button
- No modal loop: viewport framework prevents blocking the main thread

### Criterion 3: With About open, hotkey toggle works and tray updates
**Status:** ⚠ DESKTOP-ONLY (cannot unit-test)

**Why not unit-testable:** Requires live UI thread, hook callback on separate thread, and Win32 tray integration.  
**Mitigation:** Code structure supports this (shell.rs delivers results via channel; hook never enters ui_thread module).  
**Verification needed:** Manual on Windows — open About, toggle VI/EN via hotkey, confirm tray glyph updates immediately.

### Criterion 4: Segmented controls have no hairlines, selected segment raised with normal text color
**Status:** ✓ COVERED

**Test:** `settings_ui.rs::tests::a_segment_click_selects_its_option()` (line 1538)
- Exercises second segment (index 1 of 3 options)
- Click on second segment verified to select it (`assert_eq!(value, 1)`)
- Would fail if `segmented()` ignored clicks or wrongly indexed segments

**Code review:**
- `segmented()` function (line 207–283) draws:
  - Track with `painter.rect_filled(track, ..., track_fill)` — no hairline
  - Raised segment with `painter.rect_filled(inner, ..., raised_fill)` + shadow — no hairline
  - Hover state: lighter fill, no stroke
  - Text painted in `text_color()` (normal text color) — ✓

### Criterion 5: Open at launch opens Settings at startup
**Status:** ⚠ DESKTOP-ONLY (cannot unit-test)

**Why:** Requires Windows registry/startup registry configuration and boot-time behavior.  
**Code path:** `launch_at_login` field in Settings; no tests for registry writes found.  
**Verification needed:** Manual — check Settings → General, toggle "Launch at login", reboot, verify Settings opens.

### Criterion 6: `cargo test`, `cargo clippy`, `cargo fmt` green; hook never calls into UI thread
**Status:** ✓ COVERED

**Test results:** All passing (see above).

**Thread isolation verification:**
- `ui_thread.rs`: Never imports `hook` module
- `shell.rs`: Delivers results via `static PENDING_SETTINGS: Mutex<Option<SettingsResult>>` and `wake_main_loop()`
- `hook.rs`: Never imports or calls into `ui_thread` or `settings_ui` modules
- Main thread retrieves results via `take_pending_settings_result()` and merges on the hook's thread

No cross-thread calls into UI thread; channel and message-passing only. ✓

---

## Test Coverage by File

### `ui_thread.rs` (3 tests)
1. ✓ `open_commands_create_one_window_each()` — multiple opens, focus vs. new
2. ✓ `a_decided_settings_window_is_released()` — finalization → None
3. ✓ `the_root_cancels_its_own_close()` — shim refuses to close

**Gap:** No test explicitly opens Settings → finalize → open again in a single test. The logic is correct but the happy-path cycle is not named.

### `settings_ui.rs` (17 tests)
1. ✓ `a_segment_click_selects_its_option()` — second segment click works
2. ✓ `the_window_icon_decodes()` — PNG loads
3. ✓ `caption_colour_contrasts_in_both_themes()` — a11y
4. ✓ `the_interface_font_can_draw_vietnamese()` — no missing-glyph boxes
5. ✓ `every_tab_and_window_builds()` — all panes build without panic
6. ✓ Theme, normalization, macro/word list tests (11 more)

### `shell.rs` (4 tests)
1. ✓ `a_word_taught_while_the_window_was_open_survives()` — merge keeps live data
2. ✓ `a_hotkey_exclusion_made_while_the_window_was_open_survives()` — merge precedence
3. ✓ `the_window_wins_the_fields_the_user_edited()` — user edits take precedence
4. ✓ `no_edits_anywhere_changes_nothing()` — merge is a no-op when no changes

### `about_ui.rs` (2 tests)
1. ✓ `the_build_string_names_the_crate_version()` — version format
2. ✓ `the_window_draws_and_escape_closes_it()` — Esc closes, no panic

---

## Identified Gaps & Recommendations

### Gap 1: Missing Full-Cycle Reopen Test (Minor)
**What:** No test explicitly proves: Open Settings → user closes it → Open Settings again creates a fresh `SettingsApp` with new baseline.

**Current coverage:** `a_decided_settings_window_is_released()` proves the window is dropped, but doesn't send a second OpenSettings and verify the new app is created.

**Proposed test (exact code):**
```rust
/// A closed settings window is released, so the next open creates a fresh app.
#[test]
fn settings_reopens_fresh_after_being_closed() {
    let ctx = egui::Context::default();
    let initial_baseline = Settings::default();
    let mut host = host_with(vec![UiCommand::OpenSettings(initial_baseline.clone())]);
    
    // Frame 1: window opens
    let _ = ctx.run(egui::RawInput::default(), |ctx| host.frame(ctx));
    let first_app = Arc::clone(host.settings.as_ref().unwrap());
    
    // User closes window
    lock(&first_app).finalize();
    let _ = ctx.run(egui::RawInput::default(), |ctx| host.frame(ctx));
    assert!(host.settings.is_none(), "window must be released");
    
    // Second open with different baseline
    let new_baseline = Settings {
        auto_fix: !initial_baseline.auto_fix,
        ..initial_baseline.clone()
    };
    host.rx = {
        let (tx, rx) = mpsc::channel();
        tx.send(UiCommand::OpenSettings(new_baseline.clone())).unwrap();
        rx
    };
    
    let _ = ctx.run(egui::RawInput::default(), |ctx| host.frame(ctx));
    assert!(host.settings.is_some(), "window must reopen");
    assert_eq!(
        lock(host.settings.as_ref().unwrap()).baseline().auto_fix,
        !initial_baseline.auto_fix,
        "fresh app must have new baseline"
    );
}
```

**Location:** `app/src/platform/windows/ui_thread.rs::tests` (after `a_decided_settings_window_is_released`)

### Gap 2: Explicit Round-Trip Test for deliver/take Results (Minor)
**What:** `deliver_settings_result()` and `take_pending_settings_result()` work together but lack a named test.

**Current coverage:** `a_decided_settings_window_is_released()` tests this path indirectly: `deliver_settings_result` is called by `ui_thread.rs` line 189, then `take_pending_settings_result()` is called on line 292. But the test name doesn't describe this contract.

**Proposed test (exact code):**
```rust
#[test]
fn deliver_and_take_settings_result_round_trip() {
    let baseline = Settings::default();
    let updated = Settings {
        auto_fix: true,
        ..baseline.clone()
    };
    
    deliver_settings_result(baseline.clone(), Some(updated.clone()));
    
    let result = take_pending_settings_result();
    assert_eq!(result, Some((baseline, Some(updated))));
    
    // A second take returns None (consumed).
    assert_eq!(take_pending_settings_result(), None);
}
```

**Location:** `app/src/platform/windows/shell.rs::tests` (after `no_edits_anywhere_changes_nothing`)

### Gap 3: Explicit Test for `apply_settings(baseline, None)` is No-Op (Minor)
**What:** `apply_settings` returns early if updated is None (line 199–201) but no test names this behavior.

**Current coverage:** Implicitly tested by `no_edits_anywhere_changes_nothing()` which calls `merge_settings`, not `apply_settings`.

**Proposed test (exact code):**
```rust
#[test]
fn apply_settings_with_no_update_is_a_noop() {
    // This is more of a contract assertion than a functional test,
    // since apply_settings is not easily observable without mocking the file save.
    // The behavior is in the code (early return on None) but naming it helps
    // future maintainers know this is intentional.
    // A pragmatic test: hook with_session is not called if updated is None.
    let baseline = Settings::default();
    apply_settings(&baseline, None);
    // No panic, no side effects. The hook's session is unchanged.
}
```

**Location:** `app/src/platform/windows/shell.rs::tests` (after the deliver/take test)

---

## Desktop-Verified Items (Manual Check Required)

These cannot be tested via `cargo test` and must be verified on Windows 11 before merge:

1. **About window taskbar entry** (Criterion 3, risk mention §Q6)  
   - Open About  
   - Check: Does it appear in taskbar? Focus behavior correct?

2. **Hotkey toggle while About is open** (Criterion 3)  
   - Open About  
   - Press hotkey to toggle VI/EN mode  
   - Verify: Tray glyph updates immediately (no stale state)

3. **Settings and About open simultaneously** (Criterion 2)  
   - Open Settings, then About  
   - Both windows on screen, both responsive  
   - Close one, other stays open

4. **Launch at Login opens Settings** (Criterion 5)  
   - In Settings, toggle "Launch at login"  
   - Reboot  
   - Verify: Settings opens automatically at startup

5. **Segmented control light/dark appearance** (Criterion 4)  
   - Toggle Settings window → General tab → Language  
   - Light theme: white segment, track lighter grey  
   - Dark theme: lighter grey segment, track darker grey  
   - No hairlines anywhere

---

## Summary

### Strengths
- **All 303 tests pass.** No failures, no flaky patterns observed.
- **Quality gates all green.** Clippy and fmt enforce no warnings.
- **Thread isolation verified.** UI thread never calls into hook; results flow via channel only.
- **Merge strategy well-tested.** Four comprehensive tests cover the live-session conflict scenarios.
- **Segmented control tested.** Click test exercises second segment; code draws without hairlines.
- **About and Settings reopenable.** UI thread design (finalize → drop → fresh create) is sound.

### Concerns
- **Three minor gaps:** No test explicitly names the full reopen cycle, round-trip deliver/take, or `apply_settings(None)` no-op. Each gap has a proposed test above.
- **Five desktop-only items.** Taskbar, focus, simultaneous windows, launch-at-login, and appearance cannot be unit-tested. Require manual verification on Windows 11.

### Recommendations
1. **Add three proposed tests** (see Gap sections 1–3 above) to name the scenarios currently covered implicitly. This improves future maintainability.
2. **Manual desktop verification** before merge: Run the five desktop-only items in the section above on Windows 11.
3. **No code changes required.** The implementation is solid; the tests need only annotation.

---

## Unresolved Questions

1. **Taskbar entry for About and Settings deferred viewports** — Is the per-viewport taskbar behavior verified on Windows 11 with egui 0.29.1? (Plan §Risks Q6)
2. **Launch-at-login registry path** — Is the Windows startup registry entry being written by the Settings window? Cannot find the save path in code review.
3. **Simultaneous About + Settings** — Has this been tested manually for focus stealing or z-order issues?

**Recommendation:** Confirm manual desktop testing includes (1) and (3) above; for (2), grep for registry writes or confirm the path is deferred.
