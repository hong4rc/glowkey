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

/// Opens the settings window, then applies whatever came back.
///
/// Blocking is correct here: the window owns the interaction while it is open,
/// and the hook keeps running because it is driven by the system rather than by
/// this thread's own loop.
pub fn open_settings() {
    let Some(current) = hook::with_session(|session| session.snapshot()) else {
        return;
    };
    let Some(updated) = super::settings_ui::show(current.clone()) else {
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
    let merged = merge_settings(&current, &updated, live);

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
