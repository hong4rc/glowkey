---
phase: 5
title: "Publish readiness"
status: pending
priority: P2
effort: "3h"
dependencies: [1]
---

# Phase 5: Publish readiness

## Overview
Make `glowkey-engine` (and, once split, `glowkey-session` and `glowkey-input`)
something a stranger can depend on: metadata, docs, an example, a changelog,
semver guard rails, a stated MSRV.

## Requirements
- Functional: `cargo publish --dry-run -p glowkey-engine` succeeds.
- Non-functional: Rust API Guidelines checklist items that apply
  (naming, `#[non_exhaustive]`, `Debug`/`Clone` on public types, docs with
  examples, `Cargo.toml` metadata).

## Architecture
- `Cargo.toml` per library crate: `repository`, `homepage`, `documentation`,
  `readme = "README.md"`, `keywords = ["vietnamese", "telex", "vni", "ime",
  "input-method"]`, `categories = ["text-processing", "internationalization"]`,
  `rust-version` (the MSRV CI tests), `exclude` for benches fixtures.
- `crates/glowkey-engine/README.md`: what it is, a 10-line example, the `vi`
  dependency and its MIT licence, what it is not (no keyboard hooks).
- `crates/glowkey-engine/examples/type_a_word.rs`: `Engine::new(..)`, feed
  `hoongf`, print `hồng`.
- `#[non_exhaustive]` on `InputMethod`, `KeyResponse`-adjacent enums, `Decision`.
- `CHANGELOG.md` per crate (Keep a Changelog), starting at 0.1.0.
- CI: `cargo semver-checks check-release -p glowkey-engine` on PRs, added now
  with `continue-on-error: true` until the first tag, then strict (decided in
  validation); `cargo doc --no-deps -D warnings`; an MSRV job.
<!-- Updated: Validation Session 1 - semver step present from day one -->
- Root `README.md` "Layout" section: the four layers and which crate to take.

## Related Code Files
- Create: `crates/*/README.md`, `crates/*/CHANGELOG.md`, `crates/glowkey-engine/examples/type_a_word.rs`
- Modify: `crates/*/Cargo.toml`, `.github/workflows/ci.yml`, `README.md`

## Implementation Steps
1. Metadata and MSRV; `cargo publish --dry-run`.
2. README and example for the engine; `cargo run --example type_a_word`.
3. `#[non_exhaustive]`; fix the app's matches (add `_ =>` arms with a log line).
4. Changelogs; CI additions.
5. Root README layout section.

## Success Criteria
- [ ] `cargo publish --dry-run -p glowkey-engine` green.
- [ ] Example runs and prints `hồng`.
- [ ] `cargo doc --no-deps -D warnings` green for the library crates.
- [ ] CI has semver-checks (allowed to skip until a tag exists), doc, MSRV.

## Risk Assessment
- `#[non_exhaustive]` on enums the app matches exhaustively adds wildcard arms;
  each must log rather than silently do nothing (`decisions/0007`).
- `cargo semver-checks` needs a published or tagged baseline; until then the
  CI step is present but marked continue-on-error, and the plan says so.
