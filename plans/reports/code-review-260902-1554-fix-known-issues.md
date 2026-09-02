# Code review of 3763ad3 + fix disposition (260902)

Reviewer: code-reviewer subagent (adversarial, commit mode). Verdict:
DONE_WITH_CONCERNS — 1 Critical, 8 Important, 13 Minor. All dispositions below
implemented in the follow-up commit unless marked otherwise. Post-fix: 70 tests
green, clippy clean, test-run log-write verified gone (log mtime unchanged).

## Critical

1. Hotkey recording swallowed ALL key-downs system-wide (⌘Q/⌘Tab included), no
   timeout/cancel → possible full keyboard lockout. **FIXED**: recorder now
   intercepts only ⌃/⌥ combos; plain typing + all ⌘ combos pass through; Esc,
   any mouse click (tap flush path), or an app switch (set_frontmost_app vs own
   bundle id) cancels; UI is a "Custom…" segment that snaps back on cancel.

## Important — fixed

5. Double ⌃⇧E destroyed a deliberate tombstone (add() cleared it). **FIXED**:
   add() keeps tombstones; toggle is an involution again. Test added.
7. `setSelectedSegment(-1)` on SelectOne control (no-op or NSRangeException on
   the feature's happy path). **FIXED**: 5th "Custom…" segment; -1 never used.
8. No collision validation on recorded hotkeys. **FIXED (minimum)**: ⌃⇧E
   rejected while recording. Emacs-style ⌃-letter combos still allowed — user
   choice, not validated.
3. Guard not scoped to omnibox; ⌦ could eat text in odd AX surfaces. **FIXED**:
   role gate — guard fires only when focused element is AXTextField.
4. 2 sync AX IPC calls per transforming keystroke in Chromium. **PARTIAL**:
   system element + process-global 50ms timeout now OnceLock'd (setup cost gone);
   the 2 reads remain (typ. sub-ms) — accepted, documented in handoff §6.1
   including the Chromium-AX-tree side effect.
9. `Custom` hotkey makes settings unreadable by older builds; from_json falls
   back to FULL defaults → next save wipes file. **MITIGATED**: settings.json.bak
   written before every save. Field-level tolerant parse: deferred (rare
   downgrade path; bak covers recovery).
2. Guard reads AX state racing Chrome's async pipeline → probabilistic, not
   deterministic. **ACCEPTED + DOCUMENTED**: handoff §6.1 now says "mitigation,
   best-effort"; residual failure mode described.
6. Wordlist collisions far wider than the caption claimed (á→as, í→is, mã→max,
   sĩ→six, cả→car, hải→hair…). **DOCUMENTED, list kept**: option is opt-in +
   default OFF; caption and handoff now state the real cost. Pruning top-freq
   English words would gut the feature's purpose. Rejected prune; recorded the
   Unikey-style "repeat tone key to reject" as the future escape hatch.

## Minor — fixed

10. Borrow-failure ended recording while silently keeping old hotkey → now stays
    armed on borrow failure.
11. cargo test wrote user's real log → log::log gated under cfg!(test); verified.
14. Unconditional alloc at word boundary → short-circuit on both flags off.
15. AX setup per keystroke → OnceLock.
16. Silent AX failure → first failure per run logged ("AX guard unavailable").
18. Dead `ExclusionList::toggle()` (semantics diverged from Session toggle) →
    removed with its test.
19. TERMINAL_EXCLUSIONS gaps → added dev.warp.Warp-Preview, co.zeit.hyper (both
    lists).
20. Fixed 460×420 window vs ~70pt new content → 460×540; recording prompt
    shortened.
21. Pointless disk write on session-only ⌃⇧E → save skipped.
22. Latent RefCell panic in hotkey_recording_done → try_borrow.

## Minor — declined/deferred (rationale)

12. KeyUp not consumed while recording — pre-existing pattern for ALL suppressed
    keys (tap mask is KeyDown-only); not new; no observed harm.
13. key_char via UCKeyTranslate — cosmetic only (matching is by keycode); Space
    special-cased by keycode; rest deferred.
17. Excluded-Apps window shows session-suspended terminal as excluded — truthful
    for persisted state; cosmetic annotation deferred.
19b. VSCode/Xcode integrated terminals mangled after permanent ⌃⇧E un-exclude —
    deliberate: editors are not terminals; user may want VN in editors.

## Verified non-issues (per reviewer, do not "fix")

- RefCell borrow in `if *flag.borrow() { … borrow_mut() }` — temporary drops at
  the `if` condition; no panic.
- ax.rs CF memory management correct (+1 returns released once on every path).
- Timeout on system-wide element IS inherited by copied elements (process
  global).
- decide() writes neither settings nor (now) log; snapshot/serde forward-reads
  fine.

## Unresolved questions

1. Does Chrome's omnibox actually expose non-empty AXSelectedText for inline
   autocomplete (and AXTextField role)? Unverifiable headless — needs the user
   in front of Chrome; log lines will show it.
2. Wordlist trade-off breadth: acceptable as opt-in, or prune 2-letter entries
   (as/is/us/of/or)? Currently kept; user call.
