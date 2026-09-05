# GlowKey — production-readiness audit (2026-09-05)

Read-only audit of `main` @ `ce51ff0`. Every repo claim below is quoted with `path:line`;
platform facts are cited to URLs.

## What "production" means for this app

GlowKey is not a normal app. It is a **background agent that observes every keystroke on
the machine**, has no window most of the time, and is structurally indistinguishable from
a keylogger. So the bar is not "does it work":

1. **Installable by a stranger without a terminal.** A user who must paste `xattr -dr` to
   run a keyboard hook has been trained to bypass exactly the check that would protect
   them from a real keylogger. Teaching that gesture is a security harm, not just friction.
2. **Trustworthy under inspection.** Every privacy claim must be true on *both* platforms,
   checkable, and must match what is actually on disk. A claim that is right on macOS and
   silent on Windows is a false claim on Windows.
3. **Fails safe and fails visibly.** The user's typing must survive any GlowKey failure,
   and the user must be able to tell that GlowKey stopped.
4. **Diagnosable without a live repro** — version, commit, log — which the project already
   does well.
5. **Durable.** Settings and macros a user spent time on must survive crashes, corruption
   and version changes.

Current state: (3) and (4) are genuinely strong. (1) is blocked on both platforms. (2) is
false on Windows. (5) has one concrete data-loss path.

---

## P0 — blocks public release

### P0-1. The macOS release is ad-hoc signed, and CI guarantees it

**Evidence.**
- `scripts/build-app.sh:126-135` — resolves `GlowKey Developer` from the keychain; if
  absent, falls back to `codesign --force --deep -s -` (ad-hoc) with `|| true`.
- `.github/workflows/release.yml:39` runs `bash scripts/build-app.sh release release` on a
  clean `macos-latest` runner. No certificate is imported anywhere in the workflow →
  **every published DMG is ad-hoc signed**, not self-signed. The decision record
  (`docs/decisions/0006`) describes a *developer machine* configuration that CI never has.
- `README.md:71-72` and `release.yml:65-66` say "the app is signed but not notarized".
  For the shipped artifact that is not accurate: ad-hoc means *no identity at all*.

**User impact.**
- Gatekeeper reports **"GlowKey is damaged and can't be opened"** for a quarantined
  ad-hoc/self-signed bundle — a harsher path than the "unidentified developer" dialog,
  and the one where **"Open Anyway" frequently is not offered**
  ([eclecticlight](https://eclecticlight.co/2020/10/30/code-signatures-2-how-to-check-them/),
  [Gatekeeper writeup](https://blog.margrop.net/en/post/macos-gatekeeper-unsigned-app-fix/)).
- macOS Sequoia (15) **removed the Control-click → Open bypass**; the only in-GUI route is
  System Settings → Privacy & Security → Open Anyway, which for the "damaged" case is
  unreliable ([AppleInsider](https://appleinsider.com/articles/24/08/06/apple-removes-control-click-option-for-skipping-gatekeeper-in-macos-sequoia),
  [MacRumors](https://www.macrumors.com/2024/08/06/macos-sequoia-gatekeeper-security-change/)).
  So the terminal command in `README.md:76` is not a convenience — it is the only route.
- **Ad-hoc keys TCC to the cdhash** (`docs/decisions/0006`, Context). Every release changes
  the cdhash → **the user must re-grant Accessibility on every single update**, with no
  warning and the app silently sitting at the permission gate. This is the one consequence
  0006 explicitly set out to eliminate on dev machines and the release path still has.

**Remediation, ranked.**

| Option | Cost | What it buys | Verdict |
|---|---|---|---|
| Apple Developer ID + notarize + staple, hardened runtime | $99/yr + ~2h of CI work (`notarytool`, secrets, `codesign --options runtime`) | Double-click install, no terminal, stable TCC identity across releases, Homebrew cask eligible | **Recommended.** The only option that meets bar (1) |
| Import a stable self-signed cert into CI as a secret | ~1h, $0 | Fixes the per-release re-grant; **does not** fix "damaged" | Partial — do this if $99 is refused |
| Homebrew cask | — | — | **No longer available.** Homebrew is removing all casks failing Gatekeeper checks on **2026-09-01** and dropped `--no-quarantine` in 4.7 ([Homebrew#20755](https://github.com/Homebrew/brew/issues/20755), [discussion](https://github.com/orgs/Homebrew/discussions/6537)). This option closed four days ago |
| Keep documented `xattr` step | $0 | Nothing; actively trains the wrong habit for a keylogger-shaped app | Only honest as an *interim*, and the README must say plainly that the artifact is **unsigned/ad-hoc**, not "signed" |

**Effort:** 3–4h CI work + $99/yr, or 1h for the partial fix. **Minimum to unblock:** correct
the wording (30 min) so the claim matches the artifact.

### P0-2. The diagnostic log is an unconditional plaintext keylog, and PRIVACY.md does not cover Windows

**Evidence.**
- macOS line format: `app/src/platform/macos/dispatch.rs:276` —
  `KEY {ch:?} … app={app} … | {decision} | raw={raw:?} rendered={rendered:?}`.
  `docs/handoff.md:583` shows a real line containing the typed word.
- Windows logs the same content: `app/src/platform/windows/hook.rs:498-499`
  (`KEY {:?} vk= … | {decision}`), where `Decision`'s Display is
  `Emit bs={} ins={:?}` (`crates/glowkey-input/src/platform.rs:187`) — i.e. the inserted
  Vietnamese text — plus `Notice::Corrected { was, becomes }` at `hook.rs:506`, which is
  two whole words.
- **Always on.** `app/src/log.rs:134` has no gate; `GLOWKEY_DEBUG` (`log.rs:166-168`) only
  controls the stderr echo, not the file.
- **No permission hardening.** No `set_permissions`/`PermissionsExt` anywhere in `app/` or
  `crates/` (grep, 0 hits). The file is created with `OpenOptions::new().create(true)`
  (`log.rs:95-99`) → default umask, typically `0644`, in `~/Library/Logs/GlowKey/`, which
  is not behind a TCC prompt. **Any process running as the user can read it.**
- **PRIVACY.md is macOS-only prose**: "entirely on your Mac" (`PRIVACY.md:8`), "leaves your
  Mac" (`PRIVACY.md:22`), path `~/Library/Logs/...` (`PRIVACY.md:29`), verification command
  `otool` (`PRIVACY.md:54`). The Windows paths (`%LOCALAPPDATA%\GlowKey\Logs`,
  `%APPDATA%\GlowKey`) appear nowhere in PRIVACY.md. `README.md:144-150` likewise says
  "your Mac".
- Rotation is real and bounded (`log.rs:36`, `121-129`) — 5 MB × 2. So on a fast typist,
  the file holds roughly the **last several weeks of everything they typed**.

**User impact.** The single highest-value file on the machine for any other program the
user runs — passwords excepted on macOS only (secure fields never reach the tap), but not
excepted on Windows, where a low-level hook *does* see password fields in ordinary
(non-UAC) dialogs. A stranger reading PRIVACY.md on Windows is reading a document that does
not describe their machine.

**Remediation (do all four).**
1. **Redact by default.** Log the *shape*, not the text: `raw.len()`, `insert.chars().count()`,
   backspace count, decision variant, app id. Keep full text behind an explicit,
   UI-visible **"Verbose typing log"** toggle that is off by default, auto-expires (e.g. 24h
   or next restart), and shows a persistent tray/menu-bar indicator while on. ~4–6h,
   touches `dispatch.rs:276`, `hook.rs:498-508`, `prefs_model.rs`, both settings UIs.
2. **Tighten permissions**: `0o600` on POSIX at create; on Windows `%LOCALAPPDATA%` ACLs are
   already user-only, but assert it. ~1h.
3. **Retention**: delete the log (and `.1`) on a schedule or at every clean start when
   verbose is off. ~1h.
4. **Rewrite PRIVACY.md platform-neutrally** with both file paths, both verification
   commands (`otool -L` and the import-table scan CI already does at
   `.github/workflows/ci.yml:108-131`), and an explicit statement of what the verbose mode
   records. ~2h.

**Effort:** ~1.5 days. **This is the gap that most directly contradicts the project's own
stated posture**, and it is cheap to fix.

### P0-3. Windows ships nothing, and is self-declared unverified

**Evidence.** `README.md:84-91` — "There is no release artifact yet… Build it yourself:
`cargo build --release -p glowkey`". `.github/workflows/release.yml:15` has one job,
`dmg`, `runs-on: macos-latest`. `README.md:33-38` — "Chrome, Windows Terminal, VS Code,
Electron apps, elevated windows, dead-key layouts and AltGr are all unverified", hotkey
recording not implemented. `app/src/platform/windows/mod.rs:36-39` — "behaviour is
unverified until Phase 6".

**User impact.** No Windows user can install GlowKey without a Rust toolchain. That is
correct *today* given the verification state — packaging an unverified keyboard hook would
be worse. So this is a P0 on the release, not a defect to rush.

**Remediation, in order:**
1. Finish the Tier-2+ verification listed in `plans/reports/windows-verification-260905.md`.
   Until then, do not package. (Unchanged from the project's own position — this audit
   endorses it.)
2. Then package. Options:

| Option | Cost | Benefit | Verdict |
|---|---|---|---|
| Azure **Artifact Signing** (ex-Trusted Signing) + a small installer | **$9.99/mo** Basic tier ([Azure pricing](https://azure.microsoft.com/en-in/pricing/details/trusted-signing/)); individuals now eligible, no 3-year history ([MS blog](https://techcommunity.microsoft.com/blog/microsoft-security-blog/trusted-signing-is-now-open-for-individual-developers-to-sign-up-in-public-previ/4273554)) | Kills SmartScreen from day one; ~1/4 the price of the cheapest OV cert; enables winget | **Recommended — but check eligibility first: restricted to US/CA/EU/UK entities.** A Vietnam-based individual is likely ineligible |
| Traditional OV/EV code-signing cert | $400–900/yr ([My-SSL](https://my-ssl.com/learn/azure-trusted-signing-vs-code-signing-certificate)), HSM/token logistics | Same outcome, no geo restriction | Fallback if the above is geo-blocked |
| **winget** with an unsigned exe | free, ~2h manifest | Discoverable + `winget upgrade` (see P1-1) | Viable but SmartScreen/MotW on unsigned or un-reputed `.exe` is a known winget release blocker ([winget-pkgs#385483](https://github.com/microsoft/winget-pkgs/issues/385483)) — pair with signing |
| MSIX / Microsoft Store | store signing free-ish, but MSIX **sandboxes**; a global `WH_KEYBOARD_LL` hook + `SendInput` into arbitrary processes is not a Store-friendly capability | Would very likely fail certification | **Reject** |
| Bare `.exe` on the releases page | free | SmartScreen "Windows protected your PC" + AV heat on a keyboard hook (`README.md:97-100` already anticipates this) | Interim only |

**Effort:** verification is the long pole (days of manual work); packaging + signing ~1 day
once a certificate exists.

---

## P1 — needed before calling it production

### P1-1. No update mechanism of any kind

**Evidence.** Grep for `check for update|auto.update|sparkle|winsparkle|new version` across
`app/`, `crates/`, `docs/`, `scripts/`, `README.md`: **zero hits.** No `CHANGELOG.md`.

**User impact.** A background agent with no window. A user who installs 0.1.0 will run
0.1.0 forever, including through a security-relevant fix. There is no channel to reach them.

**The privacy constraint is real, and must be stated.** `PRIVACY.md:15-20` promises "GlowKey
opens no sockets and calls no networking APIs", and CI *enforces* it against the shipped
binary on both platforms (`ci.yml:108-131`, `ci.yml:148-159`, `release.yml:44-55`). **Any
in-app update check is a breaking change to that contract** — it would require deleting the
promise, deleting or narrowing the CI guard, and re-earning the trust the guard exists to
prove. For a program that sees every keystroke, that trade is bad.

**Recommended minimal honest mechanism — keep zero network in the app:**
1. **Package-manager updates** (`winget upgrade`, and on macOS a Homebrew cask *if and only
   if* P0-1 is fixed with real notarization). The updater is the OS's, not GlowKey's; the
   privacy claim survives intact. ~2h manifest per platform.
2. **A `CHANGELOG.md` + a "Releases" link in About** that opens the browser. The app makes no
   request; the *user* does. Already 90% built — About shows version+commit
   (`app/src/platform/windows/about_ui.rs:29-36`, `app/src/about_window.rs:33-42`). ~2h.
3. **Explicitly document "GlowKey never checks for updates"** in PRIVACY.md as a *feature*,
   with the release page as the channel. ~30 min.

If an in-app check is ever wanted, the only acceptable shape is: opt-in, off by default,
a single unauthenticated `GET` of a static version file, no identifiers, no telemetry, a
narrowed CI guard that still bans everything else — and a PRIVACY.md revision that says so
before the code ships. **Effort:** 4–5h for the recommended path.

### P1-2. No panic hook; a panicking GlowKey dies silently

**Evidence.**
- **The good half, and it is genuinely good.** Both hot callbacks are `catch_unwind`-wrapped
  and fail *pass-through*: macOS `app/src/platform/macos/mod.rs:319-331` (`result.unwrap_or(event.as_ptr())`)
  and Windows `app/src/platform/windows/hook.rs:323-343` (`.unwrap_or(false)` → `CallNextHookEx`).
  Also `mouse.rs:75-76`, `tray.rs:604-626`, `health.rs:208-214`, plus poison-tolerant locks
  (`foreground.rs:75-87`, `ui_thread.rs:292-298`). **A panic in the keystroke path costs one
  key its Vietnamese transform, not the user's typing.** That is the right design and it is
  implemented on both shells.
- **The missing half.** No `std::panic::set_hook` anywhere (grep: 0 hits). Default panic
  output goes to stderr — and on Windows `main.rs:27` sets
  `#![windows_subsystem = "windows"]`, so **stderr goes nowhere** (the file's own comment at
  `main.rs:25-26` acknowledges this). A panic outside a wrapped callback (the UI thread, the
  message loop, `settings_ui.rs:679`/`932`, `adapt.rs:494`) leaves **no trace in
  `glowkey.log`**.
- **No restart.** macOS login item is `SMAppService` (`app/src/login_item.rs:9-25`) — start at
  login, not KeepAlive. Windows is one `HKCU\...\Run` value (`startup.rs:22-26`). Either way,
  a dead GlowKey stays dead until next login.
- **No signal.** Process exits → tray/menu-bar icon disappears (on Windows the tray icon may
  linger as a ghost until hover). The user finds out because Vietnamese silently stopped
  working — the exact "the indicator was lying" failure mode `docs/decisions/0007` was
  written to eliminate.

**Remediation.**
1. `std::panic::set_hook` installed as the **first statement** of both `run()` entry points,
   writing `PANIC <thread> <location> <payload>` through `crate::log::log`. Note: the hook
   runs *inside* `catch_unwind` too, so wrapped-callback panics also become visible —
   currently they are completely silent. ~2h.
2. Persist a `last_run_panicked` marker (a sentinel file beside the log, not in settings) and,
   on next start, log it and show it in About / a one-shot tray balloon. ~3h.
3. Consider `panic = "abort"` in `[profile.release]` **only after** (1) — it makes crashes
   loud and consistent, but it also defeats the pass-through recovery in the hook callbacks,
   which are worth more. **Recommendation: keep unwind.**

**Effort:** ~5h.

### P1-3. Settings: the backup is destroyed by the very failure it exists for

**Evidence.** `app/src/settings_store.rs:46-72`.
- Atomic-ish write **is** present: `settings.json.tmp` then `fs::rename` (`:64-71`). Good.
- **Bug: `.bak` is overwritten with the corrupt file.** `:61-63` copies the *current on-disk
  file* to `.bak` before writing — unconditionally, with no knowledge of whether that file
  parsed. Sequence: file corrupts → `load()` returns full defaults (`:38`,
  `prefs_model.rs:123-125` `unwrap_or_default()`) → the user touches any setting → `save()`
  copies the **corrupt** file over the only good backup → the comment at `:57-60` promising
  "the .bak preserves what was there for manual recovery" is now false, and the user's
  exclusions, macros and word overrides are gone for good.
- **No `fsync`** on the temp file or the parent directory before/after rename. On power loss
  the rename can land with zero-length contents.
- **No schema version field** in `Settings` (`prefs_model.rs:17-80+`), and serde silently
  **drops unknown fields**. Downgrade round-trip is therefore lossy: an older build loads a
  newer file, ignores the new keys, and the next save deletes them permanently. There *is*
  good field-level tolerance (`#[serde(default)]` everywhere, and the
  `a_malformed_word_override_does_not_discard_the_rest_of_the_file` test at
  `prefs_model.rs:214-230`), so only a *top-level* parse error is catastrophic.
- Silent failure: a corrupt file produces defaults with **no user-visible notice**. The user
  sees their settings spontaneously reset.

**Remediation.**
1. Have `load()` return whether the parse succeeded; **skip the `.bak` copy when it did not**
   (or write to `settings.json.corrupt-<timestamp>` instead). ~1h. *This is the one real
   data-loss bug in the audit.*
2. `File::sync_all()` on the temp handle before rename; best-effort dir fsync after. ~1h.
3. Add `#[serde(flatten)] unknown: serde_json::Map<String, Value>` so a downgrade round-trip
   preserves fields it does not understand. ~2h + a fixture test.
4. Surface a one-time notice ("your settings could not be read; the previous file was kept
   as …") in the tray/menu bar. ~2h.

**Effort:** ~6h.

### P1-4. CI proves a lot; the release pipeline proves much less

**What CI genuinely proves** (and it is above average): platform-free crates build/test/lint
on Linux including a `cargo check -p glowkey` stub guard (`ci.yml:44`), MSRV 1.96 (`ci.yml:48-53`),
semver against the `v0.1.0` tag (`ci.yml:61-71`), Windows *links* and tests (`ci.yml:86-96`),
macOS builds and tests (`ci.yml:134-143`), and a **binary-level** network-link guard on both
platforms with an anti-vacuity check on each (`ci.yml:127-130`, `ci.yml:155`). The comments
are honest about the limit: "a green badge here means the tests run — never that the input
method works" (`ci.yml:83-85`).

**What it does not prove / gaps.**

| Gap | Evidence | Remediation | Effort |
|---|---|---|---|
| No artifact checksums | `release.yml:92-95` uploads only the DMG | `shasum -a 256` → attach `.sha256`; print it in the notes | 20 min |
| No build provenance | — | `actions/attest-build-provenance` | 30 min |
| Not reproducible | `dtolnay/rust-toolchain@stable` (`release.yml:19`) floats; `rustup target add` (`build-app.sh:61`) | Pin the toolchain version in `rust-toolchain.toml`; record it in the release notes | 1h |
| No Windows (or Linux) release job | `release.yml:15` — one `macos-latest` job | Add a Windows job gated on P0-3's verification | 1 day (after verification) |
| Release runs tests but not clippy/fmt | `release.yml:33` | Add the same two commands CI runs | 15 min |
| Release signs ad-hoc silently | `build-app.sh:133` `|| true` | Fail the *release* build if no identity resolves (env flag `GLOWKEY_REQUIRE_IDENTITY=1`) | 30 min |
| No supply-chain scanning | no `cargo-deny`/`cargo-audit`/Dependabot (`.github/` contains only `workflows/`) | Add `cargo-deny` (advisories + licenses — it also solves P2-1) and `.github/dependabot.yml` | 2h |
| No SBOM | — | `cargo-cyclonedx` attached to the release | 1h |
| Tag/version check is macOS-job-only | `release.yml:24-31` | Fine as-is; note it only checks `app/Cargo.toml`, not the three library crates | — |

**Effort:** ~1 day excluding the Windows job.

### P1-5. Support surface: no front door for a bug report

**Evidence.** `.github/` contains **only** `workflows/` (`ls -R .github`). No
`SECURITY.md`, `CONTRIBUTING.md`, `CHANGELOG.md`, `ISSUE_TEMPLATE/`, `CODEOWNERS`.

**What already works, and is better than most projects:** version + short commit + dirty
marker stamped at build (`app/build.rs:114-138`) and shown in About on both platforms
(`about_ui.rs:29-36`, `about_window.rs:33-42`); "Reveal Log" in both shells
(`menu_bar.rs:355`, `tray.rs:915`); `docs/handoff.md:571-624` is a genuinely excellent
triage guide.

**Gaps → remediation.**
1. `SECURITY.md` — **mandatory for this app class.** State the threat model, the log's
   contents, that no network is used, and a private disclosure address. ~1h.
2. Bug-report issue template that *requires* the About string (version+commit), OS version,
   the target application, and the relevant log excerpt — with an explicit warning that
   **the log contains typed text and should be reviewed before pasting**. ~1h.
3. A "Copy diagnostics" menu item (version, commit, OS, permission state, hook/tap health)
   that copies a redacted block to the clipboard. ~3h.
4. `CHANGELOG.md`. ~1h ongoing.
5. README accuracy: verified good — the Windows caveats (`README.md:31-38`), the linked
   `plans/reports/windows-verification-260905.md` (exists), and the paths at
   `README.md:93-95` all match `paths.rs:17-35`. **One correction needed**: "signed but not
   notarized" (`README.md:71-72`) → "ad-hoc signed" (see P0-1); and the Privacy section
   (`README.md:144-150`) is macOS-only (see P0-2). ~30 min.

**Effort:** ~1 day.

---

## P2 — polish

### P2-1. THIRD-PARTY-NOTICES.md is materially stale against Cargo.lock

**Evidence.** `Cargo.lock` contains **391** packages. `THIRD-PARTY-NOTICES.md` names six
(`vi`, `phf`, `serde`/`serde_json`, `objc2` family) and waves at the rest with "Cargo.lock is
the exact, versioned list". Missing, and all **linked into the shipped Windows binary**:
`eframe 0.29.1`, `egui 0.29.1`, `epaint 0.29.1`, `winit 0.30.13`, `glow 0.13.1`,
`wgpu 22.1.0`, `ab_glyph 0.2.32`, `image 0.25.10`, `windows-sys`, `winresource`
(`app/Cargo.toml:56-111`).

**The one that actually matters:** `egui 0.29.1` enables `default = ["default_fonts"]`
(verified in the vendored manifest, `egui-0.29.1/Cargo.toml:107-108`) and `app/Cargo.toml:108`
takes egui with default features. Those bundled fonts (Ubuntu-Light, NotoEmoji, Hack) carry
**OFL/font-specific licenses with their own notice requirements** — not MIT, and not covered
by the current file. Also, `build-app.sh:86` copies the notices into the macOS bundle;
**the Windows build ships no notices at all.**

**Remediation.** `cargo about` or `cargo-deny`'s license check to generate the file from
`Cargo.lock` in CI, fail on an unapproved license, and embed the result next to the Windows
executable. ~3h, and it becomes self-maintaining.

### P2-2. Bundle metadata / notarization prerequisites

`app/Resources/Info.plist` has no `NSHumanReadableCopyright`, and `build-app.sh:130` signs
without `--options runtime` (hardened runtime), which notarization requires — so P0-1's
recommended path needs this line changed too. `LSMinimumSystemVersion 13.0` is stated in the
plist but nowhere in README/PRIVACY, so a macOS 12 user discovers it by failure. Also
`codesign --deep` is deprecated by Apple; sign inside-out explicitly. ~1h total.

### P2-3. macOS and Windows share a settings schema but not a settings *file* story

`settings_store.rs:20-28` documents that the same schema is used deliberately so a file can
be copied between platforms — good. But GlowKey and GlowKey Dev share one settings file and
one log (`build-app.sh:15-17`), which means dev builds can corrupt a user's real settings.
Low impact for a single-developer project; note it, do not fix. ~0.

---

## Ranked summary

| # | Gap | Sev | Effort |
|---|---|---|---|
| P0-1 | macOS release is ad-hoc signed; "damaged" dialog, no GUI bypass since Sequoia, Homebrew route closed 2026-09-01, Accessibility re-grant every update | P0 | 3-4h + $99/yr |
| P0-2 | Unconditional plaintext keylog on disk, `0644`, no redaction/opt-in; PRIVACY.md silent on Windows | P0 | ~1.5d |
| P0-3 | No Windows artifact; Windows behaviour self-declared unverified | P0 | verification (days) + 1d |
| P1-1 | No update mechanism; in-app check would break the no-network contract | P1 | 4-5h |
| P1-2 | No panic hook; panics invisible on Windows, no restart, no signal (callbacks *do* fail safe) | P1 | 5h |
| P1-3 | `.bak` overwritten with the corrupt file; no fsync; lossy downgrade; silent reset | P1 | 6h |
| P1-4 | Release: no checksums, no provenance, not reproducible, macOS-only, silent ad-hoc fallback, no dep scanning | P1 | ~1d |
| P1-5 | No SECURITY.md / issue template / changelog; README signing wording wrong | P1 | ~1d |
| P2-1 | THIRD-PARTY-NOTICES stale (385 of 391 crates unlisted; egui bundled fonts' OFL uncovered); Windows ships none | P2 | 3h |
| P2-2 | No hardened runtime, no copyright string, min-OS undocumented, deprecated `--deep` | P2 | 1h |
| P2-3 | Dev and release builds share the settings file | P2 | 0 (note only) |

**Shortest credible path to "a stranger can install and trust it" on macOS:** P0-2 (privacy
truth + redaction) → P0-1 (Developer ID + notarize) → P1-3 (the data-loss bug) → P1-2 →
P1-5. Roughly one focused week plus $99. Windows is gated on verification, not on this list.

## What this audit did not cover, and why it matters

- **Correctness of the input engine.** Not examined; `cargo test --workspace` was not run
  (read-only constraint). A production claim ultimately rests on the Tier-2+ Windows
  verification that the project itself says is outstanding.
- **The `vi` crate's own supply chain.** GlowKey's entire transformation is delegated to one
  upstream dependency; its maintenance/abandonment risk was not assessed.
- **Runtime memory/CPU under a long-lived agent**, and whether `wgpu`/`glow` hold GPU
  resources after the settings window closes (`app/Cargo.toml:99-107` claims nothing runs
  while idle; unverified here).
- **Whether the ad-hoc DMG actually shows "damaged" on macOS 26** — inferred from Gatekeeper
  documentation, not reproduced on a Mac. Worth one live check before rewriting the README.
- **AV/EDR false-positive rate** on the Windows binary. For a `WH_KEYBOARD_LL` + `SendInput`
  program this is a distribution risk of its own; a VirusTotal pass on a release build would
  quantify it.

## Unresolved questions

1. Is the $99/yr Apple Developer Program acceptable? Every macOS distribution option except
   "paste this terminal command" now depends on it.
2. Is the project owner a US/CA/EU/UK entity? Azure Artifact Signing eligibility is
   geo-restricted, and it is the only cheap Windows signing route.
3. Is full-text logging load-bearing for the maintainer's current debugging workflow? If so,
   the opt-in-verbose design in P0-2 must land *before* it is switched off by default.
4. Does the project intend a Linux port? P1-4's "release matrix" recommendation assumes not.
