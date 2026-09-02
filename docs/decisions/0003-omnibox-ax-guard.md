# 0003 — Chromium omnibox guard via a scoped Accessibility read

## Status

Accepted (2026-09-02).

## Context

In Chromium browsers' address bar, inline autocomplete keeps a trailing
selection after each keystroke. GlowKey's synthetic Backspace deletes that
selection instead of a character, mangling words (`hoongf`→`hoồng`). Every blind
fix (always post Left-arrow, always post an extra delete) regresses normal
fields, which have no selection. The full fix — InputMethodKit composition —
contradicts decision 0002.

## Decision

Before emitting an edit with backspaces into a Chromium-family app (bundle-id
prefix list), make one read-only Accessibility query: is the focused element an
`AXTextField` with non-empty `AXSelectedText`? Only then post one forward-delete
to clear the selection first. Implemented in `app/src/ax.rs` +
`tap.rs::emit_edit`.

Guard rails: scoped to Chromium bundles AND `AXTextField` role (web content and
contenteditable are never touched); empty selection → nothing posted; every
failure reads as "no selection" (no behavior change); system AX element and its
process-global 50 ms messaging timeout created once.

This does not violate the blind model (decision 0002's "no host read-back"
principle applies to composing state): the guard never reads text content, only
whether a selection exists.

## Consequences

- Best-effort, not a proof: the AX read races Chrome's async renderer path, so a
  stale answer can occasionally skip or misfire the guard — a deterministic bug
  becomes a rare timing one. Accepted; EVKey has the unguarded bug permanently.
- 2–3 synchronous AX IPC round-trips per *transforming* keystroke in Chromium
  apps (typ. sub-ms, 50 ms cap), and querying AX keeps Chromium's accessibility
  tree enabled.
- Diagnosis: log lines "OMNIBOX trailing selection detected" (per fire) and
  "AX guard unavailable" (once per run).
