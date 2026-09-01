//! GlowKey macOS input method entry point.
//!
//! On macOS this launches an `IMKServer` and registers a Rust subclass of
//! `IMKInputController` (see [`controller`]) — an all-Rust InputMethodKit shell in
//! the style of the sibling `marau` project, which calls Apple frameworks directly
//! through `objc2` rather than through Swift.
//!
//! On other platforms it builds as a stub so the workspace (and the tested engine
//! crate) compiles in CI without a macOS SDK.

#[cfg(target_os = "macos")]
mod controller;

#[cfg(target_os = "macos")]
fn main() {
    controller::run();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("GlowKey is a macOS input method; this platform builds the engine only.");
}
