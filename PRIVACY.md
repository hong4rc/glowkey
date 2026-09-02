# GlowKey privacy

GlowKey is a Vietnamese input method. To do its job it sees the keys you type.
Here is exactly what it does with them, and what it does not.

## What GlowKey does

- Transforms your keystrokes into Vietnamese text, entirely on your Mac, in memory.
- Keeps a short buffer of the word currently being typed, only until that word ends
  (a space, punctuation, or a focus change), then discards it.
- Keeps a **local diagnostic log** (see below).

## What GlowKey never does

- **No network connections.** GlowKey opens no sockets and calls no networking
  APIs. It links no networking framework directly, and continuous integration
  fails the build if a direct networking link is ever added. (Like every macOS
  app that uses AppKit, it *transitively* links CloudKit and CoreData through
  AppKit — GlowKey calls neither. The honest guarantee is behavioural: no
  outbound connections, verifiable with a network monitor.)
- **No analytics, no telemetry, no accounts.**
- **Nothing you type ever leaves your Mac.**

## The local diagnostic log

To make typing bugs diagnosable without a live reproduction, GlowKey appends
each key it handles — the key, the frontmost app's bundle id, the decision, and
the current word's raw/rendered form — to
`~/Library/Logs/GlowKey/glowkey.log`. That is keystroke content, and you should
know it exists:

- It is a plain local file. It is never transmitted anywhere by anything GlowKey
  does.
- It is self-bounding (truncated past 5 MB).
- Keys typed in excluded apps in passthrough are still recorded as handled
  events; secure/password fields never reach GlowKey at all (macOS withholds
  them from event taps).
- Delete it any time (menu bar → "Reveal Log in Finder"); GlowKey recreates an
  empty one.

## What GlowKey stores

Your settings — the ignore list (and removed defaults), typing options, input
method, hotkey, macros — as JSON in
`~/Library/Application Support/GlowKey/settings.json` (plus one `.bak` of the
previous version). Macro expansions you define are stored there verbatim.

## Verifying this yourself

GlowKey is open source and built in the open. You can read the code, and you can
confirm the shipped binary links no networking framework:

```
otool -L /path/to/GlowKey.app/Contents/MacOS/GlowKey | grep -i network
```

That command should print nothing.
