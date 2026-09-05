//! GlowKey macOS entry point.
//!
//! GlowKey is a background agent (no Dock icon) that wraps the active keyboard
//! layout with Vietnamese Telex, in the style of EVKey/OpenKey: it installs a
//! `CGEventTap`, so the user's Colemak/US layout stays live and Vietnamese is added
//! on top. See [`platform::macos`]. Requires an Accessibility permission; it does
//! not operate in secure/password fields.
//!
//! On Windows the same engine and the same decision ladder run behind a
//! `WH_KEYBOARD_LL` hook and `SendInput` — see [`platform::windows`]. On every
//! other platform this builds as a stub so the workspace (and the tested engine
//! crate) compiles in CI without a macOS SDK.

// No console window on Windows.
//
// GlowKey is a background agent: it has a tray icon and nothing else. A console
// subsystem binary opens and holds a console for the life of the process, which
// the `HKCU\...\Run` entry would then pop on every single login. The macOS
// equivalent is `LSUIElement` in the bundle's Info.plist.
//
// The cost is that `eprintln!` has nowhere to go on Windows, so anything worth
// saying must reach `crate::log` instead. Startup failures already do.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(target_os = "macos")]
mod about_window;
#[cfg(target_os = "macos")]
mod app_info;
#[cfg(target_os = "macos")]
mod ax;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod default_exclusions;
#[cfg(target_os = "macos")]
mod hud;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod log;
#[cfg(target_os = "macos")]
mod login_item;
/// The invisible application menu that makes ⌘X/⌘C/⌘V and ⌘W work.
#[cfg(target_os = "macos")]
mod main_menu;
#[cfg(target_os = "macos")]
mod menu_bar;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod platform;
#[cfg(target_os = "macos")]
mod prefs;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod prefs_model;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod session_adapter;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod settings_spec;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod settings_store;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod strings;
#[cfg(target_os = "macos")]
mod welcome;

#[cfg(target_os = "macos")]
fn main() {
    platform::macos::run();
}

#[cfg(target_os = "windows")]
fn main() {
    platform::windows::run();
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn main() {
    eprintln!("GlowKey supports macOS and Windows; this platform builds the engine only.");
}
