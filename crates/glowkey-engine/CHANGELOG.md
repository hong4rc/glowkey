# Changelog

All notable changes to `glowkey-engine` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the crate follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] - 2026-09-05

First version carved out of the GlowKey application.

### Added
- `Engine`, `KeyResponse`, `BackspaceOutcome`, `BoundaryBackspace`: raw keystrokes in, a minimal UTF-16 edit out.
- `InputMethod` (Telex, VNI, simple Telex) and `PlacementStyle`.
- `remove_tones`, `is_invalid_vietnamese` (with the stop-coda tone rule), `diff`.
- Optional `serde` feature for `InputMethod` and `PlacementStyle`.
