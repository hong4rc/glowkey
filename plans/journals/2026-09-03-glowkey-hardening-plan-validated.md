---
title: "GlowKey hardening plan, validated"
date: 2026-09-03
summary: Planned and validated the six-phase hardening plan; validation surfaced that revoking Accessibility mid-run kills the tap silently.
---

# GlowKey hardening plan, validated

## What happened

Owner asked for "any improve" on a project that is already feature-complete
against the useful Unikey/EVKey set — 94 tests green, clippy clean, CI on Linux
and macOS. So the work was not features. Offered four tracks; all four taken.

Created `plans/260903-1745-glowkey-hardening-and-distribution/`, six phases:
stable signing identity, release pipeline, latency budget plus property tests,
splitting `tap.rs` (1992 lines) and `prefs_window.rs` (1423), guard coverage
plus onboarding, and tap-death recovery.

The typing-accuracy track was already fully planned in
`plans/260903-1637-unikey-phonotactics-and-restore/` (pending, never executed),
so it was cross-referenced rather than duplicated.

## Verification pass

Full tier, ~20 claims. Three failed and were corrected before the interview:

- `about_window.rs:18` already reads `CFBundleShortVersionString` — the planned
  change was a no-op; downgraded to a verification step.
- There is no `Engine::rendered()`. The field is private; `Session::current_word()`
  (`lib.rs:466`) is its only reader and `Session::process_key` (`lib.rs:1058`)
  is the per-key entry. Real names substituted.
- The `KEY …` log line is assembled at `tap.rs:637`, not in `log.rs`.

Writing a plan against remembered API names rather than grepped ones produced
all three. The verification pass is what caught them.

## The finding

Owner asked mid-session whether revoking Accessibility after startup causes lag
or a loop. Neither — `accessibility_trusted()` is called only in the startup
gate (`tap.rs:1206`, `:1298`, `:1355`, `:1363`), nothing polls after, no timers
or threads exist. Zero CPU.

But the tap dies silently. Process lives, menu bar keeps showing **VI**,
`glowkey.log` says nothing, and re-granting does not recover without a relaunch.
The `TapDisabled*` branch at `tap.rs:1147-1158` re-enables blind with no return
check and no log line, so a timeout disable is equally invisible. The status
glyph asserting VI over a dead tap is what makes it a defect rather than a
limitation. Added as Phase 6 (P1, half a day).

## Decisions

- Self-signed only, no notarization. The DMG is ad-hoc; the quarantine
  workaround is documented. No repository secrets, and the Phase 1 signing key
  never leaves the local keychain.
- Property tests (Phase 3) land before the ASCII-render restore in the other
  plan. Recorded as a real `blockedBy`/`blocks` pair, not a suggestion.
- One-time NSAlert for onboarding, reopenable from the menu.
- No network, ever — closed, not deferred. Includes a bare version-check ping.
  The CI guard that fails the build on any linked networking framework stays
  absolute.

## Next steps

Phase 3 first (it gates the other plan), or Phase 6 first — it is P1, half a
day, and fixes something a user can hit today. Phases 4, 5 and 6 all edit
`tap.rs`; run them one at a time.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
