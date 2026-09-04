---
title: "GlowKey — cross-platform: macOS, Windows, then Linux"
description: "Lift the platform-neutral decision policy out of app/src/tap/ into its own crate, re-seat macOS on it unchanged, build a Windows backend on WH_KEYBOARD_LL + SendInput, and design the Linux backend against the Wayland reality before building it. The Vietnamese engine does not change."
status: in-progress
priority: P1
effort: "15-22 days"
tags: [glowkey, cross-platform, windows, linux, architecture, port]
created: 2026-09-04
---

# GlowKey — cross-platform: macOS, Windows, then Linux

## The one thing that makes this tractable

**The hard half is already done and already proven.** `crates/glowkey-engine` is
2,851 lines, holds 164 of the workspace's tests (160 when this plan was written;
Phase 2 added four), has zero platform crates, zero
`cfg(target_os)` and zero `unsafe` — and CI compiles and tests it on
`ubuntu-latest` with `-D warnings` precisely to stop macOS leaking into it. Every
Vietnamese behaviour listed as "must not change" lives there and moves to Windows
untouched.

So this is not a rewrite. It is a port of the **delivery** layer: 4,274 lines in
`app/`, of which roughly 1,900 are AppKit UI that transfers nothing.

## The one thing that makes it dangerous

GlowKey is a **blind** input method. Its single invariant is *rendered == the text
tail at the caret*, and nothing verifies that — it is maintained purely by flushing
on every event that could move the caret. Get that wrong on a new platform and the
failure is not a missing diacritic, it is **synthesized backspaces deleting text the
user typed themselves**.

The macOS side learned this repeatedly and expensively. `docs/decisions/0008` was
written from a real incident where a blocking call in the tap callback froze the
whole machine. `docs/decisions/0007` exists because an indicator that lies about a
dead tap is a defect. The five-case Backspace ladder in `decide.rs` is five separate
fixed bugs, in order. **The port's job is to carry that hard-won ordering across, not
to rediscover it.**

## Goals

| # | Goal | Priority |
|---|------|----------|
| 1 | The decision ladder becomes platform-neutral and testable off any OS, with its ordering preserved exactly | P1 |
| 2 | macOS builds and behaves identically; the full suite stays green | P1 |
| 3 | Windows types Vietnamese in real applications, verified by a human at a real machine | P1 |
| 4 | Platform values (`HotkeyPreset::Custom.keycode`, bundle identifiers) stop being macOS-only without breaking existing settings files | P1 |
| 5 | A Windows user can install, run and trust it — tray, settings, startup, an honest indicator | P2 |
| 6 | Linux is designed against the real Wayland situation, and built only after Windows is verified | P2 |
| 7 | The engine gains no platform dependency and no `cfg(target_os)` | P1 |

## Non-goals

| Out | Why |
|---|---|
| Changing any Vietnamese behaviour | The engine is the source of truth and is already correct. A port that "improves" it while moving it cannot tell a port bug from a behaviour change. |
| TSF as the Windows mechanism | Evaluated and rejected in Phase 4 — see the decision record it produces. Short version: TSF gives composition, and the blind diff model exists *because* there is no composition. |
| Reintroducing repeat-key behaviour | Removed deliberately. |
| Replacing visible-character Backspace with keystroke-undo | Asked twice, declined twice (`docs/handoff.md` §4). |
| A cross-platform UI toolkit | Each shell is native. Tauri/Electron/Dioxus are excluded by the request and would drag a web runtime into an input method. |
| Sharing one settings file between platforms | Phase 2 decides this deliberately rather than by accident. |
| Building Linux in this plan's first pass | Designed in Phases 8-10, built after Phase 6 proves Windows. |

## Architecture

```text
crates/glowkey-engine        UNCHANGED — Vietnamese transformation, session,
                             history, backspace, settings, exclusions
        ▲
crates/glowkey-input         NEW — platform-neutral policy:
                             KeyEvent, Key, Modifiers, Decision,
                             the decision ladder, hotkey matching
        ▲
   ┌────┴─────┬──────────────┬──────────────┐
app/           app/           app/           (later)
platform/      platform/      platform/
macos/         windows/       linux/
CGEventTap     WH_KEYBOARD_LL X11 / Wayland
CGEventPost    SendInput      XTEST / uinput / IBus
NSWorkspace    SetWinEventHook  …
```

`glowkey-input` is deliberately small: a neutral event in, a `Decision` out, no I/O,
no OS types. Like the engine, it gets tested on `ubuntu-latest` in CI, which is what
mechanically prevents platform code from creeping back in.

## Phases

| # | Phase | Status | Depends on |
|---|-------|--------|------------|
| 1 | [Neutral input policy crate](./phase-01-neutral-input-policy.md) | Complete | — |
| 2 | [Platform-neutral hotkeys and app identity](./phase-02-hotkeys-and-app-identity.md) | Complete | 1 |
| 3 | [Re-seat macOS on the neutral layer](./phase-03-reseat-macos.md) | Complete | 1, 2 |
| 0 | [The engine's own tests pass on Windows](./phase-00-engine-tests-on-windows.md) | Complete | — |
| 4 | [Windows input core](./phase-04-windows-input-core.md) | Complete | 0, 3 |
| 5 | [Windows application shell](./phase-05-windows-shell.md) | Pending | 4 |
| 6 | [Windows verification on real hardware](./phase-06-windows-verification.md) | Pending | 5 |
| 7 | [Windows packaging and CI](./phase-07-windows-packaging.md) | Pending | 0 (CI job), 6 (packaging) |
| 8 | [Linux: choose the input stack](./phase-08-linux-input-stack-decision.md) | Pending | 6 |
| 9 | [Linux input backend](./phase-09-linux-input-backend.md) | Pending | 8 |
| 10 | [Linux shell and packaging](./phase-10-linux-shell-packaging.md) | Pending | 9 |

Phases 1-3 are pure refactor with **zero behaviour change** and are complete. Phase 0
is numbered zero because it is a precondition discovered after the fact, not a step
that follows 3: it repairs test portability that Phase 2 broke, and it is the gate on
every claim about the engine running on Windows. Phase 4 onward needs a Windows
machine. Phases 8-10 are designed now and built after Phase 6.

Phases 0 and 4-7 are tracked by
[issue #1](https://github.com/hong4rc/glowkey/issues/1).

## Measured on Windows, 2026-09-04

The port's premise was checked on a real Windows machine rather than assumed:

| Check | Result |
|---|---|
| `cargo check --workspace` | **Green** |
| `cargo test -p glowkey-engine --no-fail-fast` | 164 tests, **158 pass, 6 fail** |
| Vietnamese behaviour tests | **All green** — Telex, VNI, Simple Telex, order-independent tones, Quick Telex, brackets, auto-fix, stop-coda, mid-word spell check and its escape, committed-word history, `backspace_visible_char`, per-word overrides, macros, tombstones |
| The 6 failures | All in exclusion-default assertions that hardcode macOS bundle identifiers. Phase 2 made the shipped table per-target and did not move the tests with it. |

**The premise holds.** The engine transforms Vietnamese correctly on Windows today.
What does not hold is the test suite's portability, which is Phase 0 and is half a
day.

## The verification ceiling, stated plainly

This plan was written on macOS. `cargo check --target x86_64-pc-windows-msvc` works
(no linker needed) and is a real gate — it proves Win32 signatures and types. It
proves nothing about behaviour. The Windows work is now being executed on an actual
Windows machine, which raises the floor for compilation and unit tests but not for
behaviour: **the app still cannot be shown to type Vietnamese by any automated check.**

**Nothing in Phases 4-10 may be called done on a `cargo check`, and nothing may be
called done on green CI either.** Phase 6 exists because a human types into real
applications. Until then every Windows behavioural claim in this plan is a hypothesis,
and the phase files mark them as such.

## Conflicts with other plans

The port refactors `app/src/tap/` and `app/src/prefs/` heavily. **Do not run these
concurrently:**

- `plans/260903-1745-glowkey-hardening-and-distribution` — its Phase 5 (extend the
  omnibox guard to Safari) edits `ax.rs` and `emit.rs`, which Phase 3 re-seats.
  Its other phases appear already shipped (signing d6c3feb, the release workflow,
  the health monitor, the tap/prefs splits).
- `plans/260903-1637-unikey-phonotactics-and-restore` — engine-side, so the file
  overlap is small, but its Phase 2 rewrites the restore decision inside
  `Session::commit`, and Phase 2 of *this* plan touches `HotkeyPreset` in the same
  crate. Different lines, same crate; sequence them rather than interleaving.

Neither blocks this plan. This plan blocks neither. They are a merge hazard, not a
dependency.

## Success Criteria

- [x] `glowkey-input` exists, has no OS dependency, and CI tests it on Linux
- [x] The decision ladder's ordering is pinned by tests that run on any platform
- [ ] macOS: all tests green, clippy silent, behaviour unchanged by inspection
      against `docs/manual-verification.md`
- [x] `cargo check --workspace` green on Windows
- [x] `cargo test -p glowkey-engine` green **on Windows** — 164/164 (Phase 0)
- [x] A Windows build links and its own tests run — 19/19 (Phase 4)
- [ ] A Windows build types `hoongf` → `hồng` in Notepad, Chrome, Windows Terminal
      and VS Code, verified by a human (Phase 6)
- [x] Synthesized input is provably not reprocessed — the `dwExtraInfo` tag is
      checked in the callback's first statement and a test proves the guard.
      *Proven as a pure function; proven as a behaviour only in Phase 6.*
- [ ] Elevated-window failure is *detected and reported*, not silent
- [x] Existing macOS settings files keep working; no silent reinterpretation of
      `HotkeyPreset::Custom`
- [ ] A written list of Windows limitations ships with the phase
- [ ] Linux design records the Wayland decision with evidence, before any Linux code

## Decisions taken

1. **The two platforms share one settings schema.** Phase 2 settled it: `HotkeyPreset::Custom`
   carries `macos_keycode` and `windows_vk` side by side, the old `keycode` field
   reads through an alias, and the shipped exclusion table is per-target while the
   exclusion *rules* stay neutral. A macOS settings file loads on Windows. Verified by
   a real fixture file lifted off a working installation.
2. **The Windows settings UI is `winit` + `egui`, with the tray native.** Decided in
   Phase 5 rather than left to a prototype, because the alternative was oscillating
   over it. The containment rules and the idle-cost measurement that would reverse it
   are written into that phase.

## Open questions

1. **Does Chrome on Windows exhibit the omnibox trailing-selection bug at all?** The
   macOS AX guard exists for a real, reproduced defect. Whether Windows needs an
   equivalent is unknown and is a Phase 6 measurement, not an assumption.
2. **Are the shipped Windows exclusion defaults the right ones?** The table exists and
   is plausible, but nobody has typed into every application on it. A wrongly excluded
   app is indistinguishable from GlowKey being broken, so this is a Phase 6 judgement.
3. **Which Linux environments are actually in scope?** Wayland structurally forbids
   the blind global-intercept model outside compositor-specific protocols. Phase 8
   may conclude that some desktops can only be supported by failing honestly.

<!-- slug: glowkey-cross-platform-port -->
