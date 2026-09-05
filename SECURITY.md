# Security

GlowKey watches every keystroke on the machine. That is what an input method
does, and it is also why the things below are worth stating plainly.

## What GlowKey has access to

- **Every key you press**, on macOS through a `CGEventTap` and on Windows
  through a `WH_KEYBOARD_LL` hook — including in applications on the ignore
  list, because the ignore list is applied *after* the key arrives.
- **The clipboard**, but only when you choose one of the clipboard tools from
  the menu.
- **Your settings file and log**, under your own user account.

On macOS, password and other secure fields never reach GlowKey: the system
withholds secure input from event taps. **Windows has no such exemption** — a
low-level keyboard hook is not excluded from password fields by the OS. See
[`PRIVACY.md`](PRIVACY.md) for what is recorded.

## What GlowKey cannot do

- **Talk to the network.** There is no update check, no telemetry, no crash
  reporting. CI enforces this on the built binary, not on a manifest: the
  release build's import table is checked for `ws2_32`, `wininet`, `winhttp`,
  `urlmon` and `wsock32`, and the macOS job makes the equivalent check. A
  networking library cannot be linked in without the build failing.
- **Ask for administrator rights.** It does not have a manifest requesting
  elevation and does not try to acquire it. It cannot type into an elevated
  window, and it reports that in the tray rather than working around it. An
  input method asking for administrator rights is a red flag; this one does not.

## Dependencies

`cargo audit` runs in CI against the committed `Cargo.lock`. Two "unmaintained"
advisories are ignored with written justifications in
[`.cargo/audit.toml`](.cargo/audit.toml); neither is a vulnerability and neither
has a fix available. Any **new** advisory fails the build.

## Reporting a vulnerability

Open a GitHub security advisory on the repository, or a normal issue if the
problem is not sensitive. Please include:

- your OS and version;
- the version and commit from **About GlowKey** (the line is selectable so it can
  be pasted);
- what you observed, and what you expected.

Do not paste log excerpts without reading them first — the log can contain text
you typed.

## Scope

GlowKey is a local desktop application with no network access and no server.
Realistic threats are a local attacker already running as your user, and a
malicious file you import (a macro table). Findings in those categories are
wanted. Reports that require an attacker to already have administrator rights on
the machine are generally out of scope, since at that point the keyboard itself
is theirs.
