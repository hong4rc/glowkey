# Phase 8 — Linux

**Gated by:** decision B26, and phases 1–6 closing first.
**Blocked on:** a Linux machine (~5 of ~20 engineer-days cannot be done without
one).
**Status:** architecture proposed, key facts unverified. Do not start on the
strength of this document alone.

## The decision: an IBus engine, not a global hook

GlowKey's macOS and Windows shells share one shape — intercept every key,
suppress it, inject the diff from a single tagged source. **That shape is not
implementable on Linux.**

- X11's RECORD extension is *passive*: it observes, it cannot suppress.
- `XGrabKeyboard` + XTEST self-feeds — the injected events come back through the
  grab.
- Wayland gives ordinary clients **no keyboard grab at all**. There is no
  privileged path for a normal application.

So Linux gets `ibus-glowkey`: a standalone Rust process on D-Bus, registered as
an IBus engine, reusing `glowkey-engine` / `glowkey-session` / `glowkey-input`
unchanged. This is how every other Vietnamese IME on Linux works
(ibus-bamboo, ibus-unikey), and it is the conventional path rather than a
compromise.

**Rejected:** evdev/uinput (needs input-group or udev privileges, and still
cannot see which application has focus under Wayland); XTEST/X11-only (abandons
Wayland, which is the default on current GNOME and KDE).

## What the user loses, stated before any code is written

**The per-app ignore list — the feature the product leads with — degrades on
Linux.**

| Environment | Per-app exclusions |
|---|---|
| wlroots compositors | Full (compositor protocol) |
| KDE Plasma | Full (compositor protocol) |
| GNOME Wayland | **Best-effort only** — IBus client name (`focus_in_id`, e.g. `gtk3-im:firefox`) identifies IM-module clients and nothing else |
| Ozone-Wayland Electron/Chromium | **None** |
| X11 | Full |

On GNOME Wayland — the most common Linux desktop — the terminals, editors and
Electron apps you most want excluded are exactly the ones hardest to identify.
`ext-foreign-toplevel-list-v1` does not help: it exposes no focus state, and
Mutter and KWin do not implement it.

**Verify before committing:** that `focus_in_id` really carries usable client
names. The entire ignore-list story on GNOME rests on that one fact, and it is
cited but unverified. If it is false, the Linux product is a Vietnamese input
method with no per-app exclusions, which is a different product — and the README
would have to say so.

## What it costs the published crates

IBus deletes in **characters**; the engine speaks **UTF-16 code units**. They
diverge only for astral-plane text, which in practice means a macro containing an
emoji — and there the divergence deletes the user's text. The honest fix is
`Backspaces { utf16, chars }` in `crates/glowkey-input`, which is a **breaking
change across all three library crates** and forces `0.2.0` while only
`glowkey-engine` is on crates.io.

Other `Platform` changes: `CodeOrigin` tagging on `HotkeyPreset::Custom`, and two
additive `Notice` variants. Delivery-mode fallback stays out of the trait
deliberately.

## Packaging

`.deb` + `.rpm` + tarball. **AppImage and Flatpak are rejected on a technical
ground, not a preference**: an IBus engine is registered by a component XML at a
fixed host path that `ibus-daemon` execs, and neither format can put a file
there.

## Risks

- **L-2, the significant one:** the IBus forwarded-Backspace fallback may
  reintroduce exactly the native/synthetic race that the full-suppression model
  exists to eliminate (`decisions/0001`, handoff §5). Whether it does is
  **unknowable without a Linux machine**. If it does, the Linux port needs a
  different composition strategy — possibly preedit, which changes the typing
  feel the project deliberately avoided.
- Preedit versus direct commit is a product question, not only a technical one.
- ~20 engineer-days, with `hook.rs` and `settings_ui.rs` contended by three
  phases if this runs concurrently with phase 4. It should not.

## Prerequisite before any Linux code

Answer decision B26, verify the `focus_in_id` claim, and accept the degraded
ignore list on GNOME Wayland in writing. Starting without those three is how this
phase becomes twenty days that end in a product nobody wants to ship.
