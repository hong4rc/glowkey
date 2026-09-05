# GlowKey — production hardening, cross-platform completion, UI polish

**Status:** proposed, 2026-09-05. Not started.
**Base:** `main` @ `ce51ff0`.
**Origin:** six parallel audits + an adversarial arbiter pass,
`plans/reports/orchestrate-260905-1643/` (read `arbiter/result.md` first — it is
the only one that verified the others).

## What this plan is for

GlowKey works and is unusually well documented, but it is not a product a
stranger can install and trust. Three things stand in the way, and they are not
the ones a feature list would suggest:

1. **It can silently destroy the user's text and the user's settings.** The
   Windows Chromium guard forward-deletes a real character in any Chromium
   window (`inject.rs:220`), and a corrupt settings file destroys its own backup
   on the first save (`settings_store.rs:61-63`) — on macOS with no user action
   at all.
2. **It writes everything you type to an unprotected file, always, and says so
   only for macOS.** That is the single most serious thing about a keyboard
   tool, and `PRIVACY.md` does not describe the Windows half of it.
3. **Neither platform has a distributable artifact.** macOS ships ad-hoc signed
   (so Homebrew is closed as of 2026-09-01 and Accessibility must be re-granted
   every release); Windows ships nothing.

Cross-platform completion and UI polish matter, but they are *after* these.
Shipping more surface on top of a text-eating bug makes the product worse.

## Non-goals

- Reversing verified decisions (mode is session-only, Backspace deletes visible
  characters, English-restore ships off). The handoff records these as
  reaffirmed under live use; they change only on new evidence, not on an audit's
  opinion. See `decisions.md`.
- Legacy encodings (TCVN3, VNI-Windows), VIQR, clipboard encoding conversion —
  intentionally omitted, unchanged.
- Telemetry of any kind. The privacy work below only ever *reduces* what is
  recorded.

## Phases

| # | Phase | Depends on | Blocked by |
|---|---|---|---|
| 1 | [Safety fixes](phase-1-safety-fixes.md) — data loss, memory safety, silence | — | nothing |
| 2 | [Privacy and the log](phase-2-privacy-and-logging.md) | 1 | decision A2 for the toggle |
| 3 | [Chromium guard and the Windows watchdog](phase-3-correctness.md) | 1 | decision A1; hardware for the full fix |
| 4 | [Windows shell parity](phase-4-windows-parity.md) | 3 (hook.rs contention) | decisions B4, B8 |
| 5 | [macOS runtime verification](phase-5-macos-runtime.md) | — | **a Mac**; gates every macOS-side change |
| 6 | [Release and signing](phase-6-release.md) | 1, 2 | decisions A4, A5 |
| 7 | [Refactor and de-duplication](phase-7-refactor.md) | 5 | 5 for anything macOS-side |
| 8 | [Linux (IBus engine)](phase-8-linux.md) | 1–6 | decision B26; a Linux machine |

Phases 1, 2 and 6 are the "can a stranger use this" path. Phases 3 and 5 are the
"is it correct" path. 4, 7, 8 are expansion and should not start first.

## Acceptance criteria for the plan as a whole

- No code path can delete the user's text without the user having pressed a key
  that means deletion.
- No code path can destroy the user's settings or their only backup.
- What GlowKey records about typing is disclosed accurately for **every**
  platform it ships on, defaults to the least it can, and the user can delete it
  from the UI.
- Both shipped platforms have a downloadable artifact whose install path is
  documented and true.
- `cargo test --workspace` green (331 passing today), `cargo clippy --workspace
  --all-targets -- -D warnings` clean, and a new `cargo audit` CI job green.
- Every claim in `README.md` about what is verified matches what has actually
  been run.

## Evidence and provenance

Every item traces to `plans/reports/orchestrate-260905-1643/`:

| Report | What it owns |
|---|---|
| `arbiter/result.md` | **Verdicts.** Per-P0 CONFIRMED/REFUTED, contradictions adjudicated, the deduplicated backlog (A1–A5, B1–B26) this plan is built from |
| `blame-user/result.md` | The user-facing complaint list, ranked by uninstall risk |
| `arch-refactor/result.md` | Layering audit (0012 holds), duplication, the do-not-refactor list |
| `crossplat/result.md` | Windows gap list, the League discriminating test, the Linux architecture |
| `production/result.md` | Signing, distribution, updates, crash behaviour, settings durability |
| `ui-polish/result.md` | Both renderers against `ui-design.md`, spec copy, accessibility |
| `security-cve/result.md` | `cargo audit` (0 vulnerabilities / 391 crates), unsafe review, keylog analysis |

Backlog IDs (`A1`, `B11`, …) are the arbiter's and are used throughout the phase
files so the two can be read together.

## Open decisions

Nine forks need the owner, not an engineer. They are in
[`decisions.md`](decisions.md) with a recommended default for each. Phases 2, 3,
4, 6 and 8 each name the decision that gates them.
