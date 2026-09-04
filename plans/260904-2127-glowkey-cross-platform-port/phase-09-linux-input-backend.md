---
phase: 9
title: "Linux input backend"
status: pending
priority: P3
effort: "4d"
dependencies: [8]
---

# Phase 9: Linux input backend

## Overview

Build the backend Phase 8 chose. Written against the expected outcome — X11 via
`XRecord`/`XTEST`, with Wayland detected and refused — and **to be rewritten if
Phase 8 decides otherwise**. It is deliberately not detailed beyond that, because
detailing an unmade decision is how plans become fiction.

## Requirements

- Functional: on a supported session, Vietnamese typing behaves exactly as on macOS
  and Windows.
- Functional: on an unsupported session, GlowKey refuses clearly and does nothing.
- Non-functional: `glowkey-engine` and `glowkey-input` unchanged by this phase.

## Architecture

```text
app/src/platform/linux/
  mod.rs         backend selection from capability detection
  capability.rs  session detection (from Phase 8)
  x11/
    hook.rs      XRecord interception
    inject.rs    XTEST injection, UTF-16 edits → keysyms/strings
    window.rs    _NET_ACTIVE_WINDOW + WM_CLASS → AppId
  unsupported.rs the honest-refusal path
```

Application identity on X11 is `WM_CLASS`, which is the closest thing Linux has to a
bundle identifier and is stable enough for an ignore list. The default exclusion
table needs its Linux values: gnome-terminal, konsole, xterm, alacritty, kitty,
wezterm, tilix; code, jetbrains-*, sublime_text, emacs, gvim.

## Related Code Files

- Create: everything under `app/src/platform/linux/`
- Modify: `crates/glowkey-engine/src/exclusion_defaults/linux.rs`
- Modify: `app/src/platform/mod.rs`

## Implementation Steps

1. Capability detection first; the refusal path before the working path, so an
   unsupported session can never fall through into a half-working one.
2. `XRecord` interception, mapped through the Phase 1 adapter shape.
3. `XTEST` injection with the ordering the full-suppression model requires.
4. `WM_CLASS` application identity and the Linux exclusion table.
5. Verify against the same Tier 1/2/3 protocol as Phase 6, adapted to Linux
   applications: GNOME Text Editor, Firefox, Chrome, a terminal, VS Code.

## Success Criteria

- [ ] Tier 1 of the verification protocol green on X11
- [ ] Wayland sessions refuse with a clear message and no partial interception
- [ ] Terminals excluded by default and staying excluded
- [ ] `cargo test` green on Linux in CI
- [ ] A recorded verification report, as Phase 6 produces for Windows

## Risk Assessment

**This phase is written against a decision not yet made.** *Signal:* Phase 8 chooses
IBus/Fcitx5 or rules X11 out. *Response:* rewrite this phase then. That is expected,
not a failure — which is why it is short.

**`XTEST` injection ordering versus real input is unproven**, the same open question
Windows has. *Signal:* transposition under load. *Response:* same as Phase 6 —
measure before fixing.

**X11 is in decline.** *Signal:* target desktops default to Wayland. *Response:*
that is precisely why Phase 8 exists before this one, and why "unsupported" must be
an acceptable, well-communicated outcome.
