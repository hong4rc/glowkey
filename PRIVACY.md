# GlowKey privacy

GlowKey is a Vietnamese input method. To do its job it sees the keys you type.
Here is exactly what it does with them, and what it does not.

## What GlowKey does

- Transforms your keystrokes into Vietnamese text, entirely on your Mac, in memory.
- Keeps a short buffer of the word currently being typed, only until that word ends
  (a space, punctuation, or a focus change), then discards it.

## What GlowKey never does

- **No network connections.** GlowKey opens no sockets and calls no networking
  APIs. It links no networking framework directly, and continuous integration
  fails the build if a direct networking link is ever added. (Like every macOS
  app that uses AppKit, it *transitively* links CloudKit and CoreData through
  AppKit — GlowKey calls neither. The honest guarantee is behavioural: no
  outbound connections, verifiable with a network monitor.)
- **No logging of what you type.** Release builds write no keystroke content to any
  log or file.
- **No persistence of typed text.** Nothing you type is saved to disk.
- **No analytics, no telemetry, no accounts.**

## What GlowKey stores

Only your settings — the ignore list, placement style, and mode — in the standard
macOS preferences for the app. Never the text you type.

## Verifying this yourself

GlowKey is open source and built in the open. You can read the code, and you can
confirm the shipped binary links no networking framework:

```
otool -L /path/to/GlowKey.app/Contents/MacOS/GlowKey | grep -i network
```

That command should print nothing.
