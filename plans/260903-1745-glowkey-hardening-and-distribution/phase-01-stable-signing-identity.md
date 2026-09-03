---
phase: 1
title: "Stable signing identity"
status: pending
priority: P1
effort: "0.5d"
dependencies: []
---

# Phase 1: Stable signing identity

## Overview

Stop the Accessibility grant from dying on every rebuild. Today
`scripts/build-app.sh` ends with `codesign --force --deep -s -` — an **ad-hoc**
signature. TCC keys an ad-hoc-signed app to its **cdhash**, which changes with
every code change, so each install of a changed build is a new app to macOS and
the grant is dropped. Signing with a stable self-signed certificate instead makes
the designated requirement `identifier "io.glowkey.GlowKey" and certificate leaf
= H"<fixed hash>"`, which does not move when the code does.

This is the single largest day-to-day friction in the project. `docs/handoff.md`
§6.5 and §8 both name it; the visible permission gate shipped last session is a
mitigation for a problem this phase removes.

## Requirements

- Functional: after a code change is rebuilt and reinstalled, GlowKey types
  Vietnamese immediately — no permission alert, no trip to System Settings.
- Functional: a machine with no GlowKey certificate still builds and runs. The
  script falls back to ad-hoc with a one-line explanation, never fails.
- Non-functional: the certificate is a local developer artifact. Its private key
  is never committed, never printed, never uploaded.
- Non-functional: the dev variant (`io.glowkey.GlowKey.dev`) keeps its own
  separate grant — signing both with the same certificate is fine, because the
  designated requirement also pins the bundle identifier.

## Architecture

Three moving parts:

1. **The certificate.** A self-signed code-signing certificate in the login
   keychain, common name `GlowKey Developer`. Created once, by hand, via
   Keychain Access → Certificate Assistant → Create a Certificate (type: Code
   Signing, self-signed). There is no reliable non-interactive way to create a
   *trusted-for-codesigning* self-signed certificate, so this stays a documented
   one-time manual step rather than a script that pretends to automate it.

2. **Identity discovery in the build script.** `security find-identity -v -p
   codesigning` is parsed for the certificate name; found → sign with it, not
   found → ad-hoc as today.

3. **The one-time reset.** An existing ad-hoc grant does not transfer. After the
   first signed build the stale TCC entry must be cleared once
   (`tccutil reset Accessibility io.glowkey.GlowKey`) and the app re-granted —
   the last re-grant the project needs.

## Related Code Files

- Modify: `scripts/build-app.sh` — replace the unconditional ad-hoc `codesign`
  with identity discovery plus a fallback branch.
- Modify: `README.md` — the "Develop" section currently states the grant is
  tied to the ad-hoc signature and must be re-granted; that becomes the
  fallback case, with the certificate setup as the recommended path.
- Modify: `docs/handoff.md` — §6.5 and §8.
- Create: `docs/decisions/0006-stable-signing-identity.md` — why self-signed
  rather than Developer ID, and what it does not buy (Gatekeeper).

## Implementation Steps

1. Create the certificate by hand and confirm it is usable:
   `security find-identity -v -p codesigning` lists `GlowKey Developer`.
2. In `build-app.sh`, resolve the identity into a variable before the sign step;
   `SIGN_IDENTITY="${GLOWKEY_SIGN_IDENTITY:-GlowKey Developer}"` so the name is
   overridable without editing the script.
3. Sign with `codesign --force --deep --sign "$SIGN_IDENTITY" "$APP"` when the
   identity resolves; otherwise keep the ad-hoc line and print
   "no signing identity — ad-hoc signed, expect to re-grant Accessibility".
   Drop the `2>/dev/null || true` on the real path: a signing failure with a
   certificate present must be loud, not swallowed.
4. Print the resulting designated requirement in the script's summary
   (`codesign -d -r- "$APP"`), so a broken signature is visible at build time
   rather than at permission time.
5. Reset the stale grant once, install, re-grant.
6. **Prove it:** change a string in the source, rebuild, reinstall, and confirm
   Vietnamese still types with no alert. Then confirm the requirement string is
   byte-identical across the two builds.

## Success Criteria

- [ ] `codesign -d -r-` prints the same designated requirement for two builds
      made from different source
- [ ] The grant survives a code change → rebuild → reinstall cycle (typed proof:
      `hoongf` gives `hồng` immediately after relaunch)
- [ ] A build on a machine without the certificate still produces a runnable
      bundle and says why the grant will be dropped
- [ ] `docs/decisions/0006` records the choice and its limit

## Risk Assessment

- **The self-signed certificate does not actually stabilise TCC.** The mechanism
  is documented behaviour, but it is being asserted here from reasoning, not from
  an in-repo measurement. *Signal it broke:* step 6 shows the permission alert
  again, or `codesign -d -r-` differs between builds. *Response:* fall back to
  the current ad-hoc scheme (no loss — it is today's behaviour) and record the
  negative result in decision 0006 rather than deleting the file.
- **`--deep` is deprecated for signing.** It works and the bundle has no nested
  code, so it is harmless today; if it ever warns, drop it — there is nothing
  nested to sign.
- **Certificate expiry.** A self-signed certificate has an expiry date; when it
  passes, the identity stops resolving and the script silently falls back to
  ad-hoc. *Signal:* the "no signing identity" line appears in a build that used
  to sign. *Response:* create a new certificate, accept one re-grant. Set a long
  validity (10 years) at creation to push this out.
- **Never commit the private key.** The certificate lives in the keychain; no
  `.p12`, no key material, enters the repo. Anything else is a credential leak.
