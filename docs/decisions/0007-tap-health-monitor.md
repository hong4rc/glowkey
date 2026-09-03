# 0007 — Tap health: a two-second poll, because there is nothing to observe

## Status

Accepted (2026-09-03).

## Context

The Accessibility permission was checked once, at startup, and never again. If
the user turned the switch off while GlowKey was running, the tap died and
nothing in the app noticed: the process stayed alive, the menu bar kept showing
**VI**, `glowkey.log` recorded nothing, and re-granting the permission did not
help, because nothing re-entered the startup gate and nothing re-enabled the
port. The only cure was quit and relaunch, which the user had no way of knowing.

Costs nothing in CPU — there was no polling, no timer, no thread — which is why
the failure was silence rather than a hang. The status glyph asserting VI over a
dead tap is what made this a defect rather than a limitation: the app's one
indicator was lying.

A second, quieter case shared the same blindness. When the system disables a tap
on timeout or under load it delivers `kCGEventTapDisabledByTimeout` /
`ByUserInput`, and the callback re-enabled the port with no return check and no
log line — so a tap flapping under load looked exactly like a healthy one.

## Decision

Poll `CGEventTapIsEnabled` every two seconds from a `CFRunLoopTimer` on the main
run loop, and branch on the two causes, which need different remedies:

| enabled | trusted | warned already | action |
|---|---|---|---|
| yes | — | — | clear any warning, **flush**, log the recovery once |
| no | yes | no | re-enable the same port in place; give up after 30 tries |
| no | yes | **yes** | **rebuild** the tap and **flush** — trust came back after a revocation |
| no | no | — | warn after two consecutive checks |

**Every path back from a gap flushes the engine.** This was missed on the first
implementation and caught in review. It is not housekeeping: the blind model's
one invariant is *rendered == the text tail at the caret* (`docs/handoff.md` §5),
and a dead tap is the strongest possible break in it — the user's keys reach the
document **natively, unsuppressed**, while `Session` still holds the raw log and
render from before the gap. Type `hoo` (render `hô`), lose the permission
mid-word, type `ngf` (lands literally, document reads `hôngf`), re-grant: without
a flush the next letter is diffed against the stale `hô` and the emitted
backspaces delete characters the user typed themselves. The usual safety net
cannot help, either — mouse-down and caret keys flush, but those arrive *through
the tap*, and the tap is exactly what was dead.

**Refusing beats a double tap.** `create_tap` retires the previous tap first and
returns `false` rather than continuing if it cannot: attaching a second tap to
the same run loop would process every keystroke twice, and would drop the only
handles able to remove the old source, making it unrecoverable without a restart.
Refusing costs two seconds — the next tick retries.

**Giving up is a state.** A tap the system keeps disabling while the permission
is intact cannot be fixed by re-enabling. After thirty consecutive failures the
glyph stops claiming **VI**, because a lying indicator is the failure this whole
module exists to end, and the first implementation left exactly that hole in the
one branch it did not cover.

Rebuilding rather than re-enabling is not a detail: the old port was created
under a grant that no longer exists, so `CGEventTapEnable` on it does nothing.
`create_tap` is shared by startup and by recovery, so a rebuilt tap is created
exactly like the original, and it removes the previous run-loop source before
adding a new one — two live taps would process every keystroke twice, the same
failure the two app identities exist to prevent (`docs/handoff.md` §8).

The menu-bar glyph gains a third state, **⚠**, which outranks VI/EN, and the
menu gains a line naming the cause with an "Open System Settings…" item.

## Why polling, and why two seconds

**Polling because there is no alternative.** macOS exposes no public API to
observe an Accessibility grant changing — no notification, no KVO, no callback.
`AXIsProcessTrusted` answers only "right now". A tap that has been revoked also
stops delivering events entirely, including the `TapDisabled*` events, so the
callback cannot be the detector: a dead tap wakes nobody. Something has to ask.

**Two seconds because the deadline is human.** The user needs to learn the tap
died within a few seconds of trying to type, not instantly — they will discover
it by typing anyway. Against that, this is a background agent that runs all day,
so the check has to be cheap: it is one `CGEventTapIsEnabled` call, on a run loop
that already handles twenty keystrokes a second when someone is typing fast.

**Two consecutive failures before the glyph changes**, because a tap disabled
under load is usually re-enabled on the next tick, and a glyph that flickers is
worse than one that is briefly wrong.

## Consequences

- Revoking the permission is now visible within about two seconds, and
  re-granting it recovers without a relaunch.
- A flapping tap leaves a rising count in the log — the signature of a machine
  under enough load to drop taps, which was previously invisible. Logged on the
  first failure and then every thirtieth, not every tick: at two seconds apart an
  unconditional line would be some 43,000 lines a day, and the log's size cap is
  evaluated once per process, so a long-running agent would grow the file without
  bound.
- One timer wakeup every two seconds, forever. Measurable in principle,
  negligible in practice; if it ever shows up in battery terms the fix is to back
  off to five seconds once the tap has been healthy for a while.
- **Not yet verified on screen.** On some macOS versions revoking the permission
  terminates the process outright, which would make the recovery path unreachable
  (and harmless). That reproduction is the one step of this work a human has to
  do; until then the recovery branch is written but unproven.

## Alternatives rejected

- **Re-enabling the existing port on recovery.** Does nothing — the grant it was
  created under is gone.
- **Detecting from the tap callback.** A revoked tap delivers no events, so the
  callback is exactly the wrong place to look.
- **A modal alert when the tap dies.** GlowKey is an `LSUIElement` agent; a
  dialog appearing unbidden while the user is in another app is worse than a
  glyph that changed. The alert stays reserved for the startup gate, where the
  user is already waiting on the app.
