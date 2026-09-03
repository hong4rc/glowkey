# Manual verification

Everything in GlowKey that a test cannot reach. The engine has 135 headless
tests and a generated-input property suite; none of them can see a menu, a
window, a HUD flash, or a keystroke landing in Chrome. This is the list for
those, written as steps with expected results so it can be run in about fifteen
minutes after any change to `app/`.

**Status: never executed end to end.** Written 2026-09-03 alongside the welcome
panel and the tap health monitor. A checklist nobody has run is a guess about
what the app does, so treat unticked sections as unverified rather than passing.

## Before you start

```bash
bash scripts/dev-run.sh      # foreground, GLOWKEY_DEBUG=1, needs no grant
tail -f ~/Library/Logs/GlowKey/glowkey.log
```

`dev-run.sh` builds **GlowKey Dev**, a separate app with its own bundle
identifier, so nothing here disturbs the grant of the GlowKey you type with.
Never run both at once: two taps process every keystroke twice.

For the sections that need a real installed app (permission revocation, launch
at login), use `bash scripts/release-install.sh` instead and expect one
Accessibility grant.

---

## 1. First run

- [ ] Delete `~/Library/Application Support/GlowKey/settings.json`, launch, and
      the **welcome panel** appears once, after the permission gate, never
      alongside it. Two dialogs on screen at once is the bug this ordering exists
      to prevent.
- [ ] It names ⌃⇧Space and ⌃⇧E, and says terminals are excluded on purpose.
- [ ] "Got it" dismisses it. Relaunch: it does **not** come back.
- [ ] Menu → **Quick Guide…** reopens the same panel.
- [ ] With Language set to Tiếng Việt, both the welcome and the menu are in
      Vietnamese.

## 2. Typing, in a plain field (TextEdit)

- [ ] `hoongf` → `hồng`. Also `hofong` and `hoonfg` → `hồng` (tone key anywhere).
- [ ] `oo` → `ô` immediately, `aa` → `â`, `dd` → `đ`.
- [ ] `hoongff` → `hôngf` — repeating the tone key rejects it. This is the
      per-word escape hatch and must keep working.
- [ ] `exit`␣ → `exit`, not `eĩt` (auto-fix at the boundary).
- [ ] `đc`␣ → `đc`, not `ddc` (a leading đ is always deliberate).
- [ ] `left`␣ → `left`, `soft`␣ → `soft`, `gift`␣ → `gift` (the stop-coda tone
      rule; before it these came out `lèt`, `sòt`, `gìt`).
- [ ] `hồng`␣⌫`z` → `hông` (deleting the boundary re-opens the word).
- [ ] `hoongf`⌫`z` → `hôn` (mid-word backspace stays composed).
- [ ] Type `hoo`, press ←, type `f`: the `f` is literal, not a tone on the word
      you left. Same for Home/End/Page.
- [ ] Type `hoo`, click elsewhere with the mouse, type `f`: same.

## 3. Input methods and options (Settings → Typing)

- [ ] **VNI**: `viet65` → `việt`, `a6` → `â`.
- [ ] **Simple Telex**: `w` alone stays `w`, but `uw` → `ư` and `ow` → `ơ`.
- [ ] **Quick Telex** on: `cc` → `ch`, `nn` → `ng` at a syllable start;
      `letter`, `happy`, `accept` unaffected mid-word. `CCAO` → `CHAO`,
      `Ccao` → `Chao`.
- [ ] **Telex brackets** on: `[` → `ơ`, `]` → `ư`, `{` → `Ơ`, `}` → `Ư`, and
      `[f` → `ờ`. Off (default): `[` types a bracket.
- [ ] **Mid-word spell check** on: `exit` is repaired at the `x`, not at the
      space. `nguowif` still reaches `người` — the check judges the render, not
      the raw keys.
- [ ] **Restore common English words** on: `was`␣ → `was`. Off (default):
      `was`␣ → `ứa`.
- [ ] **Auto-capitalize** on: first letter of a sentence capitalises, including
      after `.`/`!`/`?`, and with brackets on `[` at a sentence start gives `Ơ`.

## 4. The per-app ignore list — the feature the app exists for

- [ ] In Ghostty/Terminal/iTerm, the glyph reads **EN** and typing is untouched.
- [ ] ⌃⇧E in a terminal enables Vietnamese and the HUD shows **VI ⚠** (the
      warning variant: a PTY ignores synthetic backspaces).
- [ ] Restart: that terminal is excluded again. Session-only by design.
- [ ] ⌃⇧E in TextEdit toggles it off, HUD shows **EN**, and it is remembered
      across a restart.
- [ ] Settings → Apps & macros → Excluded Apps: add and remove an app; a removed
      shipped default stays removed after a restart (the tombstone).

## 5. Hotkeys and the recorder

- [ ] ⌃⇧Space toggles VN/EN, HUD flashes **VI**/**EN**, glyph follows.
- [ ] Each preset in Settings works: ⌃⇧Space, ⌃Space, ⌥Space, ⌃⇧Z.
- [ ] **Custom…** arms the recorder; the caption row shows "Current: …".
- [ ] While armed: ordinary typing passes through untouched, and every ⌘
      shortcut still works. Only ⌃/⌥ combos are captured.
- [ ] Esc cancels. A mouse click cancels. Switching apps cancels.
- [ ] ⌃⇧E is refused with an explanation (reserved for the per-app toggle).
- [ ] A recorded combo survives a restart.

## 6. Menu, windows, clipboard tools

- [ ] Every menu item does what it says; the header names the app in front.
- [ ] Settings, Excluded Apps, Macros and About all **reopen after being
      closed** — `setReleasedWhenClosed(false)` is what makes this work and its
      absence is invisible until you close a window twice.
- [ ] Settings' four tabs (General / Typing / Corrections / Apps & macros) each
      lay out without clipping, at both languages.
- [ ] Macros: add `vn` → `Việt Nam`, type `vn`␣ in TextEdit, get `Việt Nam`.
- [ ] Macro export writes a file; importing it back reports `(0 added, N
      skipped)` — an existing shortcut is skipped, never overwritten.
- [ ] Import a real UniKey export: a version other than 1 is refused with an
      explanation rather than storing `Vie^.t Nam` as literal text.
- [ ] Clipboard tools: copy `Tiếng Việt`, then Remove tones → `Tieng Viet`,
      UPPERCASE, lowercase. A non-text clipboard is left alone.
- [ ] Reveal Log in Finder selects the log file.
- [ ] Launch at login: toggle on, restart the Mac, GlowKey is running.

## 7. Chromium omnibox guard

- [ ] In Chrome's address bar, `hoongf` → `hồng`, not `hoồng`. The log shows
      "OMNIBOX trailing selection detected" on the transforming keys.
- [ ] In a normal text field **inside** a Chrome page, typing is unaffected and
      that log line is absent — the guard must not fire where there is no
      trailing selection.
- [ ] Repeat in Edge, Brave and Arc.
- [ ] Read `EMIT took=` in the log for a Chromium keystroke and for a TextEdit
      keystroke. **Record both numbers**: their difference is the accessibility
      guard's real cost, which `docs/handoff.md` §6.1 still describes only as
      "typ. sub-ms" — an estimate nobody has measured.

## 8. Safari (open question)

Not yet answered: does Safari's smart search field have the same
inline-autocomplete trailing selection Chromium's omnibox does?

- [ ] With `GLOWKEY_DEBUG=1`, type `hoongf` in Safari's address bar. Record what
      happens: correct output, `hoồng`, or something else.
- [ ] If it is wrong, add `com.apple.Safari` to `CHROMIUM_BUNDLE_PREFIXES` in
      `app/src/tap.rs` and check both that it fixes the address bar **and** that
      it does not fire in ordinary Safari page fields.
- [ ] If it is already correct, record that and change no code. Two of the three
      possible outcomes here ship nothing.

## 9. Accessibility revocation and recovery

Needs the installed app (`release-install.sh`), not `dev-run.sh` — the dev loop
inherits the terminal's grant, so there is nothing to revoke.

- [ ] With GlowKey running and typing Vietnamese, turn its Accessibility switch
      **off** in System Settings → Privacy & Security → Accessibility.
- [ ] **Record whether the process survives.** On some macOS versions the system
      terminates it outright, which is the benign outcome and makes the rest of
      this section unreachable.
- [ ] If it survives: within about two seconds the menu-bar glyph becomes **⚠**,
      and the log says the permission was revoked.
- [ ] The menu's first line names the cause and offers "Open System Settings…".
- [ ] Turn the switch back **on**: within about two seconds the log says the tap
      was rebuilt, the glyph returns to VI/EN, and Vietnamese types again
      **without a relaunch**.
- [ ] Idle CPU in Activity Monitor is indistinguishable from before (the health
      check is one call every two seconds).

## 10. Permission gate, on a fresh grant

- [ ] Copy the app to a new location (or rebuild after a code change if you have
      no stable signing certificate) so the grant is dropped, then launch it.
- [ ] The alert appears, is **full width with both strings legible**, and has
      exactly two buttons. A 260-point panel with truncated text and a stray
      untitled button means `NSAlert::layout()` was not called before `window()`.
- [ ] "Open System Settings" opens the Accessibility pane and macOS's own prompt
      appears — that prompt is what registers the app in the list.
- [ ] Flipping the switch starts the app by itself, with no further clicking.

---

## Recording results

Note the date, the commit, and anything that failed, directly in the pull request
or commit message that prompted the run. Sections 7 step 4, 8 and 9 produce
**numbers and facts that belong in `docs/handoff.md`** — they are the open items
that no headless test can ever close.
