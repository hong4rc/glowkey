//! What the tray menu does, kept out of the tray's message handler.
//!
//! `tray` owns pixels and Win32 menus; this owns what a click *means*. The split
//! matters because everything here touches the session, and the session is the
//! same one the hook callback is using — so the rules about what may happen and
//! when apply to this file, not to the drawing code.
//!
//! Everything here runs on the message-loop thread, the same one the hook
//! callback runs on — with one named exception. That is what makes reaching
//! into the session safe without a lock: the two can never run at once. The
//! exception is [`deliver_settings_result`], which the UI thread calls when the
//! settings window closes: it touches only its own slot and posts a message; the
//! session is reached from [`apply_settings`], back on this thread. Nothing else
//! here may be called from anywhere but the message-loop thread.

use std::sync::Mutex;

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
    // From the foreground cache, not from what the session last recorded. The
    // session's copy is written on the keystroke path, so before the user types
    // it is `None` — and the tooltip would say "off in this app" without ever
    // naming which, in the one state where naming it is the whole point.
    let app = foreground::current();
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

/// The application in front changed: tell the session, then repaint.
///
/// Called from the foreground notification, on the message-loop thread — the
/// same place macOS calls `set_frontmost_app` from its `NSWorkspace` observer,
/// and for the same reason.
///
/// Without this the session learned the frontmost application only on the *first
/// keystroke*, and the engine fails closed on an unknown one — so from launch
/// until the user typed, `is_active()` was false and the tray showed the dimmed
/// "off in this app (ignore list)" state over an application that was not
/// excluded at all. An indicator that is wrong until you interact with it is the
/// defect `docs/decisions/0007` exists to forbid, and startup is exactly when a
/// user looks at it.
pub fn foreground_changed(app: &str) {
    hook::with_session(|session| session.set_frontmost_app(app));
    refresh_indicator();
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
    // Same source as `snapshot`, and for a sharper reason: taken from the
    // session, this was `None` until the first keystroke, so picking
    // "Vietnamese in this app" from the tray on a freshly started GlowKey did
    // nothing at all.
    let Some(app) = foreground::current() else {
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

/// Asks the UI thread for the settings window, on a snapshot of the session.
///
/// Returns at once. The window lives on the UI thread (`decisions/0011`); when
/// the user closes it, the UI thread hands the result to
/// [`deliver_settings_result`] and the main loop applies it in
/// [`apply_settings`].
pub fn open_settings() {
    let Some(current) = hook::with_session(|session| session.snapshot()) else {
        return;
    };
    super::ui_thread::open_settings(current);
}

/// Shows the About window. From the tray menu, next to Settings — which is
/// where macOS has it (`menu_bar.rs`), and where a user who has the Mac app
/// will look for it. A window, not a message box: a message box is modal, plays
/// the system sound, and its nested loop held this thread's queue so the
/// hotkey's indicator refresh never ran while it was up.
pub fn show_about() {
    super::ui_thread::open_about();
}

/// A settings result waiting for the main thread: the baseline the window was
/// opened on, and `None` for "closed without changes" or `Some` for the edit.
type SettingsResult = (glowkey_engine::Settings, Option<glowkey_engine::Settings>);

static PENDING_SETTINGS: Mutex<Option<SettingsResult>> = Mutex::new(None);

/// Called on the UI thread when the settings window has decided. Stores the
/// result and wakes the main loop, which owns the session and the file.
pub fn deliver_settings_result(
    baseline: glowkey_engine::Settings,
    updated: Option<glowkey_engine::Settings>,
) {
    let previous = PENDING_SETTINGS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .replace((baseline, updated));
    if previous.is_some() {
        // One slot. A second result before the main loop drained the first
        // should not happen (a reopen takes a tray click, which drains); if it
        // does, the earlier result is the one lost, and the log should say so.
        crate::log::log("SETTINGS a result was replaced before the main loop applied it");
    }
    hook::wake_main_loop();
}

/// The main loop's side of [`deliver_settings_result`].
pub fn take_pending_settings_result() -> Option<SettingsResult> {
    PENDING_SETTINGS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
}

/// Applies what the settings window handed back. Main thread only.
pub fn apply_settings(
    current: &glowkey_engine::Settings,
    updated: Option<glowkey_engine::Settings>,
) {
    let Some(updated) = updated else {
        return; // nothing changed
    };

    // **The hook keeps running while the window is open**, so the live session is
    // not the one the window was handed. It can have learned things in the
    // meantime: a mode toggle or a per-app exclusion from the hotkeys, and — the
    // one that actually hurts — a word the user taught GlowKey with ⌃⇧W, which is
    // a deliberate act they will not think to repeat.
    //
    // Writing `updated` straight back would destroy all of it twice over: once in
    // memory and once on disk, because `updated` carries the *pre-window*
    // baseline for every field the user did not touch.
    //
    // So the window's edits are applied as a diff against the baseline it was
    // given, on top of whatever the session looks like now. Only fields the user
    // actually changed move.
    let live = hook::with_session(|session| session.snapshot()).unwrap_or_else(|| current.clone());
    let merged = merge_settings(current, &updated, live);

    crate::settings_store::save(&merged);

    // Rebuilt rather than patched: `Settings` is the whole of what is persisted,
    // and the merge above has already decided every field.
    //
    // The session's *runtime* state is not in the file and does not survive the
    // rebuild. The composing word is genuinely gone, which is correct — the user
    // was in a settings window, so the caret has moved and the diff baseline is
    // stale either way. The frontmost application is not gone, only unpersisted,
    // so it is put back.
    let app = foreground::current();
    hook::with_session(|session| {
        *session = glowkey_engine::Session::from_settings(&merged);
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

/// Applies the settings window's edits without discarding what the session
/// learned while the window was open.
///
/// Three values, not two:
///
/// - `baseline` — what the window was handed when it opened.
/// - `edited` — what it returned.
/// - `live` — what the session looks like *now*, which may have moved: the hook
///   keeps running behind the window, so a hotkey press, a per-app toggle or a
///   ⌃⇧W correction can all have landed in between.
///
/// A field the user changed in the window takes the window's value. Every other
/// field keeps the live one. Writing `edited` straight back would silently
/// destroy the rest — including a word the user deliberately taught GlowKey,
/// which is the kind of loss that is never noticed until it is needed.
///
/// Field-by-field rather than clever, because there is no cleverness available:
/// `Settings` has no per-field dirty tracking, and inventing one for this is a
/// larger change than the list below.
fn merge_settings(
    baseline: &glowkey_engine::Settings,
    edited: &glowkey_engine::Settings,
    live: glowkey_engine::Settings,
) -> glowkey_engine::Settings {
    /// The window's value if the user changed it, otherwise the live one.
    fn pick<T: PartialEq + Clone>(baseline: &T, edited: &T, live: &T) -> T {
        if edited == baseline {
            live.clone()
        } else {
            edited.clone()
        }
    }

    glowkey_engine::Settings {
        exclusions: pick(&baseline.exclusions, &edited.exclusions, &live.exclusions),
        removed_default_exclusions: pick(
            &baseline.removed_default_exclusions,
            &edited.removed_default_exclusions,
            &live.removed_default_exclusions,
        ),
        auto_fix: pick(&baseline.auto_fix, &edited.auto_fix, &live.auto_fix),
        style: pick(&baseline.style, &edited.style, &live.style),
        input_method: pick(
            &baseline.input_method,
            &edited.input_method,
            &live.input_method,
        ),
        auto_capitalize: pick(
            &baseline.auto_capitalize,
            &edited.auto_capitalize,
            &live.auto_capitalize,
        ),
        toggle_hotkey: pick(
            &baseline.toggle_hotkey,
            &edited.toggle_hotkey,
            &live.toggle_hotkey,
        ),
        macros: pick(&baseline.macros, &edited.macros, &live.macros),
        restore_english_words: pick(
            &baseline.restore_english_words,
            &edited.restore_english_words,
            &live.restore_english_words,
        ),
        open_settings_at_launch: pick(
            &baseline.open_settings_at_launch,
            &edited.open_settings_at_launch,
            &live.open_settings_at_launch,
        ),
        language: pick(&baseline.language, &edited.language, &live.language),
        quick_telex: pick(
            &baseline.quick_telex,
            &edited.quick_telex,
            &live.quick_telex,
        ),
        telex_brackets: pick(
            &baseline.telex_brackets,
            &edited.telex_brackets,
            &live.telex_brackets,
        ),
        strict_spell_check: pick(
            &baseline.strict_spell_check,
            &edited.strict_spell_check,
            &live.strict_spell_check,
        ),
        always_macro: pick(
            &baseline.always_macro,
            &edited.always_macro,
            &live.always_macro,
        ),
        welcome_shown: pick(
            &baseline.welcome_shown,
            &edited.welcome_shown,
            &live.welcome_shown,
        ),
        word_overrides: pick(
            &baseline.word_overrides,
            &edited.word_overrides,
            &live.word_overrides,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glowkey_engine::{Settings, WordOverride, WordPreference};

    fn taught_word() -> WordOverride {
        WordOverride {
            keys: "cats".into(),
            prefer: WordPreference::Vietnamese,
        }
    }

    /// The defect this exists for: a word taught with ⌃⇧W while the settings
    /// window was open must survive the window closing.
    ///
    /// The window was handed a baseline with no word overrides and did not touch
    /// that field, so its "empty" is not an edit — it is a stale copy of the
    /// baseline, and treating it as an edit is what destroyed the word.
    #[test]
    fn a_word_taught_while_the_window_was_open_survives() {
        let baseline = Settings::default();
        // The user changed one unrelated thing in the window.
        let edited = Settings {
            auto_capitalize: true,
            ..baseline.clone()
        };
        // Meanwhile the hook learned a word.
        let live = Settings {
            word_overrides: vec![taught_word()],
            ..baseline.clone()
        };

        let merged = merge_settings(&baseline, &edited, live);
        assert_eq!(
            merged.word_overrides,
            vec![taught_word()],
            "the taught word must not be destroyed by an unrelated settings edit"
        );
        assert!(
            merged.auto_capitalize,
            "the window's own edit still applies"
        );
    }

    /// A mode or per-app change made by hotkey while the window was open also
    /// survives.
    /// The real path, not a hand-built `edited`: a window opened on a file
    /// whose exclusions are in raw order, one unrelated edit, and an app the
    /// tray excluded while the window was open. The tray's exclusion must
    /// survive. It did not while the baseline crossed the thread un-normalized:
    /// `finalize` returns the sorted effective list, so a raw baseline read as
    /// "the user edited the exclusions" and the window's list overwrote the
    /// tray's.
    #[test]
    fn a_tray_exclusion_survives_the_real_window_round_trip() {
        let opened_on = glowkey_engine::Settings {
            exclusions: vec!["zzz.exe".into(), "aaa.exe".into()],
            ..glowkey_engine::Settings::default()
        };
        let mut app = super::super::settings_ui::SettingsApp::new(opened_on.clone());
        app.draft.auto_capitalize = !app.draft.auto_capitalize;
        app.finalize();
        let edited = app.take_result().expect("decided").expect("changed");

        let mut live = opened_on.clone();
        live.exclusions.push("game.exe".into());

        let merged = merge_settings(&app.baseline(), &edited, live);
        assert!(
            merged.exclusions.iter().any(|e| e == "game.exe"),
            "{:?}",
            merged.exclusions
        );
        assert_eq!(merged.auto_capitalize, edited.auto_capitalize);
    }

    /// "Closed, nothing changed" applies nothing and touches nothing.
    #[test]
    fn applying_no_change_is_a_no_op() {
        apply_settings(&glowkey_engine::Settings::default(), None);
        assert!(take_pending_settings_result().is_none());
    }

    #[test]
    fn a_hotkey_exclusion_made_while_the_window_was_open_survives() {
        let baseline = Settings::default();
        let edited = Settings {
            quick_telex: true,
            ..baseline.clone()
        };
        let mut live = baseline.clone();
        live.exclusions.push("someapp.exe".into());

        let merged = merge_settings(&baseline, &edited, live);
        assert!(merged.exclusions.iter().any(|id| id == "someapp.exe"));
        assert!(merged.quick_telex);
    }

    /// The window still wins where the user actually edited, including when they
    /// edited the same field the session changed. Someone has to win, and the
    /// person looking at the control is the better answer.
    #[test]
    fn the_window_wins_the_fields_the_user_edited() {
        let baseline = Settings::default();
        let edited = Settings {
            auto_fix: !baseline.auto_fix,
            ..baseline.clone()
        };
        let live = Settings {
            auto_fix: baseline.auto_fix,
            ..baseline.clone()
        };
        let merged = merge_settings(&baseline, &edited, live);
        assert_eq!(merged.auto_fix, !baseline.auto_fix);
    }

    /// Nothing edited anywhere leaves the live value untouched — the merge must
    /// not be a way to quietly rewrite a file.
    #[test]
    fn no_edits_anywhere_changes_nothing() {
        let baseline = Settings::default();
        let live = Settings {
            word_overrides: vec![taught_word()],
            ..baseline.clone()
        };
        let merged = merge_settings(&baseline, &baseline, live.clone());
        assert_eq!(merged, live);
    }
}
