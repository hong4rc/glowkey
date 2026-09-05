# Phase 7 — Refactor and de-duplication

**Depends on:** phase 5 for anything macOS-side (unverifiable until then).
**Scope discipline:** every item below removes duplication that exists *today*.
No speculative abstraction, no rewrite.

## What the audit found first: the layering holds

`decisions/0012` is intact. No `cfg(target_os)`, no `unsafe`, no OS type anywhere
in `crates/*` — the five grep hits are doc prose, serde aliases and the English
word "windows" in the dictionary. Nothing policy-shaped has leaked into `app/`.
The CI job that builds the three library crates on Linux with `-D warnings` is
what keeps it that way, and it is working.

Every finding below is in the **shell** layer, which is exactly where 0012
predicted the next copy would appear. Also measured: 57 `unwrap`/`expect` hits,
of which 50 are test-only, 2 are provable UI-thread invariants, and **zero are on
either keystroke path**; both callbacks are `catch_unwind`-wrapped; every
`Mutex::lock()` handles poisoning. No `unsafe impl Send/Sync`, no `static mut`,
no `transmute`, and no use-after-free vector found.

## Items

### 7.1 — The shared log queue [B2]

Covered in phase 3.3: move `platform/windows/hook_log.rs` (bounded channel +
writer thread, zero Win32) to `app/src/log_queue.rs` and use it from both shells.
~25 macOS call sites change signature.

### 7.2 — The indicator state machine exists twice [arch P1-2]

Tested on Windows (`indicator.rs:73`, 8 tests), reimplemented inline and untested
on macOS (`menu_bar.rs:196-227`). This is the one duplication with real
cross-platform correctness value — it is the state machine decision 0007 is
about. Lift it into a shared module and let both shells render it.

### 7.3 — A macOS checkbox is bound through four parallel 9-arm matches [arch P1-1]

`prefs/tabs.rs:459`, `:500`, `prefs/mod.rs:151-243`, `macos/settings.rs` — where
Windows uses one map. `settings_spec.rs:130` already carries an
`allow(dead_code)` scar from it. Maintenance cost, not correctness; macOS-side,
so it waits on phase 5.

### 7.4 — Tray and menu-bar item lists have already drifted [arch P1-3]

`menu_bar.rs:230-396` ↔ `tray.rs:718-830`, drifted in wording exactly as decision
0010 predicted for the settings spec. Give the menu the same treatment the
settings window got: one shared description, two renderers.

### 7.5 — Macro/word validation exists three times with different rules [arch P1-5]

`session.rs:438` accepts an empty expansion; `settings_ui.rs:1486` rejects it.
Phase 1.4 introduces the shared validator; this item retires the other two
copies.

### 7.6 — The Windows hot path has zero tests [arch P1-4]

`hook.rs` is 602 lines and the keystroke path, with `#[test]` count zero, against
macOS's 31. Add these **before** phases 3.2 and 4.5 touch the file — a refactor
of an untested hot path is where an input method breaks silently.

### 7.7 — Dead API [arch]

`Engine::backspace` and its only caller `Session::backspace` are unreachable from
the app (the app uses `backspace_visible_char`). Both are published-crate API, so
removal is a semver event — fold into the `0.2.0` that phase 8 would force
anyway, or document them as intentional API. Needs a decision.

## Do NOT refactor

The audit produced an eleven-item list of things that look wrong and are
load-bearing. The main ones, each with the bug it prevents:

| Thing | Why it stays |
|---|---|
| Full suppression, single event source | Mixing native passthrough with synthesized backspaces races: `aa`→`aâ`, `hoongf`→`hoồng` |
| The boundary key replayed from GlowKey's source (`EmitThenReplayKey`) | Passing it through natively lost the race: `ddc`␣→`đddc`, space swallowed |
| The blind model (no cursor/selection read-back) | The one invariant is "rendered == the text tail at the caret"; reading back is what the design removes |
| Nothing blocks the tap callback | Every keystroke on the machine waits behind it; a window-server call there freezes the Mac (§6.9) |
| The five-case Backspace ladder | Each case is a shipped bug |
| The once-per-run frontmost bootstrap | Removing it puts a window-server call back on the hot path |
| `merge_settings` three-way merge | Protects ⌃⇧W-taught words written while Settings is open |
| The **macOS** AX-gated omnibox guard | Decision 0003. Note: this does **not** cover the Windows unconditional one, which phase 3.1 fixes |
| Exclusion tombstone rules | `saved ∪ (defaults − removed)` is how new defaults reach old files without resurrecting deliberate removals |
| The near-identical clipboard wrappers | Different OS APIs that happen to look alike |
| The off-screen egui root | Decision 0011 |

## Validation

`cargo test --workspace` (331 today), `cargo clippy --workspace --all-targets --
-D warnings`, the three library crates with and without `--features serde`,
`cargo check --target x86_64-unknown-linux-gnu` for the library crates, `cargo
doc` with `RUSTDOCFLAGS=-D warnings`, and the macOS `--all-targets` clippy — which
is what caught a stale test field last time.
