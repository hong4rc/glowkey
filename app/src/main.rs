//! GlowKey macOS entry point.
//!
//! GlowKey is a background agent (no Dock icon) that wraps the active keyboard
//! layout with Vietnamese Telex, in the style of EVKey/OpenKey: it installs a
//! `CGEventTap`, so the user's Colemak/US layout stays live and Vietnamese is added
//! on top. See [`platform::macos`]. Requires an Accessibility permission; it does
//! not operate in secure/password fields.
//!
//! On other platforms it builds as a stub so the workspace (and the tested engine
//! crate) compiles in CI without a macOS SDK.

#[cfg(target_os = "macos")]
mod about_window;
#[cfg(target_os = "macos")]
mod app_info;
#[cfg(target_os = "macos")]
mod ax;
#[cfg(target_os = "macos")]
mod hud;
#[cfg(target_os = "macos")]
mod log;
#[cfg(target_os = "macos")]
mod login_item;
/// The invisible application menu that makes ⌘X/⌘C/⌘V and ⌘W work.
#[cfg(target_os = "macos")]
mod main_menu;
#[cfg(target_os = "macos")]
mod menu_bar;
#[cfg(target_os = "macos")]
mod platform;
#[cfg(target_os = "macos")]
mod prefs;
#[cfg(target_os = "macos")]
mod settings_store;
#[cfg(target_os = "macos")]
mod strings;
#[cfg(target_os = "macos")]
mod welcome;

#[cfg(target_os = "macos")]
fn main() {
    platform::macos::run();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("GlowKey is a macOS agent; this platform builds the engine only.");
}
