# GlowKey

A Vietnamese Telex input method for macOS. Type `hoongf`, `hofong`, or `hoonfg`
and get `hồng` — the tone key can go anywhere. Its distinguishing feature is a
**per-application ignore list**: name the apps where Vietnamese input should never
fire (terminals, editors, IDEs), and it never does.

Built as a proper macOS InputMethodKit input method — no Accessibility prompt, and
it works in password fields — entirely in Rust via `objc2`.

> **Status: early. The engine works; the input method shell is being wired.**
> See [`docs/checkpoint.md`](docs/checkpoint.md) for exactly what is done and what
> is next.

## Layout

```
crates/glowkey-engine/   Vietnamese Telex logic + the ignore list. Platform-free, tested.
app/                     macOS InputMethodKit shell (objc2). Assembled into GlowKey.app.
scripts/build-app.sh     Builds a universal GlowKey.app bundle.
docs/                    Checkpoint, decisions.
```

## Develop

```
cargo test -p glowkey-engine     # the tested core — runs anywhere
cargo build --workspace          # full build, macOS
./scripts/build-app.sh           # produce build/GlowKey.app
```

## Privacy

GlowKey sees your keystrokes because that is its job, and does nothing else with
them — no network, no logging of typed text, no persistence. CI fails the build if
the binary ever links a networking framework. See [`PRIVACY.md`](PRIVACY.md).

## License

MIT. Vietnamese transformation by the MIT-licensed [`vi`](https://github.com/ZeroX-DG/vi-rs)
crate; see [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md).
