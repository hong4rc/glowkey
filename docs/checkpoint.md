# GlowKey checkpoint — superseded

**Read `docs/handoff.md` instead.** It is the maintained, current handoff (goal,
architecture, feature status, known issues, build/test/diagnose).

This file described the original **InputMethodKit / marked-text** prototype
(IMKInputController subclass, `setMarkedText`, install under
`~/Library/Input Methods`). That architecture was **replaced** by the current
**CGEventTap** design (background menu-bar agent, full suppression, synthesized
diffs — see `docs/decisions/` and the header of `app/src/platform/macos/mod.rs`), so its
verification checklist and install steps no longer apply.

Kept for history; the full old text is in git (`git log -- docs/checkpoint.md`).
Two of its observations that still matter were carried into `handoff.md`:

- `www` → `ww` is upstream `vi` Telex behavior, not an engine bug.
- Whole-word-uppercase tone placement is fixed by case-folding in
  `render()`/`apply_case()`; interior mixed case is best-effort.

Status as of 2026-09-02: all handoff §6 issues addressed (omnibox guard,
terminal hardening, opt-in English restore, hotkey recorder), adversarially
reviewed and hardened — see `plans/reports/code-review-260902-1554-fix-known-issues.md`.
Live GUI verification and the Accessibility re-grant remain with the user.
