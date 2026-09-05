# Cross-platform completion of GlowKey — engineering plan

Scope: Windows to parity, macOS held at parity, a Linux backend added. Read-only
pass over the repo at `main` (`ce51ff0`), plus web verification of Linux/Wayland
API facts. Nothing outside cross-platform completion is proposed.

---

## 0. Where the port stands (verified, not copied)

| Layer | File | State |
|---|---|---|
| Policy port | `crates/glowkey-input/src/platform.rs:30-57` (`trait Platform`, 6 methods) | Stable, 2 implementors |
| Ladder | `crates/glowkey-input/src/ladder.rs:253-441` (`decide`) | Shared, no `cfg(target_os)` |
| Event | `crates/glowkey-input/src/event.rs:18-46` (`Key`), `:89-105` (`KeyEvent`, incl. `raw_code`) | Shared |
| Backends | `app/src/platform/mod.rs:11-15` — only `macos` and `windows` are `mod`-declared | Linux absent |
| Exclusion data | `app/src/default_exclusions/mod.rs:22-30` — Linux falls through to the **macOS bundle-id table** | Placeholder, wrong for Linux |
| Entry point | `app/src/main.rs:74-77` — non-macOS/Windows `main` is an `eprintln!` stub | Linux absent |
| CI | `.github/workflows/ci.yml` — jobs `engine`, `msrv`, `semver`, `windows`, `macos` | No Linux *shell* job |

---

## 1. Windows → parity: the exact remaining gap list

Cross-checked `app/src/settings_spec.rs` rows against
`app/src/platform/windows/settings_ui.rs` and `.../hook.rs`, and the macOS menu
(`app/src/menu_bar.rs:240-390`) against the Windows tray
(`app/src/platform/windows/tray.rs:722-835`).

### 1a. Settings spec rows — every row *is* rendered

All seven `Control` variants are handled on both sides:
`settings_ui.rs:872,891,902,913,929,935,956,961` vs `prefs/tabs.rs:159,175,191,207,241,262,273`.
No spec row is missing a Windows renderer. The gaps are behavioural, below.

### 1b. Confirmed gaps

| # | Gap | Evidence | File(s) that change |
|---|---|---|---|
| **W1** | **Hotkey recording absent.** Windows shows presets only; no "Custom…" arm. | `settings_ui.rs:935-955` (ComboBox over `hotkey_choices`, no recorder); `hook.rs:416-419` comment "No recorder on this platform" | `platform/windows/settings_ui.rs`, `platform/windows/hook.rs` (add the `hotkey::capture` branch ahead of `handle`, mirroring `platform/macos/dispatch.rs:288-305`), `platform/windows/shell.rs` (arm/disarm state) |
| **W2** | **A Mac-recorded custom hotkey degrades to character matching**, layout-dependent. | `hook.rs:416-431` — `hotkey::resolve(preset, None)`, then `is_char_fallback()` warns once | `crates/glowkey-input/src/hotkey.rs` (see §4.3), `app/src/prefs_model.rs` |
| **W3** | **`Notice::PersonalWordsChanged` is dropped.** ⌃⇧W with the Personal Words editor open does not refresh it. | `hook.rs:518-519` catch-all `_ => {}` with the comment "No personal-words editor on this platform reloads live"; macOS handles it at `platform/macos/dispatch.rs:119` | `platform/windows/hook.rs`, `platform/windows/ui_thread.rs`, `platform/windows/settings_ui.rs` |
| **W4** | **No session-only-terminal warning state.** `ExclusionToggle::EnabledSessionOnly` produces the plain `VI` glyph; macOS flashes `VI ⚠`. | `platform/windows/indicator.rs:29-45` — variants are `Vietnamese / ExcludedApp / English / Broken`, none for session-only; macOS at `platform/macos/dispatch.rs:134,146` | `platform/windows/indicator.rs`, `platform/windows/hook.rs`, `platform/windows/tray.rs` |
| **W5** | **No HUD / transient feedback at all.** `app/src/hud.rs` is macOS-only (`main.rs:37-38`). | Mode toggles are visible only if the user looks at the tray | `platform/windows/` (new `hud.rs`, or a tray balloon — see risk note) |
| **W6** | **No "Reset input (if stuck)".** | macOS `menu_bar.rs:317-323`; absent from `tray.rs:756-835` | `platform/windows/tray.rs`, `platform/windows/shell.rs` |
| **W7** | **No Quick Guide / welcome.** `welcome_shown` is carried through Windows settings merge but never read. | `app/src/welcome.rs` gated macOS-only at `main.rs:62-63`; Windows merges the field at `platform/windows/shell.rs:343-346` | new `platform/windows/welcome_ui.rs` (or a `settings_ui.rs` viewport), `platform/windows/tray.rs` |
| **W8** | **No state header line in the tray menu** ("Excluded in {app}" / "Vietnamese" / "English"). Windows shows breakage only. | macOS `menu_bar.rs:260-269`; Windows `tray.rs:734-753` shows `Indicator::Broken` only | `platform/windows/tray.rs` |
| **W9** | **The mode item does not name the current hotkey.** macOS renders it from the live preset. | macOS `menu_bar.rs:286-292` (`hotkey_display`); Windows `tray.rs:756-765` hardcodes the label | `platform/windows/tray.rs` |
| **W10** | **`default_exclusions/windows.rs` was written on a Mac and is unverified.** Its own header says so. | `app/src/default_exclusions/windows.rs:9-12` | `app/src/default_exclusions/windows.rs` (data only) |

### 1c. Deliberate non-gaps (do not "fix")

- Macro import/export is a **text box** on Windows (`settings_ui.rs:1101,1194`) instead of `NSOpenPanel`. Same capability, different affordance — parity by different means.
- Hotkey control is a ComboBox not a segmented control (`settings_ui.rs:936-939`): a documented layout decision, not a gap.
- List editors are deferred viewports (`decisions/0011`), which is the faithful equivalent of separate windows.
- Windows has an omnibox guard already (`platform/windows/inject.rs:99,217-226`) and it is user-confirmed working in Edge.

### 1d. Still unverified on Windows (not gaps in code — gaps in evidence)

Chrome, Windows Terminal, VS Code, Electron, elevated windows (the UIPI path has
never met one), dead-key layouts, AltGr, two layouts at once, long-running
behaviour, idle cost. Source: `plans/reports/windows-verification-260905.md`
§"What this did NOT test" and the Tier-5 "Left unchecked" list.

---

## 2. The open Windows defect: League of Legends — the discriminating test only

Handoff: `plans/reports/windows-handoff-260905.md` §"The open question that
matters most". The log already rules out UIPI (`FOREGROUND -> league of
legends.exe (Ok)`) and injection refusal (`INJECT REFUSED: 0`).

**Do not design or build the standalone-`w` option until this returns.**

### Test procedure (exact)

Preconditions, all of which must be asserted before the first keypress:

1. **No other Vietnamese IME running.** EVKey lives at `D:\apps\evkey\EVKey64.exe`; its logon task `EVKey - Vietnamese Keyboard` was disabled this session. Confirm with `Get-Process EVKey64 -ErrorAction SilentlyContinue` returning nothing. (A second IME already contaminated a whole verification round.)
2. **League is NOT excluded.** The user's `%APPDATA%\GlowKey\settings.json` currently excludes `league of legends.exe`, `leagueclientux.exe`, `riotclientservices.exe`. Back it up, remove those three entries, restart GlowKey. Restore afterwards.
3. **Vietnamese mode ON** at the moment of the test — verify the tray reads `VI`, not `EN`.
4. **GlowKey is the release build under test**, started before League (the hook must be installed; grep the log for `HOOK first callback received`).
5. In a Custom / Practice Tool game (no other players), champion alive, in-world, **not** in chat and **not** in the shop already.

The keypress: **press `B`** (default shop hotkey). `B` is not a Telex key, not a
VNI key, and not a modifier — the ladder classifies it as a word character that
`Engine::process_key` will emit unchanged (`Emit bs=0 ins="b"`), so it exercises
the *injection path* without exercising the *transformation*. Repeat once with
`Y` (scoreboard toggle) and once with `M` (map) as corroboration; all three are
non-Telex letters with unmistakable, instant, non-destructive in-game effects.

Reading it:

- **Shop (or scoreboard, or map) opens → cause (a)**: injection reaches League; only `w`→`ư` breaks the game. Then, and only then, design the "do not transform a standalone `w`" setting.
- **Nothing happens → cause (b)**: Vanguard discards injected input wholesale. No Telex setting helps; the answer is a different delivery path for anti-cheat-protected windows, which needs its own decision record.
- **Mixed (B works, Y does not)** → neither hypothesis; capture the log and stop.

Evidence to capture either way: `%LOCALAPPDATA%\GlowKey\Logs\glowkey.log` lines
for the three keys — each must show `KEY Some('b') vk=66 … | Emit bs=0 ins="b"`
and no `INJECT REFUSED`. A missing `KEY` line means the hook is not receiving
(a different bug); an `Emit` with no in-game effect is the (b) verdict.

**Blocked on: a human at that Windows machine, with League installed, and the
user's consent** (their standing rule forbids synthetic keystrokes into a live
session; this test is a *real* keypress by the user, which is why it is safe).

Falsification guard: run the same three keys in Notepad immediately after, with
the same GlowKey process. If `b`, `y`, `m` appear in Notepad but do nothing in
League, (b) is proven rather than inferred.

---

## 3. Linux: the architecture decision

### 3.1 Decision

**Ship an IBus input-method engine — `ibus-glowkey` — as a standalone Rust
process speaking the IBus D-Bus protocol, with a pluggable *focused-application
identity provider* (X11 `WM_CLASS` → wlroots foreign-toplevel →
plasma-window-management → IBus client name → unknown).**

Not an evdev/uinput hook. Not XTEST/X11.

### 3.2 Why

**The intercept-and-inject shape is not implementable on Linux the way it is on
macOS and Windows.** On X11 the observing extension (RECORD) is passive and
cannot consume events, and the only way to consume — `XGrabKeyboard` — conflicts
with the only way to inject (`XTestFakeKeyEvent`), because the grab receives the
faked event back ([Key Sequence Transformation in X11](https://blog.zhanghai.me/key-sequence-transformation-in-x11/)).
On Wayland there is no client-facing keyboard-grab protocol at all outside the
XWayland-specific `xwayland-keyboard-grab`, which by construction only covers
X11 clients ([mutter meta-xwayland-grab-keyboard.c](https://github.com/GNOME/mutter/blob/main/src/wayland/meta-xwayland-grab-keyboard.c),
[Phoronix](https://www.phoronix.com/news/Mutter-XWayland-Key-Grabs)). Linux's
answer to "a program that transforms keystrokes for every application" is the
input-method bus, not an event tap. GlowKey's engine is unchanged by this; only
the delivery layer differs.

**IBus over Fcitx5**, for three concrete reasons:

1. **Process boundary.** An IBus engine is a separate process on D-Bus. A Fcitx5 engine is a C++ shared library loaded into `fcitx5`. GlowKey is all-Rust with no C++ toolchain anywhere in the tree (`app/Cargo.toml`); an out-of-process D-Bus engine keeps it that way and keeps the crash blast radius outside the user's IME daemon.
2. **GNOME reach.** GNOME is the default desktop on Ubuntu/Fedora/RHEL and it drives input methods through the **IBus D-Bus protocol**, not `zwp_input_method_v2` — this is why Fcitx5 must *replace* `ibus-daemon` to work there at all ([Using Fcitx 5 on Wayland](https://fcitx-im.org/wiki/Using_Fcitx_5_on_Wayland)). An IBus engine is native on GNOME Wayland, GNOME X11, and (via `ibus-daemon --xim`) XIM clients.
3. **Fcitx5 can host IBus-registered engines** in practice via its ibus frontend, so the IBus choice is not exclusive; the reverse is not true.

**Per-application identity — the feature GlowKey exists for.** IBus 1.5.27+
delivers the client name with focus: `focus_in_id`/`focus_out_id` receive the
client name and a unique object path per input context, formatted
`gtk3-im:firefox`, `gtk4-im:gnome-text-editor`, `gnome-shell`
([IBus 1.5.27 release notes](https://fedoraproject.org/wiki/Changes/IBus_1.5.27),
[IBus reference](https://ibus.github.io/docs/ibus-1.5/IBusEngine.html)). That is a
real per-app identity that **works on GNOME Wayland**, which no compositor
protocol provides (see 3.4). This single fact is what makes the IBus path the
only architecture where the ignore list survives Wayland.

**Precedent.** `ibus-bamboo` is a Vietnamese IBus engine that already stores
typing modes per application and ships an exclusion list, with six output modes
(pre-edit, surrounding-text, `ForwardKeyEvent`, and three compatibility modes)
because no single delivery mode works in every client
([README_EN](https://github.com/BambooEngine/ibus-bamboo/blob/master/README_EN.md)).
GlowKey will need the same two-or-three-mode fallback, and should copy the shape
rather than rediscover it.

### 3.3 Rejected alternatives

**evdev/uinput global hook** (the keyd/kmonad shape). Rejected:

- *Permissions*: needs read access to `/dev/input/event*` (mode 0660 `root:input`) and write access to `/dev/uinput`, i.e. the user in the `input` group plus the `uinput` module loaded, or a root daemon ([ArchWiki, Input remap utilities](https://wiki.archlinux.org/title/Input_remap_utilities)). Putting a Vietnamese IME in the `input` group grants it every keystroke on the machine at the kernel level — a strictly worse privacy posture than the Windows hook, and one the README's "checkable rather than promised" claim could not survive.
- *Suppression is all-or-nothing*: consuming a key means `EVIOCGRAB` on the **whole device**, so GlowKey would own the keyboard and be responsible for re-emitting every key, including modifiers, media keys and the compositor's own hotkeys.
- *Wrong layer*: it operates on scancodes below XKB, so GlowKey would have to reimplement the user's layout to know what `a` is — the exact opposite of "wrap whatever layout is active", which is the product's premise (README:9-11).
- *Still no app identity*: being below the compositor, it has strictly less information about focus than an IME does. It does not solve the hard problem; it forfeits the only mechanism that does.
- *Coexistence*: it would fight any installed IBus/Fcitx5 for the same keys.

**XTEST / X11-only.** Rejected: technically the closest analogue and it would work
on X11 sessions, but (a) it cannot suppress — RECORD is passive and the
`XGrabKeyboard`+`XTEST` combination is self-feeding (cited above), which is fatal
to the full-suppression invariant (`ladder.rs:398-419`, `handoff.md` §5); (b) it
covers zero Wayland sessions, and Wayland is the default session on
Fedora/Ubuntu GNOME and KDE Plasma today. Building a backend with a known
end-of-life is not worth a phase.

### 3.4 Wayland, stated plainly

- **No global keyboard grab for ordinary clients.** Confirmed above.
- **No focus/app-id protocol a normal client can use on GNOME or KDE.** `ext-foreign-toplevel-list-v1` deliberately exposes only `identifier`, `title`, `app_id`, `closed`, `done` — **no activated/focused state** ("intentionally minimalistic") — *and* it is unimplemented by Mutter (50.4) and KWin (6.6); only wlroots compositors (Sway 1.11, Hyprland 0.52.1, river, labwc) implement it ([wayland.app/protocols/ext-foreign-toplevel-list-v1](https://wayland.app/protocols/ext-foreign-toplevel-list-v1)).
- **What Fcitx5 actually uses** for the focused window and its application name: `wlr-foreign-toplevel-management` (wlroots) and `plasma-window-management` (KWin). Neither exists on GNOME ([Using Fcitx 5 on Wayland](https://fcitx-im.org/wiki/Using_Fcitx_5_on_Wayland)).
- **The IME protocol itself carries no application identity.** Fcitx's own documentation: with `zwp_input_method` "there is only one input context visible to fcitx, and fcitx cannot distinguish what application is being used", making input-method state global rather than per-window (ibid.).

### 3.5 Is the per-app ignore list implementable on Wayland?

**Yes on three of four desktops, by three different mechanisms, and the coverage
must be stated to the user rather than assumed.**

| Session | Identity source | Verdict |
|---|---|---|
| Any X11 session (GNOME X11, Plasma X11, i3, …) | `_NET_ACTIVE_WINDOW` → `WM_CLASS` | **Full.** Same fidelity as macOS/Windows. |
| wlroots (Sway, Hyprland, river, labwc) | `wlr-foreign-toplevel-management` (`app_id` + `activated` state) | **Full.** |
| KDE Plasma Wayland | `plasma-window-management` | **Full.** |
| GNOME Wayland | IBus `focus_in_id` client name (`gtk3-im:firefox`) | **Partial — best effort.** Works for every client that reaches GlowKey through a GTK/Qt IM module (this includes GNOME Terminal/VTE, GTK apps, Qt apps). Yields the useless literal `gnome-shell` for shell-owned fields, and yields nothing usable for clients that speak `text-input-v3` to Mutter directly (Chromium/Electron under Ozone-Wayland, some toolkits). |
| GNOME Wayland, unknown client | — | **None.** |

**If identity is unknown, this is what the degraded product looks like** — say it
in the README, do not paper over it:

- The VI/EN mode toggle, the hotkey, Telex/VNI, auto-fix, macros, personal words, auto-capitalise: **all still work**. The engine and ladder are untouched.
- The **per-app ignore list silently does not apply** to that client. `Platform::app_in_front()` returns `None`, the ladder's exclusion check passes, and Vietnamese is ON.
- The **terminal protection weakens** for exactly those clients. In practice the common Linux terminals (VTE/GNOME Terminal, Konsole, xterm under XWayland) *do* report an identity, so the hole is mostly Electron-class apps — where Vietnamese-on is usually what the user wants anyway.
- GlowKey must **tell the user**: a one-line IBus property / settings banner reading "Per-app list unavailable on this desktop — <reason>", and a log line naming the identity provider chosen at startup. Silence here would reproduce the §6.6 failure mode (a dead feature that looks alive).
- The safety valve is the existing per-app toggle hotkey degrading to a **global** toggle when identity is unknown, plus `Notice::AppIdentityUnavailable` (§4.4) so the toggle does not look broken.

**Do not** attempt to make GNOME Wayland report app ids by shipping a
gnome-shell extension. It moves the product into an out-of-tree JavaScript
component with its own version treadmill, for one desktop, and is out of scope.

---

## 4. What `Platform` must gain or change

Verified against `crates/glowkey-input/src/platform.rs:30-57`. Four changes; the
rest of the trait survives three backends unmodified.

### 4.1 `inject` must state its deletion unit (REQUIRED)

Today: `fn inject(&mut self, backspaces: usize, text: &str)` with backspaces in
**UTF-16 code units** (`platform.rs:31-33`, `handoff.md` §3). macOS and Windows
both delete by synthesizing Backspace keypresses, where the host counts whatever
it counts. An IBus engine deletes with
`ibus_engine_delete_surrounding_text(offset, nchars)` — **Unicode characters**,
signed offset. The two units differ for any astral-plane text, which GlowKey can
produce only through a **macro expansion containing an emoji**. A silent
off-by-N there deletes the user's text.

Proposal — additive, keeps the wire contract explicit:

```rust
/// How many units before the caret an edit removes, in both the units a
/// platform can be asked in. Equal for every string GlowKey's engine renders
/// (Vietnamese precomposed forms are all BMP); they diverge only for macro
/// text containing astral characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Backspaces {
    /// UTF-16 code units. What a Backspace-synthesizing shell needs.
    pub utf16: usize,
    /// Unicode scalar values. What `delete_surrounding_text` needs.
    pub chars: usize,
}

fn inject(&mut self, backspaces: Backspaces, text: &str);
```

`glowkey_engine::KeyResponse` gains `backspaces_chars: usize` beside its existing
`backspaces`, computed in the same `Engine::diff` pass. **Semver note:** this is a
breaking change to a public struct in a crate the `semver` CI job checks against
`v0.1.0`; it must land as `0.2.0` for all three library crates, and the job's
`baseline-rev` moves at that release (`ci.yml` semver job).

Alternative rejected: converting in the Linux shell. It cannot — `backspaces`
counts the *previous render*, which the shell never sees (the blind model).

### 4.2 `inject` needs a delivery-mode escape hatch — but **not** in the trait

The IBus reality is that `delete_surrounding_text` is unsupported by many clients
(terminals especially), which is why `ibus-bamboo` ships six modes. The fallback
is `forward_key_event(BackSpace)` per deletion, then `commit_text`. That choice
is per-client and belongs entirely to the Linux shell's `inject` implementation.
**No trait change.** Resist adding a capability method: the policy must not
branch on delivery, or the ladder stops being one specification.

### 4.3 Custom hotkey codes must be tagged with their origin (REQUIRED)

`KeyEvent::raw_code` is "the platform's own virtual key code, opaque to the
policy" (`event.rs:96-104`), and `HotkeyPreset::Custom { raw_code }`
(`hotkey.rs:44,66-69`) stores it in `settings.json`. With two platforms this
already misfires — `platform/windows/hook.rs:416-431` passes `None` and warns
once that matching falls back to the character. With three, and with Linux
offering *two* code spaces (X11 keycodes vs `xkb`/`evdev` codes vs keysyms), the
warning is not enough.

```rust
/// Which code space a recorded `raw_code` belongs to. A hotkey recorded on one
/// platform must not be matched by integer on another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CodeOrigin { Macos, Windows, LinuxKeycode }

pub enum HotkeyPreset {
    // …
    Custom { control: bool, shift: bool, option: bool, key_char: char,
             raw_code: i64, origin: CodeOrigin },
}

/// `resolve` now decides the fallback itself instead of trusting the caller to
/// pass `None`: it compares `origin` against the running platform.
pub fn resolve(preset: HotkeyPreset, here: CodeOrigin) -> Hotkey;
```

`is_char_fallback()` (`hotkey.rs:110`) keeps its meaning; the one-per-run warning
becomes correct on all three shells instead of only the one that remembered.
Backwards compatibility: `origin` deserializes as `Macos` when absent, which is
true of every settings file written before this change.

### 4.4 `Notice` gains two variants (additive, `#[non_exhaustive]` already)

```rust
/// The shell cannot name the application in front, and this is a property of
/// the session (GNOME Wayland with a client that reports no identity), not a
/// transient startup state. Sent once per identity provider, not per key.
AppIdentityUnavailable { reason: &'a str },
/// The per-app toggle was pressed while identity is unavailable, so it acted
/// globally (or not at all). Distinct from `NoAppInFront`, which means "not yet".
AppToggleDegraded,
```

`platform.rs:60-95` already carries `#[non_exhaustive]`, and Windows already has
a catch-all arm (`hook.rs:518-519`), so this compiles everywhere unchanged.

### 4.5 What does **not** change

- `Key` (`event.rs:18-46`) needs no new variants. Every identity the ladder branches on maps cleanly from an X keysym (`XK_BackSpace`, `XK_Escape`, `XK_space`, `XK_Return`, `XK_Tab`, the arrow/Home/End/Page block → `CaretMove`, letters → `Key::Letter`).
- `app_in_front`, `request_save`, `request_indicator`, `replay_key` are all satisfiable from an IBus engine (`replay_key` = `forward_key_event` of the original keyval/keycode/state, which is exactly the "from our own queue, after the edit" ordering the contract demands).
- `handle()`'s notice ordering and the ladder are untouched.

### 4.6 App-side, outside the trait

- `app/src/platform/mod.rs:11-15` gains `#[cfg(target_os = "linux")] pub mod linux;`.
- `app/src/default_exclusions/mod.rs:22-30` must stop giving Linux the macOS table. New `linux.rs`: lowercased `app_id`/`WM_CLASS`-instance names — `gnome-terminal-server`, `org.gnome.terminal`, `konsole`, `alacritty`, `kitty`, `org.wezfurlong.wezterm`, `xterm`, `code`, `jetbrains-idea`, … plus `CHROMIUM_APP_PREFIXES` = `chromium`, `google-chrome`, `brave`, `microsoft-edge`. The existing `every_terminal_is_also_a_shipped_default` test (`mod.rs:70-80`) covers it for free.
- Identity normalisation is a **Linux shell** concern, not session-crate: `gtk3-im:firefox` → `firefox`; WM_CLASS `("Navigator","firefox")` → `firefox`; wlroots `app_id` `org.mozilla.firefox` → `org.mozilla.firefox`. These three do **not** agree, which is a real correctness hazard (§8 risk L-3): pick one canonical form (lowercased `app_id` when available, else the executable-ish name) and write the mapping table once, with tests.
- `app/src/main.rs` module gates (`:34-63`) widen from `any(macos, windows)` to include `linux` for `default_exclusions`, `log`, `platform`, `prefs_model`, `session_adapter`, `settings_spec`, `settings_store`, `strings`; `main()` gains a Linux arm.
- `settings_store.rs:12,25` gains an XDG arm: `$XDG_CONFIG_HOME/glowkey/settings.json` (default `~/.config`), log at `$XDG_STATE_HOME/glowkey/glowkey.log` (default `~/.local/state`).

---

## 5. Packaging and CI

### 5.1 Linux artifact shape — decision: **`.deb` + `.rpm` + a source/tarball install script. No AppImage, no Flatpak.**

Reason, mechanical rather than aesthetic: an IBus engine is not an application
the user launches. It is registered by an XML component file installed to
`/usr/share/ibus/component/glowkey.xml`, whose `<exec>` element names an absolute
path that `ibus-daemon` execs. AppImage has no install step and no stable path,
so nothing registers the engine. Flatpak sandboxes the app away from the host's
`ibus-daemon` and from `/usr/share/ibus/component/`; an input method must be a
host service, which is precisely why no IME ships as a Flatpak. Both formats
would produce a binary the user can run and that does nothing.

Artifacts per release tag:

- `glowkey_<ver>_amd64.deb` and `glowkey-<ver>.x86_64.rpm` — binary at `/usr/lib/ibus-glowkey/ibus-engine-glowkey`, component XML, `.desktop` for the settings window, icons. `aarch64` variants once demand exists; not phase-1 scope.
- `glowkey-<ver>-linux-x86_64.tar.gz` + `install.sh` for everything else (Arch users will package it themselves; a PKGBUILD is a community artifact, not ours).
- Post-install/`postrm` must **not** restart `ibus-daemon` behind the user's back; print the one line `ibus restart` instead.

macOS and Windows artifact shapes are unchanged (`scripts/make-dmg.sh`,
`.github/workflows/release.yml`; Windows packaging remains deliberately blocked
on Phase 6 verification per `README.md:85-87`).

### 5.2 CI jobs to add (`.github/workflows/ci.yml`)

| Job | Runner | Content |
|---|---|---|
| `linux-shell` | `ubuntu-latest` | `cargo clippy -p glowkey --all-targets -- -D warnings`, `cargo build -p glowkey`, `cargo test -p glowkey`. This is the first place the Linux backend is *linked*, the same argument the `windows` job header already makes (`ci.yml`, windows job comment). |
| `linux-privacy` | `ubuntu-latest` | The Linux half of the existing two privacy guards. `ldd target/debug/GlowKey` must not name `libcurl`, `libssl`, `libnghttp2`, `libcares`; and must name `libc` (so the check is proven capable of finding something, mirroring the `USER32.dll` positive control in the Windows job). D-Bus over a unix socket is not networking and is allowed — say so in the job comment or someone will "fix" it. |
| `linux-x11-identity` | `ubuntu-latest` | `Xvfb` + a minimal WM + a test binary that opens a window with a known `WM_CLASS` and asserts the X11 identity provider returns it. This is the **only** part of the Linux app-identity story that CI can actually prove. |
| `packaging` | `ubuntu-latest` | Build the `.deb` and `.rpm`, then `dpkg-deb -c` / `rpm -qlp` assert the component XML lands at `/usr/share/ibus/component/glowkey.xml` and the `<exec>` path matches the installed binary path. Catches the single most likely silent packaging break. |

Existing jobs to amend: `engine` already `cargo check -p glowkey` on Linux — once
a real Linux backend exists this stops being a stub check and starts compiling
the shell, so its comment (`ci.yml`, engine job, last step) must be updated or it
becomes a lie. `semver` `baseline-rev` moves at the `0.2.0` release forced by §4.1.

### 5.3 What CI cannot test (state it in the job comments, as the repo already does)

- That GlowKey types Vietnamese anywhere. Same limitation the `windows` job already documents.
- Any Wayland compositor path: `wlr-foreign-toplevel-management` needs a running wlroots compositor; `plasma-window-management` needs KWin; the GNOME/IBus client-name path needs a full GNOME Wayland session with `ibus-daemon` and a GTK app. Nested sway in CI is *possible* for the wlroots provider and is the one Wayland provider worth attempting later; the other two are not CI-reachable.
- `delete_surrounding_text` support per client — a property of GTK/Qt/VTE/Electron at runtime.
- IBus registration end to end (needs a session bus and a real `ibus-daemon` with the engine installed to a system path).
- Windows: elevated windows, dead keys, AltGr, Chrome, anti-cheat. Unchanged.
- macOS: the Accessibility permission gate, the tap health monitor, every pixel.

---

## 6. Phased plan

Effort is in **engineer-days** for one person already fluent in this codebase.
"HW" = blocked on hardware or a human at a machine; those phases cannot be
completed by CI or by an agent.

```
P0 (HW) ─┐
W1..W4 ──┼─▶ P1 (port changes) ─┬─▶ L1 ─▶ L2 ─▶ L3 ─▶ L4 ─▶ L5 ─▶ L6 (HW)
M1 (HW) ─┘                      └─▶ W5 (Windows re-verify, HW)
```

### P0 — League discriminating test — 0.25 d — **HW, blocking nothing else**

Run §2 exactly. **Depends on:** nothing. **Blocks:** any standalone-`w` work
(which is *not* in this plan's scope until P0 returns (a)).
**Acceptance:** a written verdict (a) / (b) / mixed, with the three log lines
and the Notepad control result, appended to
`plans/reports/windows-handoff-260905.md`.
**Rollback:** restore `settings.json` from `settings.json.bak-before-league`.
**Risk:** Low×High — if skipped, a wasted engine feature.

### W1 — Windows hotkey recording — 1.5 d

Gaps **W1**, **W2**. Files: `platform/windows/hook.rs`,
`platform/windows/settings_ui.rs`, `platform/windows/shell.rs`, and (for W2)
`crates/glowkey-input/src/hotkey.rs` + `app/src/prefs_model.rs` per §4.3.
**Depends on:** P1 for the `CodeOrigin` half; the recorder UI half can start now.
**Acceptance:** "Custom…" appears in the Windows hotkey control; arming captures
the next Ctrl/Alt combination; Esc, a mouse click and an app switch each cancel;
`Ctrl+Shift+E` and `Ctrl+Shift+W` are refused with the reserved reason; the
recorded combination survives a restart and fires; a hotkey recorded on macOS
loads on Windows as char-fallback **with the warning logged once**; headless
tests in `crates/glowkey-input/tests/` pin `resolve` for all three origins.
**Rollback:** the recorder is one branch ahead of `handle` in `hook.rs`; deleting
it restores today's behaviour. The `CodeOrigin` field defaults to `Macos`, so a
revert leaves settings files readable.
**Risk:** Medium×Medium — the recorder sits in the keystroke path; a bug there
swallows keys. Mitigation: it may only intercept while armed, exactly as macOS
does (`platform/macos/dispatch.rs:288-305`), and arming is modal and timed out.

### W2 — Windows shell parity — 1.5 d

Gaps **W3**–**W9**. Files: `platform/windows/tray.rs`,
`platform/windows/indicator.rs`, `platform/windows/hook.rs`,
`platform/windows/ui_thread.rs`, `platform/windows/settings_ui.rs`, new
`platform/windows/welcome_ui.rs`.
**Depends on:** nothing. **Owns files disjoint from W1 except `hook.rs` and
`settings_ui.rs`** — therefore **W1 and W2 must not run in parallel**; sequence
them, W1 first.
**Acceptance:** tray menu contains a state header, "Reset input (if stuck)",
"Quick Guide…", and a mode item naming the *current* hotkey; a session-only
terminal un-exclusion shows a distinct warning indicator and its tooltip says
"until restart"; ⌃⇧W with the Personal Words viewport open updates the list
without reopening it; `welcome_shown` gates the guide to once and the menu item
reopens it. Each item confirmed by a screenshot in the report (the Tier-5 method).
**Rollback:** per-item; each is an independent menu entry or indicator variant.
**Risk:** Low×Low. `Indicator` gaining a variant is exhaustively matched, so the
compiler finds every site.

### W3 — Windows exclusion table verification — 0.5 d — **HW**

Gap **W10**. File: `app/src/default_exclusions/windows.rs` (data only).
**Acceptance:** every entry confirmed against a real process name on a Windows
machine (`Get-Process | Select ProcessName`), corrections listed in the report,
and any renamed entry given a tombstone-safe migration note.
**Risk:** Low×High — a wrong terminal name reintroduces backspaces into a
terminal, the exact bug the list prevents.

### P1 — Port changes for three backends — 2 d

§4.1, §4.3, §4.4, plus `app/src/platform/mod.rs`,
`app/src/default_exclusions/{mod.rs,linux.rs}`, `app/src/main.rs` gates,
`app/src/settings_store.rs` XDG arm.
**Depends on:** nothing. **Blocks:** L1–L6 and the W2 half of W1.
**Acceptance:** all three library crates at `0.2.0`; `cargo test --workspace`
green on macOS, Windows and Linux; `semver` job passes against the new tag;
macOS and Windows shells compile and their suites pass with `Backspaces`
threaded through; a Linux `cargo build -p glowkey` produces a binary that starts
and logs "no backend" (the shell arrives in L2, so this phase must leave the
stub honest, not half-built).
**Backwards compatibility:** `origin` absent → `Macos`; `backspaces_chars`
absent from no file (it is not persisted); settings files byte-compatible.
**Rollback:** one commit; the trait change is source-level and has no on-disk
footprint.
**Risk:** Medium×High — `Backspaces` touches the emit path on two shipping
platforms. Mitigation: `utf16` and `chars` are computed in the same place and a
property test asserts they agree for every string the engine renders; only macro
text can diverge, and that gets its own test with an astral macro.

### L1 — Linux identity providers — 2.5 d

New `app/src/platform/linux/identity/{mod.rs,x11.rs,wlr.rs,plasma.rs,ibus_client.rs}`.
A trait with four implementations and a startup probe that picks one, logs which,
and reports `AppIdentityUnavailable` when none applies. Pure Rust: `x11rb` for
X11, `wayland-client` for the two Wayland protocols.
**Depends on:** P1. **Acceptance:** the X11 provider returns the right
`WM_CLASS` under Xvfb in CI (`linux-x11-identity` job); each provider has a unit
test over a recorded protocol exchange; the probe order and the chosen provider
appear in one log line at startup; the canonicalisation table (§4.6) is tested
against all three identity spellings of Firefox.
**Rollback:** the provider set is behind one trait; falling back to
"always unknown" degrades to §3.5 rather than breaking.
**Risk:** High×Medium — three protocols, only one CI-testable.

### L2 — The IBus engine — 4 d

New `app/src/platform/linux/{mod.rs,adapt.rs,engine.rs,inject.rs,log.rs}`.
D-Bus via `zbus`; keysym→`KeyEvent` translation in `adapt.rs`;
`Platform` implementation in `engine.rs`; the delivery-mode ladder
(surrounding-text → forward-key-event) in `inject.rs`, per-client, remembered.
**Depends on:** P1, L1. **Acceptance:** `ibus-glowkey` registers and appears in
the IBus engine list; `hoongf` → `hồng` in a GTK app, compared **against code
points, not glyphs** (the discipline `plans/reports/windows-verification-260905.md`
established); mid-word backspace, boundary re-composition and auto-fix all
verified the same way; key-release events are ignored; a client without
surrounding-text support falls back and is logged once; the per-app ignore list
suppresses transformation in GNOME Terminal.
**Rollback:** the package is uninstallable and the engine is inert unless
selected in IBus; no global system state is changed.
**Risk:** High×High — this is the phase that can be wrong in ways only a human
notices. Mitigation: the `crates/glowkey-input` ladder is already proven; the new
surface is translation and delivery only, and both get a code-point harness.

### L3 — Linux shell: settings, indicator, tools — 2.5 d

Reuse `settings_spec.rs` with the existing `eframe`/`egui` renderer
(`platform/windows/settings_ui.rs` is already spec-driven and the crate is
already built with `wayland` and `x11` features — `app/Cargo.toml`, eframe
line). Extract the renderer to a shared module rather than copying it; that is
the whole point of `decisions/0010`. Indicator and menu via IBus engine
**properties** (native on GNOME, no tray needed) with an optional
StatusNotifierItem tray where one exists. Clipboard tools via `wl-clipboard`/X11
selections.
**Depends on:** L2. **Acceptance:** the four tabs render with correct Vietnamese
glyphs (the Windows font lesson applies: the default egui font cannot draw Latin
Extended Additional — load Noto Sans/DejaVu from fontconfig with the same
alphabet-drawable test `settings_ui.rs` has); every spec row is interactive; the
three list editors open; VI/EN and the per-app toggle are reachable without the
settings window; clipboard tools round-trip.
**File ownership warning:** extracting the shared renderer edits
`platform/windows/settings_ui.rs`, so **L3 cannot run in parallel with W1 or W2**.
**Risk:** Medium×Medium.

### L4 — Packaging — 1.5 d

New `packaging/linux/{glowkey.xml,glowkey.desktop,deb/,rpm/}`,
`scripts/build-linux.sh`. **Depends on:** L2. **Acceptance:** `.deb` installs on
Ubuntu LTS and Fedora's `.rpm` installs, both register the engine after
`ibus restart`, both uninstall cleanly leaving `~/.config/glowkey` intact; the
`packaging` CI job asserts the component XML path and `<exec>` path.
**Rollback:** artifacts are additive; no existing release path changes.
**Risk:** Medium×Medium — path mismatches are the classic silent failure, hence
the CI assertion.

### L5 — CI — 0.5 d

The four jobs in §5.2 plus the amended comments. **Depends on:** L2 (jobs must
have something to build). **Acceptance:** all jobs green on a PR; the privacy
job's positive control demonstrably fails when a networking crate is added
(prove it once on a scratch branch, as the Windows job's `USER32.dll` control
does).
**Risk:** Low×Low.

### L6 — Linux hardware verification — 2 d — **HW**

A tiered checklist mirroring `docs/manual-verification-windows.md`, run on at
least: GNOME Wayland (Fedora or Ubuntu LTS), GNOME X11, KDE Plasma Wayland, Sway.
**Depends on:** L1–L4. **Acceptance:** `docs/manual-verification-linux.md`
written and executed, with per-session results for: typing correctness against
code points, per-app ignore list in a terminal and a browser, the identity
provider chosen, the delivery mode chosen per client class (GTK / Qt / VTE /
Electron / XWayland), hotkeys, and idle CPU. **Every unticked row stays unticked**
— the repo's existing convention (`handoff.md` §8).
**Risk:** High×High — this is where the architecture is either confirmed or not.
**Rollback:** if GNOME Wayland fails outright, ship Linux as "X11 and wlroots and
Plasma; GNOME Wayland experimental" rather than delaying the other three.

### M1 — macOS runtime pass — 1 d — **HW**

Not new work: `docs/handoff.md` §11 item 1 is still open and the macOS side of
the engine split and shared spec is **compile-checked only**. Parity cannot be
claimed for macOS while that is true, and P1 touches the macOS emit path.
**Depends on:** P1 (run it after, so one pass covers both).
**Acceptance:** the §11 item-1 checklist executed and recorded, plus the
`Backspaces` change exercised by real typing including one astral macro.
**Risk:** Medium×High — an unverified emit-path change on the platform users
actually run today.

### W4 — Windows re-verification after P1/W1/W2 — 1 d — **HW**

Tier 1 re-run with EVKey confirmed stopped, plus the Tier-5 leftovers that need a
real click (`plans/reports/windows-verification-260905.md`, "User-owned" list 1-4).
**Depends on:** W1, W2, P1. **Acceptance:** the five user-owned items ticked or
explicitly left with a reason.

**Total: ~20.5 engineer-days**, of which **~4.75 are hardware/human-blocked**
(P0, W3, L6, M1, W4) and cannot be compressed by adding engineers.

### Sequencing constraints (file ownership)

- `platform/windows/hook.rs` is touched by **W1** and **W2** → serialise.
- `platform/windows/settings_ui.rs` is touched by **W1**, **W2** and **L3** → serialise; L3 last.
- `crates/glowkey-input/src/hotkey.rs` is touched by **W1** and **P1** → do the `CodeOrigin` change once, in P1, and let W1 consume it.
- `crates/glowkey-engine` is touched only by **P1**.
- **L1 and W2 are genuinely parallel** (disjoint file sets, no shared contract).

---

## 7. Test matrix

| Level | What | Where | Runs in CI |
|---|---|---|---|
| Unit | ladder, hotkey resolve for 3 origins, `Backspaces` agreement, keysym→`KeyEvent`, identity canonicalisation, exclusion tables | `crates/*/tests`, `app/src/platform/linux/*` `#[cfg(test)]` | yes |
| Unit | `Backspaces.utf16 == chars` for every engine render; diverges only for an astral macro | `crates/glowkey-engine` property test | yes |
| Integration | X11 identity provider against a real `WM_CLASS` | new `linux-x11-identity` job, Xvfb | yes |
| Integration | packaging paths | new `packaging` job | yes |
| Integration | link-time: Linux shell builds and links; no networking library | `linux-shell`, `linux-privacy` | yes |
| E2E | typing correctness, code points not glyphs | `docs/manual-verification-linux.md`, `scripts/verify-windows-tier*.ps1`, `docs/manual-verification.md` | **no** |
| E2E | per-app ignore list per session type | manual, L6 | **no** |
| E2E | League / anti-cheat | P0, manual | **no** |

---

## 8. Risk register (High items only)

| id | Risk | L×I | Mitigation |
|---|---|---|---|
| L-1 | GNOME Wayland per-app identity turns out unusable in practice (client names absent or useless for the apps that matter) | M×H | Probe order + honest degradation (§3.5) is designed in from the start, not retrofitted. L6 tests it explicitly. Fallback: ship GNOME Wayland as experimental. |
| L-2 | `delete_surrounding_text` unsupported in enough clients that the fallback becomes the default, and the fallback races (forwarded Backspace is a *native* key from the client's perspective — the exact race `handoff.md` §5 exists to remove) | M×H | This is the one place the full-suppression invariant does not hold on Linux. Delivery mode is per-client and remembered (ibus-bamboo precedent); the fallback must forward Backspace **and** the commit through the same IBus channel so ordering is preserved by the bus, and L6 must test it in Electron and VTE specifically. If it races, the honest answer is preedit mode for that client class, accepting underlined text. |
| L-3 | Three identity spellings for the same app (`gtk3-im:firefox`, `WM_CLASS=firefox`, `app_id=org.mozilla.firefox`) make one ignore-list entry match on one session and not another | H×M | One canonical form, one mapping table, tested. Excluded-apps window shows the raw identity alongside, so a user can see why a rule did not fire. |
| P-1 | `Backspaces` regresses the macOS/Windows emit path | M×H | Property test + M1/W4 human passes before release. |
| W-1 | League turns out to be cause (b) | ?×H | P0 answers it before any engineering is spent. |
| X-1 | A `settings.json` carried between OSes contains foreign app identities (bundle ids on Linux, etc.) that silently never match | M×M | Not a crash: the merge rule `saved ∪ (defaults − removed)` tolerates it. Document it, and have the Excluded Apps window mark entries whose form does not match this platform. |

---

## 9. Unknowable without a Linux machine (explicit list)

1. Whether `ibus-glowkey` registers and is selectable on a stock GNOME/Fedora session.
2. Whether the IBus `focus_in_id` client name is populated for the apps that matter (Firefox, Chromium/Electron, VS Code, GNOME Terminal, Konsole) — the format is documented, the *coverage* is not.
3. Which IBus delivery mode each client class actually supports, and whether the forwarded-Backspace fallback races (risk L-2).
4. Whether `wlr-foreign-toplevel-management` and `plasma-window-management` return an identity that matches what the user would call the app.
5. Idle CPU/RSS of the engine process across a long session.
6. Whether the egui settings window renders correctly under Wayland and X11, with correct Vietnamese glyphs from fontconfig.
7. Whether the `.deb`/`.rpm` post-install leaves IBus in a working state without a session restart.
8. Whether XWayland clients reach GlowKey at all (they go through XIM / `ibus-daemon --xim`, a fourth path).
9. Keyboard-layout interaction: whether GlowKey correctly wraps a non-US XKB layout (the product's premise) rather than assuming US.
10. Whether any of this survives a Fcitx5-based distro where `ibus-daemon` is replaced.

Also unknowable without the **Windows** machine: everything in §1d, plus the P0
verdict. Unknowable without the **Mac**: everything in `handoff.md` §11 item 1.

---

## Unresolved questions

1. **Is the League defect (a) or (b)?** Everything downstream of it is on hold. §2 is the test.
2. **Should Linux ship a preedit (underlined) mode at all?** GlowKey's identity is "no marked text" (`handoff.md` §1). If risk L-2 materialises, preedit is the only correct answer for some clients, and that is a product decision — not one this plan should make silently.
3. **Does the repo want a second IME frontend (Fcitx5) later?** The IBus choice does not preclude it, but it doubles the delivery surface. Recommend: no, unless L6 finds IBus failing on a distro that matters.
4. **`0.2.0` timing.** §4.1 forces a breaking release of all three library crates while only `glowkey-engine` is published (`handoff.md` §11 item 4). Publish the engine at `0.2.0` and the other two for the first time at the same version, or hold the port change until the publish story is settled?
5. **Is a tray required on Linux at all**, given IBus properties give a native menu on GNOME? Shipping both is more code and more places to disagree.
