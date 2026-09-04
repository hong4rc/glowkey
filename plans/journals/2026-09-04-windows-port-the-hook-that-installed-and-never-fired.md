---
title: "Windows port: the hook that installed and never fired"
date: 2026-09-04
summary: "Phases 0, 4 and 5 of the Windows port, Tier 1 verified against code points; two review passes caught three critical defects including a null module handle that silently disabled the whole hook."
---

# Windows port: the hook that installed and never fired

## What happened

Took GlowKey's Windows port from "phases 1-3 landed" to "types Vietnamese in
Notepad, verified against code points".

**Phase 0** — six engine tests failed on Windows. Not a behaviour bug: Phase 2
made the shipped exclusion table per-target (bundle identifiers on macOS,
executable names on Windows) and did not move the tests with it, so they asserted
`com.apple.Terminal` against a table that ships `windowsterminal.exe`. Repaired by
asking the table instead of spelling it. A seventh test,
`permanent_terminal_removal_via_editor_still_works`, was passing *vacuously* —
every assertion in it is satisfied by an identity that was never in the list. Same
class in `glowkey-input`'s ladder tests, found later by running the full
workspace suite rather than one crate.

**Phase 4** — the input core: `WH_KEYBOARD_LL` + `SendInput`, running the same
`glowkey_input::decide` the macOS tap runs. The issue listed `adapt.rs`, the
callback body and the Windows exclusion table as blocked; all three had been
unblocked by phases 1-3 landing, so the issue's own scope was stale.

**Phase 5** — tray (raw Win32), settings window (eframe/egui, delegated), startup
registry entry, clipboard tools, known-folder paths, and a four-state indicator
whose two failure causes stay distinguishable in the menu text.

## The defect that cost the most

`SetWindowsHookExW(WH_KEYBOARD_LL, ..., NULL, 0)` **returns a valid handle and
never calls the callback.**

A low-level hook lives in the installing process rather than a DLL, so the
documentation reads as though the module handle is optional. It is accepted. It
silently does not work.

The failure shape is the worst available for an input method: installation
reported success, the log said `HOOK installed`, the WinEvent hook on the *same
thread* and the *same message pump* kept delivering foreground changes, and the
process stayed alive. Three test runs produced no `KEY` line at all — not wrong
characters, nothing.

`GetModuleHandleW(null)` fixed it. `HOOK first callback received` is now logged
once per run, because without it "GlowKey decided not to transform" and "GlowKey
never saw the key" are indistinguishable in a log that simply has no KEY lines.

## Two things that looked like product bugs and were not

**A wrong `cbSize` made the test harness's `SendInput` a no-op.** The C# `INPUT`
declaration was 32 bytes; x64 wants 40, and `SendInput` rejects a wrong size
outright. Presented as "GlowKey isn't transforming". The tell was Notepad being
*empty* rather than containing `hoongf`.

**EVKey was running on the verification machine.** Surfaced as an anomaly: GlowKey
excluded, every line `Passthrough`, and `hoongf` still becoming `hồng` alongside
inbound `VK_PACKET` events GlowKey had not sent. Tier 1 survives it — hooks run
most-recently-installed first, GlowKey swallows every handled key, so EVKey is
starved of the keystrokes — but the results are caveated and should be re-run
with it stopped. Not stopped here: it is the owner's own application and they
were away.

## What the reviews caught

Two `code-reviewer` passes, both `DONE_WITH_CONCERNS`, both right about things
the green gates could not see.

First pass: a synchronous flushing file write inside the hook callback (the exact
rule `decisions/0008` exists to enforce), alignment UB reading
`TOKEN_MANDATORY_LABEL` out of a `Vec<u8>`, the keyboard layout read from our own
thread rather than the foreground window's, Caps Lock missing from the
`ToUnicodeEx` state array, and a pending-save flag that was unreachable because a
hook callback does not make `GetMessageW` return.

Second pass, after the fixes: **the flushing write was still there**, in
`inject.rs`, on the UIPI-refusal path — which fires on every keystroke while an
elevated window is in front. Missed when `hook_log` was written. Also: the
settings window destroyed words the user taught GlowKey while it was open, the
tray icon was built from an uninitialised bitmap used as a monochrome mask with
alpha zero throughout, and AltGr was detected from Right Alt alone, which on a US
layout is an ordinary Alt.

The AltGr test asserted the intended *consequence* rather than the *rule*, so it
stayed green while the rule was wrong. That is the shape of test that cannot fail.

## An accident that turned out useful

Writing a replacement AltGr test on the premise "a US layout has no AltGr
mappings" — it failed. Probing showed `Ctrl+Alt+A` → `á`, `Ctrl+Alt+5` → `€`. The
machine runs **US-International**, which is a dead-key layout. So the AltGr
handling is live here rather than theoretical, and the layout Tier 4 needs for the
`ToUnicodeEx` dead-key check is already installed.

## Decision

Kept the automated verification harness in `scripts/`. It compares code points
rather than glyphs, it is reproducible in a way a human pass is not, and it earned
its place by catching the null-module-handle defect on its first honest run — a
defect a person watching a screen would have reported as "nothing happens", with
no way to tell that from a hundred other causes.

Did not stop EVKey to get clean numbers. Killing a user's running application
while they are away is not a thing to do for a cleaner test result.

## Next steps

1. Re-run Tier 1 with EVKey stopped.
2. Dead keys on US-International — cheapest remaining check, layout already there.
3. Tier 2: Chrome's address bar, Windows Terminal, VS Code, an Electron app.
4. An elevated window, to exercise the UIPI path that is implemented and unproven.
5. Decide what the settings window should do on a second open. winit permits one
   event loop per process, so it currently opens once per run and logs why.
   That needs a design decision, not a patch.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
