# 0009 — Windows uses a low-level keyboard hook, not TSF

## Status

Accepted (2026-09-04).

## Context

Windows offers two ways to add an input method, and they are not two
implementations of one idea. They are two different models of what typing is.

**The Text Services Framework** is the supported, modern, Microsoft-blessed one.
A TSF text service is an input source the user switches to in the language bar.
It owns a *composition*: a span of provisional text the application knows is
provisional, renders with an underline, and commits when the service says so. The
application participates. It can be asked where the caret is, what the surrounding
text says, what is selected.

**A `WH_KEYBOARD_LL` hook** is the low-level one. It sits in the delivery path of
every key on the machine, sees each one after the layout has mapped it, and can
swallow it. There is no composition, no application cooperation, and no way to ask
anything about the document. Paired with `SendInput` it gives exactly
intercept / suppress / inject.

The obvious reading is that TSF is the right answer and the hook is the
expedient one. That reading is wrong for this codebase, and the reason is worth
writing down, because the argument for TSF will be made again by someone who has
not read the rest of `docs/decisions/`.

## The thing that decides it

**GlowKey has no composition, and that is not an oversight — it is the design the
entire delivery layer was debugged into.**

GlowKey is a *blind* input method. It never sees the document. Its one invariant
is that what the engine believes it rendered is the text tail at the caret, and
nothing verifies that. Every hard-won behaviour in this repository is a
consequence of working without a composition:

- `backspace_visible_char` exists because a Backspace has to land on *what the
  screen will show*, not on what a keystroke history says, and there is no marked
  text to consult.
- The five-case Backspace ladder in `glowkey-input` is five separately-fixed
  bugs, in the order they were argued into, all of them about staying in step
  with a document we cannot read.
- Full suppression — swallowing even a plain letter and re-emitting it — exists
  because mixing a natively-typed character with a synthesized edit raced in
  multiprocess applications and produced `hoongf` → `hoồng`. One ordered queue
  removed the race by construction.
- Flushing on every caret move, every focus change, every mouse click exists
  because those are the events after which the blind model's assumption is no
  longer true.

A TSF port would keep `glowkey-engine` — the Vietnamese transformation, which is
platform-free and already correct — and **discard every one of those**. Not
because they would be wrong under TSF, but because they would be *unnecessary*
under TSF, which is worse: it means the delivery half is rewritten from zero
against a model this project has no experience of, while the existing model's
accumulated corrections sit unused. The port would compile early and be wrong in
new ways for a long time.

The low-level hook, by contrast, is the same shape as the `CGEventTap` the macOS
side already runs. `KBDLLHOOKSTRUCT` → `KeyEvent` → `decide` → `SendInput` is
`CGEvent` → `KeyEvent` → `decide` → `CGEventPost` with different nouns. Phase 1
of the port lifted `decide` into `glowkey-input` precisely so both could run it,
and they do — the same copy, tested on Linux in CI, with no `cfg(target_os)` in
it.

## Decision

Windows uses `SetWindowsHookEx(WH_KEYBOARD_LL)` for interception and `SendInput`
with `KEYEVENTF_UNICODE` for injection.

Self-identification is a magic `dwExtraInfo` value (`0x47_4C_4F_57`, "GLOW"),
checked in the callback's first statement. This is the analogue of the tagged
`CGEventSource` on macOS and it is not optional: without it the hook reprocesses
its own injection, each pass generating more input than the last.

## The trade-off, stated rather than buried

| | Low-level hook | TSF |
|---|---|---|
| Matches today's model | **Yes** — intercept, suppress, inject | No — it is a composition model |
| Layout-agnostic wrapping | Yes | No — an input source the user switches to |
| Elevated windows | **No** (UIPI) | Yes |
| Store/UWP apps | Partial | Yes |
| Blocking risk | Real — `LowLevelHooksTimeout` | Lower |
| Reuses the decision ladder | **Entirely** | Discards the delivery half |

The two costs are real and are accepted deliberately.

### Cost 1: elevated windows

User Interface Privilege Isolation forbids a process at one integrity level from
sending input to a window owned by a higher one. A non-elevated GlowKey typing
into Task Manager, regedit or an elevated terminal does nothing at all —
`SendInput` returns a short count and the keystroke never arrives. There is no
error the user sees.

**GlowKey will not request elevation to work around this.** An input method
asking for administrator rights is a red flag, and correctly so: it observes every
keystroke on the machine, and a user has no way to verify what it does with them
beyond the source and the absence of a network dependency. Running it elevated
would also mean every keystroke in every elevated window passes through a process
the user has granted maximum trust — a considerably worse position than not
typing Vietnamese into Task Manager.

The obligation this creates is honesty, not capability. `docs/decisions/0007`
established that an indicator claiming to work over a dead tap is a defect rather
than a limitation. UIPI is a second way to be silently dead, with no macOS
analogue, so it needs the same treatment: `platform/windows/elevation.rs` detects
it by comparing integrity levels on the foreground-change notification, and the
tray shows it. A user who cannot type in Task Manager must be told why, in the
application, without reading this file.

### Cost 2: the callback can lose the hook

`docs/decisions/0008` was written from an incident where a blocking call inside
the macOS tap callback froze an entire machine. Windows has the identical failure
shape with a shorter fuse and less warning: a callback slower than
`LowLevelHooksTimeout` (100 ms by default) causes the system to remove the hook.
Not disable-and-report, as macOS does with `kCGEventTapDisabledByTimeout` — remove,
with no event, no error and no second chance.

So `0008`'s rule is carried over verbatim rather than rediscovered. The callback
resolves the foreground application from a `SetWinEventHook` notification rather
than querying it, queues settings writes through `Effects` rather than performing
them, and takes no lock a non-hook thread holds. Timing instrumentation matching
the macOS `EMIT took=` line records the worst case, because the number that
matters is the maximum: an average hides the single slow call that loses the hook.

## Consequences

- The Windows and macOS backends stay structurally the same, so a fix to the
  decision ladder is a fix on both platforms rather than two fixes.
- `glowkey-input` gains a second real consumer, which is what stops it drifting
  back into being macOS-shaped.
- Elevated windows, and any Store application that blocks injected input, are
  documented limitations that ship with the artifact rather than surprises.
- If TSF is revisited, it should be as a *second* backend behind the same
  `Decision` interface, not as a replacement — and only after Phase 6 has
  established what the hook actually does in real applications, so there is
  something to compare against.

## What would change this

A measurement, not an argument. If Phase 6 finds that injection ordering breaks
in Chrome or Electron in a way full suppression does not fix — the failure that
would mean the blind model itself does not survive on Windows — then the
composition model stops being an alternative and becomes the requirement. That
finding would need its own record and its own evidence. Nothing short of it
reopens this.
