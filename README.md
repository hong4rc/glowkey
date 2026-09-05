# GlowKey

A Vietnamese input method in the style of EVKey/Unikey. Type `hoongf`,
`hofong`, or `hoonfg` and get `hồng` — the tone key can go anywhere. Telex and
VNI. Its distinguishing feature is a **per-application ignore list**: name the
apps where Vietnamese input should never fire (terminals, editors, IDEs), and it
never does — toggle the current app any time with ⌃⇧E.

GlowKey is a background agent that wraps whatever keyboard layout is active —
Colemak, US, anything — and adds Vietnamese on top; there is no input-source
switching. Entirely in Rust, with a native shell per platform and no web runtime.

**macOS** is built on a **`CGEventTap`** (the EVKey/OpenKey architecture) via
`objc2`. It needs an **Accessibility** permission and cannot type in password
fields, because macOS withholds secure input from event taps.

**Windows** is built on a **`WH_KEYBOARD_LL`** hook plus `SendInput` — the same
intercept/suppress/inject shape, and deliberately not TSF
([why](docs/decisions/0009-windows-low-level-hook.md)). It needs no special
permission. It cannot type into a window running elevated (Task Manager, regedit,
an elevated terminal), because Windows blocks input from ordinary programs into
elevated ones. GlowKey detects that and says so in the tray rather than failing
silently, and it does **not** ask for administrator rights to work around it — an
input method requesting those is a red flag, and correctly so.

> **macOS — feature-complete against the useful Unikey/EVKey set**: Telex/VNI,
> per-app exclusions, auto-fix, macros (gõ tắt), auto-capitalize, configurable +
> recordable toggle hotkey, opt-in English word restore, Vietnamese interface,
> macro import/export, opt-in Quick Telex. Live GUI verification ongoing.
>
> **Windows — early.** The input core, tray and settings window are built and the
> engine's full test suite passes there. `hoongf` → `hồng` and the Backspace and
> auto-fix cases are verified in Notepad against code points. **Chrome, Windows
> Terminal, VS Code, Electron apps, elevated windows, dead-key layouts and AltGr
> are all unverified**, and hotkey recording is not implemented yet. Do not treat
> it as ready to rely on. See
> [`plans/reports/windows-verification-260905.md`](plans/reports/windows-verification-260905.md)
> for exactly what has and has not been established.
>
> See [`docs/handoff.md`](docs/handoff.md) for the full state.

## Layout

```
crates/glowkey-engine/      The Vietnamese transformation. Depends on `vi` and `phf` only.
crates/glowkey-session/     Typing policy over it: mode, ignore list, auto-fix, macros. No OS.
crates/glowkey-input/       The decision ladder and the `Platform` port every shell implements.
app/src/platform/macos/     macOS shell (objc2): event tap, menu bar, Settings, HUD.
app/src/platform/windows/   Windows shell: WH_KEYBOARD_LL hook, SendInput, tray, settings.
scripts/build-app.sh        Builds a universal macOS app bundle (release or dev variant).
scripts/make-dmg.sh         Packages build/GlowKey.app into a distributable disk image.
scripts/release-install.sh  Builds GlowKey.app, installs it to /Applications, launches it.
scripts/dev-run.sh          Builds and runs "GlowKey Dev" with debug logging.
scripts/verify-windows-*.ps1  Windows verification harnesses (type into Notepad, check code points).
docs/                       Handoff (start here), decision records, manual verification, UI design.
```

Which crate to take: `glowkey-engine` for Vietnamese typing alone;
`glowkey-session` for an input method's behaviour (it re-exports the engine);
`glowkey-input` for GlowKey's whole keyboard policy behind a `Platform` trait a
shell implements. Each has its own README, and
[`docs/decisions/0012`](docs/decisions/0012-engine-layering-and-ports.md)
records the layering.

## Install

### macOS

Download the disk image from the [releases page](../../releases) and drag
**GlowKey** to Applications. macOS will refuse to open it — *"GlowKey is damaged
and can't be opened"* — because the app is signed but not notarized, which needs
a paid Apple Developer account this project does not have. It is not damaged.
Clear the quarantine flag once:

```
xattr -dr com.apple.quarantine /Applications/GlowKey.app
```

Then open it. GlowKey asks for the **Accessibility** permission and starts by
itself once you grant it in System Settings → Privacy & Security →
Accessibility.

### Windows

There is no release artifact yet — packaging deliberately waits until the port
has been verified in real applications, which it has not been. Build it yourself:

```powershell
cargo build --release -p glowkey
.\target\release\GlowKey.exe
```

A tray icon appears; right-click it for the menu. Settings live in
`%APPDATA%\GlowKey\settings.json` and the log in
`%LOCALAPPDATA%\GlowKey\Logs\glowkey.log`.

Expect SmartScreen to object to an unsigned binary, and expect antivirus to take
an interest: a low-level keyboard hook is, structurally, what a keylogger does.
The defence is that the privacy posture is checkable rather than promised — no
network dependency is linked, CI asserts that, and the source is here.

## Develop

```
cargo test --workspace           # the headless proof — engine + policy + platform units
cargo clippy --workspace --all-targets
```

macOS:

```
bash scripts/release-install.sh  # ship it: build → /Applications → launch
bash scripts/dev-run.sh          # iterate: build and run "GlowKey Dev" in the foreground
```

Windows — these type into a real Notepad and compare code points, so they prove
something `cargo test` cannot:

```powershell
.\scripts\verify-windows-tier0.ps1   # do modifiers and hotkeys reach the hook
.\scripts\verify-windows-tier1.ps1   # hoongf → hồng, backspace, boundary, auto-fix
```

Stop any other Vietnamese input method first, or you will be measuring it instead.

The dev loop builds a **separate app** — `GlowKey Dev`, its own bundle identifier
— so it holds its own Accessibility permission and iterating never disturbs the
grant of the GlowKey you actually type with. Never run both at once: two event
taps process every keystroke twice, and both scripts stop both variants first.

The grant follows the **code signature**, so how often you re-grant depends on
how the app is signed. Ad-hoc — the default with no certificate — keys the grant
to a hash of the code, so every code change drops it. Create a self-signed
certificate once (Keychain Access → Certificate Assistant → Create a Certificate,
name `GlowKey Developer`, type "Code Signing", self-signed) and `build-app.sh`
picks it up automatically, after which a rebuild keeps the grant. `build-app.sh`
prints which identity it used and the resulting requirement, so you can see which
case you are in. Details in
[`docs/decisions/0006`](docs/decisions/0006-stable-signing-identity.md).

Either way the app asks for the permission on screen and starts by itself once
you enable it.

## Privacy

Everything happens on your Mac: no network, no analytics, no accounts. CI fails
the build if the binary ever links a networking framework. For diagnosing typing
bugs GlowKey keeps a **local** log of handled keys (including typed word content)
at `~/Library/Logs/GlowKey/glowkey.log` — it never leaves the machine, is capped
at 5 MB, and can be deleted at any time. Details in [`PRIVACY.md`](PRIVACY.md).

## License

MIT. Vietnamese transformation by the MIT-licensed [`vi`](https://github.com/ZeroX-DG/vi-rs)
crate; see [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md).
