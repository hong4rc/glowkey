//! Windows application identities: the executable's file name, lowercased.
//!
//! Not the Application User Model ID. It is absent for a great many
//! applications and it changes across installs, so an ignore list keyed on it
//! would quietly stop matching after an update — which, for the one feature
//! that keeps Vietnamese out of a terminal, is the worst possible failure.
//!
//! **Unverified.** Every name here was written on a Mac. Phase 6 checks them at
//! a real Windows machine, and getting one wrong reintroduces exactly the bug
//! the ignore list exists to prevent: synthesized backspaces mangling text in a
//! terminal. They are data, so the fix is cheap — but it has to be looked for.

/// Terminal applications. Synthetic backspaces cannot delete inside a console
/// the shell is line-editing, so Vietnamese there always produces garbage.
pub const TERMINAL_EXCLUSIONS: &[&str] = &[
    "windowsterminal.exe",
    "conhost.exe",
    "powershell.exe",
    "pwsh.exe",
    "cmd.exe",
    "wsl.exe",
    "alacritty.exe",
    "wezterm-gui.exe",
    "mintty.exe",
];

/// Applications excluded on first run — terminals and editors, where users
/// overwhelmingly want raw ASCII.
pub const DEFAULT_EXCLUSIONS: &[&str] = &[
    // Terminals — every entry in TERMINAL_EXCLUSIONS, which a test enforces.
    "windowsterminal.exe",
    "conhost.exe",
    "powershell.exe",
    "pwsh.exe",
    "cmd.exe",
    "wsl.exe",
    "alacritty.exe",
    "wezterm-gui.exe",
    "mintty.exe",
    // Editors and development environments.
    "code.exe",
    "devenv.exe",
    "idea64.exe",
    "pycharm64.exe",
    "webstorm64.exe",
    "sublime_text.exe",
    "nvim.exe",
    "vim.exe",
];

/// Chromium-family browsers. Whether the omnibox's trailing selection behaves
/// the way it does on macOS is a Phase 6 measurement, not an assumption; the
/// table exists so the guard has somewhere to look when the answer is known.
pub const CHROMIUM_APP_PREFIXES: &[&str] = &[
    "chrome.exe",
    "msedge.exe",
    "chromium.exe",
    "brave.exe",
    "vivaldi.exe",
    "opera.exe",
    "arc.exe",
];
