# glowkey-input

The keyboard policy of [GlowKey](https://github.com/hong4rc/glowkey): what to
do with one key-down event. Built on [`glowkey-session`](../glowkey-session);
no operating system in it, no `unsafe`, and CI proves both on Linux.

## The boundary

```text
platform  ──  KeyEvent  ──▶  handle ─┬─▶  decide  ──▶  Decision
                                     │
                                     └─▶  Platform::{inject, replay_key,
                                           app_in_front, request_save,
                                           request_indicator, notify}
```

`decide` is the decision ladder: pure, data in and data out, and its step order
is a specification written by bugs that reached users. It returns a `Decision`
(pass the key through, consume it, toggle the app, emit an edit, emit then
replay the key) and plain-data `Effects`.

`Platform` is the port a shell implements, and `handle` is the one call a shell
makes per key: it runs `decide`, then carries the decision and its effects out
through the trait in a fixed order. Every method on the trait is something both
of GlowKey's shells (a macOS event tap, a Windows low-level hook) already did.

## Use

```rust,ignore
use glowkey_input::{handle, hotkey, Ctx, HotkeyPreset, KeyEvent, Notice, Platform};
use glowkey_session::{AppId, Session};

struct MyShell { /* whatever your platform needs */ }

impl Platform for MyShell {
    fn inject(&mut self, backspaces: usize, text: &str) { /* SendInput, CGEventPost, … */ }
    fn replay_key(&mut self) { /* type the key being handled again, from your own source */ }
    fn app_in_front(&mut self) -> Option<AppId> { /* the frontmost app, if known */ None }
    fn request_save(&mut self) { /* set a flag; write the file later, never here */ }
    fn request_indicator(&mut self) { /* repaint the tray or menu bar */ }
    fn notify(&mut self, notice: Notice<'_>) { /* log it, flash a HUD, or ignore it */ }
}

fn on_key_down(session: &mut Session, shell: &mut MyShell, event: KeyEvent) -> bool {
    let ctx = Ctx { toggle_hotkey: hotkey::resolve(HotkeyPreset::default(), None) };
    handle(session, &event, &ctx, shell).suppresses()
}
```

Nothing a `Platform` method does may block on another process or touch the
disk: it runs inside the key callback while the original keystroke is being
dispatched. Set flags and act after the callback returns.

`hotkey` holds the toggle-hotkey presets and matching, and `hotkey::capture`
for recording a custom one. `HotkeyPreset` derives `serde` behind the optional
`serde` feature.

## Licence

MIT.
