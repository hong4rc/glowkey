//! macOS application identities: bundle identifiers, as `NSWorkspace` reports
//! them.

/// Terminal applications. Synthetic backspaces cannot delete inside a PTY (the
/// shell owns line editing), so Vietnamese in a terminal always produces
/// garbage. Un-excluding one via the ⌃⇧E hotkey is therefore session-only — a
/// deliberate, permanent removal must go through the Excluded Apps window.
pub const TERMINAL_EXCLUSIONS: &[&str] = &[
    "com.apple.Terminal",
    "com.googlecode.iterm2",
    "dev.warp.Warp-Stable",
    "dev.warp.Warp-Preview",
    "net.kovidgoyal.kitty",
    "com.github.wez.wezterm",
    "com.mitchellh.ghostty",
    "org.alacritty",
    "co.zeit.hyper",
];

/// Applications excluded on first run — terminals and editors, where users
/// overwhelmingly want raw ASCII.
pub const DEFAULT_EXCLUSIONS: &[&str] = &[
    "com.apple.Terminal",
    "com.googlecode.iterm2",
    "com.apple.dt.Xcode",
    "com.microsoft.VSCode",
    "com.jetbrains.intellij",
    "com.jetbrains.pycharm",
    "com.jetbrains.WebStorm",
    "dev.warp.Warp-Stable",
    "dev.warp.Warp-Preview",
    "net.kovidgoyal.kitty",
    "com.github.wez.wezterm",
    "com.mitchellh.ghostty",
    "org.alacritty",
    "co.zeit.hyper",
];

/// Chromium-family browsers, matched by bundle-identifier prefix. Their omnibox
/// keeps an inline-autocomplete **trailing selection** after each keystroke,
/// which a synthetic Backspace deletes instead of a character
/// (`hoongf`→`hoồng`). The omnibox guard applies only in these applications.
pub const CHROMIUM_APP_PREFIXES: &[&str] = &[
    "com.google.Chrome",
    "com.microsoft.edgemac",
    "org.chromium.Chromium",
    "com.brave.Browser",
    "com.vivaldi.Vivaldi",
    "com.operasoftware.Opera",
    "company.thebrowser.Browser", // Arc
];
