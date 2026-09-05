---
phase: 3
title: "Gates, docs, branch and merge"
status: pending
priority: P2
effort: "1h"
dependencies: [1, 2]
---

# Phase 3: Gates, docs, branch and merge

## Overview

Run the full handoff §11 gate list on the finished work, update the handoff and
the decision record where this changed what they say, land the branch on `main`,
push, and write the journal entry.

## Requirements

- Functional: every gate in §11 green; `main` fast-forwarded and pushed; a
  journal entry written.
- Non-functional: commits conventional, focused, no AI references; no plan IDs
  or phase numbers in commit messages or code comments — the invariant is
  described directly.

## Architecture

The gate list is the one in handoff §11's working notes, and it is run **on
every change**, not once at the end:

| Gate | Why it is in the list |
|---|---|
| `cargo test --workspace` | the headless suites, including the spec tests phase 1 adds to |
| the three library crates with and without `--features serde` | `serde` is optional on the engine and the session crate; a default build must not need it |
| `cargo clippy --workspace --all-targets -- -D warnings` | the Windows host |
| `cargo clippy --target aarch64-apple-darwin -p glowkey --all-targets -- -D warnings` | **the only gate that compiles phase 1's file.** It is what caught a stale test field the previous phase missed |
| `cargo check --target x86_64-unknown-linux-gnu` for the three crates | what keeps "nothing in the three library crates names a platform" true (`decisions/0012`) |
| `cargo doc` with `RUSTDOCFLAGS=-D warnings` | the crates are publish-bound; a broken intra-doc link is a release blocker |

Docs to touch, and only these:

- `docs/handoff.md` §11. Item 3 (macOS renderer parity) is done — strike it and
  say what changed. Item 1 is **not** done: rewrite it to say the session that
  ran was on Windows, and add phase 1's three items to what the next Mac session
  must watch (checkbox column, count units, row rhythm) alongside the four tabs,
  ⌃⇧Space, ⌃⇧E and ⌃⇧W. Item 2 (Tier 5) closes with a pointer to the report,
  minus whatever it left user-owned.
- `docs/manual-verification-windows.md` — the Tier 5 boxes phase 2 actually
  ticked, and its "Recording the results" pointer to the new report.
- `docs/decisions/0012-engine-layering-and-ports.md` — only if phase 2 found
  something that changes what it claims. The unit strings moving into the spec
  is `0010`'s territory, not `0012`'s, and `0010` already says the spec owns the
  rows' content; check whether it needs a sentence rather than assuming it does.
- No ADR for this plan: it changes no decision. Porting a renderer to a rule
  already decided in `260905-1145` is execution.

## Related Code Files

- Modify: `docs/handoff.md` (§11).
- Modify: `docs/manual-verification-windows.md` (Tier 5 boxes, results pointer).
- Modify: `docs/decisions/0010-shared-settings-spec.md` — only if the spec
  gaining `ListId::unit` contradicts a sentence there.
- Create: the journal entry, through the journal capability.

## Implementation Steps

1. `git switch -c` a feature branch off `main`.
2. Commit phase 1 as its own change; commit any phase 2 fix separately, each
   with its cause in the message.
3. Run the six gates above; fix and re-run rather than narrowing a gate.
4. Update the three docs.
5. `git switch main && git merge --ff-only <branch>`; push.
6. Write the journal entry: what the port changed, what Tier 5 found, and what
   is still owed — the macOS runtime pass, now with three more things on its
   list.

## Success Criteria

- [ ] All six gates green on the final tree.
- [ ] `docs/handoff.md` §11 item 3 closed, item 1 rewritten with phase 1's
      additions, item 2 pointing at the report.
- [ ] Tier 5 boxes in `docs/manual-verification-windows.md` reflect the run.
- [ ] Feature branch fast-forwarded into `main` and pushed; history linear.
- [ ] Journal entry written.
- [ ] No plan ID, phase number or audit label appears in a commit message, code
      comment or test name.

## Risk Assessment

- **`--ff-only` refuses because `main` moved.** *Signal:* the merge fails.
  *Response:* rebase the feature branch on the new `main` and re-run the gates
  before merging — a fast-forward over an unrerun gate list is the failure this
  guards against.
- **A gate is run once at the end and hides which change broke it.** *Signal:*
  a failure with two commits in flight. *Response:* the gates run per change,
  as the §11 note says.
- **The handoff is updated to say the macOS pass is done because parity code was
  written.** It is not. *Signal:* §11 item 1 disappearing. *Response:* item 1
  survives this plan with a longer list than it started with.
