---
phase: 2
title: "Release pipeline"
status: in-progress
priority: P2
effort: "1d"
dependencies: [1]
---

# Phase 2: Release pipeline

## Overview

Make `git tag v0.2.0 && git push --tags` produce something another person can
download and run. Today the only way to get GlowKey is to clone the repo, have a
Rust toolchain, and run `scripts/release-install.sh`. There is no artifact, no
release, and the version number is duplicated in three places that can already
drift: `app/Cargo.toml`, `crates/glowkey-engine/Cargo.toml`, and two keys in
`app/Resources/Info.plist` (all currently `0.1.0`, all hand-maintained).

## Requirements

- Functional: a pushed `v*` tag produces a GitHub release carrying a
  `GlowKey-<version>.dmg` (universal, ad-hoc signed — see the workflow note).
- Functional: the version exists in exactly one place and flows to the bundle.
- Functional: the About window shows the same version the release carries
  (already wired to the plist; it inherits this once the stamp lands).
- Non-functional: the privacy guard already in CI (no networking framework
  linked) runs on the release build too, not only on the debug build.
- Non-functional: no network code enters the binary. A release page the user
  visits is the update mechanism. Auto-update is **closed, not deferred** — see
  the standing decisions in `plan.md`; that includes a bare version-check ping.

## Architecture

**Single source of version.** `app/Cargo.toml`'s `package.version` wins.
`build-app.sh` reads it (`cargo metadata --format-version 1 --no-deps`, or a
`grep`-free `cargo pkgid` parse) and writes both `CFBundleShortVersionString`
and `CFBundleVersion` with PlistBuddy, exactly as it already rewrites four other
Info.plist keys. The literal `0.1.0` in `Info.plist` becomes a placeholder that
is always overwritten — the same pattern the bundle identifier already uses.

**DMG.** `hdiutil create` over a staging directory holding `GlowKey.app` and a
symlink to `/Applications`. No background image, no fancy layout — a drag
target and nothing to maintain.

**Workflow.** A second GitHub Actions workflow, `release.yml`, on `push: tags:
v*`. It reuses the existing macOS job's build steps, then packages.

**Decided at validation: self-signed only, no notarization.** CI produces an
**ad-hoc-signed** DMG and the release notes carry the
`xattr -dr com.apple.quarantine /Applications/GlowKey.app` instruction that
Gatekeeper makes necessary. No Apple Developer account, no repository secrets,
no `notarytool` step — there is nothing here for a leaked credential to be. The
Phase 1 certificate is a **local developer** identity and is deliberately not
put into CI: exporting its private key as a secret is the one way this phase
could create a security problem, and there is no benefit, since a self-signed
certificate does not satisfy Gatekeeper either.

Revisit only when someone other than the owner needs to install GlowKey. At that
point the change is additive — `--options runtime`, `notarytool submit --wait`,
`stapler staple`, three secrets — and nothing built here has to be undone.

## Related Code Files

- Modify: `scripts/build-app.sh` — derive version from Cargo.toml, stamp the plist.
- Create: `scripts/make-dmg.sh` — stage, `hdiutil create`, verify.
- Create: `.github/workflows/release.yml` — tag-triggered build + package + release.
- **No change needed:** `app/src/about_window.rs:18` already reads
  `CFBundleShortVersionString` from the running bundle. Once `build-app.sh`
  stamps the version, the About window follows for free — verify, do not edit.
- Modify: `README.md` — an "Install" section that is not "clone and build".
- Modify: `docs/handoff.md` §8.

## Implementation Steps

1. Add version derivation to `build-app.sh`; assert non-empty and fail loudly if
   the parse returns nothing (a silently empty CFBundleVersion makes an
   un-launchable bundle).
2. Verify the About window picks the stamped version up (it already reads the
   plist at `about_window.rs:18`, so this is a check, not a change).
3. Write `scripts/make-dmg.sh`; verify the produced image mounts, and that the
   app inside it launches from `/Applications` after a drag.
4. Write `.github/workflows/release.yml`: checkout → toolchain → both targets →
   `build-app.sh release` → `make-dmg.sh` → `gh release create` with the DMG.
5. Write the release notes template with the quarantine workaround, and put the
   same instruction in the README's Install section — a user who hits
   "GlowKey is damaged and can't be opened" needs the fix where they are
   looking, not in a release note they have scrolled past.
6. Move the "no networking framework linked" assertion so it also covers the
   release binary that actually ships.
7. Tag `v0.2.0` and verify the release end to end on a Mac that has never built
   this source.

## Success Criteria

- [ ] Version appears once in the repo; bundle, About window and DMG name agree
- [ ] `release.yml` produces a mountable DMG from a tag
- [ ] The app from that DMG runs on a clean Mac after the documented
      quarantine command (permission grant expected once; that is inherent to any
      event tap, not a defect)
- [ ] The privacy assertion runs against the shipped binary
- [ ] README tells a non-developer how to install

## Risk Assessment

- **Ad-hoc DMG hits Gatekeeper — accepted, not open.** macOS refuses a
  downloaded ad-hoc app with "damaged and can't be opened" until the quarantine
  attribute is stripped. The owner chose self-signed at validation with this
  known; it is a documented limitation, not a risk to be resolved. The artifact
  is therefore for people who can run one Terminal command. *Do not* spend
  engineering effort working around Gatekeeper — the only real fix is a
  Developer ID, and that decision has been made for now.
- **Universal build in CI is slow.** Two targets plus objc2 from cold is a long
  job. Only on tags, so it costs nothing per push; add a cargo cache if it
  becomes annoying.
- **No secrets, and it stays that way.** This phase adds no repository secret
  and must not: the Phase 1 signing key never leaves the local keychain. If
  notarization is adopted later, the app-specific password becomes a secret and
  must never be echoed in a workflow step — but that is a future phase's problem,
  not this one's.
- **A tag that does not match Cargo.toml.** `v0.2.0` tagged while Cargo.toml
  still says `0.1.0` produces a mislabelled release. Add a workflow step that
  compares the tag to the derived version and fails on mismatch.

## Outcome — 2026-09-03

Version now lives in exactly one place. `build-app.sh` reads
`app/Cargo.toml`'s `package.version` and stamps both plist keys, so the literal
`0.1.0` in `Info.plist` is a placeholder that is always overwritten — the same
pattern the bundle identifier already used. The About window needed no change:
it already read `CFBundleShortVersionString` from the running bundle, so it
follows for free (this was one of three claims the plan's verification pass
caught as wrong before implementation).

`scripts/make-dmg.sh` stages the app plus an `/Applications` symlink and builds a
UDZO image. Tested end to end, not just written: the produced
`GlowKey-0.1.0.dmg` is 1.1 MB, passes `hdiutil verify`, mounts as
`/Volumes/GlowKey 0.1.0`, and the app inside reports version `0.1.0`,
identifier `io.glowkey.GlowKey`, and `Mach-O universal (x86_64 arm64)`.

`.github/workflows/release.yml` turns a `v*` tag into a release: it fails first
if the tag disagrees with `app/Cargo.toml`, runs the tests, builds, asserts the
**shipped** binary links no networking framework (CI previously only checked the
debug build), packages, and publishes with release notes carrying the quarantine
command. `actionlint` is clean on it and on the existing `ci.yml`.

Per the validation decision there is **no notarization and no secret in CI**: the
signing identity is a local developer certificate and exporting its private key
into repository secrets would be the one way this phase could create a security
problem, while buying nothing — a self-signed certificate does not satisfy
Gatekeeper either. CI signs ad-hoc and the notes say so.

### Left open

- The clean-Mac install test. Needs a Mac that has never built this source, and
  it will need `xattr -dr com.apple.quarantine` — expected, documented in the
  release notes and the README, and the accepted cost of not buying a Developer
  ID.
- The workflow has never executed, because the repository has no git remote.
