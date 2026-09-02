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
> recordable toggle hotkey, opt-in English word restore. Live GUI verification
> ongoing. See [`docs/handoff.md`](docs/handoff.md) for the full state.

## Layout

```
crates/glowkey-engine/   Vietnamese logic, settings, ignore list. Platform-free, tested.
app/                     macOS shell (objc2): event tap, menu bar, Settings, HUD.
scripts/build-app.sh     Builds a universal GlowKey.app bundle.
scripts/dev-run.sh       Stop + rebuild + relaunch with debug logging.
docs/                    Handoff (start here), decision records, UI design.
```

## Develop

```
cargo test --workspace           # the headless proof — engine + tap decision tests
cargo clippy --workspace --all-targets
bash scripts/build-app.sh release   # produce build/GlowKey.app
```

After a rebuild the ad-hoc re-sign can drop the Accessibility grant — re-enable
GlowKey in System Settings → Privacy & Security → Accessibility.

## Privacy

Everything happens on your Mac: no network, no analytics, no accounts. CI fails
the build if the binary ever links a networking framework. For diagnosing typing
bugs GlowKey keeps a **local** log of handled keys (including typed word content)
at `~/Library/Logs/GlowKey/glowkey.log` — it never leaves the machine, is capped
at 5 MB, and can be deleted at any time. Details in [`PRIVACY.md`](PRIVACY.md).

## License

MIT. Vietnamese transformation by the MIT-licensed [`vi`](https://github.com/ZeroX-DG/vi-rs)
crate; see [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md).
