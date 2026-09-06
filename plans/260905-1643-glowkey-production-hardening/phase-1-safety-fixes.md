---
phase: 1
title: "Safety fixes: data loss, memory safety, silence"
status: completed
priority: P1
effort: "1d"
dependencies: []
---

# Phase 1 — Safety fixes

**Status:** ready. No dependencies, no open decisions, no file contention with
any other phase except `settings_store.rs` (owned here).
**Verifiable on Windows** by `cargo test --workspace` + `cargo clippy`.

Everything here is a defect that can lose the user's data or hide a failure from
them. None of it needs hardware or a product call.

## Context

The arbiter confirmed each item by opening the cited line. Two were found during
verification rather than by the audits themselves and are marked ARBITER-FOUND.

## Items

### 1.1 — The settings backup is destroyed by the file it is supposed to protect [B1]

`settings_store.rs:38` loads through `from_json`, which at `prefs_model.rs:123`
does `unwrap_or_default()` — a parse failure is indistinguishable from an empty
file. `save()` at `:61-63` then copies the current on-disk file to `.bak`
*unconditionally*, so the first save after a corrupt load overwrites the last
good backup with the corrupt content. The comment at `:57-60` promises recovery
in exactly the case it destroys.

**ARBITER-FOUND aggravation:** on macOS this needs no user action at all. A
corrupt file yields `welcome_shown = false`, so the welcome screen shows and
`save_settings()` runs at launch (`macos/mod.rs:478-481`). The backup dies before
the user has touched anything.

**Change:** have `load()` report whether the file parsed. On a parse failure,
never overwrite `.bak` — preserve the unreadable file as `.corrupt-<timestamp>`
and log it. Add `sync_all()` before the rename so the write is durable.

**Test:** a corrupt fixture → `.bak` untouched, `.corrupt-*` written, defaults
loaded. Headless.

### 1.2 — Unbounded scan of another process's clipboard buffer [B11]

`clipboard.rs:97-104` walks a `u16` pointer looking for NUL with no bound. The
buffer belongs to whichever process last set the clipboard, and its
NUL-termination is that process's promise, not a guarantee. A malformed or
truncated `CF_UNICODETEXT` block reads past the allocation: a crash, or GlowKey's
own heap bytes handed back into the clipboard.

**Change:** bound the scan by `GlobalSize(handle) / 2`, and treat an unterminated
buffer as the full bounded length rather than a failure.

**Test:** unit-test the scan helper against a non-terminated slice. Headless.

### 1.3 — Failures that reach nobody [B9]

`main.rs:27` states stderr goes nowhere under `windows_subsystem = "windows"`,
and `settings_store.rs:54,66,70` reports save failures with `eprintln!`. So
"could not save your settings" is invisible on the platform where it is
invisible. There is no `panic::set_hook` on either platform, so a panic outside
the two `catch_unwind`-guarded callbacks leaves no trace anywhere.

**Change:** install `std::panic::set_hook` in both `run()` entry points, routing
to `crate::log`. Replace the three `eprintln!` with `crate::log`.

Note the existing `catch_unwind` in both hot callbacks (`macos/mod.rs:319-331`,
`windows/hook.rs:323-343`) is correct and symmetric — a panic costs one key its
transform, never the user's typing. This phase adds the missing outer net, it
does not touch that.

**Test:** none automatic; assert by inspection that no `eprintln!` remains in the
app crate outside `main.rs`.

### 1.4 — Imported macros are not validated as untrusted data [B12]

`macros.rs:100-101` accepts control characters (`\r`, `\n`, C0) in an expansion,
and the Windows import (`settings_ui.rs:1208-1215`) has no size cap where macOS
has 4 MB. The import path is the app's only untrusted-file surface — EVKey and
UniKey tables people download.

The expansion is inserted via `KEYEVENTF_UNICODE` as text, never as virtual key
codes, so an imported macro **cannot** press Ctrl or Enter (verified). This is a
data-hygiene fix, not an escape-to-execution one.

**Change:** one shared validator in the session crate — reject control
characters, cap expansion length — used by parse, by add, and by both import
paths. Also settles the divergence where `session.rs:438` accepts an empty
expansion and `settings_ui.rs:1486` rejects it (decision: reject; see
`decisions.md`).

**Test:** table-parse tests for control characters and the cap. Headless.

### 1.5 — Two small Windows hardening items [B23, B24]

- `shell.rs:148` launches `explorer.exe` by bare name. Rust's `Command` has not
  searched the CWD since 1.58, so the "run from Downloads" vector does not
  apply — but the *application directory* is still searched, so an
  `explorer.exe` planted beside `GlowKey.exe` by a zip extraction is reachable.
  Use an absolute path from `GetSystemWindowsDirectoryW`.
- `settings_ui.rs:133` builds the font path from the `SystemRoot` environment
  variable. Same API, same fix.
- `single_instance.rs:31,48` exits silently when the mutex is held, which is
  indistinguishable from a crash if another process squats the name. Log the
  refusal before exiting.

### 1.6 — Supply-chain hygiene [B13-lite]

`cargo audit` is clean today: **0 vulnerabilities across 391 crates** (1239
advisories). Two unmaintained notices, neither actionable:

- `ttf-parser 0.25.1` (RUSTSEC-2026-0192) — Windows-only, reached solely through
  the settings window's font stack (`ab_glyph → epaint → egui → eframe`), parses
  only system fonts and egui's compiled-in ones, and is `#![forbid(unsafe_code)]`.
  **No fix exists**: 0.25.1 is the latest and current egui still depends on it.
- `paste 1.0.15` (RUSTSEC-2024-0436) — **not compiled at all**; lock-file-only
  behind a wgpu feature that `eframe default-features = false` excludes.

**Change:** `.cargo/audit.toml` ignoring both IDs *with the justification above
written next to them*, plus a `cargo audit` CI job so a **new** advisory fails
the build. Add `SECURITY.md` and checksums on release artifacts.

The point of this item is not the two known notices; it is that nothing is
watching for the next one.

## Validation

```
cargo test --workspace                                   # 331 passing today
cargo clippy --workspace --all-targets -- -D warnings
cargo audit
```

## Risk and rollback

All changes are local and additive; none touches the decision ladder, the
engine, or either keystroke hot path. `settings_store.rs` is the only file with
cross-phase interest (phase 8 would add an XDG arm). Rollback is per-commit.
