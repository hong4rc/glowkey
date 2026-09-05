//! The Windows backend: GlowKey as a background process that wraps the active
//! keyboard layout with Vietnamese, the way the CGEventTap does on macOS.
//!
//! ## How it works
//!
//! A `WH_KEYBOARD_LL` hook intercepts key-down events after the system layout has
//! mapped them, so the user's layout stays in effect and GlowKey sees the
//! already-mapped character. GlowKey **suppresses every key it handles** and
//! re-emits the engine's `(backspaces, insert)` diff through `SendInput` — a
//! plain append re-emits the character, a transform sends N backspaces then the
//! new Vietnamese text. There is no composition and no marked text: every
//! keystroke is written straight to the document.
//!
//! Suppressing *every* key rather than passing plain ones through is what makes
//! the output deterministic, and it is carried across from macOS rather than
//! rediscovered — see `hook`'s module note.
//!
//! Injected events carry a magic `dwExtraInfo` and the hook's first statement
//! skips them, which is what prevents a feedback loop. That is `inject`'s
//! [`inject::GLOWKEY_INJECTED`], the analogue of the tagged `CGEventSource`.
//!
//! ## Constraints, inherent to the mechanism
//!
//! - **UIPI**: a non-elevated process cannot inject into a window owned by a
//!   higher integrity level, so typing into Task Manager, regedit or an elevated
//!   terminal does nothing. Detected by [`elevation`] and shown rather than
//!   hidden. GlowKey does not request elevation — see
//!   `docs/decisions/0009-windows-low-level-hook.md`.
//! - **`LowLevelHooksTimeout`**: the system removes a hook whose callback is too
//!   slow, without warning. This is why nothing in the callback may block.
//!
//! The decision itself is not here at all: it lives in `glowkey-input`, with no
//! operating system in it, and this module translates into and out of it.
//!
//! ## What is not built yet
//!
//! Phase 4 is the input core. The tray, the settings window, startup, the
//! clipboard tools and the honest four-state indicator are Phase 5, and
//! **behaviour is unverified until Phase 6** — nothing here has been shown to
//! type Vietnamese into a real application by any automated check.

pub mod adapt;
pub mod clipboard;
pub mod elevation;
pub mod foreground;
pub mod hook;
pub mod hook_log;
pub mod indicator;
pub mod inject;
pub mod mouse;
pub mod paths;
pub mod settings_ui;
pub mod shell;
pub mod single_instance;
pub mod startup;
pub mod tray;

/// Starts the hook and runs until the process exits.
pub fn run() {
    // Before anything else. Two GlowKeys means two hooks, two trays and two
    // injectors sharing one log file and one settings file, and every symptom
    // after that is a function of which hook the system called first. Observed on
    // a real machine; see `single_instance`.
    let Some(_instance) = single_instance::claim() else {
        // Quietly. A user who launches it twice has not done anything wrong.
        return;
    };

    let settings = crate::settings_store::load();
    // Before anything that can produce a user-visible string. GlowKey's users are
    // Vietnamese and Unikey ships a Vietnamese interface; an input method is the
    // last place to make someone read a second language.
    crate::strings::set_language(settings.language);

    // The writer thread first: everything below logs, and after the hook is
    // installed every log call has to be non-blocking.
    hook_log::start();

    hook::set_state(&settings);

    // Before the keyboard hook: it is delivered on this thread's message queue
    // too, and the bootstrap query inside it must happen while nothing is being
    // typed.
    if !foreground::install() {
        eprintln!("GlowKey: could not watch for application switches.");
        // Not fatal, but not silent either. Without the notification the ignore
        // list would only ever see the application that was in front at startup,
        // which means Vietnamese in a terminal — the failure the ignore list
        // exists to prevent.
        crate::log::log("STARTUP no foreground notification — the ignore list will not update");
    }

    // The mouse hook before the keyboard one, so a click can never be missed
    // while keys are already being handled. Non-fatal on failure: GlowKey still
    // types correctly, it just stops being safe to click mid-word.
    mouse::install();

    if !hook::install() {
        eprintln!("GlowKey: failed to install the keyboard hook.");
        crate::log::log("STARTUP failed to install the keyboard hook");
        return;
    }
    crate::log::log("STARTUP hook installed — running");

    if !tray::install() {
        eprintln!("GlowKey: could not create the tray icon.");
        crate::log::log("STARTUP no tray icon — the indicator will not be visible");
    }
    // After the tray exists, so the first state it shows is the real one. The
    // session already knows the frontmost application by now — the foreground
    // bootstrap above pushed it in — which is what keeps this from painting the
    // fail-closed "unknown application" state at launch.
    shell::refresh_indicator();

    hook::run_message_loop();
    tray::remove();
    hook::uninstall();
    mouse::uninstall();
}
