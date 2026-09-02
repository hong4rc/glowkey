# 0004 — Terminal exclusions: tombstoned defaults, session-only hotkey un-exclusion

## Status

Accepted (2026-09-02).

## Context

Synthetic backspaces cannot delete inside a PTY (the shell owns line editing),
so Vietnamese in a terminal always mangles (`work`→`ưởk` garbage). Terminals
ship in `DEFAULT_EXCLUSIONS`, but two failure modes kept recurring: (a) the user
accidentally un-excluded Ghostty with ⌃⇧E and the removal persisted; (b) a
default added in a new release never reached settings files written before it.

## Decision

Two mechanisms in `ExclusionList` (engine, `exclusion.rs`):

1. **Tombstones.** Settings persist `removed_default_exclusions`. The effective
   list at load is `saved ∪ (DEFAULT_EXCLUSIONS − tombstones)`: new defaults
   self-heal into old files, deliberate removals stay removed. `add()` keeps an
   existing tombstone (an explicit entry wins over it anyway), so an accidental
   remove/add pair cannot destroy a recorded removal.
2. **Session-only hotkey un-exclusion for terminals.** ⌃⇧E on a bundle in
   `TERMINAL_EXCLUSIONS` suspends the exclusion in memory only (HUD "VI ⚠");
   the persisted list keeps it, so a restart re-excludes. Permanent removal of a
   terminal requires the Excluded Apps window (a deliberate act).

## Consequences

- An accidental ⌃⇧E in a terminal is now recoverable by restart and loudly
  flagged; it can never silently disarm the protection.
- A user who *wants* a terminal permanently un-excluded must use the editor;
  the hotkey alone can never achieve it (accepted asymmetry).
- Editors with integrated terminals (VSCode, Xcode) are defaults but not
  "terminals": ⌃⇧E removes them permanently — a user may legitimately want
  Vietnamese in an editor.
- The Excluded Apps window lists a session-suspended terminal as excluded
  (true for the persisted state) even while it is live-enabled; cosmetic
  annotation deferred.
