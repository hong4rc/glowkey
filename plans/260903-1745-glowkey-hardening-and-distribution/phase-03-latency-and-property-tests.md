---
phase: 3
title: "Latency budget and property tests"
status: in-progress
priority: P1
effort: "1d"
dependencies: []
---

# Phase 3: Latency budget and property tests

## Overview

The engine has one load-bearing invariant, stated in `docs/handoff.md` §5:
**"rendered == the text tail at the caret."** Everything the app does rests on
it, and every reported typing bug is a violation of it. It is currently asserted
only by hand-written examples — good ones (94 tests, a 51-word spell-check
corpus, thirteen bracket words), but examples cannot cover a keystroke space this
large, and the next change to auto-fix is exactly the kind that breaks it in a
case nobody thought to write down.

Second gap: an input method's latency is a feature, and GlowKey has no number
for it. The engine re-derives the whole word through the `vi` crate on every
keystroke, and in Chromium the tap adds up to 2–3 AX IPC round-trips per
transforming key (capped at 50 ms). Nobody has measured either.

**This phase gates other work.** Decided at validation: the ASCII-render restore
in `plans/260903-1637-unikey-phonotactics-and-restore/` (its Phase 2) does not
start until these property tests are green. That plan's frontmatter carries the
`blockedBy` entry. Nothing else in the repository is blocked.

## Requirements

- Functional: a property test suite that generates keystroke sequences and
  asserts the diff invariant, running in CI on Linux with the engine job.
- Functional: an engine benchmark with a committed number, and a wall-clock
  regression guard generous enough not to flake on shared CI runners.
- Functional: the tap logs how long it spent per handled key, so the real
  in-app cost — including the AX guard — is visible in
  `~/Library/Logs/GlowKey/glowkey.log`.
- Non-functional: the property tests must be *proved* to work by deliberately
  breaking the invariant once and watching them fail.

## Architecture

**The invariant, mechanically.** For a keystroke sequence, track a model string
`tail`. For each key, `Session::process_key(ch)` (`lib.rs:1058`) answers
`KeyResponse { handled, backspaces, insert }`. Apply it: drop `backspaces`
UTF-16 code units from the end of `tail`, append `insert`. Assert
`tail == session.current_word()` (`lib.rs:466`; there is no `rendered()`
accessor — the field is private and `current_word` is its only reader). That single equality is the
whole blind model; when it holds for every generated sequence, the app is
correct by construction, and when it fails the generated sequence is a minimal
repro.

Properties worth stating, in value order:

1. **Diff consistency** (above) — the one that matters.
2. `backspaces` never exceeds the previous render's UTF-16 length. A backspace
   into the host's pre-existing text is data loss, the worst failure this app has.
3. No panic and no infinite loop for any ASCII input, including the punctuation
   and bracket keys.
4. `Session::backspace_visible_char` (`lib.rs:1438`) either returns false or leaves the render
   equal to the previous render minus its last character — the contract
   `tap.rs` relies on to decide whether to flush.
5. Idempotence of a boundary: committing at a boundary twice does not emit twice.

Generators stay in ASCII printable, weighted toward the Telex-significant keys
(`a e o d w f s r x j z` and the vowels) so the space explored is the space that
transforms. Run each property across all three input methods and with each
opt-in option on and off — the options are exactly where untested interactions
live (Quick Telex × brackets × strict spell check × English restore is a
16-cell matrix that no hand-written test covers).

**Latency.** `criterion` bench over representative words (`hoongf`, `nguowif`,
`vieetj`, a 9-letter English word that triggers auto-fix). Separately, a plain
`#[test]` that runs 10,000 keystrokes and asserts a generous ceiling — this is
the CI guard; the criterion numbers are for the record. In-app, add elapsed
microseconds to the existing log line format in `app/src/log.rs`, measured from
tap-callback entry to emit completion; that is the number that includes AX.

## Related Code Files

- Create: `crates/glowkey-engine/tests/properties.rs` — the property suite.
- Create: `crates/glowkey-engine/benches/keystroke.rs` — criterion bench.
- Modify: `crates/glowkey-engine/Cargo.toml` — `proptest` and `criterion` as
  dev-dependencies, `[[bench]]` with `harness = false`.
- Modify: `app/src/tap.rs:637` — the `KEY …` line is assembled at that call
  site, so the elapsed-µs field is added there. `log.rs` only prefixes sequence
  number and uptime and needs no change.
- ~~Modify `.github/workflows/ci.yml`~~ — **struck, no change needed.** The
  engine job already runs `cargo test -p glowkey-engine`, which covers every test
  target in the crate including the two new ones, and `cargo clippy -p
  glowkey-engine --all-targets` already builds the bench. The workflow needs no
  new lines. (What it does need is a git remote and a formatting decision — see
  the Outcome.)
- Modify: `docs/handoff.md` §7 (the log line gains a field) and §8.

## Implementation Steps

1. Add `proptest`; write property 1 alone and run it. Expect it to find
   something — if it passes on the first run with a wide generator, suspect the
   harness before believing the engine.
2. Triage whatever it finds: a genuine invariant violation is a bug to fix and
   pin; a case where the invariant does not apply (an unhandled key, a flush) is
   a harness correction, and the reason must be written into the test.
3. Add properties 2–5.
4. Add the options matrix and the three input methods as proptest parameters.
5. **Prove the suite works:** temporarily make `KeyResponse` under-report
   `backspaces` by one, confirm property 1 fails with a shrunk repro, revert.
   Record the shrunk case in the commit message.
6. Add the criterion bench; commit the measured numbers into the phase record.
7. Add the wall-clock ceiling test with a margin wide enough for a loaded CI
   runner (target: ceiling at roughly 10× the measured local time).
8. Add elapsed-µs to the `KEY` line at `tap.rs:637`; read the log while typing
   in Chrome and in a plain field, and record both numbers — that difference is the AX guard's real
   cost, which §6.1 currently describes only as "typ. sub-ms".

## Success Criteria

- [x] Property 1 holds across all three input methods and the full options matrix
- [x] The suite demonstrably fails on a deliberately broken diff (step 5)
- [x] Engine per-keystroke time is a committed number
- [ ] The log shows per-key elapsed time (**done**); the Chromium-vs-plain
      difference recorded in `docs/handoff.md` §6.1 (**open** — needs a live
      granted build, see Outcome)
- [ ] CI runs the property tests on every push (**blocked** — no git remote, and
      the engine job's fmt step fails on pre-existing drift; see Outcome)
- [x] Any bug the properties find is fixed and pinned by a named regression test
      (none found; the mutation test proves the suite would catch one)

## Risk Assessment

- **The properties find real bugs and the phase grows.** This is a success, not
  a risk, but it does mean the estimate is soft. *Response:* fix what is found;
  if a finding is large enough to be its own piece of work, pin it with an
  `#[ignore]`d failing test and a note, rather than silently weakening the
  property. Never weaken a property to make it pass.
- **Proptest flakiness in CI.** A random suite that fails one run in fifty is
  worse than no suite. *Response:* fixed seed via `PROPTEST_CASES` and a
  committed `proptest-regressions` file, which is the standard practice and
  makes failures reproducible.
- **Timing tests are inherently flaky on shared runners.** *Signal:* the ceiling
  test fails on CI but not locally. *Response:* widen the margin once; if it
  flakes again, move the timing assertion out of CI and keep it as a local
  `--ignored` test. A flaky guard gets disabled and then rots.
- **Criterion adds compile time to every `cargo test`.** It does not — benches
  build only under `cargo bench` — but `[[bench]] harness = false` must be set
  or cargo will try to run it as a test target.

## Outcome — 2026-09-03

### Property tests: the invariant holds

`crates/glowkey-engine/tests/properties.rs`, four tests, 2048 generated cases
each by default. **No bug found.** Re-run at 60,000 cases per property: still
clean.

That is a real result rather than an absent one, because the suite was proved to
have teeth first. Step 5 was pulled forward and run before believing the pass:
mutating `diff()` (`lib.rs:1566`) to under-report `backspaces` by one made
property 1 fail and shrink to the minimal input `['o', 'o']` —

```
key 'o': screen "oô" != engine "ô" (tail before "o", edit bs=0 ins="ô")
minimal failing input: keys = ['o', 'o']
```

— which is precisely the `aa`→`aâ` race `docs/handoff.md` §5 records as the bug
that forced the full-suppression design. A suite whose first shrink reproduces
the project's defining bug from scratch is working.

Coverage was then checked empirically rather than assumed, because a property
that never reaches the delicate path passes for the wrong reason. Over 20,000
random sequences the model reached: **450** boundary re-compositions, **1,573**
mid-word shrinks, **922** flushes, **152** auto-fix restores, **5,163**
transforming keystrokes. All three Backspace branches fire.

The model mirrors `tap.rs::decide` deliberately — word character to
`process_key`, boundary to `commit` plus the restore edit, and Backspace through
the same three-case ladder with the *host* performing the delete. It tracks the
document as `committed` plus `tail`, so an edit reaching past the start of the
word is caught as the data-loss bug it would be rather than saturating silently.

### Latency: measured, and not a suspect

`cargo bench -p glowkey-engine`, Apple Silicon laptop, release:

| Bench | Time |
|---|---|
| Worst single keystroke (last key of `nguowif`) | **2.54 µs** |
| Whole word + commit, `hoongf` | 7.09 µs |
| Whole word + commit, `nguowif` | 8.20 µs |
| Whole word + commit, `ddaaij` | 8.45 µs |
| Whole word + commit, `strength` (auto-fix restore fires) | 8.90 µs |
| Whole word + commit, `vieetj` | 6.72 µs |
| VNI `viet65` / `hoang2` / `d9a1i` | 5.91 / 5.11 / 5.30 µs |

Per-keystroke, typing `hoongf` in a loop: **2 µs** release, **9 µs** in the
unoptimised profile CI runs. The ceiling in `tests/latency.rs` is 250 µs — 28×
the debug figure — so it cannot flake and still catches a microsecond path
turning into a millisecond one.

The conclusion worth keeping: the engine's re-derive-everything design costs
about two microseconds a key. A fast typist leaves 100,000 µs between
keystrokes, and the omnibox AX guard is capped at 50,000. **The engine is not
where latency can come from**, and `took=` in the log now says so per keystroke.

### Not done

- **The in-app `took=` numbers in a live Chromium window versus a plain field.**
  The field is in place and the log carries it; reading it needs a granted build
  and someone typing in Chrome. `docs/handoff.md` §6.1 still says "typ. sub-ms",
  and it is still an estimate — now explicitly marked as one. This is the one
  success criterion this phase leaves open, and it is user-side by nature.
- **"CI runs the property tests on every push" cannot be confirmed.** The repo
  has **no git remote**, so `.github/workflows/ci.yml` has never executed. On top
  of that, the engine job's first step is `cargo fmt -p glowkey-engine -- --check`
  and the committed code fails it (drift in roughly twenty files, present in
  `HEAD`, unrelated to this phase) — so even with a remote the job would stop
  before reaching any test. No CI change was made: `cargo test -p glowkey-engine`
  already runs both new test targets, and `cargo clippy -p glowkey-engine
  --all-targets` already builds the bench, so the workflow needs no new lines —
  it needs a remote and a formatting decision. Raised with the owner.

### Verification run

```
cargo test --workspace                    134 passed, 0 failed
cargo clippy --workspace --all-targets    silent
cargo fmt --check (new files only)         clean
```

## Review — 2026-09-03

`code-reviewer` was run against the implementation with the phase's acceptance
criteria as context. It confirmed the parts that were meant to be careful — the
Backspace three-case order, who performs the delete, the boundary
commit-then-replay, and that `carry_out`'s extraction is byte-identical to the
old inline match — and then found the hole somewhere else.

### The finding that mattered

**The restore edit's backspace count was never verified.** The boundary path
applied `commit()`'s auto-fix edit and checked only that it did not delete *too
much*. An edit deleting too *little* left the difference stranded on screen, and
because `commit` clears the re-composition memory whenever it restores, nothing
downstream would ever have noticed. The reviewer proved it rather than arguing
it: mutating `commit` to under-delete by one UTF-16 unit left **all four property
tests green**.

That is the worst possible place for the suite to be blind, because
`plans/260903-1637-unikey-phonotactics-and-restore/` Phase 2 rewrites that exact
decision — and this suite is the gate it was supposed to pass through.

Fixed by checking the restore edit exactly: its backspace count must equal the
rendered word's UTF-16 length (a restore replaces the *whole* word, so the count
is fully determined), and the resulting screen must equal what the edit inserted.
Re-running the reviewer's mutation now fails two tests with a precise message:

```
restore at boundary '1': deletes 2 UTF-16 units but the rendered word "oùa" is 3
 — it must replace the whole word
```

### Also fixed

| # | Finding | Fix |
|---|---|---|
| H2 | Proptest persistence was silently off — an integration test cannot find `lib.rs`, so every run printed a warning and kept no record. The plan's own flake mitigation was a comment describing something that did not happen. | `FileFailurePersistence::Direct` at an explicit path. A counterexample is now a committed file that re-runs first. |
| M3 | `note_boundary` was missing from the model's boundary path, so `pending_capital` only ever came from the session's initial state and the sentence-restart path after `.`/`!`/`?` never ran — roughly 9,000 uncovered paths reported as covered. | One line, in the same order the tap uses. |
| M4 | The timed span included `save_settings`'s file write and `hud::flash`'s first-call window creation, so §7's "a large `took=` means the AX guard" was false for every hotkey and per-app toggle — it would send the next person debugging latency at the wrong subsystem. | The span is now the emit alone, logged as its own `EMIT took=` line. That is where the only millisecond-scale cost is. |
| M5 | Moving the log write after the decision inverted causality (`TOGGLE`/`OMNIBOX`/`RUNAWAY` got *lower* sequence numbers than the KEY line that caused them) and lost the KEY line entirely if the emit path panicked. | KEY line restored to its original position; timing moved into `emit_edit`. |
| L6 | The unhandled-key branch pushed onto the tail while the engine had just reset, so a failure would surface one key late in the wrong place. Unreachable, but wrong. | Modelled as the boundary it is. |
| L7 | The comment claimed 48 configurations; `options()` generates 384. | Corrected, cases raised 2048 → 4096 (~10 samples per configuration), and the deterministic test renamed to what it actually covers. |
| L8 | "Committing twice does not emit twice" was the plan's property 5 but was never actually exercised. | Now asserted for real. |
| L9 | Property 4 calls `backspace_visible_char` without the `recompose_after_boundary_backspace` that always guards it in the tap, and goes vacuous whenever property 1 regresses. | Both documented in place; the wider contract is deliberate. |
| L10 | Dead `session` binding in the bench, and `vieetj` described as VNI while benched as Telex. | Removed / corrected. |
| Q1 | `commit`'s **macro-expansion** edit — a different edit shape, checked before auto-fix — was never reached, since no macros were configured. Same class of gap as H1. | `macro_defined` added to the generated options with the `vn` → `Việt Nam` shortcut. |
| Q2 | The model pops one `char` while `Screen::apply` counts UTF-16 units; those agree only while renders are precomposed BMP, which was assumed rather than checked. | Asserted, so a decomposed render fails loudly instead of diverging silently. |

### Judged and kept

The reviewer confirmed the 250 µs ceiling will not flake (it measured 8 µs/key
debug; shared runners are 2–4× slower, leaving a 7–12× margin) and agreed the
plan's escalation — widen once, then `#[ignore]` — is the right response if it
ever goes red. It also noted the ceiling test runs on both the Linux and macOS
jobs for no extra signal, since it is platform-free. Left as is: the duplicate
run costs a tenth of a second and removing it means adding platform conditions to
a workflow that currently has none.

### Verification after the fixes

```
cargo test --workspace                    134 passed, 0 failed
cargo clippy --workspace --all-targets    0 warnings
cargo fmt --check (the three new files)    clean
PROPTEST_CASES=60000 (release)             4 passed
cargo bench -p glowkey-engine -- --test    all pass
crates/glowkey-engine/src/lib.rs           sha 8bc82a8b… — byte-identical, no
                                           mutation residue
```
