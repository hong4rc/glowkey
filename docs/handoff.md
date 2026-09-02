# GlowKey — Session Handoff & Status

Purpose: give a fresh session everything needed to continue GlowKey — the goal,
how it works, what's built, what's broken, and how to build/test/diagnose. Read
this first, then `docs/checkpoint.md` and the `decisions/` records for depth.

---

## 1. What GlowKey is

A **Vietnamese input method for macOS**, in the style of **EVKey/Unikey**. It is a
background menu-bar agent (no Dock icon) that installs a **`CGEventTap`** to *wrap*
the active keyboard layout: the user keeps their Colemak/US layout, and GlowKey
adds Vietnamese on top. It is **not** an InputMethodKit input method and uses **no
marked text / composition** — it writes straight to the document by suppressing
keys and re-emitting synthesized events.

- **All-Rust** via the `objc2` ecosystem (matches the sibling `marau` project).
- Core feature the user cares most about: the **per-app ignore list** (Vietnamese
  off in terminals/editors, on elsewhere), toggled per app and remembered.
- Typing methods: **Telex** (default) and **VNI**.

## 2. Goal (current)

Feature parity with the **useful** parts of Unikey/EVKey, correct typing in
normal text fields, and a polished native-macOS UX. Legacy encodings (TCVN3,
VNI-Windows), VIQR, and clipboard-encoding conversion are **intentionally
omitted** — every modern macOS app is Unicode NFC, so they add no value.

## 3. Architecture

Cargo workspace:
- **`crates/glowkey-engine`** — platform-free Vietnamese logic. Knows nothing
  about macOS; unit-tested on any OS.
- **`app/`** — the macOS binary `GlowKey` (objc2 shell).

### Engine (`crates/glowkey-engine/src/`)
- `lib.rs`: `Engine` keeps the **raw keystroke log** for the current word and
  **re-derives** the whole rendering each key via the `vi` crate (`vi::TELEX` /
  `vi::VNI`), producing a **`KeyResponse { handled, backspaces, insert }`** diff
  (backspaces in **UTF-16 code units**). `Session` wraps `Engine` + all state:
  mode, exclusions, style, auto-fix, auto-capitalize, input method, toggle-hotkey
  preset, macros, open-settings-at-launch, and the recomposition memory.
- `config.rs`: `Settings` (serde JSON) — the persisted subset. Tolerant of missing
  and unknown keys.
- `exclusion.rs`: `ExclusionList` + `DEFAULT_EXCLUSIONS` (terminals/editors).

### Shell (`app/src/`)
- `tap.rs` — the `CGEventTap`. **Full-suppression model**: GlowKey suppresses
  **every** letter it handles and re-emits the diff from a **single tagged
  `CGEventSource`** via `CGEventPost(SessionEventTap)`. This is the crux of
  correctness (see §5). Tags its own events and skips them (feedback-loop guard);
  a latching **circuit breaker** caps runaways. `decide()` is a pure function of
  event + session and is unit-tested with real `CGEvent`s.
- `menu_bar.rs` — `NSStatusItem`, live **VI/EN glyph**, menu (per-app toggle,
  mode, auto-fix, launch-at-login, reset, reveal log, Settings, About, Quit).
- `prefs_window.rs` — Settings window (General / Typing / Excluded apps / Macros)
  plus the separate **Excluded Apps** and **Macros** windows.
- `about_window.rs`, `hud.rs` (toggle flash), `login_item.rs` (SMAppService),
  `app_info.rs` (frontmost app), `settings_store.rs` (file I/O), `log.rs`.

## 4. Features implemented (all committed, test-covered where headless-possible)

- Order-independent Telex tone marks (`hoongf`/`hofong`/`hoonfg` → `hồng`),
  immediate diacritics (`oo`→`ô`).
- **VNI** input method (`viet65`→`việt`) — Settings picker.
- **Per-app exclusions**, independent + remembered; ⌃⇧E to toggle the current app.
- **Auto-fix**: at a boundary, restore raw keys when the result isn't valid
  Vietnamese (`exit`, not `eĩt`).
- **Re-composition**: `hồng`␣⌫`z` → `hông` (deleting the boundary re-opens the word).
- **Caret-navigation flush**: arrows/Home/End/Page flush the diff baseline.
- **Auto-capitalize** first letter of each sentence (opt-in).
- **Configurable toggle hotkey** (⌃⇧Space / ⌃Space / ⌥Space / ⌃⇧Z) **plus a
  recorder**: "Record Custom…" in Settings captures the next ⌃/⌥ combo
  (`HotkeyPreset::Custom`; Esc cancels; ⌘ not allowed).
- **Chromium omnibox guard**: before emitting backspaces in a Chromium browser,
  one AX check (`AXSelectedText` non-empty?) detects the omnibox's
  inline-autocomplete trailing selection and clears it with a forward-delete
  (`app/src/ax.rs`). Normal fields have no selection → provably untouched.
- **Exclusion tombstones**: `removed_default_exclusions` in settings; at load the
  effective list is `saved ∪ (defaults − tombstones)`, so new shipped defaults
  reach old settings files without resurrecting deliberate removals.
- **Session-only terminal un-exclusion**: ⌃⇧E in a known terminal
  (`TERMINAL_EXCLUSIONS`) enables Vietnamese only until restart (HUD shows
  "VI ⚠"); permanent removal only via the Excluded Apps window.
- **Restore common English words** (opt-in, Settings → Typing): a committed word
  whose raw keys are a common English word (embedded list, `english.rs`) is
  restored even when the render is valid Vietnamese (`was`→`was`, not `ứa`).
  Off by default — it inverts the ambiguity for `cats`→`cát`, `car`→`cả`.
- **Macros (gõ tắt)**: `vn `→`Việt Nam `, managed in the Macros window.
- **Launch always in Vietnamese** (mode is session-only, never persisted).
- **Open Settings on launch** (toggle), **Launch at login**, **VI/EN glyph + HUD**,
  **Reveal Log in Finder**, **About** window, **Reset input**.
- **Persistent logging** → `~/Library/Logs/GlowKey/glowkey.log` (see §7).

## 5. Key decisions (the "why")

- **Full suppression, single source.** Mixing native passthrough with synthesized
  backspaces races (a native char and a later synthetic backspace arrive out of
  order in multiprocess apps → `aa`→`aâ`, `hoongf`→`hoồng`). Suppressing every
  letter and emitting all edits from one `CGEventPost` FIFO removes the race by
  construction. This is how EVKey/OpenKey work. See `decisions/` + `tap.rs` header.
- **Mode is session-only.** Persisting the global VN/EN toggle let one accidental
  ⌃⇧Space at quit make the app launch disabled ("aa not work"). Now it always
  launches Vietnamese; only exclusions/auto-fix/style/method/macros/hotkey persist.
- **Blind model.** The engine has no cursor/selection/host-text read-back; its one
  invariant is "rendered == the text tail at the caret." Everything that can move
  the caret (shortcuts, mouse, arrows, app switch) calls `flush()`.

## 6. KNOWN ISSUES / STATUS (updated 2026-09-02, second session)

1. **Chrome/Edge omnibox** — FIX SHIPPED, needs live verification. The guard
   (`tap.rs::emit_edit` + `ax.rs`): when an edit with backspaces is about to land
   in a Chromium browser AND the focused element's `AXSelectedText` is non-empty,
   post one forward-delete to clear the inline-autocomplete selection first. In a
   normal field the selection is empty → nothing posted; ⌦ is also a no-op at
   text end. Scoped by bundle-id prefix (`CHROMIUM_BUNDLE_PREFIXES`). If it
   misbehaves, the log line "OMNIBOX trailing selection detected" shows each fire.
2. **Terminals** — HARDENED. ⌃⇧E in a known terminal (`TERMINAL_EXCLUSIONS`) now
   un-excludes for the session only (HUD "VI ⚠"); restart re-excludes. Shipped
   defaults merge into old settings files at load (tombstones in
   `removed_default_exclusions`), so `org.alacritty` etc. self-heal. Permanent
   removal is still possible, but only via the Excluded Apps window.
3. **English/Telex ambiguity** — MITIGATED by the opt-in "Restore common English
   words" (curated list, `english.rs`). Still inherent in principle: the option
   trades `was`→`ứa` for `cats`→`cát`, hence default off.
4. **All GUI is unverifiable headless** (unchanged) — new controls to eyeball:
   English-restore checkbox, "Record Custom…" + "Current: …" hotkey row, "VI ⚠"
   HUD variant.
5. **Accessibility re-grant after rebuild** — the ad-hoc re-sign drops the grant;
   after `build-app.sh` the relaunched app waits at the permission gate until the
   user re-enables it in System Settings → Privacy & Security → Accessibility.

## 7. Diagnosing from the log (do this first for any reported typing bug)

`~/Library/Logs/GlowKey/glowkey.log` records every handled key:
```
#42 +3.4s KEY Some('o') code=41 app=com.mitchellh.ghostty mode=Vietnamese active=true | Emit bs=1 ins="ô" | raw="hoo" rendered="hô"
```
It shows the **app**, whether Vietnamese was **active**, the **decision**, the
**emitted diff**, and the engine's **raw/rendered**. The emits are almost always
correct — a reported bug is usually (a) wrong app active (terminal/omnibox) or
(b) host-side delivery. `GLOWKEY_DEBUG=1` also echoes to stderr.

## 8. Build / test / run

```bash
cargo test --workspace         # ~49 tests, all green; the headless proof
cargo clippy --workspace --all-targets   # must be 0 warnings
bash scripts/build-app.sh release        # → build/GlowKey.app (universal)
bash scripts/dev-run.sh                  # stop + rebuild + relaunch w/ GLOWKEY_DEBUG=1
```
- **Accessibility permission** required (System Settings → Privacy → Accessibility).
  Granted per bundle path; an ad-hoc re-sign can drop it and re-prompt.
- Does not work in secure/password fields (macOS withholds those events).
- The project lives at `~/project/ai/glowkey` inside the `ai/` container of ~25
  repos (`ai/` is NOT a repo; `ls` prints empty in the sandbox — use glob/find).

## 9. objc2 / Rust gotchas (bit us before)

- **`setReleasedWhenClosed(false)`** is required (and is `unsafe`) or a window
  can't reopen after the user closes it — macOS frees it on close.
- Clippy flags **unnecessary `unsafe`** on objc2 methods that are actually safe
  (setters, `labelWithString`, `activateFileViewerSelectingURLs`, …); only
  `msg_send!` and some class constructors need `unsafe`.
- **VNI digits must extend the word**, not end it — `Engine::is_syllable_char` and
  the tap's word-char test are method-aware (letters always; digits only in VNI).
- **Telex-safe test words**: avoid `w f j s x z r`, double vowels, and `dd` — they
  transform. (`next`→`nẽt`, `good`→`gôd`.) Use `hi man big cat top van go`.
- `cp`/`rm` may be interactive-aliased in this shell; use `cat >` / `command cp`.

## 10. Where the records are

- Plans: `plans/260901-1919-...` (UI/ignore/auto-fix), `plans/260902-1230-...`
  (remaining fixes + deferred omnibox), `plans/260902-1425-...` (Unikey/EVKey copy).
- Decisions: `docs/decisions/000{1,2,3}-*.md`. Checkpoint: `docs/checkpoint.md`.
  UI design: `docs/ui-design.md`.

## 11. Suggested next steps for a new session

1. Re-grant Accessibility (the 2026-09-02 rebuild dropped it — §6.5), then verify
   by eye: the omnibox guard in Chrome (`hoongf`→`hồng` in the address bar), the
   "VI ⚠" HUD on ⌃⇧E in Ghostty, the new Settings controls, hotkey recording.
2. If the omnibox guard proves itself, consider extending it beyond Chromium
   (Safari's address bar has the same autocomplete pattern) — kept narrow first.
3. Everything in §6 is otherwise shipped; plan record:
   `plans/260902-1515-fix-known-issues/plan.md`.
