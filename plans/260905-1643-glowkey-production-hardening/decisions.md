# Decisions

**Answered 2026-09-05** — 1 (disable the guard), 2 (redact by default,
opt-in verbose), 3 (stay developer-audience and say so). The rest remain open.

Nine forks that an engineer must not settle alone. Each has a recommended
default and what would flip it. Arbiter IDs in brackets.

---

### 1. The Windows Chromium guard, right now [A1]

`inject.rs:220` fires a `VK_DELETE` before **any** backspace-bearing edit in
**any** Chromium app — Chrome, Edge, Electron apps, Slack, VS Code. macOS first
asks accessibility whether a selection actually exists; Windows asks nothing.

- **Keep it:** the omnibox stays correct; typing Vietnamese mid-paragraph in
  Gmail or Facebook in Chrome silently eats the character to the right of the
  caret.
- **Disable it:** mid-paragraph typing is safe again; the omnibox regresses to
  the visible `hoongf` → `hoồng` bug.

**DECIDED: disable it** until focus-cached detection exists. A visible
mis-render in one field is recoverable; silent deletion in a document is not,
and the user cannot even tell it happened.

*Flips it:* the 30-second Gmail mid-paragraph test shows no deletion in practice.
That test is item 1 of the hardware list and should be run before deciding.

---

### 2. Is full-text logging load-bearing for your debugging? [A2]

Every key you type is written to disk with its character, and on macOS with the
whole composing word (`raw=`/`rendered=`). No toggle exists; the file has default
permissions; `PRIVACY.md` describes only macOS. The handoff's §7 workflow
("diagnose from the log first") is built on those exact fields.

- **DECIDED:** redact by default (decision, counts, timings — no characters),
  with an opt-in "diagnostic typing log" that auto-expires.
- *Flips it:* you genuinely cannot diagnose a typing bug without `raw=`/
  `rendered=` — then keep verbose, but still off by default.

Either way the docs fix and the "Delete log" menu item are unconditional.

---

### 3. $99/yr Apple Developer Program [A4]

Every GUI distribution route for macOS now requires notarization. Homebrew
closed the unsigned-cask route on 2026-09-01 (verified:
Homebrew/brew#20755). Without it, users get the "damaged" Gatekeeper path plus a
fresh Accessibility grant on **every** update, because ad-hoc signing keys TCC to
the cdhash.

- **DECIDED: no.** GlowKey stays a build-it-yourself project for a technical
  audience, and the README says so plainly rather than implying a smooth
  install. Phase 6.1's notarization work is not scheduled.

---

### 4. Windows code signing [A5]

Azure Artifact Signing is $9.99/mo but **individuals are eligible in US/CA
only**; Vietnam is on neither the individual nor the organisation list
(Microsoft Learn, verified). An OV certificate is roughly $400–900/yr.

- **Recommended: defer.** Windows is self-declared unverified; do not spend on
  signing before the port earns distribution.
- *Flips it:* you have an eligible entity to sign under.

---

### 5. Windows Settings: live-apply or an Apply button? [B4]

`ui-design.md:111` promises every change applies live. The Windows renderer
applies only on close (`settings_ui.rs:1397`), by a deliberate three-way merge
that protects ⌃⇧W-taught words while the hook keeps running.

- **Recommended: live-apply per edit** through the existing merge, keeping
  `finalize()` as the safety net.
- *Flips it:* per-edit merge is unsafe under session ownership → keep the button,
  label it, and write the exception into `ui-design.md`.

---

### 6. How does a Windows user pick an app to exclude? [B8]

Today: a text box wanting an `.exe` filename. macOS gets a file picker with real
names and icons. This is the headline feature with the most hostile input
surface in the product.

- **Recommended:** running-process picker plus the free-text escape hatch.
- *Flips it:* time budget → `IFileOpenDialog` over `Program Files` only.

---

### 7. Editors in the shipped exclusion defaults [B14]

VS Code, Sublime and friends ship pre-excluded. Terminals are defensible;
editors are where many people write Vietnamese (Markdown, commit messages,
notes).

- **Recommended: drop editors, keep terminals.**
- *Flips it:* your own use says otherwise. No telemetry exists to answer it.

---

### 8. `open_settings_at_launch` defaults `true` [B22]

An app whose stated value is being unobtrusive opens a window at every launch.

- **Recommended:** default `false` — once the Windows welcome guide exists
  (phase 4), so first-run discoverability does not regress.

---

### 9. Linux, and what it costs the published crates [B26]

The recommended architecture is an **IBus engine** — not the evdev/uinput global
hook that mirrors macOS/Windows, because Linux has no equivalent of an
intercept-and-inject tap (X11 RECORD is passive, `XGrabKeyboard`+XTEST self-feeds,
Wayland gives ordinary clients no keyboard grab).

Two consequences you should accept explicitly before any code is written:

- **The per-app ignore list — GlowKey's headline feature — degrades on Linux.**
  Full support on wlroots and Plasma; best-effort on GNOME Wayland (IM-module
  clients only, via the IBus client name); **none** for Ozone-Wayland
  Electron/Chromium apps. On GNOME Wayland, the terminals and editors you most
  want excluded are exactly the ones hardest to identify.
- IBus deletes in **characters**, not UTF-16 code units, which forces a breaking
  `Backspaces { utf16, chars }` change across all three library crates — a
  `0.2.0` while only `glowkey-engine` is published.

- **Recommended: not until phases 1–6 close.** ~20 engineer-days, ~5 of them
  needing a Linux machine you do not currently have.
- *Flips it:* Linux users are the actual target audience.

Also unverified and load-bearing: that IBus `focus_in_id` really carries usable
client names (`gtk3-im:firefox`). The whole ignore-list story on GNOME rests on
it. Verify before committing to the architecture.

---

### Not up for decision without new evidence [B15]

The audits questioned three things the handoff records as settled under live
use. The review rules say a verified decision reverses on new evidence, not on
an abstract concern. **Recommended: no change.**

| Decision | Why it stands |
|---|---|
| Mode is session-only, never persisted | One accidental ⌃⇧Space at quit made the app launch dead ("aa not work") |
| Backspace deletes visible characters, not keystrokes | Questioned twice in live use, reaffirmed both times; the alternative needs a second Backspace mode that exists only after a repair |
| English restore ships off | Turning it on makes `á`, `ú`, `cả`, `cát` untypeable in that key order |
