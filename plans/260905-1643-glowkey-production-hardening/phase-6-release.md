---
phase: 6
title: "Release, signing and distribution"
status: in-progress
priority: P2
effort: "2-3d"
dependencies: [1, 2]
---

# Phase 6 — Release, signing and distribution

**Depends on:** phases 1 and 2 (do not distribute more widely while the app can
eat text or logs it silently).
**Gated by:** decisions A4 ($99 Apple) and A5 (Windows signing).

## 6.1 — The macOS artifact is ad-hoc signed, and CI guarantees it [A4]

`decisions/0006` describes a stable self-signed identity. That applies to **dev
machines only**. `build-app.sh:126-135` falls back to `codesign -s -` when
`GlowKey Developer` is not in the keychain, and `release.yml:35-39` runs it on a
bare `macos-latest` runner that never imports a certificate. `0006:69` says so
outright: "CI signs ad-hoc."

Three consequences:

1. **Gatekeeper's harshest path.** Ad-hoc is not "unidentified developer", it is
   no identity — commonly the "damaged" dialog, and Sequoia removed the
   Control-click bypass. (The macOS 26 wording is inference, not reproduced —
   confirm it in phase 5 before rewriting install docs.)
2. **Accessibility must be re-granted on every single update**, because TCC keys
   the grant to the cdhash (`0006:10-11`) and ad-hoc signing changes it every
   build. For a background agent with no window, that reads as "the new version
   is broken".
3. **Homebrew is closed.** Casks failing Gatekeeper are being removed as of
   2026-09-01 and `--no-quarantine` is gone (verified: Homebrew/brew#20755). The
   obvious distribution route for a developer-audience Mac tool no longer exists
   without notarization.

**Now, unconditional:** `README.md:69-71` says "signed but not notarized". Ad-hoc
is not signed in any sense a user benefits from. Fix the wording — this is the
project's own honesty standard applied to its own README.

**With decision A4:** Developer ID + `notarytool` + hardened runtime in
`release.yml`, which also ends the re-grant problem.

## 6.2 — Windows ships nothing [A5]

Correctly, for now: the port is self-declared unverified and
`release.yml:16` has one macOS job. Do not package before phase 5's Tier 2.

When it is time: Azure Artifact Signing is $9.99/mo but **individuals are
eligible in US/CA only** and Vietnam appears on neither eligibility list
(Microsoft Learn, verified). OV certificate is ~$400–900/yr. MSIX/Store is a dead
end — a global keyboard hook will not certify. Unsigned means SmartScreen, which
for a keyboard tool is a reasonable thing for a user to heed.

## 6.3 — There is no way for a user to learn an update exists

A background agent with no window, no update check and no channel. Note this is a
**privacy contract change**: `PRIVACY.md` promises no network, and CI *enforces*
it by asserting no networking DLL is linked in the built binary
(`ci.yml:127-130`) — a genuinely good guard, better than most projects' privacy
claims, and one that would have to be relaxed deliberately and disclosed.

Options, cheapest first: document a check-this-page URL; a GitHub release feed
the user opts into; a signed appcast. Recommended: do nothing automatic until
6.1 is settled, and say plainly in the README how to find out.

## 6.4 — Pipeline hygiene [B13]

- No `cargo audit` in CI (phase 1 adds it).
- No checksums or provenance on release artifacts.
- Floating toolchain in the release job (`release.yml:19`).
- No `SECURITY.md`, no `CHANGELOG`, no issue template — for an app whose triage
  workflow depends on users pasting a version, a commit and a log excerpt. The
  About window already exposes version+commit selectably for this purpose; there
  is nowhere to paste it to.
- `THIRD-PARTY-NOTICES.md` is stale against a 391-crate lock file, and egui's
  bundled fonts carry OFL terms that are not covered. Windows ships no notices at
  all. Generate with `cargo about`.

## Validation

A release dry-run producing a signed, checksummed artifact whose documented
install steps have been followed on a clean machine by someone who did not build
it.
