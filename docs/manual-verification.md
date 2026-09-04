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
- [ ] `hoongf`␣`s`⌫⌫`z` → `hông` — it survives a word typed and deleted in
      between. The reported bug; the memory used to die on the next keystroke.
- [ ] `hoongf,`␣⌫⌫`z` → `hông`, and the same with `.` in place of the comma. A
      second boundary in a row used to throw the whole history away, which left
      the bug above reachable one comma later.
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
- [ ] **Deleting a mistake undoes the escape** (reported from live use): type
      `hoongf` → `hồng`, then `a` → `hoongfa`, then ⌫ → **`hồng`**, and typing
      `s` after it gives `hống` — the word is still live, not dead literal text.
      The word is replaced once per press and **nothing to the left of it moves**
      — that is the check, not a character count. The glyph count legitimately
      jumps (`hoongfa` is seven, `hồng` is four), because the Backspace is
      suppressed and the repair rewrites the whole word in a single edit.
- [ ] Same sequence in Chrome's address bar — the repair emits backspaces, so it
      goes through the omnibox guard.
- [ ] Keep deleting: `hồng` → `hồn` → `hồ`, still transforming, never re-escaping.
- [ ] With the spell check **off**, `hoongfa` ⌫ gives `hồng` as it always did —
      the ordinary path must be untouched.
- [ ] Type `aal` (shows `aal`), press ⌫ once: you get **`â`**, the state before
      the `l`. Deliberate, and the one judgement call in this fix — say if it
      feels wrong.
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

## 8b. Personal words and the correction hotkey

The engine half is covered by 27 headless tests; what needs eyes is the window,
the HUD flash, and — most of all — that the correction lands where it should,
since it is the only edit in GlowKey that reaches back over a boundary character
into text that is already committed.

- [ ] Type `was`␣ in TextEdit. It becomes `ứa` (the shipped default).
- [ ] Press **⌃⇧W**. It becomes `was ` — the space survives — and the HUD flashes
      `ứa → was` legibly, without clipping.
- [ ] Type `was`␣ again: `was` straight away, no keystroke needed.
- [ ] **Quit GlowKey, relaunch, type `was`␣.** Still `was`. If it reverts, the
      decision was never written to disk and the feature has not learned
      anything.
- [ ] Settings → Corrections → **Personal Words…** lists `was — as typed`.
      **Flip** it, type `was`␣: now `ứa`. **Remove** it: back to `ứa` by rule.
- [ ] With the window open, correct a different word with ⌃⇧W — the list updates
      without reopening.
- [ ] Close the window and reopen it twice.
- [ ] Add a word by hand with both buttons; add a blank one (nothing happens).
- [ ] In Tiếng Việt, every string in the window and the caption is Vietnamese.

**The three that shipped broken — check each explicitly:**

- [ ] Type `was`␣, press ⌃⇧W, press **⌫**, then type `f`. Expect `wasf`. If you
      get `wừa`, the corrected word is re-composing and the diff baseline is
      lying.
- [ ] Type `xin chào was`, press **Escape** (or any function key), then ⌃⇧W.
      Expect **nothing to happen**. If the space before `ứa` disappears or a
      control character appears, a non-inserting key is being charged as a
      boundary.
- [ ] Type `was` then **Tab** (into another field), then ⌃⇧W. Expect nothing. Then
      the same with **Return** in Slack or Messages — the message is already
      sent, and an edit landing there would reopen or resend it.
- [ ] Type `was`␣, switch to another app, press ⌃⇧W. Expect nothing.

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
      check is one call every two seconds, and it skips while you are typing).

## 9b. The freeze — do this one first (handoff §6.9, `decisions/0008`)

The most important check here, because the failure it looks for hurts the whole
machine rather than GlowKey. Toggling the permission used to stall every keystroke
on the Mac: a blocking window-server call inside the tap callback, with the busy
window server supplied by the System Settings sheet itself.

- [ ] **Keep typing in TextEdit while you flip the Accessibility switch off**, and
      again while you flip it back on. Nothing anywhere should hitch — not
      GlowKey, not System Settings, not the Dock, not the menu bar clock.
- [ ] `grep 'TAP disabled by timeout' ~/Library/Logs/GlowKey/glowkey.log` — expect
      **no lines**. Any line here means something in the keystroke path blocked
      and the whole rule of `decisions/0008` has been broken again.
- [ ] Switch apps a few times (to a terminal and back) and confirm Vietnamese
      still turns off in the terminal. Frontmost now arrives by notification
      rather than a per-keystroke query, so a regression shows up here as
      Vietnamese firing in a terminal — the ignore list is the thing this fix
      could plausibly have broken.
- [ ] Type a long paragraph and read the `EMIT took=` figures: the maximum should
      stay in the hundreds of microseconds. Before the fix the median was 58 µs
      but the maximum 22.4 ms. Record the numbers — §7 of the handoff carries
      them as the baseline.

## 9c. The UI pass of 2026-09-04

All of this is AppKit that no test can reach, and several items are new structure
(a main menu, scroll views, layout constraints) rather than new text — so this
section is where a regression would show up first.

- [ ] **Paste works.** Open Settings → Apps & macros → Macros…, copy `Việt Nam`
      from anywhere, and press ⌘V in the expansion field. Also ⌘C, ⌘X, ⌘A and ⌘Z.
      None of these worked before the main menu existed; this is the check that
      matters most.
- [ ] ⌘W closes the focused GlowKey window; ⌘, opens Settings while a GlowKey
      window is focused; ⌘Q quits.
- [ ] **Nothing is cut off.** Excluded Apps with the shipped defaults: every row
      reachable by scrolling, and the window resizes. Same for Macros after
      importing a large table, and for Personal Words.
- [ ] **The excluded list reads like apps.** Real icons, names as Finder shows
      them ("Visual Studio Code", not "VSCode"), sorted by name. Exclude an app,
      delete it from /Applications, reopen: it stays, greyed, marked "(not
      installed)".
- [ ] **The glyph has three states.** `VI` in TextEdit; **dimmed** `VI` in a
      terminal (excluded); `EN` after ⌃⇧Space. Confirm the excluded and English
      states are visibly different — they were identical before.
- [ ] **The shortcut shown is the one that works.** Settings → General, change the
      toggle hotkey; the menu-bar menu's "Vietnamese input (…)" row and the menu's
      Quick Guide… must both name the new combo, not ⌃⇧Space.
- [ ] **Macros ask before overwriting.** Add `vn` twice with different expansions:
      the second offers Replace / Cancel. Import a file containing a shortcut you
      already have: it offers Keep Existing / Replace / Cancel once for the file,
      and the counts afterwards match what you chose.
- [ ] **About names the build.** Version reads `0.1.0 (<commit>)`, with a trailing
      `+` if you built from a dirty tree, and the line can be selected and copied.
- [ ] Switch the interface language to Vietnamese with Personal Words open: it
      must come back in Vietnamese. It was the one window the rebuild forgot.

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
or commit message that prompted the run. Sections 7 step 4, 8, 9, 9b and 9c produce
**numbers and facts that belong in `docs/handoff.md`** — they are the open items
that no headless test can ever close.
