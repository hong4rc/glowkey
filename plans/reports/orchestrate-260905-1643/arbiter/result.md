# Arbiter — adversarial verification and ranking of the six audits

`main` @ `ce51ff0`, 2026-09-05. Read-only. Every verdict below was reached by opening the cited line myself, running a read-only command, or fetching the primary source. Where I could not (macOS runtime, hardware, live apps) the verdict says so.

Legend: **CONFIRMED** / **PARTLY-TRUE** / **REFUTED** / **UNVERIFIABLE-HEADLESS**. "ARBITER-FOUND" = surfaced by verification, not raised by any report.

---

## 1. P0 verdicts

| # | Report → claim | Verdict | Evidence seen |
|---|---|---|---|
| 1a | **arch P0-1** — macOS tap callback does a mutex-guarded flushed file write per keystroke, violating 0008 | **CONFIRMED (fact)**, severity downgraded to **P1** | `dispatch.rs:105` `Notice::Decided → crate::log::log(...)` runs inside `notify`, inside the tap callback. `emit.rs:97` a second write per emit. `log.rs:145-155`: `mutex.lock()` → `write_all` → `flush()` → `rotate()` (rename + reopen) — all synchronous. `0008` text: "file I/O, and any lock that a non-tap thread can hold" forbidden, no macOS carve-out. **But** the code treats it as deliberate: `dispatch.rs:96-101` wants the KEY line "on disk already" before a panicking emit; `hook_log.rs:8-9` explicitly knows macOS is affected and did not port. Consequence is bounded on macOS (health.rs re-enables a timed-out tap; Windows loses the hook permanently), hence P1 not P0. |
| 1a′ | arch P0-1 — `hook_log.rs` is a drop-in | **PARTLY-TRUE** | No Win32 imports (verified: `std::sync::*`, `crate::log` only). Not literally drop-in: `hook_log::log(String)` vs `log::log(&str)` → ~25 macOS call sites change (`&format!` → `format!`, literals → `.into()`), and the `dispatch.rs:96-101` "on disk already" comment becomes false (arch acknowledges). |
| 1a″ | **ARBITER-FOUND** | — | macOS also performs the **settings file write** inside the callback on hotkey/per-app-toggle keys: `dispatch.rs:193,200 → save_settings() → settings_store::save` (copy .bak + write + rename). `emit.rs:60-66` comment admits it. Windows defers this to the message loop (`hook.rs:279-282`). Larger than the log write; same fix class. |
| 1b | **production P1-3** — `.bak` overwritten with the corrupt file | **CONFIRMED**, and worse on macOS | `settings_store.rs:38` `from_json` (`prefs_model.rs:123` `unwrap_or_default`) discards parse status; `save():61-63` copies the on-disk file to `.bak` unconditionally. **ARBITER-FOUND aggravation:** a corrupt file yields `welcome_shown=false` → macOS shows the welcome and calls `save_settings()` at launch (`macos/mod.rs:478-481`) — `.bak` is destroyed **with zero user action**. On Windows a no-change close does not save (`shell.rs:205` early return), but `open_settings_at_launch: true` (`prefs_model.rs:106`) invites the first edit. **blame-user P0-5's mechanism is wrong** ("the next save also overwrites once the defaults have been saved twice") — the *first* save does it. |
| 1c | **blame-user P0-3** — no Windows hook liveness path; tray says VI forever | **CONFIRMED** | `grep SetTimer\|WM_TIMER\|thread::spawn` in `platform/windows/` → only `hook_log.rs:53` and `ui_thread.rs` spawns. `HOOK` is zeroed only by `uninstall()` (`hook.rs:137`). `indicator::state` returns `HookGone` only when `!installed` (`indicator.rs:74-75`); `shell.rs:47` passes `hook::is_installed()` raw. Comment at `hook.rs:182-184` ("the indicator pairs it with a liveness check") is false. `FIRST_CALL` (`:336`) is a one-shot log flag, not a counter. Windows has no analogue of `macos/health.rs`. |
| 1d | **blame P0-2 / prod P0-2 / sec B1-B3** — unconditional plaintext keystroke log | **CONFIRMED on all sub-claims** | *What:* Windows `hook.rs:498-503` `KEY {ch:?} vk= mods= app= \| {decision}` where `Decision` Display is `Emit bs= ins="…"` (`platform.rs:187`); `Corrected {was}->{becomes}` at `:506`. macOS `dispatch.rs:276-279` adds `raw=… rendered=…` (whole composing word). *When:* `platform.rs:118-122` fires `Notice::Decided` for every key-down that reaches `handle`, whatever the decision (excluded apps included — PRIVACY.md:35 admits). Windows filters only own-injected events and key-ups (`hook.rs:365-377`). *Gate:* `log.rs:134-137` — only `cfg!(test)`; `GLOWKEY_DEBUG` (`:166-168`) gates stderr echo only. *Perms:* `log.rs:95-99` `OpenOptions::create().append()`; no `set_permissions` anywhere. *Secure fields:* macOS claim (`mod.rs:31`, PRIVACY.md:36) has no Windows counterpart in code. *PRIVACY.md:* macOS-only throughout (read in full). *Roaming (sec B2):* `paths.rs:17-18` settings in `FOLDERID_RoamingAppData`, comment says "roam with the user's profile" — macros verbatim in a roaming file. |
| 1e | **sec B4** clipboard NUL scan; **Chromium guard** | **CONFIRMED** (coordinator-established; re-read `clipboard.rs:97-104`, `inject.rs:220-222`) | Note on the guard: `inject.rs:96-99`'s excuse ("reaching that state requires moving the caret without flushing") is wrong — a click flushes *and* leaves the caret mid-field; the next `bs>0` edit fires `VK_DELETE` into real text. **crossplat §1c lists the guard as a "deliberate non-gap … user-confirmed working in Edge" — REFUTED as a non-gap.** `windows-verification:231` confirms only the omnibox case; nobody typed mid-paragraph. |
| — | **blame P0-4** League unusable, no standalone-`w` option | **UNVERIFIABLE-HEADLESS** | `windows-handoff:130-165` consistent; cause (a)/(b) undetermined. Handoff says the exclusion is "a workaround, not a decision". |
| — | **blame P0-6 / prod P0-1** macOS release ad-hoc signed; README says "signed" | **CONFIRMED** | `build-app.sh:133` `codesign -s - … \|\| true` when no identity; `release.yml:35-38` comment admits ad-hoc, no cert import; `0006:69` "CI signs ad-hoc". README:69-71 "signed but not notarized" misleads (ad-hoc = no identity). TCC-keyed-to-cdhash: `0006:10-11`. |
| — | **prod P0-3** no Windows artifact, self-declared unverified | **CONFIRMED** | README:84-87; `release.yml:16` one macOS job. Production correctly endorses the gating. |
| — | **ui P0-1** Windows excluded-app add is a free-text exe name | **CONFIRMED**, severity → **P1** | `settings_ui.rs:1022-1026` `TextEdit::singleline`, `normalize_exe_name`. Works; hostile UX, not a blocker. |
| — | **ui P0-2** no HUD on Windows | **CONFIRMED** | `grep -i hud platform/windows/` → nothing. |
| — | **ui P0-3** no Windows welcome/Quick Guide | **CONFIRMED** | `welcome` appears in `platform/windows/` only as the merged field `shell.rs:343-346`. |
| — | **ui P0-4** Windows settings apply only on close; doc says live | **CONFIRMED**, severity → **P1** | `settings_ui.rs:1397-1399` `finalize()` only on `close_requested`; only `Language` applies live (`:888`). `ui-design.md:111` "every change applies live". However `shell.rs:203-245` shows close-time three-way merge is a *designed* contract (hook keeps running; ⌃⇧W-taught words preserved). Doc/renderer contradiction, not a bug in the merge. |
| — | **ui P0-5** Windows tray never states current state | **CONFIRMED** | `tray.rs:722` first item is `"GlowKey"`; `describe()` used only for `Indicator::Broken` (`:738`). |

Other spot checks: **arch "379 tests pass"** → PARTLY-TRUE: my `cargo test --workspace` = **22 targets, 331 passed, 0 failed, 0 ignored** (count differs, all green). **arch "hook.rs zero tests"** → CONFIRMED (`grep -c '#\[test\]'` = 0). **arch P1-6** three `eprintln!` in `settings_store.rs:54,66,70` → CONFIRMED. **sec B5** no control-char filter in `parse_table` (`macros.rs:100-101`) → CONFIRMED. **sec B7** bare `Command::new("explorer.exe")` (`shell.rs:148`) → PARTLY-TRUE: Rust std has not searched the CWD on Windows since 1.58, so the "run from Downloads" CWD vector is REFUTED; the *application directory* is still searched, so a planted `explorer.exe` beside `GlowKey.exe` (zip extraction) remains valid (medium confidence — verify against std docs before citing). **sec B8** `SystemRoot` env for fonts (`settings_ui.rs:133`) → CONFIRMED. **crossplat 1a "all seven Control variants rendered on both sides"** → CONFIRMED (`settings_ui.rs:872-961`, `prefs/tabs.rs:159-273` each cover all 7 of `settings_spec.rs:194-209`).

---

## 2. Contradictions adjudicated

| Topic | Positions | Ruling |
|---|---|---|
| **macOS "at parity"** | crossplat: no, compile-checked only. ui-polish: 6/10/18 rhythm etc. match (VERIFIED). README:26: "feature-complete". handoff §11: "compile-checked only and has not been run" (`handoff.md:702-703`). | All compatible once "parity" is split: **code-read parity of layout constants is real** (ui-polish read both renderers); **runtime parity is unestablished** (never run since the engine split). README's "feature-complete" is unbacked by a run → blame P1-14 stands. Any macOS-touching change (esp. B2 below) is unverifiable until the M1 pass. |
| **Windows renders all settings_spec Control variants** | crossplat yes; ui-polish/blame imply gaps. | **crossplat is right.** The gaps (no recorder, text-field exclusion add, Alt+Space dropped at `settings_ui.rs:257-266`) are *behavioural inside* a rendered arm, not missing arms. |
| **ttf-parser severity** | security: informational/unmaintained only. | **CONFIRMED** by `cargo-audit.txt` and the RustSec page (fetched: no CVE, no fix, suggests `skrifa`). No report inflated it. production's "no supply-chain scanning" is compatible. |
| **Chromium guard** | blame P0-1 (destroys text) vs crossplat §1c (non-gap, confirmed in Edge) vs arch §9.9 (do not refactor). | blame is right. crossplat conflated "omnibox case works" with "guard is correct". arch §9.9 refers to the **macOS AX-gated** guard (`emit.rs:70-86`, 0003) — it must not be read as protecting the Windows unconditional one. |
| **`.bak` mechanism** | blame P0-5 (second save) vs production P1-3 (first save). | production right on mechanism; blame right that it deserves high rank given the macOS launch-time auto-save (ARBITER-FOUND). |
| **Log write in callback: violation or design?** | arch: 0008 violation. Code comments (`dispatch.rs:96-101`, `hook_log.rs:8-9`): deliberate. | 0008 text is unambiguous and has no carve-out; the comments predate/ignore it. Violation confirmed; consequence bounded (P1). |
| **Menu item name** | sec B1 "Reveal Log" at `tray.rs:823`; arch "Show log folder". | Same item, `tray.rs:824` says "Show log folder". Trivial. |

---

## 3. External / unsupported claims

| Claim | Source report | Arbiter status |
|---|---|---|
| Homebrew removes casks failing Gatekeeper on **2026-09-01**; `--no-quarantine` dropped in 4.7 | production P0-1 | **VERIFIED** ([Homebrew/brew#20755](https://github.com/Homebrew/brew/issues/20755); ~387 casks deprecated). Plan-changing: Homebrew route is closed without notarization. |
| Azure Artifact Signing $9.99/mo; individuals eligible; restricted to "US/CA/EU/UK entities" | production P0-3 | **PARTLY-TRUE.** Price verified ([pricing](https://azure.microsoft.com/en-us/pricing/details/artifact-signing/)). Eligibility is **stricter** than stated: individuals **US/CA only**; organisations US/CA/EU/UK/AU/NZ/JP/KR/SG/CH/NO/IL ([Microsoft Learn](https://learn.microsoft.com/en-us/azure/artifact-signing/overview)). Vietnam appears in neither → production's "likely ineligible" conclusion holds, stronger. |
| RUSTSEC-2026-0192 ttf-parser unmaintained, no fix | security A.1 | **VERIFIED** (fetched advisory). |
| Ad-hoc TCC keyed to cdhash → re-grant every release | production P0-1 | **VERIFIED** from repo's own `0006:10-11`. |
| Gatekeeper shows "damaged" for ad-hoc on macOS 26; "Open Anyway" unreliable | production P0-1 | Consistent with the project's own observed text (README:69-70, `release.yml:66`); macOS-26-specific behaviour and "Open Anyway unreliable" are **inference, not reproduced** (production says so itself). |
| Sequoia removed Control-click Open bypass | production P0-1 | Cited (AppleInsider/MacRumors, Aug 2024); widely reported; **not re-verified by arbiter**. |
| IBus 1.5.27 `focus_in_id` client names (`gtk3-im:firefox`) | crossplat §3.2 | Cited (Fedora change page); **unverified by arbiter**. Load-bearing for the whole Linux architecture — verify before P1/L1. |
| Mutter 50.4 / KWin 6.6 do not implement `ext-foreign-toplevel-list-v1` | crossplat §3.4 | Cited (wayland.app); **unverified**. |
| X11 `XGrabKeyboard` + XTEST self-feeding | crossplat §3.2 | Cited (blog); **unverified**. |
| ibus-bamboo six output modes | crossplat §3.2 | Cited (README); **unverified**. |
| OV cert $400–900/yr; winget-pkgs#385483 SmartScreen blocker; MSIX would fail Store certification | production P0-3 | **Cited but unverified**; MSIX claim is inference. |
| `Ctrl+Shift+E` is Chrome's "search with…" | blame P1-8 | **Unverified**, low impact. |
| Rust `Command` does not search CWD on Windows (≥1.58) | **arbiter** | Medium confidence; stated so B7's remediation is scoped correctly. |

---

## 4. Deduplicated ranked backlog

Effort S <½d, M ½–3d, L >3d. "Now?" = code-fixable this session without a human/hardware/product decision.

| ID | Title | Sev | Verdict | Evidence | Raised by | Effort | Fix risk | Now? |
|---|---|---|---|---|---|---|---|---|
| **A1** | Windows Chromium forward-delete fires on every `bs>0` edit in any Chromium app, no selection check | **P0** | CONFIRMED | `inject.rs:96-99,220-222` | blame P0-1; crossplat §1c (wrongly non-gap); coordinator | S (disable) / M (focus-cached guard via `EVENT_OBJECT_FOCUS` in `foreground.rs`) | Disabling regresses omnibox `hoồng`; scoping needs HW | **Decision** (interim), then plan |
| **A2** | Plaintext keystroke log always on, no toggle, default perms, no delete action; PRIVACY.md macOS-only; Windows settings roam | **P0** | CONFIRMED | `log.rs:134-155`, `hook.rs:498-506`, `dispatch.rs:276-279`, `paths.rs:17-18`, PRIVACY.md | blame P0-2; prod P0-2; sec B1/B2/B3 | S (docs + Delete-log) + M (opt-in/redaction, both UIs) | Low | Docs/Delete-log **now**; toggle needs decision (maintainer debugging need) |
| **A3** | No Windows hook-liveness watchdog; `HookGone` unreachable for the dead-hook case; comment over-claims | **P0** | CONFIRMED | `hook.rs:137,182-186`, `shell.rs:47`, `indicator.rs:74` | blame P0-3 | S (fix comment) / M (timer: `GetLastInputInfo` vs last-callback tick → `HookGone` → reinstall) | Medium — false positive = reinstall loop; needs dead-hook repro | Comment **now**; watchdog plan + HW |
| **A4** | macOS release ad-hoc signed; README "signed" wording wrong; TCC re-grant every update; Homebrew closed | **P0** (release) | CONFIRMED | `build-app.sh:133`, `release.yml:35-38,66`, README:69-71, `0006:69` | prod P0-1; blame P0-6 | S wording; M CI (Developer ID + notarytool + hardened runtime, prod P2-2) | Low | Wording **now**; rest = **$99 user decision** + Mac |
| **A5** | No Windows artifact; port self-declared unverified; signing geo-blocked for a VN individual | **P0** (release) | CONFIRMED | README:84-91; `release.yml:16` | prod P0-3; blame P0-6 | L (Tier 2 verification) + M (package) | — | **Blocked**: HW + signing decision |
| **B1** | `.bak` overwritten with corrupt file on first save; macOS saves at launch automatically; no schema version; silent reset | P1 (high) | CONFIRMED + ARBITER-FOUND | `settings_store.rs:38,61-63`, `macos/mod.rs:478-481`, `prefs_model.rs:123` | prod P1-3; blame P0-5 | S (skip `.bak` copy when load failed / write `.corrupt-<ts>`; fsync) | Low | **Now** |
| **B2** | macOS tap callback: synchronous flushed log write per key + settings file write on hotkey keys (0008 violation) | P1 | CONFIRMED + ARBITER-FOUND | `dispatch.rs:105,193,200`, `emit.rs:97`, `log.rs:145-155` | arch P0-1 | S–M (move `hook_log.rs` → `app/src/log_queue.rs`, ~25 call sites; defer save to run loop as Windows does) | Low–medium; **unverifiable until Mac run** | Code now, verify on Mac |
| **B3** | League unusable; cause (a) `w→ư` vs (b) Vanguard drops injected input | P1 | UNVERIFIABLE | `windows-handoff:130-165` | blame P0-4; crossplat §2 | 0.25d test; feature TBD | — | **Blocked HW** (user presses `B`) |
| **B4** | Windows settings apply only on close; `ui-design.md:111` promises live | P1 | CONFIRMED | `settings_ui.rs:1397-1399,888`; `shell.rs:203-245` | ui P0-4 | M | Medium (touches merge contract) | **Decision**: live-apply vs Apply button + doc exception |
| **B5** | No HUD on Windows | P1 | CONFIRMED | grep | ui P0-2; blame P1-10; crossplat W5 | M | Low | Plan |
| **B6** | No Windows welcome/Quick Guide; ⌃⇧E/⌃⇧W undiscoverable; ⌃⇧W in one caption only | P1 | CONFIRMED | `shell.rs:343-346`; `settings_spec.rs:327-335,399-400` | ui P0-3/P1-10/P1-11; blame P1-8; crossplat W7 | M | Low | Plan |
| **B7** | Windows tray: no state header, mode item lacks hotkey, no Reset input; menus drifted | P1 | CONFIRMED | `tray.rs:722,738`; `menu_bar.rs:230-396` | ui P0-5/P1-6/P2-6; crossplat W6/W8/W9; arch P1-3 | S–M | Low | Header **now** (`describe()` exists); rest plan |
| **B8** | Excluded-app add is free-text exe; `default_exclusions/windows.rs` unverified | P1 | CONFIRMED | `settings_ui.rs:1022-1026`; `windows.rs:8-11` | ui P0-1; blame P2-22; crossplat W10 | M–L | Low | **Decision** (picker scope) + HW for table |
| **B9** | No panic hook; stderr goes nowhere on Windows; `settings_store` uses `eprintln!` | P1 | CONFIRMED | `main.rs:27`; `settings_store.rs:54,66,70` | prod P1-2; arch P1-6 | S | Low | **Now** |
| **B10** | No Windows hotkey recorder; Mac-recorded hotkey char-matches; Alt+Space dropped silently | P1 | CONFIRMED | `hook.rs:416-431`; `settings_ui.rs:257-266` | blame P1-9; crossplat W1/W2; ui P2-4 | M–L (`hook.rs` + `settings_ui.rs` + `hotkey.rs`) | Medium (recorder sits in keystroke path) | Plan (after A3; caption **now**) |
| **B11** | Clipboard NUL scan unbounded | P1 | CONFIRMED | `clipboard.rs:97-104` | sec B4; coordinator | S (`GlobalSize` bound) | Low | **Now** |
| **B12** | Macro expansions accept `\r\n`/C0; Windows import uncapped; import replaces silently, no backup | P1 | CONFIRMED | `macros.rs:100-101`; `settings_ui.rs:1208-1215` | sec B5; blame P1-17; ui P1-14; arch P1-5 | S (shared validator in session crate) | Low | **Now** (filter + cap); UX later |
| **B13** | Release/supply-chain hygiene: no `cargo-audit` in CI, no checksums/provenance, floating toolchain, no SECURITY.md/CHANGELOG/issue template, no update channel | P1 | CONFIRMED | `.github/` = workflows only; `release.yml:19,92-95` | prod P1-1/P1-4/P1-5; sec C3 | S each | Low | audit.toml + CI job + checksums + SECURITY.md **now**; rest plan |
| **B14** | Editors shipped as default exclusions | P1 | CONFIRMED | `windows.rs:39-47`, `macos.rs:23-36` | blame P1-7 | S | Product | **Decision** |
| **B15** | Mode not persisted; Backspace deletes chars not keystrokes; English-restore trade-off | P1 | CONFIRMED as *decisions* | handoff §4/§5/§6.3 | blame P1-11/12/13 | M each | Reverses verified decisions | **Decision**; default: no change without new evidence |
| **B16** | macOS never run since engine split; Windows Tier 1 contaminated by EVKey | P1 | CONFIRMED | `handoff.md:702-703`; `windows-verification:10-35` | blame P1-14/15; crossplat M1/W4 | 1d + 1d | — | **Blocked HW** |
| **B17** | Idle ≈2% CPU / ~101 MB on Windows, unexplained | P1 | CONFIRMED recorded | `windows-verification:341` | blame P1-16 | M investigate | — | **Blocked HW** |
| **B18** | Duplication: toggle binding ×4 (macOS), indicator ×2, menu list ×2, validation ×3; `hook.rs` untested | P1 | CONFIRMED | arch §2 citations | arch P1-1..P1-5 | M each | Medium (macOS side unverifiable) | Plan |
| **B19** | Windows drops `PersonalWordsChanged`; no session-only-terminal warning state | P1 | CONFIRMED | `hook.rs:518-519`; `indicator.rs:29-45` | crossplat W3/W4 | S–M | Low | Plan (hook.rs contention) |
| **B20** | Spec copy: `khôi phục` ×2, `Kiểu gõ/Kiểu gõ`, `Gõ tắt` collision, AutoFix named 3 ways, orthography mix | P2 | CONFIRMED | `settings_spec.rs:341-343,372-374,390-391,405,417-420` | ui P1-1..P1-4/P1-13/P2-1..3; blame P2-18/19 | S (one file, headless tests) | Low | **Now** |
| **B21** | macOS: Settings not scrollable/resizable; About lacks icon/Copy; Add App accepts any file | P2 | CONFIRMED (absence) | `prefs/tabs.rs:59-61,93`; `about_window.rs`; `prefs/mod.rs:279-301` | ui P1-5/P1-12/P1-15 | S–M | Needs Mac | Code now, verify HW |
| **B22** | `open_settings_at_launch` defaults `true` | P2 | CONFIRMED | `prefs_model.rs:106` | ui P1-7; blame P2-21 | S | Product | **Decision** (small) |
| **B23** | Bare `explorer.exe`; `SystemRoot` env for font path | P2 | PARTLY-TRUE / CONFIRMED | `shell.rs:148`; `settings_ui.rs:133` | sec B7/B8 | S | Low | **Now** |
| **B24** | Single-instance mutex squat exits silently | P2 | CONFIRMED | `single_instance.rs:31,48` | sec B6 | S (log line) | Low | **Now** |
| **B25** | THIRD-PARTY-NOTICES stale; egui bundled-font OFL uncovered; Windows ships none | P2 | Not re-verified (licence content) | `Cargo.lock` 391 pkgs | prod P2-1 | S–M (`cargo about`) | Low | Plan |
| **B26** | Linux port (IBus engine, identity providers, packaging, CI) | P2 (scope) | Architecture plausible; key facts unverified (§3) | crossplat §3–6 | crossplat | L (~20 d) | High; forces `0.2.0` on published crates | **Decision** + Linux HW |

---

## 5. Sequencing

**File contention (verified against the backlog):** `platform/windows/hook.rs` — A3, B10, B19, arch P1-4 tests. `platform/windows/settings_ui.rs` — B4, B8, B10, B20-adjacent captions, arch P2-1, crossplat L3. `settings_store.rs` — B1, B9, (Linux XDG arm). `tray.rs` — B7 only. macOS files — B2, B21, arch P1-1/P1-2: **all unverifiable until B16's Mac pass**.

### (i) Fix now — low risk, this session, no contention
1. **B1** `.bak` guard (`settings_store.rs`): have `load()` report parse failure; skip the copy (or write `.corrupt-<ts>`); `sync_all` before rename. ~1h. Test: corrupt fixture → `.bak` untouched.
2. **B11** `GlobalSize` bound in `clipboard.rs:98-104`. ~20 min.
3. **B9** `std::panic::set_hook` → `crate::log` in both `run()`s; `settings_store` `eprintln!` → `log`. ~1h. (Same file as B1 — do together.)
4. **B12** control-char reject + expansion length cap in `Macro::parse_table`/one shared validator; 4 MB cap on the Windows import box. ~2h.
5. **B23/B24** absolute `explorer.exe`, `GetSystemWindowsDirectoryW` for fonts, log the mutex refusal. ~1h.
6. **B13-lite** `.cargo/audit.toml` ignoring the two IDs with justification + `cargo audit` CI job; `shasum` on the DMG; `SECURITY.md`. ~2h.
7. **A2-lite** PRIVACY.md platform-neutral rewrite (both log paths, both settings paths, roaming note, no secure-field exemption on Windows); "Delete log" tray/menu item; README:71 "signed" → "ad-hoc signed" (**A4-lite**). ~2h.
8. **A3-lite** fix the lying comment at `hook.rs:182-184` (say "no liveness check exists yet").
9. **B7-lite** replace `tray.rs:722` "GlowKey" with `indicator.describe(app)` as the disabled header.
10. **B20** spec copy pass in `settings_spec.rs` + a "row label ≠ section title" test.
11. **B10-lite** caption on the Windows hotkey row saying there is no recorder.

### (ii) Needs a plan phase
- **A2-full** opt-in "diagnostic typing log" toggle, redaction by default, retention; both settings UIs. Gate on decision (iii).
- **A3** liveness watchdog (`hook.rs`, message-loop timer). **First** among hook.rs touchers. Pair with arch P1-4 tests before B10/B19.
- **B2** shared `log_queue` + defer macOS settings write to the run loop. Small code; **cannot be signed off until B16**.
- **Windows shell parity** in this order to respect contention: B4 (apply model — changes the contract others build on) → B8 → B10 (after A3) → B19 → B5/B6 (new files, parallel-safe) → arch P2-1 split. crossplat W1/W2 serialisation stands.
- **B18** refactors: arch P1-2 (shared indicator) is the one with cross-platform correctness value (0007); P1-1/P1-3/P1-5 are maintenance; all macOS-side effects wait on B16.
- **B13-rest** release pipeline (provenance, pinned toolchain, Windows job gated on A5), **B25** notices.
- **A1-full** focus-cached omnibox detection off the hot path (extend `foreground.rs` WinEvent hook with `EVENT_OBJECT_FOCUS`); needs HW to learn the omnibox's UIA identity.
- **B26** Linux — only after (iii).

### (iii) Needs the user to decide (recommend a default each)
| Fork | Default recommendation | What flips it |
|---|---|---|
| **A1 interim**: keep guard (omnibox `hoồng`, visible) vs disable (silent mid-text deletion in page bodies) | **Disable** the unconditional guard now; re-enable behind focus detection. Silent deletion is worse than a visible mis-render. | Gmail/Facebook mid-paragraph test shows no deletion in practice. |
| **A2**: is full-text logging load-bearing for the maintainer? | Ship redacted-by-default + opt-in verbose with auto-expiry. | Maintainer cannot debug without `raw=/rendered=` — then keep verbose but still off by default. |
| **A4**: $99/yr Apple Developer Program | Yes, if macOS distribution is a goal — every GUI route now depends on it (Homebrew verified closed). | Project stays developer-audience only. |
| **A5**: Windows signing — VN individual is outside Azure eligibility | Defer packaging (as production endorses); when ready, OV cert or a US/CA/EU entity. | Owner has an eligible entity. |
| **B4**: live-apply vs explicit Apply + doc exception | Live-apply per edit through the existing merge; keep `finalize` as safety net. | Session-ownership makes per-edit merge unsafe → Apply button. |
| **B14** editors in defaults; **B22** settings window at launch | Drop editors; default `false` once B6 exists. | Telemetry-free, so: user preference. |
| **B15** mode persistence / Backspace semantics / English restore | **No change** — verified decisions (handoff §5, "reaffirmed twice"); revisit only with new evidence per review rules. | Repeated live reports. |
| **B8** picker scope | Running-process picker + free-text escape hatch. | Time budget → `IFileOpenDialog` only. |
| arch Qs: empty macro expansion; duplicate-macro UX; `Session::backspace` fate | Reject empty; confirm-before-replace on both; keep `Session::backspace` documented until 0.2.0. | — |
| **B26** Linux scope and `0.2.0` timing | Not until A1–A5 + B16 close. | Linux users are the target. |

### (iv) Blocked on hardware / a human at the machine
- **B3** League `B` test (crossplat §2 procedure) — decides standalone-`w`.
- **B16** macOS runtime pass (`handoff §11.1`) — **gates** B2, B21, arch P1-1/P1-2, and any README "feature-complete" claim. Windows Tier 1 re-run with EVKey stopped; Tier 2 (Chrome incl. **mid-paragraph Gmail test for A1**, Terminal, VS Code, Electron).
- **A3** dead-hook reproduction (a deliberately slow callback build) to validate the watchdog.
- **B8/W3** exclusion table against real process names.
- **B17** idle CPU profile.
- **A4** confirm Gatekeeper text on macOS 26 before rewriting install docs.
- **B26** L6 Linux sessions.

---

## 6. Arbiter checklist

| Question | Answer |
|---|---|
| Did every job produce its artifact? | **Yes** — six `result.md` files; security-cve also has `cargo-audit.txt/.json` (0 vulnerabilities, 2 unmaintained: `paste`, `ttf-parser`). |
| Did any fail, time out, or express uncertainty? | No failures/timeouts evident. Uncertainty explicitly stated by: production (tests not run; "damaged" on macOS 26 inferred), ui-polish (SUSPECTED labels, 5 items), crossplat (§9 ten unknowables), blame-user (item 26 SUSPECTED), security (macOS objc2 bodies not read line-by-line). arch expressed none; its test count (379) does not match my run (331 pass, 0 fail). |
| Do outputs contradict? | Yes, six places (§2). Material ones: crossplat calling the Windows Chromium guard a non-gap (REFUTED); blame P0-5's `.bak` mechanism (wrong, production right); "parity" meaning (resolved by splitting code-read vs runtime). |
| Were claims supported by paths/output/citations? | Overwhelmingly yes, path:line throughout; every headline I opened was where it was said to be. Inference-as-fact: crossplat §1c; blame P0-5 mechanism; arch "drop-in"; production Azure geo (too generous); ui-polish P0 severities inflated on P0-1/P0-4; sec B7 CWD vector. |
| Are unresolved questions listed? | Yes, in all six; consolidated below. |

---

## 7. Unresolved questions (consolidated, deduplicated)

1. Does the Windows Chromium forward-delete eat a character mid-paragraph in Chrome/Gmail? (blame Q1) — one 30-second HW test; gates A1's interim decision.
2. League: `w→ư` or Vanguard dropping injected input? (blame Q2, crossplat Q1) — HW.
3. Is full-text `raw=/rendered=` logging load-bearing for the maintainer's debugging? (production Q3) — gates A2's default.
4. $99 Apple Developer Program acceptable? (production Q1) — gates A4.
5. Is the owner an Azure-eligible entity? Individuals are US/CA only. (production Q2, corrected) — gates A5's cheap route.
6. Windows live-apply feasible under the session-ownership model, or Apply button + doc exception? (ui Q1) — gates B4.
7. Picker scope for excluded apps on Windows (ui Q2) — gates B8.
8. Empty macro expansion allowed? Duplicate-macro UX one behaviour? (arch Q1/Q3, ui Q5) — gates B12 shared validator.
9. `Session::backspace`: public API or artifact? (arch Q4) — gates a 0.2.0 removal.
10. What is the 2% idle CPU doing? (blame Q3) — HW.
11. Does the Windows exclusion UI mark shipped defaults? (blame Q4) — ui-polish P2-9 says the tombstone section exists at `settings_ui.rs:1064-1097`; row badging not confirmed by anyone.
12. Linux: ship at all; preedit mode acceptable; `0.2.0` timing for three crates; tray vs IBus properties (crossplat Q1–Q5) — product.
13. Are the IBus `focus_in_id` client-name and compositor-protocol claims true for the apps that matter? — unverified by arbiter and unknowable without a Linux machine (crossplat §9).
14. Does Gatekeeper on macOS 26 actually show "damaged" for the ad-hoc DMG? (production, self-flagged) — HW.
