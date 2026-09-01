# GlowKey — where things stand, and what needs you at a Mac

This is the handoff for when you're back. The engine is built and tested; the
things that remain need a GUI, System Settings, or your Apple Developer account —
none of which can be done headlessly.

## What is done and verified

- **The Vietnamese engine** (`crates/glowkey-engine`) — tested, 18 passing tests.
  - `hoongf` / `hofong` / `hoonfg` → `hồng` (free tone placement, all orders)
  - `oo` → `ô` immediately, `nguyeenx` → `nguyễn`, `dduwowcj` → `được`, `quar` → `quả`
  - Uppercase: `NGUYEENX` → `NGUYỄN`, `Hoongf` → `Hồng`
  - Backspace replays raw keys; focus change flushes the word
- **The per-app ignore list** (the primary feature) — tested, including the
  critical rule that **exclusion beats the VN/EN hotkey**.
- **The all-Rust InputMethodKit shell** compiles and links against
  `InputMethodKit.framework`. This was the biggest unknown — whether you can
  subclass `IMKInputController` from Rust via objc2 — and the answer is yes.
- App bundle scaffolding: `Info.plist`, `scripts/build-app.sh`, CI (including a
  privacy guard that fails if the binary ever links a networking framework),
  `PRIVACY.md`, `LICENSE`, `THIRD-PARTY-NOTICES.md`.

## Adversarial review — fixed and outstanding

A code review brute-forced 475k keystroke sequences. The engine core (diff,
UTF-16 counting, NFC, case round-trip) came back **provably sound**. Five real
defects were found at the boundaries; the fixable ones are fixed:

- **Fixed — CRITICAL: the Obj-C class was never registered.** objc2 registers a
  `define_class!` class lazily, and `run()` never referenced it, so IMK would have
  resolved the controller to nil and been silently inert. `run()` now calls
  `GlowKeyController::class()` before starting the server.
- **Fixed — interior capitals were destroyed** (`iPhone`→`iphone`). Untransformed
  words now emit their keys verbatim, preserving case. Tested.
- **Fixed — excluding the current app mid-word corrupted the document.** The
  session now flushes the engine on every inactive keystroke. Tested.
- **Fixed — the ignore list failed open** (unknown app → transform). `is_active()`
  now fails **closed**: nothing transforms until the shell reports the frontmost
  app. Tested. (Consequence: the rendering layer *must* resolve the bundle id in
  `activateServer:` or Vietnamese will not type at all — the safe direction.)

Two items remain and both need the rendering layer, not the engine:

- **The caret-invalidation contract (M3).** The engine's edits assume the current
  word is still the document tail. `flush()` is now documented as mandatory on any
  caret/selection move; `activateServer:`/`deactivateServer:` already flush. When
  you wire `handleEvent:`, you must also flush on arrow keys and mouse clicks, or a
  later keystroke can delete unrelated text. This is the same class of bug every
  IMK input method has around mouse clicks.
- **`www` → `ww` (upstream `vi`).** Typing `www.example.com` yields
  `ww.example.com`. This is `vi`'s own Telex behavior, not an engine bug. Left as a
  known limitation; the fix is a temp-off key or excluding the browser.

## What is NOT done — and why it waited for you

### 1. Phase 1 — verify the premise (do this first, ~1 hour, no code)

Before more building, confirm GlowKey has a reason to exist. macOS already ships
Vietnamese input (it *is* UniKey's engine), so free tone placement is not the
differentiator — the per-app ignore list and true InputMethodKit behavior are.

Install EVKey (evkey.vn) and enable macOS's built-in Vietnamese, then check:

- Does EVKey prompt for **Accessibility** on install? (InputMethodKit does not.)
- Does EVKey work in a **password field**? (CGEventTap-based tools do not.)
- Does the built-in have a **per-application ignore list**? (It does not.)

If EVKey needs no prompt AND works in password fields AND has a good ignore list,
the niche is occupied — consider just using it. Otherwise, GlowKey's niche is
real: InputMethodKit's no-prompt, works-in-passwords behavior plus per-app control.
Write the result into `docs/decisions/0000-why-glowkey.md`.

### 2. The shell's rendering layer (needs a GUI to verify)

`app/src/controller.rs` currently returns `false` from `handleEvent:client:` —
it is **inert and safe**, so an installed build touches nothing. To make it type,
the remaining conventional objc2 work is:

- Decode the `NSEvent` in `handleEvent:client:` (keycode, characters, modifiers);
  filter modifier chords (⌘/⌃/⌥) so shortcuts like ⌘S are never eaten.
- Call `session.process_key(ch)` and render the returned `KeyResponse` to the
  client via `insertText:replacementRange:` — or, preferred for compatibility,
  `setMarkedText:...`. The engine already returns `backspaces` (UTF-16) + `insert`.
- Resolve the frontmost bundle id (`NSWorkspace.frontmostApplication`) in
  `activateServer:` and call `session.set_frontmost_app(id)` so the ignore list
  applies. Verify it arrives **before** the first keystroke.

None of this is unit-testable — it needs the install-and-type loop below.

### 3. Build, install, and actually type

```
./scripts/build-app.sh
cp -R build/GlowKey.app ~/Library/Input\ Methods/
# log out/in, then System Settings → Keyboard → Input Sources → + → GlowKey
```

Debug with `log stream --predicate 'process == "GlowKey"'` (breakpoints don't work
— the process is launched by `imklaunchagent`). **Log no keystroke content.**

### 4. Menu bar, hotkey, preferences (Phase 3)

The engine `Session` already exposes everything the UI drives: `toggle_mode`,
`exclusions_mut`, `set_style`, `set_frontmost_app`. The macOS side (`NSStatusItem`,
the ignore-list editor, the ⌘Space-style toggle) is conventional AppKit via objc2.

### 5. Signing & notarization (Phase 4) — needs your Apple Developer membership

A hard prerequisite with an enrolment lead time. Nothing routes around it.

## One decision I made without you

`vi` misplaces the tone for whole-word uppercase (`NGUYEENX` → `NGUỸÊN`). I fixed
it in the engine by folding case out before transformation and back after — the
approach `xkey` uses. ALL-CAPS and Title-case are exact; arbitrary interior mixed
case (`hoOngf`) is best-effort. If you want interior mixed case exact too, that is
a larger change. See `render()`/`apply_case()` in `crates/glowkey-engine/src/lib.rs`.
