# 0008 — Nothing may block inside the tap callback

## Status

Accepted (2026-09-04).

## Context

The user reported that toggling GlowKey off froze the whole Mac — not GlowKey,
the Mac. Their log made the mechanism unambiguous:

```
#3   +15.266s  FRONTMOST -> com.apple.systempreferences
#6   +32.510s  TAP disabled by timeout — re-enabled (#1 this run)
#9   +42.794s  TAP disabled by timeout — re-enabled (#3 this run)
#10  +42.974s  FRONTMOST -> com.apple.loginwindow      ← the permission sheet
```

Five timeouts in one run, bracketing exactly the moment they were revoking the
Accessibility grant.

**`kCGEventTapDisabledByTimeout` is not a warning about GlowKey being slow. It is
the record of a system-wide stall.** A `CGEventTap` at
`kCGHIDEventTap`/`kCGSessionEventTap` sits in the delivery path of every key the
machine processes, and delivery is synchronous: while our callback has not
returned, **every keystroke in the system is waiting on us**. macOS eventually
gives up and disables the tap, which is the log line — but the freeze the user
felt happened before that, in the blocking call itself.

Two calls were blocking on that thread, and only one of them was new:

1. **Per keystroke.** `refresh_frontmost_at_word_start` asked NSWorkspace which
   application was in front, at every word start, *inside the tap callback*. That
   is a synchronous round-trip to the window server. Pre-existing, and in the
   hottest path there is.
2. **Every two seconds.** The health monitor from `0007` calls
   `CGEventTapIsEnabled` — another window-server round-trip — from a
   `CFRunLoopTimer` on the **same run loop** as the tap callback. A slow answer
   there does not merely delay the timer; it delays the callback behind it.

Neither is slow when the window server is idle. Both block for as long as it
takes when it is not — and an authentication sheet, `loginwindow`, or the
Accessibility pane being toggled is exactly when it is not. That is why the bug
presented as "revoking the permission freezes my Mac": revocation is not the
cause, it is the load that exposes the cause.

The measured emit latency agrees. Median **58 µs**, maximum **22.4 ms** — against
an engine pinned at 2 µs per keystroke (`crates/glowkey-engine/tests/latency.rs`).
Three orders of magnitude between the median and the tail is the signature of a
call that waits on somebody else, not of work being done.

## Decision

**The keystroke path makes zero window-server calls.** Everything it needs is
either already in memory or arrives by notification.

**The frontmost application comes from a notification, not a question.**
`NSWorkspaceDidActivateApplicationNotification` fires on every switch and
`menu_bar` already observes it, calling `set_frontmost_app`. The tap keeps one
bootstrap query — GlowKey can start while an application is already frontmost, so
no activation notification is coming for that one — guarded to fire **once**, on
the first keystroke of the run, and never again.

**A keystroke arriving is proof the tap is alive.** The health check now returns
immediately if a key was handled within the last three seconds
(`HEALTH_SKIP_AFTER_KEYSTROKE`). Asking the window server to confirm what just
walked through the door is the definition of a wasted round-trip, and this one is
charged to the thread that must never block. `TapState::last_key_at` is a
`Cell<Option<Instant>>` written in `tap_dispatch`; the cost in the hot path is one
store.

**The belt-and-braces frontmost check moved to the idle timer.** It still exists
— a stale frontmost application means Vietnamese firing in a terminal, which is
the failure the per-app ignore list exists to prevent — but it now runs from
`check_tap_health` on a tick where nobody is typing
(`TapState::refresh_frontmost_if_idle`). Same safety net, off the hot path.

The rule generalises past this fix, which is why it is a decision record and not
a bug fix note: **anything that can wait on another process is forbidden inside
`tap_dispatch` and everything it calls.** Window-server calls, Accessibility
calls, file I/O, and any lock that a non-tap thread can hold. The one deliberate
exception is the Chromium omnibox guard (`0003`), which is capped at 50 ms and
fires only on transforming keystrokes in Chromium applications — it was accepted
knowing this cost, and it is now the only such call left in the path.

## Consequences

- Revoking Accessibility, opening an authentication sheet, or hammering the
  window server no longer stalls typing system-wide.
- The health monitor is quiet while the user is typing, which is also when it was
  least useful: the check answers a question the keystrokes already answered.
  During real idle it runs exactly as `0007` describes.
- Frontmost tracking now has two sources — the notification for every switch, one
  bootstrap query at startup, and the idle timer as reconciliation. Three paths
  into one piece of state is more machinery than one question per keystroke, and
  that is the trade: the question was correct and unaffordable.
- The remaining exposure is the omnibox guard's accessibility round-trip. It is
  bounded and opt-out (it only fires in Chromium), but it is the same class of
  hazard, and if timeouts are ever seen again with the frontmost query gone, that
  is where to look next.
- **Needs live verification.** The reproduction is the user's own: toggle the
  Accessibility grant off while typing and confirm nothing wedges.
  `docs/manual-verification.md` §9 carries it.

## Alternatives rejected

- **Making the frontmost query asynchronous.** The answer is needed for *this*
  keystroke — whether to transform it at all. An answer that arrives later is a
  different feature (and a wrong one: it would transform the first keys typed in
  a terminal).
- **Raising the tap timeout.** Not configurable, and it would trade a freeze the
  system recovers from for a longer one it does not.
- **Dropping the health poll.** That is `0007`'s whole subject: a revoked tap
  delivers no events, so nothing else can notice it died. Skipping the poll while
  typing keeps the detection and removes the cost, which is the only combination
  worth having.
- **Moving the health poll to its own thread.** It would stop delaying the
  callback, but `CGEventTapIsEnabled` and the tap port are not documented as
  thread-safe, and the timer is not the only offender — the per-keystroke query
  was the bigger one and lives on the callback thread by definition. Fixing the
  path is better than moving one of its two problems.
