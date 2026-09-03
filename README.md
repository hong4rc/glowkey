# GlowKey

A Vietnamese input method for macOS in the style of EVKey/Unikey. Type `hoongf`,
`hofong`, or `hoonfg` and get `hồng` — the tone key can go anywhere. Telex and
VNI. Its distinguishing feature is a **per-application ignore list**: name the
apps where Vietnamese input should never fire (terminals, editors, IDEs), and it
never does — toggle the current app any time with ⌃⇧E.

GlowKey is a background menu-bar agent built on a **`CGEventTap`** (the
EVKey/OpenKey architecture), entirely in Rust via `objc2`. It wraps whatever
keyboard layout is active — Colemak, US, anything — and adds Vietnamese on top;
there is no input-source switching. Consequences of that architecture: it needs
an **Accessibility** permission, and it cannot type in password fields (macOS
withholds secure input from event taps).

> **Status: feature-complete against the useful Unikey/EVKey set** — Telex/VNI,
> per-app exclusions, auto-fix, macros (gõ tắt), auto-capitalize, configurable +
> recordable toggle hotkey, opt-in English word restore, Vietnamese interface,
> macro import/export, opt-in Quick Telex. Live GUI verification ongoing. See
> [`docs/handoff.md`](docs/handoff.md) for the full state.

## Layout

```
crates/glowkey-engine/      Vietnamese logic, settings, ignore list. Platform-free, tested.
app/                        macOS shell (objc2): event tap, menu bar, Settings, HUD.
scripts/build-app.sh        Builds a universal app bundle (release or dev variant).
scripts/make-dmg.sh         Packages build/GlowKey.app into a distributable disk image.
scripts/release-install.sh  Builds GlowKey.app, installs it to /Applications, launches it.
scripts/dev-run.sh          Builds and runs "GlowKey Dev" with debug logging.
docs/                       Handoff (start here), decision records, manual verification, UI design.
```

## Install

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

## Develop

```
cargo test --workspace           # the headless proof — engine + tap decision tests
cargo clippy --workspace --all-targets
bash scripts/release-install.sh  # ship it: build → /Applications → launch
bash scripts/dev-run.sh          # iterate: build and run "GlowKey Dev" in the foreground
```

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
