---
phase: 2
title: "Auto-capitalize sentence start"
status: completed
priority: P2
effort: "2h"
dependencies: []
---

# Phase 2: Auto-capitalize first letter of sentence

## Overview
Unikey's "Viết hoa chữ đầu câu": when enabled, the first letter of a sentence
(document start, or after `.`/`!`/`?` + space) is capitalized automatically.

## Requirements
- Functional: opt-in toggle (default off). At a sentence start, the first typed
  letter of the next word is uppercased in the rendered output.
- Non-functional: must not fight manual capitals; off by default; test-covered.

## Architecture
- Track a lightweight "sentence-start" flag in the engine/session: true at start
  and after a sentence-ending punctuation boundary; cleared once a letter is typed.
- On the first letter of a word when the flag is set and the option is on, treat
  the raw key as uppercase for rendering.
- `Settings.auto_capitalize: bool` (default false) + Settings checkbox.

## Related Code Files
- Modify: `crates/glowkey-engine/src/config.rs`, `.../lib.rs` (flag + option)
- Modify: `app/src/tap.rs`, `app/src/prefs_window.rs` (checkbox)

## Implementation Steps
1. Add `auto_capitalize` setting + Session plumbing.
2. Sentence-start tracking at boundaries (space after `.`/`!`/`?`, and fresh start).
3. Apply capitalization to the first letter when enabled.
4. Settings checkbox under Typing.
5. Tests: `xin chaof. hello` → `Xin chào. Hello` when on; unchanged when off.

## Success Criteria
- [ ] Sentence starts capitalize when on; no effect when off; manual caps preserved.
- [ ] Tests green, clippy clean.

## Risk Assessment
Medium. Sentence detection is heuristic (abbreviations like "e.g." over-trigger).
Keep it simple (only `.`/`!`/`?` + space). Signal it broke: unwanted capitals mid-
line → default off, and document the heuristic limit.
