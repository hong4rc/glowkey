# Orchestrate run — GlowKey improvement, refactor, cross-platform, production, UI, CVE

**Run:** `orchestrate-260905-1643` · `main` @ `ce51ff0` · 2026-09-05
**Request:** improve the app, refactor, plan for Windows/macOS/Linux, make it
production-ready, polish the UI, have a user say what is bad, check CVEs.
**Output:** `plans/260905-1643-glowkey-production-hardening/`

## Routing

Live probe of nine coding-agent CLIs found one (`gemini 0.21.3`, auth
unverified). Every job routed to **in-session subagents**: all work was
read-only analysis over a clean tree, needing no separately selectable model and
no isolation beyond read-only. No worktrees — the coordinator owned the only
writes (this directory, then the plan). `cargo-audit` was installed and run by
the coordinator; its output was handed to the security job rather than
letting it browse advisories by hand.

## Jobs

| Job | Agent | Status | Output |
|---|---|---|---|
| `blame-user` | code-reviewer | success | 26 ranked complaints, 6 P0 |
| `arch-refactor` | code-reviewer | success | 1 P0, 6 P1, 6 P2 + do-not-refactor list |
| `crossplat` | planner | DONE_WITH_CONCERNS | Windows gaps, League test, IBus architecture |
| `production` | researcher | success | 11 gaps, 3 P0 |
| `ui-polish` | ui-ux-designer | success | 5 P0, 17 P1, 11 P2 |
| `security-cve` | general-purpose | DONE_WITH_CONCERNS | 0 vulnerabilities; 6 posture findings |
| `arbiter` | kongming | DONE_WITH_CONCERNS | verdicts, contradictions, 31-item merged backlog |

No timeouts, no permission stops, no failed jobs.

## Arbiter checklist

| Question | Answer |
|---|---|
| Every job produced its artifact? | Yes — seven `result.md`, plus `cargo-audit.{txt,json}` |
| Any failure, timeout or uncertainty? | No failures. Uncertainty declared by five of six jobs and carried into the plan |
| Contradictions? | Six, all adjudicated by reading the code (§2 of the arbiter report) |
| Claims supported? | Yes, `path:line` throughout; every headline the arbiter opened was where it was said to be |
| Routes met capability and risk floors? | Yes — read-only jobs on read-only routes |
| Destructive actions? | None. No repository file was modified by any job |
| Unresolved questions listed? | Yes — 14, consolidated in the arbiter report §7, distilled to 9 in `decisions.md` |

## What verification changed

The arbiter pass was not a formality — it altered the plan:

- **REFUTED:** `crossplat` listed the Windows Chromium forward-delete as a
  deliberate non-gap "confirmed working in Edge". Only the omnibox case was ever
  tested; nobody typed mid-paragraph. It is now the plan's top item.
- **Mechanism corrected:** `blame-user` said the `.bak` backup dies on the second
  save. It dies on the **first**.
- **Severity corrected:** the macOS blocking log write is real but bounded
  (`health.rs` recovers a timed-out tap) → P0 to P1. Two `ui-polish` P0s
  similarly reduced to P1.
- **ARBITER-FOUND ×2:** macOS auto-saves at launch when `welcome_shown` defaults
  false, so a corrupt settings file destroys its own backup with **zero user
  action**; and macOS performs the whole settings file write **inside the tap
  callback** on hotkey keys, which is larger than the log write the audit
  reported and the same 0008 violation.
- **External claims:** Homebrew's 2026-09-01 unsigned-cask removal verified;
  Azure signing eligibility found *stricter* than reported (individuals US/CA
  only — Vietnam on neither list); one security finding (`explorer.exe` CWD
  search) partly refuted and re-scoped.
- **Counts:** the architecture job's "379 tests" is 331 passing, 0 failing.

## CVE result

`cargo audit`: **0 vulnerabilities** across 391 crates against 1239 advisories,
independently cross-checked against OSV.dev with known-bad controls. Two
unmaintained-only notices, neither actionable: `ttf-parser 0.25.1` (Windows
settings-window font stack; no fix exists — latest release, current egui still
depends on it; `#![forbid(unsafe_code)]`) and `paste 1.0.15` (**not compiled at
all** — lock-file-only behind an excluded wgpu feature). Zero yanked crates.

Nothing to fix. The real work is to notice the *next* one: `.cargo/audit.toml`
with justifications plus a CI job, in phase 1.

The substantive security findings were not dependencies but the app's own
behaviour: an always-on plaintext keystroke log with default permissions and a
macOS-only privacy policy, and an unbounded read of another process's clipboard
buffer.

## Ranked outcome

The five things that matter, in order:

1. **A1** Windows Chromium forward-delete eats real characters in any Chromium
   or Electron window.
2. **A2** Always-on plaintext keystroke log, no gate, default permissions,
   `PRIVACY.md` macOS-only, Windows settings roam off the machine.
3. **A3** No Windows hook-liveness check at all — a comment claims one exists.
4. **B1** Corrupt settings file destroys its own backup on the first save;
   automatic at launch on macOS.
5. **A4/A5** No trustworthy artifact on either platform; Homebrew route closed.

Full 31-item backlog: `arbiter/result.md` §4. Sequencing: §5.

## Reproduction

Read `arbiter/result.md` first, then the six source reports. Every verdict cites
`path:line` at `ce51ff0`. Re-run the dependency half with `cargo audit`.
