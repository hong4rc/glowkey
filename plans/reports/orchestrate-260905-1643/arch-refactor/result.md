# GlowKey — architecture & refactor audit

Read-only audit of the workspace at `ce51ff0` (branch `main`, clean tree).
Verification run on this host: `cargo clippy --workspace --all-targets` — **clean, no
warnings**; `cargo test --workspace` — **22 targets, 379 tests, all pass** (Windows host,
so `app/src/platform/macos/*` is not compiled or tested here).

The 0012 layering holds. This is a drift-and-duplication report, not a redesign.

---

## 0. Layering verdict (task item 3)

Checked mechanically:

- `grep -rn "target_os|windows|macos|objc|cocoa" crates/*/src` → 5 hits, **all in prose or
  data**: `hotkey.rs:54,57` (serde aliases `keycode`/`macos_keycode` — a file-format
  compatibility fact, not an OS dependency), `app_id.rs:7` (doc comment),
  `english.rs:48` (the English word "windows" in the dictionary).
- `grep -rn "unsafe" crates/*/src` → 2 hits, both the `#![deny(unsafe_code)]` line in
  `glowkey-input/src/lib.rs:35` and its doc comment.
- Nothing policy-shaped has leaked into `app/`. `settings_spec.rs` is presentation data,
  `default_exclusions/` is the product's platform tables (0012 puts them there on
  purpose), `prefs_model.rs` is the file schema, `session_adapter.rs` is the one
  file-to-session seam.

**No layering violations found.** Every finding below is *inside* `app/`, in the shell
layer 0012 deliberately left per-platform — which is exactly where it predicted the next
copy would appear.

---

## 1. File and function size outliers (task item 1)

`find app crates -name '*.rs' -exec wc -l {} +`, 25,030 lines total. Everything over ~480:

| Lines | File | Verdict |
|---|---|---|
| 2120 | `app/src/platform/windows/settings_ui.rs` | **Missing seam.** 1550 lines of code + 570 of tests. Holds the egui spec walker *and* three list editors (`excluded_body` :1006, `macros_body` :1102, `words_body` :1228) *and* the validation rules (:1470–1550). See P2-1. |
| 1072 | `app/src/platform/windows/tray.rs` | Borderline justified: ~230 lines are `draw_glyph` GDI (:473–634), the rest is one Win32 window + menu. Menu item list should move out (P1-3). |
| 986 | `crates/glowkey-session/src/session.rs` | Justified. It is the facade 0012 names; 60 `pub fn`, mostly 3–8 lines, each a documented policy knob. Splitting it would move names without reducing anything. |
| 897 | `app/src/platform/macos/tests.rs` | Justified (test file, 31 tests). |
| 811 | `crates/glowkey-session/tests/properties.rs` | Justified (proptest invariant suite). |
| 793 | `app/src/settings_spec.rs` | Justified — ~430 lines are the `TABS` const, i.e. the data 0010 exists to have in one place. |
| 778 | `crates/glowkey-input/tests/ladder.rs` | Justified. |
| 690 | `crates/glowkey-engine/src/engine.rs` | Justified; longest fn is `rerender` (:430) and the spell-check table. |
| 679 | `app/src/prefs/mod.rs` | **Missing seam.** :78–428 is one `define_class!` holding 26 ObjC action methods; :151–243 alone is nine near-identical checkbox handlers. Collapses under P1-1. |
| 602 | `app/src/platform/windows/hook.rs` | Justified by subject (it is the hot path and the state that must stay on one thread) — but **zero tests**, see P1-4. |
| 595 | `app/src/platform/windows/adapt.rs` | Justified: 200 lines of code, 180 of tests, and the dead-key/AltGr handling is irreducible. |
| 535 | `app/src/prefs/tabs.rs` | Justified for AppKit layout; the two 9-arm toggle matches (:459, :500) leave under P1-1. |
| 502 | `app/src/menu_bar.rs` | **Missing seam.** `rebuild` (:230–396) is 166 lines of straight-line menu construction; `update_glyph` (:196–227) is an untested reimplementation of `indicator.rs`. |
| 494 | `app/src/platform/macos/mod.rs` | Justified. |
| 485 | `app/src/prefs_model.rs` | Justified (schema + 15 serde tests). |
| 484 | `app/src/platform/windows/shell.rs` | Justified; `merge_settings` (:277) is load-bearing and tested six ways. |

No single function is pathological. The length problems are all *lists that should be
data*: menu items, checkbox handlers, settings accessors.

---

## 2. Ranked backlog

### P0-1 — The macOS tap callback writes the log synchronously, on every keystroke

**Where.** `app/src/platform/macos/dispatch.rs:104` (`Notice::Decided` →
`crate::log::log`), `emit.rs:31`, `emit.rs:86`, `emit.rs:93` (`EMIT took=` on *every*
emit), `mod.rs:361` (tap re-enable), `health.rs:80`. The sink is
`app/src/log.rs:133–161`: `mutex.lock()` → `write_all` → **`flush()`** → and, once
`written > MAX_BYTES` (5 MB), `rotate()` at `log.rs:121` which does a `rename` and
re-`open`s the file — all inside the callback.

**Why it hurts today.** `docs/decisions/0008` is unambiguous: *"anything that can wait on
another process is forbidden inside `tap_dispatch` and everything it calls… file I/O, and
any lock that a non-tap thread can hold."* The keystroke path currently performs two
synchronous flushed writes per transforming keystroke, and periodically a rename. The
Windows side already treats this as unacceptable and solved it —
`app/src/platform/windows/hook_log.rs` is a bounded `sync_channel` with a writer thread,
and its module doc says exactly why. **The fix exists in the repo and is filed under the
wrong platform.** `hook_log.rs` contains no Win32 whatsoever (verified: its only imports
are `std::sync::*` and `crate::log`).

Note also that `emit.rs:93`'s `EMIT took=` measurement *excludes* the log write that
reports it, so the instrumentation cannot see this cost.

**Proposed change.** Move `platform/windows/hook_log.rs` to `app/src/log_queue.rs`
unchanged, start its writer thread from both `run()` entry points, and route the tap
callback's logging through it. Then make `log::log` private to the queue's writer
(`pub(crate)` → module-private + a `log_queue::log` re-export) so a synchronous write from
a callback becomes a compile error rather than a review item. That is a net *removal* of
one of the two logging APIs, not a new abstraction.

**Blast radius.** `app/` only; no crate, no file format, no settings. macOS log line
ordering becomes "queued in order, written slightly later" — the `Notice::Decided`-first
contract in `glowkey-input/src/platform.rs` is preserved because the queue is FIFO. One
consequence to accept deliberately: the comment at `dispatch.rs:96–101` claims the KEY
line is "on disk already" before a possibly-panicking emit path. With a queue it is
*enqueued*, not on disk. Under `catch_unwind` the process survives and the writer drains,
so the line still lands; say so in the comment.

**Test that would protect it.** `hook_log.rs` already has 2 tests; add one asserting
`log_queue::log` returns without touching the filesystem when no writer exists, and keep
the module privacy as the real enforcement.

---

### P1-1 — A macOS checkbox is bound through four parallel 9-arm matches; Windows uses one

**Where.**

| | macOS | Windows |
|---|---|---|
| read value | `app/src/prefs/tabs.rs:459–472` | `settings_ui.rs:997–1004` via `settings_spec.rs:129–143` |
| write value | `app/src/platform/macos/settings.rs` (45 `*_and_save` methods, 399 lines) | same `settings_field` map |
| bind action | `app/src/prefs/tabs.rs:500–513` (`toggle_selector`) | — |
| handle action | `app/src/prefs/mod.rs:151–243` (nine `*_changed` ObjC methods) | — |

**Why it hurts today.** `settings_spec.rs` was created (0010) so that one row edit lands on
every platform. It achieved that for *layout* and missed it for *binding*: adding a tenth
`Toggle` is one edit on Windows and four on macOS, and the failure mode of missing one is
a checkbox that renders, clicks, and does nothing. The spec's own tests
(`settings_spec.rs:547+`, 11 tests) check placement, not binding, so nothing catches it.
`settings_spec.rs:130` already carries the scar: `#[cfg_attr(target_os = "macos",
allow(dead_code))]` on `settings_field`, i.e. the shared map exists and macOS opts out of it.

**Proposed change.** Add `Toggle::apply(self, state: &TapState, on: bool)` next to
`toggle_value` (one match, replacing the four), and collapse the nine ObjC handlers into
one `toggleChanged:` whose sender tag is the `Toggle` index — the same tag technique
`prefs/mod.rs:330` already uses for word rows. `macos/settings.rs` loses roughly 20 of its
45 methods.

**Blast radius.** macOS shell only. No spec change, no file-format change.

**Test that would protect it.** A table test over `Toggle::ALL` asserting
`toggle_value(apply(t, v)) == v` for both values — runs on macOS, and the equivalent
already exists implicitly on Windows through `settings_field`.

---

### P1-2 — The indicator state machine exists twice: tested on Windows, inline and untested on macOS

**Where.** `app/src/platform/windows/indicator.rs:73–172` (`state()` + `glyph()` +
`dimmed()` + `describe()`, 8 tests, and the severity ordering is documented as *the rule*)
versus `app/src/menu_bar.rs:196–227` (`update_glyph`, an inline three-branch `if`, no
tests) plus `menu_bar.rs:238–268` (the dead-tap menu header) and `menu_bar.rs:260–276`
(the excluded/VN/EN header).

**Why it hurts today.** These are the same product decision — *what does the user see when
GlowKey is off, off here, or broken* — expressed twice, and 0007 makes it a defect class
rather than cosmetics. They have already diverged: Windows distinguishes two breakages and
names the offending app in the tooltip; macOS has one. `indicator.rs` is pure logic over
`InputMode`/`bool`/`Reach` — the only platform type is `Reach`, which macOS can supply as a
constant (it has no UIPI analogue).

**Proposed change.** Move `Indicator`, `Breakage`, `state()` and `describe()` to
`app/src/indicator.rs` (shared by both shells, as `settings_spec.rs` is). macOS passes
`hook_installed = !tap_is_dead()` and `Reach::Ok`. `menu_bar::update_glyph` becomes
`glyph()` + `dimmed()`; the menu header becomes `describe()`.

**Blast radius.** Both shells' indicator surface. Windows behaviour must not change (its
8 tests pin it); macOS gains the second breakage variant it never had, so check the
strings before landing.

**Test that would protect it.** The existing 8 tests in `indicator.rs` move with the code
and start covering macOS for the first time.

---

### P1-3 — The tray menu and the menu-bar menu are the same item list, written twice, already drifted

**Where.** `app/src/menu_bar.rs:230–396` (166 lines) ↔
`app/src/platform/windows/tray.rs:718–830` (113 lines).

Same list, same order, same bilingual strings for most items. Already drifted:

| Item | macOS | Windows |
|---|---|---|
| launch | "Open at login" / "Khởi động cùng máy" | "Start at login" / "Khởi động cùng máy" |
| log | "Reveal Log in Finder" | "Show log folder" |
| mode | "Vietnamese input ({hotkey})" | "Vietnamese input" (no hotkey shown) |
| per-app | "Enable/Disable for “X”" | "Vietnamese in X" (checkmark) |
| only there | Quick Guide…, Reset input (if stuck) | Reinstall the keyboard hook |

**Why it hurts today.** This is precisely the drift 0010 was written about — same
wording/caption/shape divergence, same three-week timescale, now in the menu instead of
the settings window — and the remedy is already established in this repo, so it is a
pattern to apply rather than one to invent.

**Proposed change.** `app/src/menu_spec.rs`: an ordered `&[MenuItem]` where an item is
`{ command: MenuCommand, label: Text, check: Option<Check> }`, plus the small per-platform
availability filter (`Reset input` is macOS's circuit breaker, `Reinstall hook` is
Windows'; both stay, gated by an `availability` field rather than by being absent from one
list). Each shell keeps its own renderer and its own `handle_command`, exactly as the two
settings renderers do. Items whose wording is legitimately platform-native ("Finder" vs
"folder") carry two `Text`s or stay per-platform — do not force a lowest common
denominator.

**Blast radius.** Both shells' menu construction. No behaviour change if the spec is
transcribed item-for-item; the drift table above is the checklist.

**Test that would protect it.** The same shape `settings_spec.rs` uses: every
`MenuCommand` appears exactly once, both languages present, no empty label. Plus the
existing `tray.rs:996 every_command_id_is_unique`.

---

### P1-4 — The Windows keystroke path has no tests; the macOS one has 31

**Where.** `app/src/platform/windows/hook.rs` (602 lines: `dispatch` :354,
`handle_key` :397, `HookPort` :457–522, `take_pending_save` :561) — **zero `#[test]`**.
Compare `app/src/platform/macos/tests.rs` (31 tests driving `TapState::decide` with real
`CGEvent`s).

**Why it hurts today.** Every Windows-side finding in this report touches code adjacent to
this file, and a refactor there is currently unprotected. The state machine that *is*
tested (`shell.rs` merge, `adapt.rs` translation, `indicator.rs`, `settings_ui.rs`) brackets
`hook.rs` on both sides without covering it.

**Proposed change.** No production change. Add tests for the parts that need no Win32
call: `HookPort`'s `Platform` impl against a `Session` (mirroring
`crates/glowkey-input/tests/platform.rs`, which already has 9), and the
`pending_save`/`pending_refresh` drain (`take_pending_save` returns `Some` exactly once per
`request_save`). `handle_key`'s `KBDLLHOOKSTRUCT` argument is a plain repr(C) struct and
can be constructed in a test.

**Blast radius.** Tests only.

---

### P1-5 — Macro and personal-word validation is implemented three times, with different rules

**Where.** `crates/glowkey-session/src/session.rs:438–450` (`add_macro`: trims the
shortcut, rejects empty shortcut, replaces case-insensitively, **accepts an empty
expansion**, does not trim the expansion) and `session.rs:353–359` (`set_word_override`:
trims + lowercases, rejects empty) versus
`app/src/platform/windows/settings_ui.rs:1470–1550` (`normalize_exe_name`,
`normalize_word_keys`, `validate_macro` — **rejects an empty expansion** — `upsert_macro`,
`upsert_word_override`) versus the macOS path `app/src/prefs/mod.rs:372–418`, which asks
"Replace “X”?" before calling `add_macro_and_save` and does not check the expansion at all.

**Why it hurts today.** Same persisted data, two shells, two validity rules. A macOS user
can create a macro with an empty expansion that the Windows editor would refuse and that
`import_macros` discards; the Windows editor replaces a duplicate silently where macOS asks.
The Windows copies exist because that renderer edits a detached `Vec<Macro>` draft rather
than the live session, which is a legitimate reason for a *different call site* but not for
a different rule.

**Proposed change.** Put the rule once in `glowkey-session` as free functions over the
collections — `macros::upsert(&mut Vec<Macro>, shortcut, expansion, edit_index)` and
`overrides::upsert(&mut Vec<WordOverride>, keys, prefer, edit_index)` — and have both
`Session::add_macro` and `settings_ui.rs` call them. Pick one answer for the empty
expansion and write it in the doc comment.

**Blast radius.** `glowkey-session` gains two `pub fn`s (published crate — additive);
`settings_ui.rs` loses ~80 lines and 6 tests move with them. macOS behaviour changes only
if you choose to enforce the non-empty expansion there.

**Test that would protect it.** The 6 upsert tests at `settings_ui.rs:1578–1652` move into
the session crate and start covering both shells.

---

### P1-6 — A failed settings write is silent on Windows

**Where.** `app/src/settings_store.rs:56`, `:63`, `:66` — three `eprintln!` and no
`crate::log::log`. `app/src/main.rs:20–27` states the constraint plainly: the binary is
built `windows_subsystem = "windows"`, so *"`eprintln!` has nowhere to go on Windows, so
anything worth saying must reach `crate::log` instead."* The Windows `run()` obeys this
everywhere else (`platform/windows/mod.rs:90–101` pairs each `eprintln!` with a log line);
`settings_store` does not.

**Why it hurts today.** A settings write that fails — read-only profile, full disk,
roaming profile hiccup — produces no user-visible and no diagnosable trace on Windows.
Every user preference silently stops persisting. That is the same "silently dead, and the
indicator does not say so" class `0007` and `0009` both refuse to accept.

**Proposed change.** Replace the three `eprintln!` with `crate::log::log` (keeping stderr
via the existing `GLOWKEY_DEBUG` echo at `log.rs:159`). Two lines of change.

**Blast radius.** None behavioural.

**Test that would protect it.** `settings_store.rs` has no tests at all; a test that
`save()` into an unwritable path returns without panicking and after which `load()` still
yields the previous file would cover the whole function for the first time.

---

## 3. The `Platform` port and a third backend (task item 4)

`crates/glowkey-input/src/platform.rs` — six methods. Judged against a hypothetical
X11/Wayland shell:

**What is right.** `inject(backspaces, text)` / `replay_key()` / `request_save()` /
`request_indicator()` / `notify()` all map cleanly onto XTEST or a virtual-keyboard
protocol. `app_in_front() -> Option<AppId>` returning a cache is explicitly allowed.
`Notice` is `#[non_exhaustive]` with a defaulted `notify`, so a Linux shell implements
nothing it has no surface for. `Ctx` carrying only the resolved `toggle_hotkey`, with
⌃⇧E/⌃⇧W fixed in `hotkey.rs`, keeps the ladder platform-free. `HotkeyKey::Char` already
exists as the cross-platform fallback for a combination recorded elsewhere
(`hotkey.rs:85`), and `hotkey.rs:36–42` states the rule for adding a second recorder.
**Nothing in the port would have to be faked.**

**What a Linux shell would nonetheless have to reimplement — none of it in the port, all
of it in `app/`:**

1. The settings-accessor surface. macOS has 45 methods (`macos/settings.rs`), Windows has
   3 actions plus a draft-and-merge (`shell.rs:92–141`, `:277`). A third shell writes a
   third one. P1-1 shrinks the macOS copy; a genuinely shared surface is out of scope for
   this backlog and should not be invented before the third shell exists.
2. The indicator model (P1-2) and the menu list (P1-3) — both currently Windows-only or
   duplicated.
3. `settings_store::settings_path()` (`settings_store.rs:11–29`) needs a third `cfg` arm.
   Correct as-is; noting it so it is not a surprise.
4. The async log queue (P0-1) is filed under `platform/windows/`.

**One contradiction in the port's own documentation.** `platform.rs:26–31` says of *every*
method: *"None of them may block on anything outside the process."* But `app_in_front`'s
own doc four lines later says *"A shell that can afford a fresh query answers it now
(macOS)"*, and `macos/dispatch.rs:76–82` does exactly that — a
`crate::app_info::frontmost()` window-server round trip inside the tap callback, on the
`ToggleApp` path. It is bounded (hotkey presses only) and deliberate (0012 records it), but
the trait doc as written forbids it. **Fix the doc, not the code** — say the query is
permitted only on a decision that is already user-initiated, and name the reason.

---

## 4. Error handling on hot paths (task item 5)

`grep -rn "unwrap()\|expect(\|panic!" app/src crates/*/src | wc -l` → **57**. Classified:

- **50 are in `#[cfg(test)]` code** (`macos/tests.rs` 26, `log.rs` 12, `ui_thread.rs` 8,
  and single hits in `adapt.rs:494`, `shell.rs:418`, `single_instance.rs:111,126`,
  `paths.rs:70,71`, `startup.rs:147`, `elevation.rs:204`). Not a risk.
- **2 are production `expect` on structural invariants**, both on the UI thread, both
  provably total: `settings_ui.rs:679` (`Tab::spec` indexing `TABS`, pinned by
  `the_tabs_are_the_four_the_macos_window_has`) and `settings_ui.rs:932`
  (`settings_field` on a non-`LaunchAtLogin` toggle, pinned by the spec's placement tests
  and by the `Control::Checkbox(Toggle::LaunchAtLogin)` arm immediately above).
  `settings_spec.rs:753` is the same invariant asserted in a test. Acceptable as written —
  they encode a fact the tests hold.
- **Zero `unwrap`/`expect`/`panic!` on either keystroke path.** Both callbacks are
  `catch_unwind`-wrapped (`macos/mod.rs:327`, `windows/hook.rs:330-ish`) and both fall back
  to passthrough. `RefCell` access on the hot path is uniformly `try_borrow` /
  `try_borrow_mut`.
- **Both `Mutex::lock()` sites handle poisoning** rather than unwrapping
  (`foreground.rs:81–92` with a one-shot report, `shell.rs:183,198` via
  `PoisonError::into_inner`, `log.rs:145` via `if let Ok`).

One inconsistency, **P2**: `macos/mod.rs:193`, `:249`, `:281`, `:287` use panicking
`borrow_mut()` on `last_bundle_id` / `recording_hotkey` where every other access in the
file uses `try_borrow_mut()`. These run on the main thread from the NSWorkspace observer,
and the tap callback drops its borrows before any AppKit call (that is what `Deferred` in
`dispatch.rs:39–51` is for), so re-entrancy is unlikely rather than impossible. Make them
`try_borrow_mut` for uniformity; it costs nothing and removes the last panic-by-construction
from that file.

---

## 5. Test coverage shape (task item 6)

**Pinned by tests (safe to refactor behind):** the engine transformations (5 test files),
the decision ladder (`tests/ladder.rs`, 30), the session policy including a proptest
invariant suite (`properties.rs`), hotkey matching (13), `Platform`/`handle` ordering
(`tests/platform.rs`, 9), the settings spec structure (11), the settings file schema
(`prefs_model.rs`, 15), Windows key translation and dead keys (`adapt.rs`, 13), the Windows
settings window's behaviour including keyboard focus and screen-reader labels
(`settings_ui.rs`, 24), the indicator state machine (8), tray glyph contrast and tooltip
truncation (6), the three-way settings merge (`shell.rs`, 6), viewport lifetime
(`ui_thread.rs`, 5), and the macOS tap decision path (`macos/tests.rs`, 31 — macOS hosts
only).

**Compile-checked only — a refactor here is unprotected:**

| Area | Lines | Risk if it regresses |
|---|---|---|
| `app/src/prefs/*` (AppKit settings + 3 list windows) | ~2,180 | A dead checkbox or a list that does not reload. Invisible in CI; only a human on a Mac notices. This is where P1-1 lands, so add its table test first. |
| `app/src/menu_bar.rs` | 502 | The glyph lying about state — the exact defect 0007 forbids. P1-2 fixes this by moving the logic somewhere already tested. |
| `app/src/platform/windows/hook.rs` | 602 | See P1-4. |
| `app/src/settings_store.rs` | 72 | Silent settings loss (P1-6). Zero tests today. |
| `app/src/platform/windows/foreground.rs` update/reach path | ~150 of 411 | Stale frontmost app → Vietnamese in a terminal. Only `file_name_of` is tested. |
| `about_window.rs`, `hud.rs`, `welcome.rs`, `main_menu.rs`, `ax.rs`, `app_info.rs`, `login_item.rs` | ~700 | Presentation; low. |

**Nothing here is a phantom test.** The suite tests behaviour, including the awkward parts
(surrogate-pair truncation, `Escape` closing a list window, arrow keys not stealing focus
across a segmented control, a word taught by hotkey surviving a settings-window round trip).

---

## 6. Dead code / unused paths (task item 7)

**`Engine::backspace` — the handoff's claim is confirmed, with one correction.**
`grep -rn "\.backspace("` across the workspace returns exactly two call sites:
`crates/glowkey-session/src/session.rs:561` (inside `Session::backspace`) and
`crates/glowkey-engine/tests/telex.rs:111`. So `Engine::backspace` (`engine.rs:308`) is
reachable only through `Session::backspace` (`session.rs:559`) — and **`Session::backspace`
itself has no caller either**: the ladder uses `backspace_visible_char` /
`recompose_after_boundary_backspace` (`glowkey-input/src/ladder.rs:130`), and neither shell
calls it.

Correction to "dead": both are `pub` on **published crates** with `repository`/`docs.rs`
metadata, so they are API surface, not dead code. **P2 action:** decide explicitly —
either document `Session::backspace` as the simple-consumer entry point (a crate user who
wants raw-key replay without the five-case ladder) and keep the `telex.rs` test as its
pin, or drop `Session::backspace` before 0.2.0 and leave `Engine::backspace`. Do not
delete silently; the semver CI job (`ci(semver)`, commit `32da9a3`) checks against the
`v0.1.0` tag and will fail.

No other unreachable code found: `cargo clippy --all-targets` is clean, and the two
`allow(dead_code)` attributes (`settings_spec.rs:130`, `:169`) are the macOS opt-out from
`settings_field` / `ListId::ALL` that P1-1 removes.

---

## 7. Concurrency and shared state (task item 8)

**Windows — the discipline holds, verified rather than assumed.**
`hook.rs` keeps the session in a `thread_local! RefCell` (`:51`) so the hook callback, the
tray and the message loop are the same thread by construction and no lock is possible.
`with_session` (`:197`) returns `None` on re-entry instead of panicking. The UI thread
never touches it — `grep "with_session\|foreground::" ui_thread.rs settings_ui.rs
about_ui.rs` returns nothing but a doc comment, matching 0011's claim. The one
cross-thread channel is `shell::PENDING_SETTINGS` (`shell.rs:177`), a `Mutex<Option<..>>`
written by the UI thread and drained by the main loop, with poisoning recovered and a
log line when a result is overwritten (`shell.rs:186–192`). Saves are drained after
`DispatchMessageW` (`hook.rs:279–300`) and once more after `WM_QUIT` (`:302–316`).

Three fragile spots, none unsound:

1. **P2 — `wake()` posts to `GetCurrentThreadId()`, not to `MAIN_THREAD`**
   (`hook.rs:544–558`). Correct inside the callback, where they are the same thread. But
   `mark_dirty()` (`:252`) is `pub`, calls `request_save` → `wake()`, and if it is ever
   called from the UI thread the wake goes to the wrong queue and the save waits for the
   next unrelated main-thread message. No such caller exists today (only `shell.rs`).
   Make `wake()` use `MAIN_THREAD` like `wake_main_loop` (`:161`) — identical behaviour in
   the callback, and the trap disappears.
2. **P2 — `foreground::STATE` is a `Mutex` locked from the hook callback**
   (`foreground.rs:68`, reached via `HookPort::app_in_front` → `current()` → `with_state`).
   Every other locker is the same message-loop thread, so it is uncontended today, and
   0009's rule ("no lock a non-hook thread holds") is satisfied. But the type does not
   enforce that — a future reader on the UI thread would silently put a cross-thread lock
   in the hot path. Either move `State` into the same `thread_local!` as the session, or
   add a comment at `foreground.rs:68` naming the invariant the way `hook.rs:46–54` does.
3. Informational: `TrackPopupMenu` runs a nested message loop (`tray.rs:846`), so the
   post-dispatch drain in `run_message_loop` does not run while the tray menu is open. A
   save requested at that moment lands when the menu closes. Bounded and self-correcting;
   no action.

**macOS — one thread by design; the risk is re-entrancy, not races.**
`TapState` is `RefCell`/`Cell` on the main run-loop thread (`mod.rs:104–140`), and the
health timer runs on that same run loop (0008 explains why it must not move). The
`Deferred` struct (`dispatch.rs:39–51`) exists precisely to keep AppKit — which can pump
the run loop and re-enter the tap — outside the session borrow, and `run()`
(`dispatch.rs:219–262`) drops the borrow before `handle_key_down` performs anything. That
is sound. `create_tap` (`health.rs:104–120`) refuses to proceed if it cannot take both
borrows, with a comment explaining that a second tap on one run loop would double every
edit. Also sound.

Two cross-thread atomics, both fine: `DISABLED` (`mod.rs:88`) and `TAP_DEAD`
(`health.rs:89`), `Relaxed`, single-writer, read for display.

The residual macOS hazards are the ones already filed: **P0-1** (blocking I/O on the tap
thread) and the `borrow_mut()` inconsistency in §4.

---

## 8. Remaining P2 items

- **P2-1 — Split `settings_ui.rs` (2120 lines).** The three list editors
  (`excluded_body` :1006, `macros_body` :1102, `words_body` :1228 — ~320 lines) plus the
  validation helpers (:1470–1550) into `settings_ui/lists.rs`. Mechanical; the validation
  half leaves the file entirely under P1-5. Blast radius: one file. Protected by the
  existing 24 tests.
- **P2-2 — `macos/settings.rs`'s 45 accessors (399 lines).** Roughly 20 disappear under
  P1-1. The rest (`import_macros_and_save`, `macro_conflicts`, `reset`, …) are genuine
  one-per-feature methods; leave them.
- **P2-3 — The `EnabledSessionOnly` save rule is written three times:**
  `glowkey-input/src/platform.rs` in `handle`'s `ToggleApp` arm,
  `windows/shell.rs:118–121`, and `macos/settings.rs:42–47` — where macOS saves
  *unconditionally*. Benign today (a suspended terminal is still in `app_ids`, so the file
  written is byte-identical — verified at `exclusion.rs:186–192`), but it is one rule in
  three places and only two of them agree. Have the macOS menu path call
  `session.toggle_app_exclusion` and honour the returned `ExclusionToggle`, as the port and
  the Windows shell do.
- **P2-4 — `borrow_mut()` → `try_borrow_mut()`** at `macos/mod.rs:193,249,281,287` (§4).
- **P2-5 — Fix the `Platform` trait's blocking-contract doc** (`platform.rs:26–31`) to
  match `app_in_front`'s documented exception (§3).
- **P2-6 — Decide `Session::backspace`'s fate** before 0.2.0 (§6).

---

## 9. Do NOT refactor — load-bearing things that look wrong

Each of these has a decision record or a comment recording the incident that produced it.
Touching them without new evidence reverses a verified decision.

1. **Full suppression** — swallowing even a plain letter and re-emitting it
   (`platform/windows/mod.rs:7–16`, `dispatch.rs` replay handling). It looks like
   pointless work per keystroke. It removed the `hoongf` → `hoồng` race in multiprocess
   applications by construction (0009). One ordered queue is the whole point.
2. **The blind model — no composition, no reading of the document.** 0009 argues at length
   why TSF is not an improvement but a rewrite of the delivery half. "What would change
   this" is stated there and is a measurement, not an argument.
3. **Nothing blocks in the callback** (0008). This is the rule P0-1 *restores*, not one to
   relax. In particular do not "simplify" `hook_log`'s bounded channel into an unbounded
   one, and do not move the macOS health poll to its own thread (0008 rejects that
   explicitly: `CGEventTapIsEnabled` is not documented thread-safe).
4. **The five-case Backspace ladder** (`glowkey-input/src/ladder.rs:130+`, six tests named
   `backspace_case_*`). Five separately-fixed bugs in the order they were argued into.
5. **`EmitThenReplayKey` replaying the boundary key from GlowKey's own source** instead of
   letting the original through (`platform.rs` `Decision::EmitThenReplayKey` arm,
   `dispatch.rs:171–186`). The comment names the exact regression (`ddc`␣ → `đddc`).
6. **The off-screen one-point root viewport on Windows** (0011, `ui_thread.rs`). It looks
   like a hack; a hidden root stops receiving redraws on egui 0.29.1 (#3655, #5229) and
   would never drain its queue.
7. **`refresh_frontmost_at_word_start`'s once-per-run guard** (`macos/mod.rs:252–283`).
   The three-source frontmost tracking is more machinery than one query per keystroke, and
   0008 records that trade deliberately: the query was correct and unaffordable.
8. **`merge_settings`'s three-way merge** (`shell.rs:277–350`). It looks like
   over-engineering for a settings window; it is what stops a word taught with ⌃⇧W while
   the window was open from being destroyed on close. Six tests.
9. **The Chromium omnibox forward-delete guard** (`emit.rs:70–86`, 0003). The one
   remaining accessibility round trip on the hot path, capped and opt-out, accepted knowing
   the cost.
10. **The clipboard tools' apparent duplication** (`menu_bar.rs:169–188` ↔
    `windows/clipboard.rs:25–60`). Both are ~15 lines wrapping a platform clipboard around
    `glowkey_session::remove_tones`; the shared part is already shared. Leave it.
11. **`ExclusionDefaults::new` folding terminals into excluded** (`exclusion.rs:38–48`) and
    **`ExclusionList::remove` keeping the tombstone** (`:180–190`). Both look redundant;
    both have their reasoning written in place and tests behind them.

---

## 10. Suggested order

1. P0-1 (log queue) — it is the only correctness item, and the fix already exists in-tree.
2. P1-6 (two-line silent-save fix) and P2-4/P2-5 (doc + borrow consistency) — trivial,
   land alongside.
3. P1-4 (tests for `hook.rs`) — before anything else touches Windows.
4. P1-1 (macOS toggle binding) with its table test — the biggest maintenance win.
5. P1-2 (shared indicator), then P1-5 (one validation rule), then P1-3 (menu spec).
6. P2-1/P2-2 fall out of the above; P2-3 and P2-6 are decisions, not work.

---

## Unresolved questions

1. **Empty macro expansion**: allowed (macOS/`Session::add_macro`) or rejected
   (Windows/`validate_macro`)? P1-5 needs one answer.
2. **macOS gaining `Breakage::ElevatedWindow`** under P1-2: it has no macOS meaning, so
   either macOS renders only `HookGone` from the shared enum, or the enum stays two-variant
   and macOS never constructs the second. Which reads better is a product call.
3. **Duplicate-macro UX**: macOS asks "Replace “X”?" (`prefs/mod.rs:392–406`), Windows
   replaces silently. 0010's principle says one behaviour; which one is not obvious.
4. **`Session::backspace`**: intended public API for simple consumers, or an artifact of
   the split? Affects whether the semver job blocks a 0.2.0 removal.
