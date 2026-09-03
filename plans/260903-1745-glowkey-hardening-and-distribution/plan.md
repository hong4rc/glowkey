---
title: "GlowKey — signing, release, latency proof, and guard coverage"
description: "The four improvement tracks left once typing accuracy is planned elsewhere: a stable signing identity that stops the Accessibility grant dying on every rebuild, a real release pipeline, a latency budget plus property tests over the engine's core invariant, a modularization of the two oversized shell files, and the guard/onboarding work still marked unverified in the handoff."
status: pending
priority: P1
effort: "5 days"
tags: [glowkey, signing, release, testing, refactor, ux]
blocks: [260903-1637-unikey-phonotactics-and-restore]
created: 2026-09-03
---

# GlowKey — signing, release, latency proof, and guard coverage

## Overview

GlowKey is feature-complete against the useful Unikey/EVKey set: 94 tests green,
clippy clean, CI on Linux and macOS, no networking linked. What is left is not
features. It is the things around the features — the app cannot be handed to
another person, the signature churn costs a permission re-grant on every build,
the engine's one load-bearing invariant is asserted only by hand-written
examples, two shell files have grown past the point where they can be read, and
three items in `docs/handoff.md` §6/§11 are still marked "needs live
verification".

Scope was chosen by the owner across four offered tracks; all four were taken.
The typing-accuracy track is **not duplicated here** — it is already planned in
full at `plans/260903-1637-unikey-phonotactics-and-restore/` (status: pending,
never executed). See "Relationship to the typing-accuracy plan" below.

## Goals

| # | Goal | Priority |
|---|------|----------|
| 1 | A rebuild stops dropping the Accessibility grant | P1 |
| 2 | `git tag v0.2.0` produces an installable artifact another person can run | P2 |
| 3 | The engine's "rendered == the text tail at the caret" invariant is machine-checked, and keystroke latency has a number | P1 |
| 4 | `tap.rs` (1992 lines) and `prefs_window.rs` (1423 lines) split along real boundaries, zero behavior change | P2 |
| 5 | The omnibox guard is either extended to Safari or proven unnecessary there; a new user is told what the app does | P2 |
| 6 | Revoking Accessibility while GlowKey runs stops being a silent death | P1 |

## Phases

| # | Phase | Status | Depends on |
|---|-------|--------|------------|
| 1 | [Phase 1: Stable signing identity](./phase-01-stable-signing-identity.md) | Code done; **you must create the certificate** | — |
| 2 | [Phase 2: Release pipeline](./phase-02-release-pipeline.md) | Done; DMG built and mount-tested | 1 |
| 3 | [Phase 3: Latency budget and property tests](./phase-03-latency-and-property-tests.md) | Done (committed `51471cf`, `5c4cb6c`); two non-code criteria open | — |
| 4 | [Phase 4: Split the two oversized shell files](./phase-04-split-shell-modules.md) | Done; 135 tests unchanged | 3 |
| 5 | [Phase 5: Guard coverage and first-run onboarding](./phase-05-guard-coverage-and-onboarding.md) | Welcome + checklist done; **Safari probe needs you** | — |
| 6 | [Phase 6: Survive permission revocation and tap death](./phase-06-survive-tap-death.md) | Code done; **revocation reproduction needs you** | — |

**Execution order, decided at validation:** Phase 3 runs **first**, before
anything in the typing-accuracy plan. Phases 1, 5 and 6 are independent and can
slot in anywhere — though Phase 6 is P1 and cheap, and fixes a defect a user can
hit today. Phase 3 before Phase 4 is deliberate: the property tests are the safety net the
refactor leans on, and a pure-move refactor without one is an unverifiable
claim. **Phase 4 and Phase 5 both edit `app/src/tap.rs`** — Phase 4 rewrites it
wholesale into a directory — so they must never run concurrently. Run 5 first,
or run 4 to completion first; either order works, overlapping does not.

## Relationship to the typing-accuracy plan

`plans/260903-1637-unikey-phonotactics-and-restore/` owns the engine's
correctness residue (8,362 dictionary words still wrong after auto-fix). It is
still pending and is the highest user-facing value available.

- **File ownership is disjoint.** That plan changes
  `crates/glowkey-engine/src/` (auto-fix, phonotactics); Phase 4 here changes
  only `app/src/`. They can proceed in parallel without conflict.
- **A hard ordering, decided at validation.** Phase 3's property tests land
  **before** that plan's Phase 2 (ASCII-render restore). Restoring a word whose
  render is ASCII-but-different is precisely the change most likely to break the
  diff invariant, and a proptest harness catches that class of bug where a
  word-list corpus does not. This was offered as a recommendation and the owner
  made it a requirement: the typing plan does not start until Phase 3 here is
  green.

## Standing decisions carried forward

| Decision | Choice |
|---|---|
| InputMethodKit composition path | **Out.** Contradicts the CGEventTap design (`docs/decisions/0002`). |
| Legacy charsets, VIQR, clipboard encoding conversion | **Out.** Every modern macOS app is Unicode NFC. |
| Network access of any kind | **Out, permanently.** Confirmed at validation 2026-09-03. CI fails the build if a networking framework is linked, and that stays absolute — no Sparkle, no download, and no version-check ping either. A version ping is still a network call from an app that sees every keystroke. The GitHub release page is the update mechanism; the user checks it. README and PRIVACY.md both make this claim today and it is not being weakened. |
| Telemetry, analytics, crash reporting | **Out.** A keystroke-observing agent does not phone home. |

## What is left, and all of it needs a human

Every phase is implemented. Five things remain and none of them can be done
headless — they need a screen, a keyboard, or a keychain:

1. **Create the signing certificate** (Phase 1). Keychain Access → Certificate
   Assistant → Create a Certificate, name `GlowKey Developer`, type "Code
   Signing", self-signed. Then `tccutil reset Accessibility
   io.glowkey.GlowKey`, re-grant once, and check that a later rebuild does not
   ask again. That last check is the only proof the phase's central claim is
   true; if it fails, amend `docs/decisions/0006` — the fallback is today's
   behaviour, so nothing is lost.
2. **Reproduce the permission revocation** (Phase 6, step 1). Record whether the
   process survives at all; on some macOS versions the system kills it, which
   makes the recovery branch unreachable and harmless.
3. **Probe Safari's address bar** (Phase 5, step 1). Two of the three outcomes
   ship no code.
4. **Run `docs/manual-verification.md` once, end to end.** It has never been
   executed, and it says so in its own first paragraph.
5. **Install from the DMG on a clean Mac** (Phase 2). Expect the Gatekeeper
   refusal and the `xattr` command — documented, and the accepted cost of not
   buying a Developer ID.

## Carried forward from Phase 3

Phase 3's implementation is done, reviewed and verified — property suite green
(including against the mutation that proved its one real blind spot), latency
measured, `EMIT took=` in the log. Two of its six acceptance criteria are not
met, neither of them code, and both need the owner:

1. **The live `EMIT took=` figures in Chromium versus a plain field.** The field
   is in the log; reading it needs a granted build and someone typing in Chrome.
   Until then §6.1's "typ. sub-ms" stays an estimate, and is now marked as one.
2. **"CI runs the property tests on every push" is unverifiable.** The repository
   has **no git remote**, so `.github/workflows/ci.yml` has never executed. And
   the engine job's first step is `cargo fmt -p glowkey-engine -- --check`, which
   fails on formatting drift in roughly twenty committed files — present in
   `HEAD`, unrelated to this phase — so even with a remote the job would go red
   before reaching any test. No workflow change was made, because none is needed:
   `cargo test -p glowkey-engine` already covers both new test targets. What is
   needed is a remote and a decision about the drift.

Neither blocks the typing-accuracy plan: its gate was "the property tests are
green", and they are.

## Success Criteria

- [ ] Two consecutive `scripts/release-install.sh` runs with a code change in
      between, and the Accessibility switch is still on after the second
- [ ] A tag produces a downloadable artifact that installs and runs on a Mac
      that has never built the source
- [ ] `cargo test --workspace` includes property tests that fail when the diff
      invariant is deliberately broken (proved by a temporary mutation)
- [ ] Keystroke latency has a committed number, engine-side and in-app
- [ ] No file in `app/src/` exceeds ~700 lines; behavior identical (same test
      set green, unchanged)
- [ ] `docs/handoff.md` §6 carries no item still described as unverified that
      this plan touched
- [ ] Revoking the Accessibility switch while the app runs is visible within
      seconds, and re-granting recovers without a relaunch

## Open questions

1. **Is Safari worth guarding?** Phase 5 measures before it builds. If Safari's
   smart search field does not show the same trailing-selection pattern, the
   answer is a recorded negative result and no code. Resolved by Phase 5 step 1,
   not by discussion.

Questions 2 and 3 of the original draft (Developer ID, auto-update) were
answered at validation — see the Validation Log.

## Validation Log

### Session 1 — 2026-09-03

**Verification pass (Full tier, 5 phases).** ~20 claims checked against the
codebase. Verified: 17. Failed: 3, all corrected before the interview:

| Claim | Reality | Fix |
|---|---|---|
| Phase 2 would point the About window at the plist | `app/src/about_window.rs:18` already reads `CFBundleShortVersionString` | Downgraded from a change to a verification step |
| Phase 3 would assert against `engine.rendered()` | No such accessor; the field is private, `Session::current_word()` (`lib.rs:466`) is its only reader, and the per-key entry is `Session::process_key` (`lib.rs:1058`) | Real names substituted throughout |
| Phase 3 would add the timing field in `log.rs` | The `KEY …` line is assembled at `tap.rs:637`; `log.rs` only prefixes sequence and uptime | Call site corrected, `log.rs` dropped from the file list |

Phase 4 was additionally sharpened with two verified Rust facts (an inherent
`impl` may live in any module of the defining crate; a private field is visible
to descendant modules) and the exact static to protect (`DISABLED` at
`tap.rs:102` — the only file-scope one; the rest are function-local `OnceLock`s).

**New phase raised during the interview.** The owner asked whether revoking
Accessibility after startup causes lag or a loop. Answer, read out of the code:
neither — `accessibility_trusted()` is called only in the startup gate
(`tap.rs:1206`, `:1298`, `:1355`, `:1363`), nothing polls afterwards, and there
are no timers or threads. But the tap dies silently: the process lives, the menu
bar keeps showing **VI**, the log says nothing, and re-granting does not recover
without a relaunch. The `TapDisabled*` branch at `tap.rs:1147-1158` re-enables
blind with no return check and no log line, so a timeout disable is equally
invisible. Added as **Phase 6** (P1, half a day).

**Decisions.**

| # | Question | Decision | Effect |
|---|---|---|---|
| 1 | Developer ID + notarization ($99/yr)? | **Self-signed only, for now** | Phase 2 ships an ad-hoc DMG with the quarantine workaround documented; the notarization branch is removed, not stubbed. Revisit only when someone other than the owner needs to install it. |
| 2 | Which plan runs first? | **Phase 3 here, then the typing-accuracy plan** | The soft ordering becomes hard. Property tests are the safety net the ASCII-render restore lands on top of. |
| 3 | Form of the first-run onboarding? | **One-time `NSAlert`, reopenable from the menu** | Phase 5 keeps `welcome.rs` and the "Quick Guide" menu item as planned. |
| 4 | Auto-update? | **No network, ever** | Recorded as a standing decision above. Not deferred — closed. Includes version-check pings. |

### Whole-Plan Consistency Sweep

Re-read `plan.md` and all five phase files after propagation. Checked for stale
terms, superseded assumptions, and duplicated contracts.

- "Decision gate" language in Phase 2 step 5 removed — the decision is made.
- Phase 2's Gatekeeper risk rewritten: it is no longer a risk to be resolved by
  a gate, it is an accepted, documented limitation.
- Phase 3's "soft ordering" wording reconciled with the hard ordering here.
- Plan open questions reduced from three to one; the two closed ones now appear
  only in this log, not as open items.
- Phase 6 added after the sweep began; re-checked against all five earlier
  phases. It shares `app/src/tap.rs` with Phases 4 and 5, so the same
  no-concurrency rule applies to all three — recorded in Phase 6 and in the
  ordering note above.
- No unresolved contradictions.

<!-- slug: glowkey-hardening-and-distribution -->
