//! What the tray menu does, kept out of the tray's message handler.
//!
//! `tray` owns pixels and Win32 menus; this owns what a click *means*. The split
//! matters because everything here touches the session, and the session is the
//! same one the hook callback is using — so the rules about what may happen and
//! when apply to this file, not to the drawing code.
//!
//! Everything here runs on the message-loop thread, the same one the hook
//! callback runs on. That is what makes reaching into the session safe without a
//! lock: the two can never run at once. It is also why nothing here may be called
//! from anywhere else.

use glowkey_engine::{ExclusionToggle, InputMode};

use super::indicator::{self, Indicator};
use super::{foreground, hook, tray};

/// Everything the menu needs to draw itself, taken in one pass.
///
/// One snapshot rather than a series of accessors: the menu is built in a single
/// loop, and reading the mode at the top and the exclusion at the bottom is how a
/// menu comes to describe two different moments as though they were one.
pub struct Snapshot {
    pub indicator: Indicator,
    pub app: Option<String>,
    pub mode_is_vietnamese: bool,
    pub app_excluded: bool,
    pub auto_fix: bool,
}

/// Takes the snapshot. `None` when the session is unavailable.
#[must_use]
pub fn snapshot() -> Option<Snapshot> {
    let app = hook::current_app();
    let reach = foreground::reach();
    let installed = hook::is_installed();
    hook::with_session(|session| {
        let mode = session.mode();
        // `is_active` collapses mode and exclusion, which is right for deciding
        // whether to transform a key and wrong for an indicator: the user most
        // needs to tell "I turned it off" from "it is off *here*".
        let app_excluded = mode == InputMode::Vietnamese && !session.is_active();
        Snapshot {
            indicator: indicator::state(installed, reach, mode, app_excluded),
            app: app.clone(),
            mode_is_vietnamese: mode == InputMode::Vietnamese,
            app_excluded,
            auto_fix: session.auto_fix(),
        }
    })
}

/// Repaints the tray from the current state. Call after anything that could
/// change it.
pub fn refresh_indicator() {
    let Some(snapshot) = snapshot() else {
        return;
    };
    tray::refresh(snapshot.indicator, snapshot.app.as_deref());
}

/// Flips Vietnamese on or off.
pub fn toggle_mode() {
    let mode = hook::with_session(glowkey_engine::Session::toggle_mode);
    if let Some(mode) = mode {
        crate::log::log(&format!("TOGGLE mode -> {mode:?} (menu)"));
        hook::mark_dirty();
        refresh_indicator();
    }
}

/// Adds or removes the application in front from the ignore list.
pub fn toggle_current_app() {
    let Some(app) = hook::current_app() else {
        crate::log::log("TOGGLE app ignored — no foreground application resolved yet");
        return;
    };
    let outcome = hook::with_session(|session| session.toggle_app_exclusion(&app));
    let Some(outcome) = outcome else {
        return;
    };
    crate::log::log(&format!("TOGGLE app {app:?} -> {outcome:?} (menu)"));
    // A session-only suspension changes nothing persisted — by design the
    // snapshot still excludes the terminal, so saving would write back the same
    // file and imply the suspension survives a restart. It does not.
    if outcome != ExclusionToggle::EnabledSessionOnly {
        hook::mark_dirty();
    }
    refresh_indicator();
}

/// Turns the English-word restore on or off.
pub fn toggle_auto_fix() {
    let now = hook::with_session(|session| {
        let next = !session.auto_fix();
        session.set_auto_fix(next);
        next
    });
    if let Some(next) = now {
        crate::log::log(&format!("SETTING auto_fix -> {next} (menu)"));
        hook::mark_dirty();
    }
}

/// Opens the folder holding the log, so a user filing a report can find it.
///
/// The folder rather than the file: opening the log itself picks whatever editor
/// is registered for `.log`, which on a stock machine is nothing at all.
pub fn reveal_log() {
    let Some(dir) = super::paths::log_dir() else {
        return;
    };
    // `explorer.exe` rather than ShellExecute, so this needs no COM
    // initialisation on a thread that is running a keyboard hook.
    let _ = std::process::Command::new("explorer.exe").arg(dir).spawn();
}

/// Opens the settings window, then applies whatever came back.
///
/// Blocking is correct here: the window owns the interaction while it is open,
/// and the hook keeps running because it is driven by the system rather than by
/// this thread's own loop.
pub fn open_settings() {
    let Some(current) = hook::with_session(|session| session.snapshot()) else {
        return;
    };
    let Some(updated) = super::settings_ui::show(current) else {
        return; // nothing changed
    };
    crate::settings_store::save(&updated);

    // Rebuilt rather than patched field by field: `Settings` is the whole of
    // what the window can change, and a per-field apply would need a line per
    // field that someone will forget to add when the next one lands.
    //
    // The cost is that the session's *runtime* state — which application is in
    // front, any word being composed — is not in the file and does not survive
    // the rebuild. The composing word is genuinely gone, which is correct: the
    // user was in a settings window, so the caret has moved and GlowKey's diff
    // baseline is stale either way. The frontmost application is not gone, it is
    // just not persisted, so it is put back.
    let app = foreground::current();
    hook::with_session(|session| {
        *session = glowkey_engine::Session::from_settings(&updated);
        if let Some(app) = app.as_deref() {
            session.set_frontmost_app(app);
        }
    });
    crate::log::log("SETTINGS applied from the settings window");
    refresh_indicator();
}

/// Reinstalls the keyboard hook after Windows removed it.
///
/// The remedy the `⚠` menu offers for [`super::indicator::Breakage::HookGone`].
/// An indicator that reports a fault without offering the fix is only half of
/// what `docs/decisions/0007` asks for.
pub fn reinstall_hook() {
    hook::uninstall();
    let ok = hook::install();
    crate::log::log(&format!("HOOK reinstall requested from the menu (ok={ok})"));
    refresh_indicator();
}
