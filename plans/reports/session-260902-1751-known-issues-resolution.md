# Session report — 2026-09-02 (15:15–19:00) — known-issues resolution

Goal (stop-hook): fix all issues, implement all needed features. All four
handoff §6 items addressed + adversarial review + hardening + docs. Working
tree clean; all work committed to main.

## Commits (this session)

- `3763ad3` feat: resolve all known issues — omnibox guard, terminal hardening,
  English restore, hotkey recorder
- `dce0249` fix: harden the four new features per adversarial review
- `e4cfe11` docs: retire stale InputMethodKit checkpoint
- `65932fa` docs: decision records 0003–0005
- `84ab739` docs: bring README and PRIVACY in line with shipped reality

## What shipped

1. Chromium omnibox mangling → AX-guarded forward-delete, scoped to Chromium
   bundles + AXTextField + non-empty selection. Best-effort (races Chrome's
   async pipeline) — decision 0003.
2. Terminal exclusions → tombstoned defaults merge at load; ⌃⇧E in a terminal
   is session-only + "VI ⚠" HUD — decision 0004. Ghostty protected.
3. English/Telex ambiguity → opt-in "Restore common English words" (~370-word
   embedded list), default OFF — decision 0005.
4. Hotkey recorder → "Custom…" segment arms capture of next ⌃/⌥ combo. Safety:
   only ⌃/⌥ combos intercepted; Esc/click/app-switch cancel; ⌃⇧E reserved.

Hardening from review (dce0249): tombstone survives toggle pairs; no
setSelectedSegment(-1); settings.json.bak before save; tests no longer write
the user's log; AX element cached; role-scoped guard; window resized; honest
captions. Full disposition:
plans/reports/code-review-260902-1554-fix-known-issues.md.

## Verification

- cargo test --workspace: 70/70 green. clippy --all-targets: clean.
- Verified empirically: tests leave log + settings mtimes unchanged.
- build-app.sh release OK; app relaunched (PID 17148 at time of writing).

## ak skills used

ak-code-review (commit mode, via code-reviewer subagent) — found 1 Critical /
8 Important / 13 Minor; all fixed, documented, or explicitly declined with
rationale in the review report.

## Blocked on user (nothing more can be done headless)

1. **Accessibility re-grant**: the rebuild's ad-hoc re-sign dropped the grant.
   App is polling; enable GlowKey in System Settings → Privacy & Security →
   Accessibility. Checked at 16:36 / 17:15 / 17:51 — not yet granted.
2. **Live verification by eye**: omnibox guard in Chrome (`hoongf` in address
   bar; log shows "OMNIBOX trailing selection detected" per fire), "VI ⚠" HUD
   on ⌃⇧E in Ghostty, Settings window (new checkbox, 5-segment hotkey picker,
   window at 460×540), hotkey recording flow, alacritty re-appearing in the
   excluded list (tombstone merge).

## Unresolved questions

1. Does Chrome's omnibox report non-empty AXSelectedText + AXTextField role for
   inline autocomplete? Guard's premise; only verifiable live.
2. Keep the 2-letter English words (as/is/us/of/or) in the opt-in list, or
   prune? Currently kept; costs á/í/ú/ò/ỏ in trailing-tone key order when ON.
3. setSelectedSegment on the new 5-segment picker — verify visually once.
