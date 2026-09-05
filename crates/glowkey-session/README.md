# glowkey-session

The typing policy of [GlowKey](https://github.com/hong4rc/glowkey), built on
[`glowkey-engine`](../glowkey-engine) and re-exporting it: VN/EN mode, the
per-application ignore list, auto-fix at a word boundary, sentence
capitalisation, text-expansion macros and per-word overrides.

Still no operating system in it. What an application is called (a bundle
identifier, an executable name) is an opaque `AppId`, and which applications
ship excluded is handed in by the caller as `ExclusionDefaults`.

## Use

```rust
use glowkey_session::{ExclusionDefaults, ExclusionList, Session};

// The caller's tables: what ships excluded, and which of those are terminals
// (a hotkey only suspends a terminal's exclusion until restart).
let defaults = ExclusionDefaults::new(["com.apple.Terminal", "com.microsoft.VSCode"], ["com.apple.Terminal"]);
let mut session = Session::builder()
    .exclusions(ExclusionList::with_defaults(defaults))
    .auto_fix(true)
    .build();

session.set_frontmost_app("com.apple.TextEdit");
let edit = session.process_key('a'); // KeyResponse { backspaces, insert, .. }
assert!(session.is_active());

session.set_frontmost_app("com.apple.Terminal");
assert!(!session.is_active()); // excluded: keys pass through untouched
```

`Session` is the facade: feed it keys with `process_key`, boundaries with
`commit`, Backspace with `backspace`, and read the edits it returns. It keeps
the recent words so deleting back into a committed word re-opens it, and it
restores raw keystrokes when a word turns out not to be Vietnamese.

## Persistence

`serde` derives for `Macro`, `WordOverride`, `InputMode` and the engine's
`InputMethod` and `PlacementStyle` are behind the optional `serde` feature.
`ExclusionList::from_saved(ids, removed_defaults, defaults)` rebuilds the list
from a settings file so that a default added in a later release reaches
existing files without resurrecting one the user removed on purpose.

## Licence

MIT.
