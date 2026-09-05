# GlowKey security audit — dependency CVEs + code posture

Date 2026-09-05. Repo `D:/project/github/glowkey` @ `ce51ff0`, clean tree. Read-only audit; **no repo file was modified**, no bump applied.

## Threat model (stated first, so nothing below is inflated)

Local desktop app. No server, no sockets, no listener, no auto-update. Runs **unelevated**, at the user's own integrity level. So:

- **Remote exploitation: not in scope.** There is no network input surface at all (proved below).
- **Real attackers:** (1) another process running **as the same user** (commodity infostealer, a "collect my logs" support tool, a backup/sync agent); (2) a **local admin / SYSTEM** or anyone with the disk (no FDE); (3) a **malicious data file** the user is socially engineered into importing (a shared UniKey/EVKey macro table) or into putting on the clipboard.
- **Not an attacker:** a same-user process that plants a symlink or overwrites GlowKey's own settings. It already has the user's full privileges; there is no boundary to cross. Those are noted as informational, not findings.

The dominant risk in this app is **not memory safety** — it is that a program which by design sees every keystroke on the machine **writes those keystrokes to a plain file, unconditionally, with no opt-out**, and only half discloses it.

---

# A. Dependency advisories

## A.0 Tooling and completeness — COMPLETE

| Check | Tool | Scope | Result |
|---|---|---|---|
| Advisory scan | **`cargo audit`** (run by the coordinator) | **1239 advisories** in the RustSec DB vs **391 crate dependencies** in `Cargo.lock` | exit 0, **0 vulnerabilities**, 2 unmaintained warnings |
| Independent corroboration | **OSV.dev `/v1/querybatch`** (this agent) | all **391** `[[package]]` entries from `Cargo.lock`, name+version | identical result: same 2 IDs, nothing else |
| OSV pipeline control | OSV, deliberate known-bad queries | `smallvec 1.6.0`, `atty 0.2.14` | both flagged correctly → the query was not silently returning empty |
| Yanked versions | crates.io **sparse index** (`index.crates.io`), per crate/version | 388 non-workspace crates | **0 yanked** |
| Network-capable crates | `cargo tree --target all -e normal` | whole graph | **none present** (see §B7) |

`grep -c '^\[\[package\]\]' Cargo.lock` = **391**. Both scans covered all 391. The dependency check is **complete and tool-produced**, not a manual advisory browse. No advisory ID in this report is inferred — each was fetched from the OSV record and matches `cargo audit`'s output.

## A.1 Advisory table

| Crate | Version in lock | Advisory | Class / severity | Reachable in GlowKey? | Fix |
|---|---|---|---|---|---|
| `ttf-parser` | 0.25.1 | **RUSTSEC-2026-0192** (2026-06-28) | *informational: unmaintained*. **No CVSS, no CVE, no vulnerability** — the author declared the crate unmaintained. | **Compiled on Windows only, and only for the settings window.** Path: `ttf-parser → owned_ttf_parser → ab_glyph → epaint → egui → eframe → glowkey`. Not present at all on `aarch64-apple-darwin` (macOS uses AppKit, not eframe). See §A.3 for what it actually parses. | **None available.** 0.25.1 *is* the latest published version. `ab_glyph 0.2.32` (latest) still depends on it, and so does `egui` up to current 0.36.1 — upgrading eframe does **not** remove it. |
| `paste` | 1.0.15 | **RUSTSEC-2024-0436** (2024-10-07) | *informational: unmaintained*. Archived by its author. No vulnerability. | **Not reachable — not even compiled.** `cargo tree -i paste --target all -e normal,build,dev` prints *nothing*. It is a lock-file-only entry, reached solely through `metal → wgpu-hal`, and wgpu is never built: `app/Cargo.toml` pins `eframe = { default-features = false, features = ["glow", ...] }`. It is present in `Cargo.lock` because the lock is the union over all targets/features. | **No action needed.** If ever built, `pastey` is the drop-in fork. |

**No CVEs. No memory-safety, DoS, or crypto advisories anywhere in the graph.** Both entries are "the maintainer stopped maintaining it" notices, which is a supply-chain-durability signal, not an exploitable bug.

## A.3 What `ttf-parser` being unmaintained actually means here

The prompt flagged font parsers as usual CVE carriers, so this deserves a straight answer rather than a shrug:

- **It only runs on Windows, and only while the settings/About window is open.** The tray and the keystroke hook never touch it. The window is created on demand and destroyed on close (`app/Cargo.toml` comment; `app/src/platform/windows/ui_thread.rs:59`), so nothing in this path runs while GlowKey sits idle observing keystrokes.
- **Its input is not attacker-controlled in any realistic scenario.** The only fonts fed to it are (a) egui's compiled-in `epaint_default_fonts`, and (b) two files read from the system font directory — `app/src/platform/windows/settings_ui.rs:136`, `std::fs::read(format!("{root}\\Fonts\\{file}"))` for `segoeui.ttf` and `consola.ttf`. `%WINDIR%\Fonts` is admin-writable only. There is **no path by which a user-supplied font reaches the parser**. (One caveat on `{root}` — see finding **B8**.)
- **`ttf-parser` is `#![forbid(unsafe_code)]`** (confirmed against upstream `src/lib.rs:36`). A parsing bug in it is a panic or a wrong glyph, not memory corruption. GlowKey's font load is best-effort and already tolerates failure (`install_system_font` returns a bool and falls back to egui defaults).

**Verdict: accept and record.** There is no maintained replacement reachable from egui today (`skrifa` would require epaint to switch, which is upstream's decision, not GlowKey's). The honest mitigation is to note it in `THIRD-PARTY-NOTICES.md` / the audit log and re-check when egui migrates. Do **not** upgrade eframe expecting this to clear — it will not.

## A.4 Yanked versions

**None.** All 388 non-workspace `name@version` pairs were resolved against the crates.io sparse index and checked for `"yanked":true`. Zero hits.

## A.5 Duplicate major versions

43 crates appear at two or more incompatible versions. This is **hygiene/binary-size, not security** — there is no advisory on any of them. Worth naming only because a duplicated crate means two copies to patch if one ever *does* get an advisory. Notables:

- `windows-sys` at **four** majors: 0.52, 0.59, 0.60, 0.61. GlowKey itself pins 0.61; 0.52/0.59/0.60 arrive via `winit`/`x11rb`/`rustix`/`parking_lot`. `windows-targets` 0.52+0.53 and the 9 `windows_*` import-lib crates duplicate in lockstep behind them.
- `objc2` 0.5 + 0.6 and its seven `objc2-*` framework crates at 0.2 + 0.3: GlowKey pins the 0.6/0.3 generation; the 0.5/0.2 generation comes from `winit 0.30`. macOS-only.
- `syn` 1+2+**3**, `thiserror` 1+2, `rand` 0.8+0.9, `rustix` 0.38+1, `bitflags` 1+2, `hashbrown` 0.15+0.17, `redox_syscall` 0.4+0.5+0.9, `getrandom` 0.3+0.4, `glow` 0.13+0.14, `miniz_oxide` 0.8+0.9.

Every duplicate traces to `winit 0.30` / `eframe 0.29` lagging the ecosystem. The single lever is the eframe/egui upgrade in §C — and that is a **risky** bump, not a safe one.

---

# B. Code security posture — ranked findings

Ranked by (impact on the app's core privacy promise) × (realism of the attacker). Each states the attacker and the access required.

---

## B1 — HIGH · Keystroke content is written to disk unconditionally, with no opt-out

**Where**
- `app/src/log.rs:134` — `log()` is unconditional; the only gate is `cfg!(test)`.
- `app/src/platform/macos/dispatch.rs:276` — the macOS KEY line: `"KEY {ch:?} code={code} mods={mods} app={app} mode={mode:?} active={active} | {decision} | raw={raw:?} rendered={rendered:?}"`. `raw`/`rendered` are the **whole word being composed**, from `session.debug_state()` (`dispatch.rs:274`).
- `app/src/platform/windows/hook.rs:498` — the Windows KEY line: `"KEY {:?} vk={} mods={} app={} | {decision}"`. Per-character, not the word — but `event.ch` is still the literal typed character, and `{decision}` carries the emitted diff.
- Paths: macOS `~/Library/Logs/GlowKey/glowkey.log` (`log.rs:65-70`); Windows `%LOCALAPPDATA%\GlowKey\Logs\glowkey.log` (`log.rs:74-78` → `platform/windows/paths.rs:30`).
- Retention: 5 MB live + one 5 MB `.1` generation = **up to 10 MB of the user's recent typing on disk at all times** (`log.rs:36`, `log.rs:121-129`).

**What this means concretely.** On macOS a reconstructable transcript of typed words. On both platforms, everything the user types outside excluded apps and outside secure fields: chat messages, search queries, document text, and — because GlowKey cannot tell them apart — anything typed into a non-secure field that happens to hold a secret (an API key pasted-then-retyped, a recovery phrase, a 2FA code, a password in an app that does not use a secure field). The frontmost app id is logged alongside, so the transcript is **attributed**.

**Attacker + access required.** Any code running **as the same user** — the single most common post-compromise position, and exactly what commodity infostealers enumerate (`%LOCALAPPDATA%\*\*.log` is a standard sweep). No privilege escalation needed, no exploit needed: the file is simply there and readable. Additionally: a local admin/SYSTEM, a backup or endpoint-management agent, forensic recovery of an unencrypted disk, and any "please attach your logs" support flow — a user who follows the app's own "Reveal Log" affordance and emails the file is hand-delivering their typing history.

**Why this is the top finding.** It converts an ordinary same-user compromise (bad) into a keylogger transcript (much worse), *and* GlowKey supplies the keylogger itself. Every other finding here is smaller than this one.

**Remediation (in priority order)**
1. **Make content logging opt-in and off by default.** Ship a "Diagnostic logging" toggle in Settings, default off. Keep lifecycle lines (STARTUP/HOOK/TAP/HEALTH/EMIT-timing) always on — they are what actually diagnoses the hook-death class of bug and contain no typed text.
2. **Redact by default when it is on.** Log the key *class* and the decision, not the glyph: `KEY <letter> code=41 app=… | Emit bs=1 ins=<1ch>`, with `raw=`/`rendered=` behind a separate, explicitly-labelled "include typed text" switch that resets itself after N minutes or one app restart. Most typing bugs reproduce from the decision ladder, not the literal characters.
3. **Tighten permissions** — see B3.
4. **Add "Delete log" next to "Reveal Log"** (`menu_bar.rs:355`, `platform/windows/tray.rs:823`). Today the only documented way to clear it is to find and delete the file by hand.
5. **Warn at the reveal point.** "This file contains text you have typed" next to the menu item, so a user about to attach it to a bug report knows what they are attaching.

---

## B2 — HIGH · `PRIVACY.md` is macOS-only, and its central promise is untrue on Windows

**Where** `PRIVACY.md` (whole file), vs `app/src/platform/windows/paths.rs:17-36`, `app/src/settings_store.rs:26-28`.

Three concrete gaps:

1. **The Windows log path is not disclosed anywhere.** PRIVACY.md names only `~/Library/Logs/GlowKey/glowkey.log`. A Windows user reading it has no way to learn that `%LOCALAPPDATA%\GlowKey\Logs\glowkey.log` exists and holds their keystrokes. For a keystroke-observing app, an undisclosed keystroke file is the worst possible documentation defect.
2. **"Nothing you type ever leaves your Mac" is false on Windows for settings.** `settings_dir()` is `FOLDERID_RoamingAppData` (`paths.rs:18`), i.e. `%APPDATA%` — the **roaming** profile. On a domain-joined or Entra-joined machine with roaming profiles or Enterprise State Roaming, `%APPDATA%\GlowKey\settings.json` is **copied off the machine to the profile server** by design. That file contains the user's **macros verbatim** (PRIVACY.md says so itself) plus their word overrides and ignore list. Macros are exactly where people put canned personal text. The log is correctly in `LOCALAPPDATA` and does not roam (and `paths.rs:24-29` says so deliberately) — but settings do, and nothing tells the user.
3. **Everything is phrased "your Mac".** The whole document predates the Windows port. The CI networking guarantee is described only via `otool`; the Windows equivalent (the PE import-table scan, `.github/workflows/ci.yml:108-131`) is a genuinely good guard and goes unmentioned.

**Attacker + access required.** No exploit — this is an informed-consent failure. It becomes an exposure when a domain administrator, a profile-server backup, or anyone with access to the roaming share reads `settings.json`.

**Remediation.** Rewrite `PRIVACY.md` per platform: name both log paths, both settings paths, state plainly that `%APPDATA%` roams on managed Windows and what that implies for macros, and cite both CI guards. If roaming is not wanted, move settings to `FOLDERID_LocalAppData` (a one-line change in `paths.rs:18`, but it is a migration — existing files must be read from the old location once).

---

## B3 — MEDIUM · Log and settings files are created with default permissions; no explicit 0600/DACL

**Where** `app/src/log.rs:91-102` (`create_dir_all` + `OpenOptions::new().create(true).append(true)`), `app/src/settings_store.rs:50-69` (`create_dir_all` + `fs::write`). Confirmed by grep: **no** `set_permissions`, `PermissionsExt`, `0o600`, or `SECURITY_ATTRIBUTES` anywhere in `app/src`.

- **macOS:** the directory lands at 0755 and the file at 0644 under a typical 022 umask. It is protected today only *incidentally*, because `~/Library` is itself 0700. That is an inherited property of someone else's directory, not a guarantee GlowKey makes — and it evaporates on a relaxed `~/Library`, a redirected/network `$HOME`, or a permissive umask.
- **Windows:** `%LOCALAPPDATA%` and `%APPDATA%` inherit an ACL granting the user, SYSTEM, and **Administrators**. Other standard users cannot read it; local admins can, without any prompt.

**Attacker + access required.** A second **local user account** on the same machine (macOS, in the loosened-`$HOME` cases) — no privileges beyond a login. On Windows, a local administrator (who is already privileged, so this is a defence-in-depth gap rather than a boundary break).

**Remediation.** After creating the file, set mode `0o600` and the directory `0o700` on Unix (`std::os::unix::fs::PermissionsExt`). On Windows, either build an explicit DACL granting only the current user, or at minimum document the inherited ACL in PRIVACY.md so the admin-readability is stated rather than assumed. Apply to `settings.json`, `settings.json.bak`, and `settings.json.tmp` too — the `.bak` (`settings_store.rs:62`) is a full second copy of the macros and inherits the same defaults.

---

## B4 — MEDIUM · Unbounded scan of a clipboard block supplied by another process (out-of-bounds read)

**Where** `app/src/platform/windows/clipboard.rs:97-104`:

```rust
// SAFETY: CF_UNICODETEXT is documented NUL-terminated.
let len = unsafe {
    let mut len = 0;
    while *ptr.add(len) != 0 { len += 1; }
    len
};
```

The length is derived **purely by scanning for a NUL**, trusting the documented invariant of a `CF_UNICODETEXT` block. But that block was allocated and filled by **whatever process last wrote the clipboard** — it is not GlowKey's memory and the invariant is not enforced by the kernel. A process that calls `SetClipboardData(CF_UNICODETEXT, h)` with a `GlobalAlloc` block containing no terminating NUL makes this loop read past the end of the allocation until it happens to hit a zero word or an unmapped page.

**Impact.** Out-of-bounds read → at best a crash of the input method (a local DoS that kills typing for the user), at worst `String::from_utf16_lossy` at line 106 pulling adjacent heap bytes from GlowKey's own process into the clipboard when the user then clicks a transform tool — i.e. **process-memory disclosure into the clipboard**. GlowKey's heap contains the composing word and the loaded macro table.

**Attacker + access required.** Any process running as the same user that can write the clipboard, plus one user click on Tray → a clipboard tool. Same-privilege, so not a privilege boundary break — but unlike the symlink cases below this is a genuine memory-safety defect with a disclosure channel, and it is trivially fixable.

**Remediation.** Bound the scan by the actual allocation:

```rust
let cap = unsafe { GlobalSize(handle) } as usize / 2;   // u16 units
let len = (0..cap).take_while(|&i| unsafe { *ptr.add(i) } != 0).count();
```

(`GlobalSize` needs `Win32_System_Memory`, already in the feature list.) `GlobalSize` may round up, but it is a hard upper bound on mapped memory, which is exactly what the loop is missing.

---

## B5 — MEDIUM · An imported macro table is a text-injection primitive; the Windows path has no size cap

**Where** `crates/glowkey-session/src/macros.rs:77-106` (`parse_table`), `app/src/prefs/macros_window.rs:194-260` (macOS file import), `app/src/platform/windows/settings_ui.rs:1208-1215` (Windows paste-in import), `app/src/platform/windows/inject.rs:104-114` (injection).

**The good news first, since the prompt asked specifically.** The imported table is treated as **data everywhere**. Verified end to end:
- `parse_table` splits on the *first* `:` and does nothing else — no escape processing, no substitution, no template language (`macros.rs:96-104`).
- The JSON branch is `serde_json::from_str::<Vec<Macro>>(..).unwrap_or_default()` (`macros.rs:86`) into a two-`String` struct. No `serde(deny_unknown_fields)` gap that matters, no deserialization side effects, no untagged enum.
- Expansions reach the OS through `inject::emit_edit`, which encodes them as **`KEYEVENTF_UNICODE` text events** (`inject.rs:108-113`) — never as virtual-key codes. A macro therefore **cannot** press Enter-as-VK, Ctrl combinations, or system hotkeys. On macOS the equivalent path is the single tagged `CGEventPost` queue.
- No expansion is ever passed to a shell, a format string, or a path.

**The residual risk.** The expansion is nonetheless **typed into whatever window is focused, verbatim, on a trigger the user cannot see coming**. Two concrete gaps:
1. **No control-character filter.** Nothing rejects `\r` / `\n` / `\t` in an expansion (`macros.rs:100` requires only non-empty; `validate_macro`, `settings_ui.rs:1486-1490`, only trims). `KEYEVENTF_UNICODE` with U+000D/U+000A is delivered as a character, and many controls (chat composers, single-line forms, terminals) treat it as **submit**. So a shared "Vietnamese macro pack" can contain `chao:<a long message>\r` that auto-sends when the user types `chao`.
2. **No size cap on the Windows import.** macOS caps the imported file at 4 MB with a clear error (`macros_window.rs:212-226`). The Windows import is a paste into an `egui::TextEdit::multiline` (`settings_ui.rs:1201-1206`) with **no cap at all** — every parsed row is upserted in a loop (`settings_ui.rs:1213-1215`). A large pasted table is an in-memory blowup and a settings file the hook thread must re-parse.

**Attacker + access required.** Social engineering: the user must download and import a hostile `.txt` macro table (the README itself points at UniKey/EVKey tables as "the main way a curated list arrives"), then later type the trigger shortcut. No code execution, no privilege gain — the ceiling is "text you did not intend gets typed, possibly sent, into the app you are in".

**Remediation.** In `Macro::parse_table` (or a shared validator so both platforms get it), reject or strip expansions containing `\r`, `\n`, or other C0 controls, and cap expansion length (a few hundred chars covers every real gõ tắt). Mirror the macOS 4 MB cap onto the Windows import box. Show the import summary as "N macros imported" with a preview, so a table with surprising content is visible before it is live.

---

## B6 — LOW · Single-instance mutex is squattable → silent local denial of the input method

**Where** `app/src/platform/windows/single_instance.rs:31` — `const MUTEX_NAME: &str = r"Local\GlowKey-SingleInstance-8f3a1c";` — and `claim_named` at :62-85, which returns `None` on `ERROR_ALREADY_EXISTS`; the caller then exits **quietly and deliberately** (":48 — quietly, because a user who double-clicks the icon twice has not done anything wrong").

Any process in the same session can `CreateMutexW` on that fixed, hard-coded name first. GlowKey then exits at every launch — including at logon — and **says nothing**: no tray icon, no dialog, and no log line (the `None` path writes nothing; only the *failure to create* path logs, :71). The user sees an input method that "just doesn't start", with no diagnostic anywhere.

**Attacker + access required.** A same-user process in the same interactive session. `Local\` (not `Global\`) is the right choice and correctly reasoned in the module docs — it confines this to one session and prevents the cross-user lockout, so the blast radius is already minimised. Same-privilege attacker, availability-only impact.

**Remediation.** Cheap and worthwhile: log the refusal ("another instance holds the slot — exiting") so the silent-start-failure has a paper trail, and consider confirming the incumbent really is GlowKey (e.g. locate its tray/message window) before deferring. A squatter that holds the mutex but owns no GlowKey window is then distinguishable from a real second instance.

---

## B7 — LOW · `Command::new("explorer.exe")` resolves through the search path

**Where** `app/src/platform/windows/shell.rs:148` — `std::process::Command::new("explorer.exe").arg(dir).spawn()`. The only process spawn in the entire workspace (verified by grep).

A bare program name goes through Windows' executable search order, which includes the **application directory and the current working directory** before `System32`. If GlowKey's CWD is somewhere an attacker can write — the classic case is a user running the binary straight out of `Downloads`, where a drive-by `explorer.exe` may already sit — then Tray → "Reveal Log" executes the planted binary instead, at the user's privileges.

**Attacker + access required.** Write access to GlowKey's CWD (i.e. a same-user process, or a hostile archive extracted alongside the exe), plus one user click. Same-privilege, so this is binary-planting hygiene rather than escalation — but it is a one-line fix and this app carries more trust than most.

**Remediation.** Use an absolute path: `Command::new(Path::new(&system_root).join("explorer.exe"))`, with `system_root` from `GetSystemWindowsDirectoryW` (not the `SystemRoot` env var — see B8).

---

## B8 — LOW · System font path is built from the `%SystemRoot%` environment variable

**Where** `app/src/platform/windows/settings_ui.rs:133-136`:

```rust
let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
...
let Ok(bytes) = std::fs::read(format!("{root}\\Fonts\\{file}")) else { return false; };
```

The bytes are handed to `egui::FontData::from_owned` → `ab_glyph` → `ttf-parser` (the unmaintained crate from §A). A caller who controls GlowKey's environment redirects `SystemRoot` and chooses which file gets parsed as a font.

**Attacker + access required.** Whoever launches the process — i.e. the same user. No boundary crossed, and `ttf-parser` is `forbid(unsafe_code)`, so the ceiling is a panic or garbled text. Flagged mainly for **consistency**: `platform/windows/paths.rs` exists precisely because "an environment variable can be absent, stale, or pointing somewhere else entirely" and deliberately asks the system via `SHGetKnownFolderPath`. This one site quietly does the thing that module was written to avoid.

**Remediation.** `GetSystemWindowsDirectoryW` (or `SHGetKnownFolderPath(FOLDERID_Fonts)`), matching `paths.rs`.

---

## B9 — INFORMATIONAL · Settings write follows symlinks (`.tmp` / `.bak`)

`app/src/settings_store.rs:62-68` writes `settings.json.tmp` with `fs::write` (follows symlinks), then renames over `settings.json`, and copies the old file to `settings.json.bak`. A pre-planted symlink at either predictable name redirects a write. **Not a finding:** planting it requires write access to `%APPDATA%\GlowKey` / `~/Library/Application Support/GlowKey`, which means same-user, which already has the user's full privileges. Recorded only so a future reviewer does not re-derive it. The `.bak` *is* relevant to B3, since it is a second full copy of the macros with the same default permissions.

---

## B10 — `unsafe` audit: clean. No use-after-free across the UI/hook boundary.

The prompt asked specifically about raw pointers in the hook callbacks, FFI strings, and UAF across the UI/hook thread boundary. Findings:

**Whole-workspace negatives (grep-verified across `app/src` and `crates/*/src`):**
- **No `unsafe impl Send` / `unsafe impl Sync`** anywhere.
- **No `static mut`** anywhere.
- **No `transmute`** anywhere.

Those three absences are what make the thread-boundary question answerable at all — there is no hand-rolled sharing to audit.

**Distribution.** 33 files contain `unsafe`, dominated by `app/src/prefs/mod.rs` (45), `menu_bar.rs` (27), `platform/windows/tray.rs` (26). Those are objc2 `msg_send!` / `define_class!` bodies and Win32 GDI menu-and-icon calls — UI construction, not the keystroke path. The library crates are essentially clean: `crates/glowkey-input/src/lib.rs` has 2 occurrences and `glowkey-engine`/`glowkey-session` have none, which matches the stated design that the engine, session, and policy crates are platform-free.

**Windows hook callback — `app/src/platform/windows/hook.rs`**
- `hook_callback` (:329) wraps the body in `catch_unwind`, so a panic cannot unwind into Win32 C frames; on panic the key passes through unchanged. Correct.
- The one load-bearing raw-pointer deref is `:361`, `let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };`. It is correctly **preceded** by the `if code != 0 { return false; }` guard at :355-357 — the `HC_ACTION` contract that makes the cast valid is checked before the cast, not after. This is the exact ordering bug I was looking for, and it is not present.
- State is `thread_local! { RefCell<Option<HookState>> }` (:47-52), touched only from the installing thread, and every access uses `try_borrow`/`try_borrow_mut` with a conservative fallback (:383-390, :199, :218) — re-entry yields "pass the key through" rather than a panic in a C callback. No lock a UI thread could be holding is ever taken in the callback.

**macOS tap callback — `app/src/platform/macos/mod.rs`**
- `tap_callback` (:321) is `extern "C-unwind"` + `catch_unwind`, same conservative shape.
- `TapContext` is `Box::into_raw`-leaked at :443 and explicitly documented as leaked for the program's lifetime (:299). The `&*(user_info as *const TapContext)` at :339 therefore dereferences memory that is **never freed** — a deliberate leak that makes UAF structurally impossible rather than merely unlikely. Interior state is `Cell`/`RefCell`, single-threaded on the run loop.

**Cross-thread data flow (the UAF question).** The Windows settings window runs on its own thread (`ui_thread.rs:64`), and the log writer on another (`hook_log.rs:53`). Everything crossing those boundaries is either an **owned value in an `mpsc` channel** (`UiCommand::OpenSettings(Settings)` — a clone, `ui_thread.rs:42`; `String` log lines, `hook_log.rs:71-79`) or a **`Mutex`** (`shell.rs:177`, `static PENDING_SETTINGS: Mutex<Option<SettingsResult>>`). No pointer, reference, or borrowed buffer crosses a thread. The only cross-thread wake is `ctx.request_repaint_of` (documented thread-safe) and `PostThreadMessageW` (`hook.rs:170`), both non-blocking. **No use-after-free vector found.**

**FFI strings.** `single_instance.rs:63` builds an explicitly NUL-terminated `Vec<u16>` that outlives the `CreateMutexW` call. `paths.rs` frees the `SHGetKnownFolderPath` result with `CoTaskMemFree` as required. `clipboard.rs` is the one string-length defect and is written up as **B4**.

**Handle discipline — notably good.** `elevation.rs:72-81` and `single_instance.rs:36-43` both wrap raw `HANDLE`s in `Drop` guards. `elevation.rs:65-71` documents *removing the class* of double-free rather than fixing an instance, and correctly leaves `GetCurrentProcess()`'s pseudo-handle unwrapped (:85-87). `clipboard.rs:113-161` gets the hardest case right: `GlobalFree` is called **only** on the paths where `SetClipboardData` did not take ownership (:131, :156), and `EmptyClipboard` is deliberately placed *after* every fallible step so a failure cannot destroy the user's clipboard (:141-153).

**Alignment.** `elevation.rs:147-155` allocates the `TOKEN_MANDATORY_LABEL` buffer as `Vec<u64>` rather than `Vec<u8>`, with a comment explaining that reading a misaligned struct through a reference is UB regardless of what x86 tolerates. That is a real, non-obvious correctness issue handled correctly.

---

## B11 — Elevation: confirmed clean, cannot be tricked into a privilege issue

`app/src/platform/windows/elevation.rs`. Verified:

- **GlowKey never requests, acquires, or impersonates elevated rights.** No manifest file exists in the tree (`find app -iname '*.manifest'` → empty), and `build.rs:44-60` calls `winresource` only for the icon and version strings — `set_manifest` is never called, so there is no `requestedExecutionLevel`. Grep for `requireAdministrator` / `runas` / `ShellExecute` returns nothing but a comment saying `ShellExecute` was deliberately avoided. `AdjustTokenPrivileges`, `ImpersonateLoggedOnUser`, and service installation are all absent.
- **Only read rights are taken.** `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, ...)` (:103) and `OpenProcessToken(.., TOKEN_QUERY, ..)` (:121). `PROCESS_QUERY_LIMITED_INFORMATION` is specifically the right that is granted across an integrity boundary — the reasoning at :98-101 is correct, and the alternative (full query right) would have made every elevated window `Unknown`.
- **The detection cannot be turned into a privilege issue, because it grants nothing.** `foreground_reach` returns one of three enum values that only drive an on-screen indicator. Nothing branches on it to attempt a privileged operation. Tricking it yields at most a wrong glyph.
- **It fails closed.** Every error path returns `Reach::Unknown` (:56, :95, :105, :122, :143, :167, :176, :180), and `Unknown` is deliberately kept distinct from `Ok` (:38-43) precisely because "the query was refused" is the *expected* symptom of the boundary being detected. Collapsing them is the bug they avoided.
- **The comparison is strictly `>`** (:59), pinned by the test at :218-225. `>=` would report every ordinary window as blocked; `!=` would too. Numeric comparison rather than matching named constants correctly handles intermediate levels like medium-plus (0x2100).

The module's stated position — "The answer is to detect it and show it, never to ask for elevation. An input method requesting administrator rights is a red flag, and correctly so" (:11-14) — is implemented as written.

---

## B12 — Network: the "no networking" claim holds for the dependency graph, not just the binary

CI checks the **built artifact** on both platforms — a PE import-table scan for `ws2_32/wininet/winhttp/urlmon/wsock32` with a positive control that `USER32.dll` *is* found (`.github/workflows/ci.yml:108-131`), and `otool -L | grep -Ei '/Network\.framework|CFNetwork|/libcurl'` with an `test -x` guard against passing vacuously (:148-159). Both guards are honestly constructed — each proves it can fail.

I checked the **dependency graph** independently, which the prompt asked for and CI does not cover. `cargo tree --target all -e normal` filtered against `reqwest|hyper|tokio|ureq|curl|native-tls|rustls|openssl|isahc|surf|attohttpc|http|h2|socket2|mio|async-std|smol|trust-dns|hickory|quinn|websocket|tungstenite` returns **exactly one line, and it is a false positive**: `smol_str` (an inline-string type from winit, matched on the `smol` prefix). There is **no HTTP client, no TLS stack, no async runtime, no socket crate, and no DNS resolver** anywhere in the graph, on any target.

Two capabilities worth naming for completeness, neither of which is networking:
- **`webbrowser 1.2.4`** (via `egui-winit → eframe`) — launches the user's default browser for a hyperlink. It does not open a socket; it hands a URL to the OS. It is nonetheless the one component that can cause an outbound request as a *side effect of a click*, so it belongs in an honest disclosure.
- **`image 0.25.10` + `png 0.18.1`** (via `eframe`) — image decoders, historically a CVE-carrying class. Here the only input is `include_bytes!("../../../Resources/AppIcon.png")` (`settings_ui.rs:114`), i.e. compiled into the binary. **No attacker-controlled image ever reaches them.** No advisory on either version.

The privacy claim is sound. Its *documentation* is the weak part (B2), not its truth.

---

# C. Proposed dependency bumps

Nothing here is required to fix a vulnerability — there are none. These are hygiene, and each is marked with its real risk.

| # | Change | Rationale | Risk |
|---|---|---|---|
| 1 | **Do nothing about `paste 1.0.15`** | RUSTSEC-2024-0436 is unmaintained-only and the crate is **never compiled** (lock-file-only, behind `metal`/wgpu which `default-features = false` excludes). Suppressing or bumping it would be motion without effect. | **safe (no-op)** |
| 2 | **Do nothing about `ttf-parser 0.25.1`** | RUSTSEC-2026-0192 is unmaintained-only. 0.25.1 **is** the latest release; `ab_glyph 0.2.32` (latest) and every current `egui` still depend on it. There is no version to bump to. Parses only admin-writable system fonts, Windows-only, `forbid(unsafe_code)`. | **safe (no-op)** |
| 3 | Add both IDs to a **`.cargo/audit.toml` `[advisories] ignore`** list with a one-line justification each, and wire `cargo audit` into CI | Makes CI fail on a *new* advisory instead of staying permanently yellow on two accepted informational ones. This is the change that actually buys something. | **safe** |
| 4 | `eframe`/`egui` **0.29 → 0.36** | Collapses much of the duplicate-major table in §A.5 (`winit 0.30` drags in the old `objc2` 0.5/0.2 generation, `windows-sys` 0.52/0.59/0.60, `glow` 0.13). **Does not remove `ttf-parser`.** Windows settings window only; macOS is unaffected (AppKit). | **RISKY** — seven minor versions of a UI toolkit with per-release breaking changes. `settings_ui.rs` is ~1500 lines against this API (`id_salt`, `FontData::from_owned`, `ViewportBuilder`, `icon_data::from_png_bytes` all moved or changed across this range). Do it as its own change with the manual Windows verification pass, never bundled with a security fix. |
| 5 | Nothing for yanked crates | Zero yanked versions in the lock. | **n/a** |

**Priority.** #3 is the only one worth doing now. The security work that matters for this app is entirely in section B — **B1 and B2 first**, then B4 (a genuine memory-safety bug with a cheap, local fix), then B3.

---

# Appendix — what was checked, so a re-audit knows what to skip

- `Cargo.lock`: 391 packages, all scanned by `cargo audit` (1239 advisories) and independently by OSV batch; OSV pipeline validated with known-bad controls; all 388 non-workspace versions checked for yanks against the crates.io sparse index.
- `cargo tree -i` for `paste`, `ttf-parser`, `owned_ttf_parser`, `webbrowser`, `arboard`, `image`, `png`, `serde_json`, `zlib-rs`, `xml-rs`, `quick-xml` on `x86_64-pc-windows-msvc` and `aarch64-apple-darwin`.
- Read in full: `app/src/log.rs`, `app/src/settings_store.rs`, `app/src/platform/windows/{paths,elevation,single_instance,clipboard,hook_log,inject}.rs`, `crates/glowkey-session/src/macros.rs`, `PRIVACY.md`, `.github/workflows/ci.yml` (networking guards), `app/build.rs`.
- Read in part: `app/src/platform/windows/{hook,shell,ui_thread,settings_ui}.rs`, `app/src/platform/macos/{mod,dispatch}.rs`, `app/src/prefs/macros_window.rs`, `docs/handoff.md` §5/§7.
- Grepped workspace-wide: `unsafe` (33 files, counted per file), `unsafe impl Send`/`Sync`, `static mut`, `transmute`, `Command::new`, `thread::spawn`, `set_permissions`/`PermissionsExt`/`SECURITY_ATTRIBUTES`, `requestedExecutionLevel`/`runas`/`ShellExecute`, `read_to_string`/`fs::read`, `raw=`/`rendered=`.

**Not covered** (out of the delegated scope, flagged for whoever picks it up): the macOS `prefs/` and `menu_bar.rs` objc2 `define_class!` bodies were classified by grep and read only around the tap path, not line-by-line — they are the largest concentration of `unsafe` in the tree (72 occurrences across two files), though all of it is UI construction off the keystroke path. Code signing, notarization, and the installer/update channel were not examined.
