---
phase: 8
title: "Linux: choose the input stack"
status: pending
priority: P2
effort: "2d"
dependencies: [6]
---

# Phase 8: Linux — choose the input stack

## Overview

Decide, with evidence, how GlowKey works on Linux — and where it honestly cannot.
This is a **design and decision phase**: it produces a decision record and a
capability-detection design, not a backend. Built after Windows is verified, per the
owner's sequencing.

## Requirements

- Functional: a written decision covering X11, Wayland, and the major compositors.
- Functional: a capability-detection design that fails loudly rather than pretending.
- Non-functional: no Linux code that would need rewriting once the decision lands.

## Architecture

### The problem, stated honestly

**Wayland structurally forbids what GlowKey does.** The blind model needs to
intercept every key globally and inject synthetic input into whatever is focused.
Wayland's security model exists specifically to prevent one client doing that to
another. There is no portable equivalent of `CGEventTap` or `WH_KEYBOARD_LL`.

The realistic options, none of them universal:

| Approach | Works on | Cost / catch |
|---|---|---|
| X11 `XRecord` + `XTEST` | X11, and XWayland clients only | The closest analogue to today's model. Dying platform, but still the default on many desktops. |
| `libei` / XDG RemoteDesktop portal | Modern Wayland compositors | Requires a portal permission grant; support is uneven and evolving. |
| `evdev` + `uinput` | Any Linux, including Wayland | Below the display server: needs device access (a udev rule or group membership), and **cannot see which application is focused** — which breaks the per-app ignore list, GlowKey's headline feature. |
| IBus / Fcitx5 engine | Wherever the framework is used | The *sanctioned* path and how UniKey-family IMEs actually ship on Linux. But it is a composition model, so it is Linux's TSF — same trade-off as Phase 4, same reason to be wary. |

### The likely conclusion

A **two-backend** design, with explicit detection:

- **X11 present** → `XRecord` + `XTEST`, preserving the blind model exactly, with
  `_NET_ACTIVE_WINDOW` + `WM_CLASS` for application identity.
- **Wayland** → refuse to run the blind backend, and either ship an IBus/Fcitx5
  engine (a genuinely different delivery path reusing the same engine) or tell the
  user plainly that their session is unsupported.

The thing this phase must *prevent* is a backend that half-works on Wayland and
corrupts text, which is worse than not running.

`evdev`+`uinput` deserves a real look but probably loses on the ignore list alone:
an input method that cannot tell a terminal from a text field cannot protect the
terminal, and that is the feature GlowKey exists for.

## Related Code Files

- Create: `docs/decisions/0010-linux-input-stack.md`
- Create: `app/src/platform/linux/capability.rs` (design + detection only)
- Modify: `plans/.../phase-09`, `phase-10` — refine once the decision lands

## Implementation Steps

1. Enumerate target environments: GNOME/Wayland, GNOME/X11, KDE/Wayland, KDE/X11,
   Sway, XFCE. Record what each permits.
2. Test the detection path: `XDG_SESSION_TYPE`, `WAYLAND_DISPLAY`, `DISPLAY`, and
   whether XWayland is available.
3. Prototype `XRecord`+`XTEST` far enough to confirm interception and injection work
   with the ordering the model needs — a spike, not a backend.
4. Assess IBus/Fcitx5 as the Wayland answer, including how much of `glowkey-input`
   survives a composition-based delivery path.
5. Write `decisions/0010` with the choice, the rejected options, and the environments
   declared unsupported.
6. Design the honest-failure path: what the tray says, what the log says, what the
   user is told.

## Success Criteria

- [ ] `decisions/0010` written with evidence per environment, not assumptions
- [ ] Detection logic designed and unit-testable from environment variables
- [ ] The unsupported set is named explicitly
- [ ] A spike proves `XRecord`+`XTEST` can intercept and inject in the right order
- [ ] Phases 9 and 10 rewritten against the actual decision

## Risk Assessment

**The temptation is to ship something that appears to work on Wayland.** Under
XWayland a hook may partly function, covering some windows and not others — which
in a blind model means editing text it cannot see. *Signal:* "it works for me on
GNOME". *Response:* partial interception is the worst possible state; detect and
refuse.

**IBus/Fcitx5 is a second delivery model to maintain**, with the same
composition-versus-diff tension as TSF. *Signal:* the shared layer starts growing
composition concepts. *Response:* if it goes that way, it is a separate delivery
adapter over the same engine, and `glowkey-input` must not learn about composition.

**This phase can expand without limit.** *Signal:* prototyping turns into building.
*Response:* the deliverable is a decision record and a spike. Phase 9 builds.
