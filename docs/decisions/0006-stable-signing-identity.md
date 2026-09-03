# 0006 — Signing: a stable self-signed identity, and no notarization

## Status

Accepted (2026-09-03).

## Context

`scripts/build-app.sh` ended with `codesign --force --deep -s -` — an **ad-hoc**
signature. TCC keys an ad-hoc-signed application to its **cdhash**, and the
cdhash changes with every code change, so every install of a changed build was a
brand new application as far as macOS was concerned and the Accessibility grant
was silently dropped. The app then sat at the permission gate, and the developer
went to System Settings again.

This was the largest day-to-day friction in the project. Two earlier pieces of
work are mitigations for it rather than fixes: the visible permission gate
(§6.5), which stopped an `LSUIElement` agent with no icon from merely looking
dead, and the separate **GlowKey Dev** app identity (§8), which at least kept the
dev loop from invalidating the grant of the app you actually type with.

The cdhash mechanism is visible at build time now. `build-app.sh` prints the
designated requirement, and for an ad-hoc build it reads:

```
cdhash H"3d9e745785cab5d73dac50a903e4fae5db737f42" or cdhash H"5196b39e…"
```

Two hashes, one per architecture, both of which move whenever the code does.

## Decision

**Sign with a stable self-signed certificate when one is present, ad-hoc
otherwise.** `build-app.sh` resolves the identity through
`security find-identity -v -p codesigning`, defaulting to the name
`GlowKey Developer` and overridable with `GLOWKEY_SIGN_IDENTITY`. When it
resolves, the signing failure path is deliberately **not** silenced: with a
certificate present, a failure to sign is a real problem and must be loud rather
than falling back behind the developer's back. When it does not resolve, the
build still succeeds and says why the grant will be dropped — a machine without
the certificate must still be able to build the app.

Creating the certificate stays a **manual, documented step**: Keychain Access →
Certificate Assistant → Create a Certificate, name `GlowKey Developer`, type
"Code Signing", self-signed, ten-year validity. There is no reliable
non-interactive way to produce a self-signed certificate the system trusts for
code signing, and a script that pretended to automate it would fail in a way the
developer could not diagnose.

**No notarization.** Notarization requires a paid Apple Developer account
($99/year). The owner chose self-signed at plan validation, with the consequence
understood.

## Consequences

- A rebuild keeps the grant, once the certificate exists. The stale ad-hoc TCC
  entry has to be cleared **once** (`tccutil reset Accessibility
  io.glowkey.GlowKey`) and the app re-granted — the last re-grant the project
  needs.
- A downloaded release **is refused by Gatekeeper**: "GlowKey is damaged and
  can't be opened." It is not damaged; it is quarantined. The release notes and
  the README carry the one command that clears it
  (`xattr -dr com.apple.quarantine /Applications/GlowKey.app`). This makes the
  artifact usable by someone who can run a Terminal command, and awkward for
  anyone else. That is the accepted cost of not buying a Developer ID.
- The signing key stays in a developer's keychain and is deliberately **not** put
  into CI as a secret. Exporting a private key into repository secrets is the one
  way this could create a security problem, and it would buy nothing: a
  self-signed certificate does not satisfy Gatekeeper either, so CI signs ad-hoc.
- The certificate expires. When it does the identity stops resolving and the
  build quietly falls back to ad-hoc — the "no signing identity" line in the
  build output is the signal. Ten-year validity pushes this out far enough not to
  matter in practice.

## Not yet verified

The mechanism is documented Apple behaviour and the cdhash half of it is now
demonstrated in the build output, but **the claim that a self-signed certificate
makes the grant survive a code change has not been tested on this machine** — no
certificate exists here yet. The test is two builds from different source with
the same printed designated requirement, followed by a rebuild that does not ask
for permission again. If it turns out not to hold, the fallback is today's
behaviour (no loss) and this record should be amended rather than deleted.

## Alternatives rejected

- **Developer ID + notarization.** The correct answer for distributing to other
  people, and the owner's decision is to revisit it only when someone other than
  the owner needs to install GlowKey.
- **Working around Gatekeeper.** There is no legitimate way, and effort spent
  there is effort not spent on the input method. The only real fix is
  notarization.
- **A signing key in CI.** Rejected on both security and pointlessness grounds,
  as above.
