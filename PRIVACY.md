# GlowKey privacy

GlowKey is a Vietnamese input method. To do its job it sees the keys you type —
on **both** macOS and Windows. Here is exactly what it does with them, and what
it does not.

## What GlowKey does

- Transforms your keystrokes into Vietnamese text, entirely on your own machine,
  in memory.
- Keeps a short buffer of the word currently being typed, only until that word
  ends (a space, punctuation, or a focus change), then discards it.
- Keeps a **local diagnostic log** (see below). This is the part to read.

## What GlowKey never does

- **No network connections.** GlowKey opens no sockets and calls no networking
  APIs. Continuous integration fails the build if a networking library is ever
  linked — and it checks the **built binary's import table**, not a manifest, so
  the check cannot pass vacuously.
  (On macOS, like every AppKit application, it *transitively* links CloudKit and
  CoreData through AppKit; GlowKey calls neither. The honest guarantee is
  behavioural: no outbound connections, verifiable with a network monitor.)
- **No analytics, no telemetry, no accounts, no update check.**
- **Nothing you type is ever sent anywhere by GlowKey.**

## The local diagnostic log — please read this

To make typing bugs diagnosable without a live reproduction, GlowKey appends a
line for **every key it handles**. Those lines contain the text you typed.

| | macOS | Windows |
|---|---|---|
| Where | `~/Library/Logs/GlowKey/glowkey.log` | `%LOCALAPPDATA%\GlowKey\Logs\glowkey.log` |
| What | the key, the frontmost app, the decision, and the current word's raw and rendered forms | the key's character, the frontmost app, the decision, and the text emitted |
| Size | bounded at 5 MB plus one previous generation | same |

Things you should know about it:

- **It is keystroke content on disk.** Up to about 10 MB of what you have typed,
  attributed to the application you typed it in.
- It is a plain local file, and nothing GlowKey does transmits it anywhere. But
  any program running under your own user account can read it.
- **Keys typed in excluded (ignored) apps are still recorded.** The ignore list
  decides whether GlowKey *transforms* a key, not whether it *sees* one.
- **Secure and password fields:**
  - **macOS — excluded.** They never reach GlowKey at all; the system withholds
    secure input from event taps.
  - **Windows — not excluded.** A low-level keyboard hook receives no such
    exemption from the operating system, and GlowKey does not implement one.
    Assume anything you type into a password field on Windows can appear in the
    log.
- Delete it any time — macOS: menu bar → *Reveal Log in Finder*; Windows: tray →
  *Show log folder*. GlowKey recreates an empty one.

If you send a log to report a bug, **read it first**.

## What GlowKey stores

Your settings — the ignore list (and removed defaults), typing options, input
method, hotkey, macros, personal word decisions — as JSON, plus one `.bak` of the
previous version:

| macOS | Windows |
|---|---|
| `~/Library/Application Support/GlowKey/settings.json` | `%APPDATA%\GlowKey\settings.json` |

**Macro expansions you define are stored there verbatim.** On Windows that
location is the *roaming* part of your profile, which means that on a managed or
domain account it may be **copied to a server and to your other machines** by
Windows itself, as roaming profiles are designed to do. If your macros contain
anything you would not want copied off the machine, that is the thing to know.
The log does not roam; it is kept in the local part of the profile deliberately.

If GlowKey ever finds a settings file it cannot read, it moves it aside as
`settings.corrupt-<timestamp>.json` rather than deleting it, and starts on
defaults.

## Verifying this yourself

GlowKey is open source and built in the open. You can read the code, and you can
confirm the shipped binary links nothing that can open a socket:

```
# macOS
otool -L /path/to/GlowKey.app/Contents/MacOS/GlowKey | grep -i network
```

```powershell
# Windows — the import table, as CI checks it
$b = [IO.File]::ReadAllBytes(".\target\release\GlowKey.exe")
[Text.Encoding]::ASCII.GetString($b) -match 'ws2_32|wininet|winhttp|urlmon'
```

The first should print nothing; the second should be `False`.
