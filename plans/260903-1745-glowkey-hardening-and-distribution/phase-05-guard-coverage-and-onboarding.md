---
phase: 5
title: "Guard coverage and first-run onboarding"
status: pending
priority: P2
effort: "1d"
dependencies: []
---

# Phase 5: Guard coverage and first-run onboarding

## Overview

Two loose ends from `docs/handoff.md` §6/§11, plus the one piece of UX a
background agent with no Dock icon genuinely needs.

The omnibox guard is deliberately narrow: seven Chromium bundle-id prefixes,
and nothing else. §11.2 proposes extending it to Safari, whose smart search
field has the same inline-autocomplete pattern — but proposes it as a guess.
This phase measures first and builds only if the measurement says to.

Separately: GlowKey grants itself Accessibility, puts a glyph in the menu bar,
and then says nothing. A new user does not know that ⌃⇧Space toggles Vietnamese,
that ⌃⇧E excludes the current app, or that their terminal is already excluded on
purpose. That is the app's whole value proposition, undiscoverable.

## Requirements

- Functional: a recorded answer to "does Safari's address bar exhibit the
  trailing-selection pattern?", backed by an AX probe, not by reasoning.
- Functional: if yes, the guard covers Safari, with the same narrowness — the
  check must not fire in ordinary Safari web content.
- Functional: a one-time welcome after the first successful permission grant,
  naming the two hotkeys, the per-app ignore list, and where Settings is.
- Functional: a written manual verification checklist, so the GUI work that
  cannot be tested headless (`docs/handoff.md` §6.4) has a repeatable script
  instead of an ad-hoc look.
- Non-functional: the welcome shows once, ever, and is dismissible forever. An
  agent app that nags is worse than one that says nothing.

## Architecture

**The Safari probe.** `app/src/ax.rs` already answers exactly the question that
needs asking: "does the focused element have a non-empty text selection?". The
probe is a temporary log line, not new machinery — widen
`CHROMIUM_BUNDLE_PREFIXES` to include `com.apple.Safari` behind
`GLOWKEY_DEBUG`, type a transforming word into the address bar, and read the
log. Three outcomes:

| Probe result | Action |
|---|---|
| `AXTextField` + non-empty `AXSelectedText`, same as Chrome | Extend the guard; rename the constant to reflect that it is no longer Chromium-only |
| Selection empty, or the element is not an `AXTextField` | Record the negative result in `docs/handoff.md` §6.1 and write no code |
| Typing is already correct in the address bar | Same — record it, ship nothing |

The third outcome is real and likely: Safari's autocomplete timing differs from
Chrome's, and the bug may simply not occur.

**Welcome.** Confirmed at validation: a one-time `NSAlert`, reopenable from the
menu — not a first-run Settings window, and not dropped. A `welcome_shown: bool`
in `Settings` (`crates/glowkey-engine/src/config.rs`,
which is already tolerant of missing keys, so old settings files default to
false and see it once). Shown from `tap::run` after `wait_for_accessibility`
returns, as an `NSAlert` — the same mechanism as the permission gate, including
the `NSAlert::layout()`-before-`window()` ordering that §6.5 records as
load-bearing. One button, "Got it". Reopenable from the menu ("Quick Guide"),
so dismissing it is not destructive.

**Verification checklist.** `docs/manual-verification.md`: the ordered list of
things only a human can confirm — every Settings control, the HUD variants
including "VI ⚠", hotkey recording and its cancel paths, the menu, both windows
reopening after close, the omnibox in each browser. Written as steps with
expected results, so it can be run in ten minutes after any GUI change.

## Related Code Files

- Modify: `app/src/tap.rs` — the browser prefix list; the welcome call site.
  (Note: this collides with Phase 4's rewrite of `tap.rs`, and with Phase 6,
  which adds the health timer to the same file. Land them one at a time — never
  concurrently.)
- Modify: `app/src/ax.rs` — only if the probe says yes.
- Create: `app/src/welcome.rs` — the alert, modelled on `about_window.rs`.
- Modify: `crates/glowkey-engine/src/config.rs` — `welcome_shown`.
- Modify: `app/src/menu_bar.rs` — "Quick Guide" item.
- Modify: `app/src/strings.rs` — both languages for every new string; the
  Vietnamese interface option means English-only strings are a regression.
- Create: `docs/manual-verification.md`.
- Modify: `docs/handoff.md` §6.1, §6.4.

## Implementation Steps

1. Run the Safari probe. Record the log lines verbatim in this phase file under
   an "Outcome" heading, whichever way it goes.
2. Extend the guard only if outcome 1. If extended, verify the negative case
   too: type into a Safari *web page* text field and confirm the guard does not
   fire (the log's "trailing selection detected" line must be absent).
3. Add `welcome_shown` to `Settings` with a unit test that an old settings JSON
   without the key deserializes to `false`.
4. Write `welcome.rs`; wire it after the permission gate; add the menu item.
5. Both strings in `strings::t(english, vietnamese)` for every new string.
6. Write `docs/manual-verification.md` and then **run it once, end to end**,
   fixing whatever it turns up. A checklist that has never been executed is
   fiction.
7. Update `docs/handoff.md`: §6.1 gains the Safari result, §6.4's list of
   unverified GUI items is either cleared or reduced to what remains.

## Success Criteria

- [ ] The Safari question has a recorded, evidence-backed answer
- [ ] If the guard was extended: it fires in Safari's address bar and provably
      not in Safari web content
- [ ] A fresh settings file shows the welcome exactly once; the menu item
      reopens it; an old settings file also sees it once
- [ ] Every new string exists in English and Vietnamese
- [ ] `docs/manual-verification.md` exists and has been executed once
- [ ] `docs/handoff.md` §6 no longer lists an item this phase resolved

## Risk Assessment

- **The AX guard is a race, and widening it widens the race.** §6.1 already
  records that the AX read can return a stale answer, converting a deterministic
  bug into a rare timing one. Adding Safari adds a second app where a stale read
  can misfire — a forward-delete in a field with no selection is a no-op with the
  caret at the end, but the caret is not always at the end. *Signal:* a character
  disappears after the caret while typing in Safari. *Response:* revert the
  extension. Chromium-only is a perfectly good resting state, and the guard's
  narrowness is a feature.
- **AX queries force Safari to keep its accessibility tree on**, as they already
  do for Chromium. A real cost, paid only if the probe justifies it.
- **The welcome alert appears at the wrong moment.** It fires right after the
  permission grant, when the user is already in a modal frame of mind; getting
  the ordering wrong (before the gate, or twice) is the failure mode. *Signal:*
  two dialogs on screen at once — the exact bug §6.5 records from the permission
  prompt. *Response:* show it only from the path where the gate returned
  successfully, never from the gate itself.
- **File collision with Phase 4.** Stated above and in Phase 4. Sequence them.
