---
title: "GlowKey — a personal word list that learns"
description: "Replace the all-or-nothing English-restore toggle with a per-word decision the user can make in one keystroke and then forget: an explicit override list, a window to inspect it, and a correction hotkey that fixes the word just typed and remembers the choice."
status: pending
priority: P1
effort: "2 days"
tags: [glowkey, engine, ambiguity, ux]
created: 2026-09-03
blocks: [260903-1637-unikey-phonotactics-and-restore]
---

# GlowKey — a personal word list that learns

## Overview

`docs/handoff.md` §6.3 records the English/Telex ambiguity as the one limitation
that is **inherent in principle**: the same keystrokes are legitimate Vietnamese
and legitimate English, and no blind rule can decide between them. `was` is `ứa`;
`cats` is `cát`; `car` is `cả`.

The current mitigation is a single global switch, `restore_english_words`, and
its trade-off is a paragraph long — turning it on makes `á`, `í`, `ú`, `ò`, `ỏ`,
`mã`, `sĩ`, `thú`, `cả`, `hải`, `tả`, `cát` and `sét` untypeable in their natural
key order. That is why it ships **off**, which means the default experience is
`was` → `ứa` for everyone who types any English at all.

The ambiguity is per word. The setting is global. That mismatch is the whole
problem, and it is the reason a curated list of 66 English words is doing a job
that only the person typing can do.

There is also a gesture the engine already understands and immediately forgets.
Pressing the diacritic key again rejects it (`cass` → `cas`, `hoongff` →
`hôngf`) — UniKey's own escape hatch, pinned by
`repeating_the_diacritic_key_rejects_it`. The user says "not this word" and then
has to say it again, forever, every time they type it.

## Goals

| # | Goal | Priority |
|---|------|----------|
| 1 | A per-word override that beats the global toggle in both directions | P1 |
| 2 | A window to see, edit and undo what the list holds | P1 |
| 3 | One keystroke that fixes the word just typed **and remembers it** | P1 |

## Non-goals, and why

| Out | Why |
|---|---|
| **Inferring intent from undo patterns** | The tempting version of "learns" watches for the user backspacing an auto-fix and concludes something. It is a guess about intent, it fires on ordinary typos, and an input method that silently changes its own behaviour from a misread gesture is worse than one that never learns. Every write to the list comes from an explicit act. |
| Import/export of the list | Macros have it because UniKey and EVKey have a table format to be compatible with. There is no external format for this, and no second tool to exchange with. |
| Syncing between machines | No network, permanently (`plans/260903-1745-…` standing decisions). |
| Growing `english.rs` | The curated list stays as it is. This plan makes it overridable, which is the fix; making it longer only moves the trade-off around. |
| A suggestion UI, candidate window, or inline popup | GlowKey has no composition and no marked text (`docs/decisions/0002`). A candidate window is a different application. |

## Phases

| # | Phase | Status | Depends on |
|---|-------|--------|------------|
| 1 | [Phase 1: The override list](./phase-01-override-list.md) | Pending | — |
| 2 | [Phase 2: The Personal Words window](./phase-02-personal-words-window.md) | Pending | 1 |
| 3 | [Phase 3: The correction hotkey](./phase-03-correction-hotkey.md) | Pending | 1, 2 |

**The order is deliberate and the UI comes before the learning.** Phase 3 writes
to the list automatically; Phase 2 is how the user sees what it wrote and takes
it back. Shipping the writer before the viewer would mean a file the user cannot
inspect quietly accumulating decisions on their behalf — which is precisely the
failure mode the non-goals above are written to avoid.

Phase 1 is useful on its own: a hand-editable list in `settings.json` already
fixes `was` without breaking `cát`.

## Conflict with the ASCII-render restore plan

`plans/260903-1637-unikey-phonotactics-and-restore/` Phase 2 rewrites the restore
decision inside `Session::commit` — **the same function and the same decision**
this plan hooks into. They compose logically (an override beats a rule) but they
will collide textually.

- **Do not run them concurrently.**
- **Recommendation: this plan first.** It is smaller, and it hands the user an
  escape hatch for whatever the ASCII-render restore gets wrong. That work
  changes auto-fix's behaviour on thousands of words at once; going in with a
  per-word override already available is a better order than going in without
  one. Recorded in both plans.

## Success Criteria

- [ ] `was`␣ gives `was` and `cát`␣ gives `cát`, with the global English-restore
      toggle **off**, because the list decides each one
- [ ] An override wins over auto-fix, over the English list, and over the global
      toggle — in both directions — and loses only to a macro
- [ ] The list is visible, editable and removable in the UI, and survives a restart
- [ ] One keystroke after a word swaps it and records the preference; typing the
      same word again needs no keystroke
- [ ] Nothing writes to the list except an explicit user action
- [ ] `cargo test --workspace` green, `cargo clippy --workspace --all-targets`
      silent, and the property suite in `crates/glowkey-engine/tests/properties.rs`
      still holds (it now checks the restore edit exactly, so a wrong override
      edit fails it)
- [ ] `docs/handoff.md` §6.3 rewritten: the limitation is no longer "mitigated by
      a global toggle with a wide trade-off"

## Open questions

1. **Which key for the correction hotkey?** Phase 3 proposes a fixed ⌃⇧W,
   matching ⌃⇧E's fixed per-app toggle rather than adding a second configurable
   recorder. Needs a check that it collides with nothing common.
2. **Should an override be case-sensitive?** `is_common_english` lowercases, and
   Phase 1 follows it. `Cats` at a sentence start should presumably obey the same
   override as `cats`; if that turns out wrong, the list needs a case column.

<!-- slug: glowkey-personal-word-list -->
