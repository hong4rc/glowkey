# GlowKey — what is bad, from a Unikey/EVKey user of ten years

Read-only pass, 2026-09-05, `main` @ `ce51ff0`. Every complaint is anchored to a
file:line or a doc quote. Ranked: **P0** = I uninstall. **P1** = I complain
loudly. **P2** = papercut.

---

## P0 — I uninstall

### 1. In Chrome/Edge, every toned syllable eats the character to the right of my caret

**Evidence.** `app/src/platform/windows/inject.rs:221`

```rust
pub fn needs_omnibox_guard(backspaces: usize, app: Option<&str>) -> bool {
    backspaces > 0 && app.is_some_and(crate::default_exclusions::is_chromium_app)
}
```

`is_chromium_app` matches the whole browser (`chrome.exe`, `msedge.exe`, …,
`default_exclusions/windows.rs:52-61`), not the address bar. The comment at
`inject.rs:88-100` admits it: macOS gates the forward-delete on an accessibility
read; Windows "fires this unconditionally for Chromium applications instead …
if the caret is mid-field *while composing*, the forward-delete eats the
character after it." The excuse — "reaching that state requires moving the caret
without flushing" — is **wrong**. Click into the middle of a Facebook comment or
a Gmail draft (flush happens, fine), then type `hoongf`. The `oo` edit carries
`bs=1`, so `VK_DELETE` fires with real text after the caret and one character of
my sentence is gone. Their own test comment says so:
`inject.rs:233` — "firing it anywhere else spends a forward-delete on a field
that never had a selection, which in a normal editor deletes a real character."

**Why it matters.** Typing Vietnamese into a browser *is* the use case. Inserting
a word mid-sentence is normal editing, not an exotic path. Silent character
deletion is the worst class of input-method bug — you find it after you posted.

**Fixed looks like.** Scope the guard to a caret actually at end-of-field
(cached UIA read off the hot path, or a heuristic on the composing state), or
restrict it to the omnibox by window class, or drop the forward-delete and
re-derive the whole word instead. Shipping a delete key into arbitrary browser
text on a guess is not acceptable.

### 2. On Windows GlowKey writes my passwords to a plaintext file, and PRIVACY.md is macOS-only

**Evidence.** `crates/glowkey-input/src/platform.rs:115` fires
`Notice::Decided` for **every** key-down, before any exclusion or mode branch.
`app/src/platform/windows/hook.rs:499` logs it with the character:

```rust
"KEY {:?} vk={} mods={} app={} | {decision}", event.ch, ...
```

`app/src/log.rs:24` — "this records the text you type". `PRIVACY.md:36-37`
claims "secure/password fields never reach GlowKey at all (macOS withholds them
from event taps)". That is a **macOS** guarantee. `WH_KEYBOARD_LL` has no such
exemption: bank logins, Chrome password fields, `sudo` in Windows Terminal
(excluded apps are still logged — `PRIVACY.md:35` admits it) all land in
`%LOCALAPPDATA%\GlowKey\Logs\glowkey.log`, up to 5 MB plus one rotated
generation (`log.rs:35`). There is **no setting to turn logging off** — grep
finds only `GLOWKEY_DEBUG` for stderr echo (`log.rs:168`). PRIVACY.md never says
the word Windows, and its verification recipe is `otool`.

**Why it matters.** README:98 already concedes "a low-level keyboard hook is,
structurally, what a keylogger does" and answers "the privacy posture is
checkable". The checkable posture is: it keeps a keystroke log including
passwords, by default, with no off switch, undocumented on the platform where it
is most dangerous.

**Fixed looks like.** Logging off by default (or a "Diagnostics" toggle in
General), redaction of the character when the decision is `Passthrough`, a
Windows section in PRIVACY.md that states plainly that Windows has no secure
input exemption, and a "Delete log" menu item next to "Show log folder"
(`tray.rs:824`).

### 3. On Windows nothing notices when Windows kills the hook — the tray keeps lying "VI"

**Evidence.** `hook.rs:180-187`:

```rust
/// **Not proof that it is being called.** Windows can remove a slow hook without
/// telling us, and this still reports `true` afterwards — which is why the
/// indicator pairs it with a liveness check rather than trusting it alone.
pub fn is_installed() -> bool { HOOK.load(...) != 0 }
```

There is no such pairing. `shell.rs:47` passes `hook::is_installed()` straight
into `indicator::state(installed, …)` (`indicator.rs:73`). `grep` for
`SetTimer|WM_TIMER|thread::spawn` across `app/src/platform/windows/` returns
**nothing** — no watchdog at all. macOS has `platform/macos/health.rs` polling
every two seconds (handoff §6.6); Windows has an empty comment claiming the same
protection. `manual-verification-windows.md` Tier 4 states the risk:
"`LowLevelHooksTimeout` is 100 ms and Windows removes a hook that reaches it
**without any event, error or warning**."

**Why it matters.** The failure is silence: I keep typing, Vietnamese stops
happening, the tray still says VI, and the only recovery — "Reinstall the
keyboard hook" (`tray.rs:750`) — is only offered when `Breakage::HookGone` is
set, which the dead-hook case never sets. I will conclude the app is broken and
go back to EVKey.

**Fixed looks like.** A message-loop timer that checks a "callback seen since
last tick" counter (already partly there: `FIRST_CALL`, `hook.rs:336`) and, when
input is arriving system-wide but not to us, flips to `Breakage::HookGone` and
reinstalls. Also fix the comment, which currently over-claims.

### 4. It breaks my games, and unlike EVKey there is no way to stop `w` becoming `ư`

**Evidence.** `plans/reports/windows-handoff-260905.md`: "GlowKey is unusable in
League of Legends while EVKey is fine … `KEY Some('w') vk=87 | Emit bs=0
ins="ư"` … There is **no setting to disable standalone `w`**, and UniKey/EVKey do
have that option". The shipped workaround was to add `league of legends.exe`,
`leagueclientux.exe`, `riotclientservices.exe` to the user's own settings — and
the same report notes that costs Vietnamese in game chat, which EVKey gives.

**Why it matters.** W is the second ability key in every MOBA and half of WASD.
"Uninstall it before you play" is not a product. EVKey solved this a decade ago.

**Fixed looks like.** Run the one discriminating test the report specifies (press
`B` in League with GlowKey on), then either ship the standalone-`w` option
(UniKey has it; it is a `Definition` tweak next to `SimpleTelex`,
`handoff.md` §4) or document that games need exclusion — but do not leave it
open while calling the port "built".

### 5. A settings file GlowKey cannot parse silently becomes factory defaults

**Evidence.** `app/src/prefs_model.rs:122`:

```rust
pub fn from_json(json: &str) -> Self { serde_json::from_str(json).unwrap_or_default() }
```

`settings_store.rs:36` — "tolerant: corrupt → default". `settings_store.rs:56`
admits the consequence: "`Settings::from_json` falls back to FULL defaults on any
parse error (e.g. an older build reading a newer enum variant), and the next save
would then overwrite the user's file with defaults". The mitigation is one
`.json.bak`, which the *next* save also overwrites once the defaults have been
saved twice — nothing tells me any of this happened.

**Why it matters.** That file is my ignore list, my whole macro table (gõ tắt
curated over years, imported from EVKey), and my personal-word decisions. Losing
it to a one-character JSON truncation, a half-written file after a power cut, or
downgrading a build — there is **no `version` field in `Settings`** (grep:
none) — with no dialog and no log line is exactly the "data loss" a keyboard
utility must never do.

**Fixed looks like.** Distinguish "missing" from "unparseable": on a parse error,
refuse to overwrite, log loudly, show a tray warning naming the `.bak`, and add a
schema `version` so a newer file read by an older build is recognised rather than
guessed at.

### 6. There is no way for a normal Vietnamese user to install this

**Evidence.** Windows: README:85-87 — "There is no release artifact yet …
Build it yourself: `cargo build --release -p glowkey`", plus "Expect SmartScreen
to object … expect antivirus to take an interest" (README:97). macOS:
README:70-77 — the app is not notarized, macOS says *"GlowKey is damaged and
can't be opened"*, and the fix is `xattr -dr com.apple.quarantine`. There is no
updater anywhere (grep for Sparkle/update machinery: none).

**Why it matters.** My mother types Vietnamese. EVKey is a `.exe` you double-click.
"Install Rust" and "run this Terminal command that says the app is not damaged,
trust me" removes the entire non-developer audience — i.e. the audience of a
Vietnamese input method. And with no updater, every fix above requires me to
notice, re-download, and re-`xattr`.

**Fixed looks like.** A signed Windows installer (even a cheap OV cert stops
SmartScreen escalating), notarization on macOS or a first-run helper that does
the quarantine removal for the user, and any update check at all.

---

## P1 — I complain loudly

### 7. It ships silently disabled in VS Code, Xcode, IntelliJ, and every terminal

**Evidence.** `default_exclusions/macos.rs:23-36` and
`default_exclusions/windows.rs:29-49`: `code.exe`, `devenv.exe`, `idea64.exe`,
`pycharm64.exe`, `webstorm64.exe`, `sublime_text.exe`, `nvim.exe`, `vim.exe`,
plus nine terminals. README calls the ignore list the "distinguishing feature".

**Why it matters.** Terminals, yes — obviously right. Editors, no. I write
Vietnamese commit messages, Vietnamese comments, Vietnamese strings, and
Vietnamese in the chat panel of VS Code. Unikey and EVKey exclude nothing by
default, so a switcher's first experience is "it does not work in my editor" with
no error, no HUD, only a tray glyph they have not learned to read yet.

**Fixed looks like.** Ship terminals excluded, not editors. If editors stay, say
so in the first-run guide and show something on screen the first time a keystroke
is passed through because of an exclusion.

### 8. Ctrl+Shift+E and Ctrl+Shift+W are undiscoverable on Windows, and unrebindable everywhere

**Evidence.** `grep welcome` shows `welcome.rs` is reachable only from
`platform/macos/mod.rs:427-482` and `menu_bar.rs:148` — **Windows has no
first-run guide and no Quick Guide menu item** (`tray.rs:704-837` has no such
entry). The only place `Ctrl+Shift+E` is named is a caption in the *Apps &
macros* tab and a read-only row in *General*
(`settings_spec.rs:325-337`, `settings_spec.rs:395-402`); `Ctrl+Shift+W` appears
only in the *Personal words* caption (`settings_spec.rs:401`). Both are fixed:
handoff §4 — "The hotkey is fixed, like ⌃⇧E … and the recorder refuses both."

**Why it matters.** The per-app toggle is advertised as the product's whole
point. On Windows I have to open Settings, find the fourth tab, and read a
caption to learn it exists. Meanwhile Ctrl+Shift+E is Chrome's "search with…"
and Ctrl+Shift+W closes a window in several apps — GlowKey swallows them
globally with no opt-out.

**Fixed looks like.** A first-run window on Windows too, both shortcuts in the
tray menu next to the actions they perform, and both made rebindable/disableable.

### 9. No hotkey recorder on Windows, and a Mac-recorded hotkey matches by *character*

**Evidence.** `settings_spec.rs:200` — "macOS appends its recorder as 'Custom…';
Windows has no recorder and shows the presets alone." `hook.rs:416-428` logs
"the custom toggle was recorded on another platform — matching by character,
which depends on the keyboard layout. Re-record it here to fix." — which is
impossible, because there is no recorder. `settings_ui.rs:252-262` also silently
drops `Alt+Space` from the offered presets.

**Why it matters.** I get three presets. EVKey lets me pick any combination. And
a settings file the project brags is cross-platform (`settings_store.rs:21-24`)
carries a hotkey that Windows can only fuzzy-match and cannot repair.

### 10. Toggling Vietnamese on Windows gives me no on-screen feedback

**Evidence.** `hud.rs` exists (macOS, handoff §4 "VI/EN glyph + HUD"); `grep -i
hud app/src/platform/windows/*.rs` returns nothing, and `tray.rs` uses no balloon
or toast API. The only feedback is the tray glyph
(`indicator.rs:93-100`, "VI"/"EN"/"!") — and the Tier 1 report records that this
glyph was **invisible** on the user's machine because it was hardcoded near-white
("the tray glyph was invisible … 'vi may be easy, but en is something wrong,
cannot see that'", `windows-verification-260905.md`).

**Why it matters.** I toggle dozens of times an hour. Unikey and EVKey both flash
something. Looking down at a 16-px tray glyph after every toggle is a tax, and
the mode is not persisted (below), so I toggle more, not less.

### 11. Mode resets to Vietnamese on every launch, and is not remembered per app

**Evidence.** handoff §4 — "**Launch always in Vietnamese** (mode is
session-only, never persisted)"; §5 — "Persisting the global VN/EN toggle let one
accidental ⌃⇧Space at quit make the app launch disabled".

**Why it matters.** The fix for one bug became a permanent behaviour. I work in
English most of the day and Vietnamese in chat; EVKey remembers where I left it,
and remembers per application. GlowKey gives me a global list of apps that are
*permanently off* or a mode I must re-toggle after every reboot — neither is
"remember what I was doing."

### 12. Backspace deletes characters, not keystrokes — the opposite of what Unikey trained me for

**Evidence.** handoff §4 — "`hoongf` `s` ⌫ ⌫ `z` … deleting characters gives
`hốn` → `hố` where deleting keystrokes would give `hoong` → `hông`". The doc
notes it was "questioned twice in live use and reaffirmed both times".

**Why it matters.** Two independent reports of the same thing is not two people
being wrong. Fixing a tone by backspacing is the single most common repair a
Telex typist makes; landing on `hố` instead of `hông` costs me the whole word.

**Fixed looks like.** At minimum an option. "It would mean a second Backspace
mode" is an implementation cost, not a user argument.

### 13. "Restore common English words" is a trap, and it is off, so `was` becomes `ứa` by default

**Evidence.** handoff §6.3 — with it ON, "á→as, í→is, ú→us, ò→of, ỏ→or, mã→max,
sĩ→six, thú→thus, cả→car, hải→hair, tả→tar, cát→cats, sét→sets" become
untypeable in that key order.

**Why it matters.** Both settings are wrong. Off: I type English words inside
Vietnamese sentences all day and they mangle. On: ordinary Vietnamese syllables
stop working. The per-word ⌃⇧W list is a decent answer but requires me to train
the app one word at a time, and (item 8) I do not know the shortcut exists.

### 14. The macOS "feature-complete" claim is not backed by anything that has been run

**Evidence.** README:26-29 — "**macOS — feature-complete against the useful
Unikey/EVKey set** … Live GUI verification ongoing." handoff §11 — "the **macOS
side of all of it is compile-checked only** and has not been run." handoff §8 —
the manual checklist "has **never been run end to end**; treat unticked sections
as unverified." handoff §6.6, §6.9 — the tap-health recovery and the
system-freeze fix are both "FIXED, needs live verification".

**Why it matters.** "Feature-complete" and "compile-checked only, never run" are
in the same repository on the same day. The Windows warning in the README is
admirably honest; the macOS line is marketing.

### 15. The one Windows verification round was contaminated and was never re-run

**Evidence.** `windows-verification-260905.md`, first section: "**EVKey64 was
running throughout.** … **Tier 1 should be re-run with EVKey stopped before any
of this is called settled.** That was not done here". "Next, in order: 1. Re-run
Tier 1 with EVKey stopped." No later report does it.

**Why it matters.** The five PASS rows everyone points at were measured with a
competing IME in the hook chain. Everything downstream — README:33-38, handoff
§5b "What is verified" — inherits that caveat and mostly drops it.

### 16. Idle cost: 2% of a core and 100 MB for a keyboard helper

**Evidence.** `windows-verification-260905.md` Tier 5: "30 s sample: **593.75 ms**
CPU (**≈2.0%** of one core), working set **~101 MB**".

**Why it matters.** EVKey idles at nothing. 2% of a core while I am not typing is
a laptop-battery complaint, and 100 MB resident for an input method is what an
`eframe` UI thread kept alive for the life of the process (`decisions/0011`)
costs. Nobody has explained the 2%; the report itself calls it "the baseline",
not an acceptable figure.

### 17. Importing my EVKey macro table can silently destroy the table I already have

**Evidence.** `crates/glowkey-session/src/session.rs:486-495`: with
`MacroConflict::Replace`, an imported row overwrites an existing macro; handoff
§4 warns "`add_macro` is add-*or-replace*". There is no backup of the macro table
before an import and no undo — and the only copy of the table lives in the same
`settings.json` that item 5 can reset to defaults. A real UniKey export whose
header version is not 1 is refused outright (handoff §4, VIQR bodies).

**Why it matters.** My gõ tắt table is the most personal data in the app. Import
is precisely the moment a ten-year EVKey user runs, once, on day one.

---

## P2 — papercuts

18. **"Gõ tắt" means two different things in the Vietnamese UI.** Macros section
    is `Gõ tắt` (`settings_spec.rs:405`) and Quick Telex is `Gõ tắt phụ âm`
    (`settings_spec.rs:349`). A Vietnamese user reads both as "macro".
19. **"Fix as I type, not at the space"** (`settings_spec.rs:373`) never says it
    is a spell check, is nested under Auto-fix, and its Vietnamese caption
    repeats the label almost verbatim.
20. **Telex bracket shortcuts leak.** handoff §4: "`an[` gives `anow`", and
    turning the option on stops `[`/`]` reaching any app at all.
21. **Settings window opens on every boot by default** —
    `prefs_model.rs:110` `open_settings_at_launch: true`. A background agent that
    throws a window at me at every login is what I disable EVKey's control panel
    for.
22. **Windows app identity is the bare lowercased exe name**
    (`default_exclusions/windows.rs:1-12`, its own comment calls the table
    "**Unverified.** Every name here was written on a Mac"). Any `code.exe`
    anywhere matches; a renamed portable install does not.
23. **No "clear log" action** — the tray offers only "Show log folder"
    (`tray.rs:824`); PRIVACY.md tells me to delete the file by hand.
24. **Elevated windows fail permanently** (README:20-24). Correct and honest, but
    `windows-handoff-260905.md` confirms the UIPI path "is implemented and has
    never met one" — so the message I would see there is itself untested.
25. **Windows keyboard/tab navigation and the three list editors are entirely
    unverified** — Tier 5 left `Manage…`, tab switching, focus rings and "an edit
    saved on a third open" unticked because posted pointer input could not drive
    the egui viewports.
26. **SUSPECTED: the exclusion list gives no sign of which entries are shipped
    defaults.** If I remove one it is tombstoned forever
    (`prefs_model.rs:22-27`, `exclusion.rs:70-88`), which is right, but I found no
    UI marking. Confirm by reading `prefs/excluded.rs` and
    `platform/windows/settings_ui.rs:1050-1100` for a "default" badge.

---

## The switch test: what I lose on day one moving from EVKey

| I had in EVKey/Unikey | GlowKey |
|---|---|
| Double-click installer, signed | build from source (Windows) / `xattr` incantation (macOS) — README:70-91 |
| Remembers VN/EN across restarts, per app | always launches Vietnamese, global mode — handoff §4, §5 |
| Any hotkey I want, recordable | 3 presets on Windows, no recorder — `settings_spec.rs:200` |
| Works in games; standalone-`w` option | unusable in League, no such option — `windows-handoff-260905.md` |
| On-screen feedback when toggling | tray glyph only on Windows — no `hud` there |
| Backspace undoes keystrokes | undoes characters — handoff §4 |
| Works everywhere by default | editors + terminals pre-excluded silently |
| Auto-update | none |

What is genuinely better — per-app ignore list, the stop-coda tone rule
(`handoff.md` §4, `ukengine.cpp:2352` parity), full-suppression removing the
`hoồng` race — does not survive items 1–6.

---

## What I would fix, in order

1. Scope or kill the Chromium forward-delete (item 1) — it destroys text today.
2. Logging off by default + a Windows privacy statement (item 2).
3. Hook-liveness watchdog on Windows, and correct the lying comment (item 3).
4. Run the League `B` test, then ship the standalone-`w` option or exclude games
   with an explanation (item 4).
5. Stop clobbering an unparseable settings file; add a schema version (item 5).
6. Re-run Tier 1 with EVKey stopped, then align README/handoff claims with what
   was actually observed (items 14, 15).
7. Windows first-run guide and both shortcuts in the tray menu (item 8).
8. Drop editors from the shipped exclusion defaults (item 7).

## Unresolved questions

1. Does the Chromium forward-delete actually eat a character mid-paragraph in
   Chrome? Read of `inject.rs` says yes; nobody has typed it. One 30-second test
   in a Gmail draft settles it — do it before anything else.
2. Is the League failure `w`→`ư` or Vanguard dropping injected input? Still the
   discriminating test in `windows-handoff-260905.md`.
3. What is the 2% idle CPU on Windows actually doing?
4. Does the Windows exclusion UI mark shipped defaults (item 26)?
