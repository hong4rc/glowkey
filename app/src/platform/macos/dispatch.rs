//! One native key event in, one consumed-or-passed-through out.
//!
//! This used to be `decide.rs` and it used to hold GlowKey's decision ladder.
//! The ladder now lives in `glowkey-input`, where it can be tested off any
//! operating system and where Windows and Linux will run the same copy. What is
//! left here is everything that is genuinely macOS:
//!
//! - translating the `CGEvent` (`super::adapt`) and calling the policy;
//! - hotkey **recording**, because the value it produces is a macOS key code;
//! - the [`Platform`] port: the log line, the HUD, and carrying out a
//!   [`Decision`] with `CGEventPost`, including the replay of a boundary key
//!   that must land *after* the edit rather than racing it.
//!
//! The port on this platform *queues* its edits rather than posting them, and
//! [`TapState::handle_key_down`] posts the queue once the policy has returned.
//! That keeps [`TapState::decide`] free of every side effect but the log, which
//! is what lets the tests in `super::tests` drive it with real `CGEvent`s on a
//! developer's machine without typing into it or writing to their settings.

use std::ptr::NonNull;

use glowkey_input::hotkey::{self, HotkeyCapture};
use glowkey_input::HotkeyPreset;
use glowkey_input::{Ctx, Decision, KeyEvent, Notice, Platform};
use glowkey_session::{AppId, ExclusionToggle, InputMode, KeyResponse, Session};
use objc2_core_graphics::{CGEvent, CGEventField};

use super::adapt::{integer_field, modifier_names, unicode_char};
use super::emit::post_key_with_flags;
use super::TapState;

/// What the port put off until the policy returned: the edits to post, whether
/// to replay the key, whether to repaint the menu bar.
///
/// Posting from inside `decide` would be fine for the app and wrong for the
/// tests, which call `decide` with real events on a developer's machine. So the
/// port records, and `handle_key_down` performs, in the same order.
#[derive(Debug, Default)]
struct Deferred {
    edits: Vec<KeyResponse>,
    replay: bool,
    refresh_glyph: bool,
    /// Text for the on-screen HUD, if any. Deferred with the rest because the
    /// HUD is AppKit, and AppKit may pump the run loop: a re-entrant tap
    /// callback while the session is borrowed would pass its key through.
    hud: Option<String>,
    /// The Personal Words window must reload. Deferred because its refresh
    /// reads the session, which the policy holds until it returns; reloading
    /// inside the borrow emptied the list and zeroed the counters.
    personal_words_changed: bool,
}

/// The macOS side of the port, alive for one key.
struct TapPort<'a> {
    state: &'a TapState,
    event: NonNull<CGEvent>,
    deferred: Deferred,
    /// The display name of the app `app_in_front` resolved, for the toggle's
    /// log line and HUD.
    app_name: Option<String>,
}

impl Platform for TapPort<'_> {
    fn inject(&mut self, backspaces: usize, text: &str) {
        self.deferred.edits.push(KeyResponse {
            handled: true,
            backspaces,
            insert: text.to_string(),
        });
    }

    fn replay_key(&mut self) {
        self.deferred.replay = true;
    }

    fn app_in_front(&mut self) -> Option<AppId> {
        // Resolved *now* (not the cached value) so ⌃⇧E always toggles the app
        // you are actually in, even before you have typed.
        let (name, bundle_id) = crate::app_info::frontmost()?;
        self.app_name = Some(name);
        Some(AppId::from(bundle_id))
    }

    fn request_save(&mut self) {
        // Not written here: `handle_key_down` owns the write, so `decide` stays
        // disk-free for the tests.
        self.state.pending_save.set(true);
    }

    fn request_indicator(&mut self) {
        // Deferred: `refresh_glyph` asks the session whether Vietnamese is
        // active, and the session is borrowed by the policy until it returns.
        self.deferred.refresh_glyph = true;
    }

    fn notify(&mut self, notice: Notice<'_>) {
        match notice {
            // Logged before the decision is carried out, deliberately. The lines
            // the other notices write must come *after* the KEY line that caused
            // them or the log reads backwards, and the KEY line for a key whose
            // emit path panics (the callback is `catch_unwind`-wrapped, so the app
            // survives) has to be on disk already.
            Notice::Decided {
                decision, session, ..
            } => crate::log::log(&self.state.key_log_line(self.event, decision, session)),
            Notice::ModeToggled(mode) => {
                eprintln!("GlowKey: {mode:?} mode");
                crate::log::log(&format!("TOGGLE mode -> {mode:?}"));
                // Brief on-screen confirmation for the hotkey (no menu is open).
                self.deferred.hud = Some(
                    if matches!(mode, InputMode::Vietnamese) {
                        "VI"
                    } else {
                        "EN"
                    }
                    .to_string(),
                );
            }
            Notice::PersonalWordsChanged => self.deferred.personal_words_changed = true,
            Notice::Corrected { was, becomes } => {
                crate::log::log(&format!(
                    "CORRECT {was:?} -> {becomes:?} — swapped and remembered"
                ));
                self.deferred.hud = Some(format!("{was} → {becomes}"));
            }
            Notice::AppToggled { app, outcome } => {
                let name = self.app_name.clone().unwrap_or_else(|| app.to_string());
                let described = match outcome {
                    ExclusionToggle::Excluded => "disabled",
                    ExclusionToggle::Enabled => "enabled",
                    // A terminal re-enabled by hotkey mangles Vietnamese (a PTY
                    // ignores synthetic backspaces): allowed, but only until
                    // restart, and said so.
                    ExclusionToggle::EnabledSessionOnly => {
                        "enabled UNTIL RESTART (terminal: Vietnamese will mangle in a PTY)"
                    }
                };
                eprintln!("GlowKey: {described} Vietnamese for “{name}”");
                crate::log::log(&format!("TOGGLE app {name:?} -> {described}"));
                // Brief on-screen confirmation for the hotkey (no menu is open);
                // the warning variant marks the risky terminal case.
                self.deferred.hud = Some(
                    match outcome {
                        ExclusionToggle::Excluded => "EN",
                        ExclusionToggle::Enabled => "VI",
                        ExclusionToggle::EnabledSessionOnly => "VI ⚠",
                    }
                    .to_string(),
                );
            }
            _ => {}
        }
    }
}

impl TapState {
    /// Processes one key-down event and applies the result. Returns `true` to
    /// consume the event (suppress the original), or `false` to let it through.
    pub(super) fn handle_key_down(&self, event: NonNull<CGEvent>) -> bool {
        self.refresh_frontmost_at_word_start();
        let was_recording = *self.recording_hotkey.borrow();
        let (decision, deferred) = self.run(event);
        // The edits first: they are the keystroke's latency.
        for edit in &deferred.edits {
            self.emit_edit(edit);
        }
        if deferred.replay {
            // Replay the boundary key from GlowKey's own source rather than
            // letting the original through. Letting it through loses the race:
            // the original is the event being dispatched right now, so the host
            // applies it *before* the backspaces this edit just posted, and the
            // edit then eats the boundary key instead of the word it meant to
            // replace (`ddc`␣ → `đddc`, `work`␣ → `ưwork`, space swallowed).
            // Replaying puts it at the tail of the same ordered queue, which is
            // the single-source invariant the rest of the tap already keeps.
            let keycode = integer_field(event, CGEventField::KeyboardEventKeycode) as u16;
            let flags = unsafe { CGEvent::flags(Some(event.as_ref())) };
            post_key_with_flags(&self.source, keycode, flags, true);
            post_key_with_flags(&self.source, keycode, flags, false);
        }
        if deferred.refresh_glyph {
            crate::menu_bar::refresh_glyph();
        }
        if deferred.personal_words_changed {
            crate::prefs::personal_words_changed();
        }
        if let Some(text) = &deferred.hud {
            crate::hud::flash(text);
        }
        // A finished hotkey recording changed the hotkey; persist it here rather
        // than in decide(), which stays free of disk side effects.
        if was_recording && !*self.recording_hotkey.borrow() {
            self.save_settings();
        }
        // Anything else the policy changed that has to survive a quit: a word
        // decision the correction hotkey recorded, an app toggled. Saved here so
        // `decide` stays disk-free, which is what lets the tests drive it against
        // the user's real settings file without writing to it.
        if self.pending_save.replace(false) {
            self.save_settings();
        }
        decision.suppresses()
    }

    /// Decides what to do with one key-down event, performing nothing but the
    /// log line. What the app would have posted is dropped; the tests want the
    /// answer, not the keystrokes.
    #[cfg(test)]
    pub(super) fn decide(&self, event: NonNull<CGEvent>) -> Decision {
        self.run(event).0
    }

    /// Runs the policy for one key-down and returns its decision with the
    /// actions the port put off.
    ///
    /// Two macOS-only steps bracket the call: hotkey recording ahead of it, in
    /// the position the recording branch has always occupied, and the deferred
    /// posting afterwards. In between, the ladder and the port.
    fn run(&self, event: NonNull<CGEvent>) -> (Decision, Deferred) {
        let key = super::adapt::key_event(event);

        // Hotkey recording (started from Settings): capture the next ⌃/⌥ combo as
        // the custom toggle hotkey; Escape cancels. Not part of the policy,
        // because what it produces is a macOS virtual key code.
        if *self.recording_hotkey.borrow() {
            return (self.capture_hotkey(&key), Deferred::default());
        }

        // Resolving the preset needs a shared borrow only, and it happens before
        // the mutable one so the two never overlap.
        let preset = self.toggle_hotkey();
        let toggle_hotkey = hotkey::resolve(preset, preset.raw_code());
        if toggle_hotkey.is_char_fallback() && !self.warned_hotkey_fallback.replace(true) {
            // A combination recorded on another platform: there is no macOS key
            // code to match, so it falls back to the character, which is only
            // right while the user stays on the layout they recorded it with.
            // Said once, not once per keystroke.
            crate::log::log(
                "HOTKEY the custom toggle was recorded on another platform — matching by \
                 character, which depends on the keyboard layout. Re-record it here to fix.",
            );
        }
        let ctx = Ctx { toggle_hotkey };

        let Ok(mut session) = self.session.try_borrow_mut() else {
            return (Decision::Passthrough, Deferred::default());
        };
        let mut port = TapPort {
            state: self,
            event,
            deferred: Deferred::default(),
            app_name: None,
        };
        let decision = glowkey_input::handle(&mut session, &key, &ctx, &mut port);
        (decision, port.deferred)
    }

    /// Builds the log line for one handled key and its decision.
    ///
    /// Takes the session as it stood when the decision was made: `ToggleApp`
    /// changes the exclusion state when it is carried out, and logging afterwards
    /// would report the new `active=` value for the keystroke that caused the
    /// change, which reads backwards when tracing a bug.
    fn key_log_line(
        &self,
        event: NonNull<CGEvent>,
        decision: &Decision,
        session: &Session,
    ) -> String {
        let ch = unicode_char(event);
        let code = integer_field(event, CGEventField::KeyboardEventKeycode);
        let mods = modifier_names(unsafe { CGEvent::flags(Some(event.as_ref())) });
        let app = self.last_bundle_id.borrow().clone().unwrap_or_default();
        let (raw, rendered, mode, active) = session.debug_state();
        format!(
            "KEY {ch:?} code={code} mods={mods} app={app} mode={mode:?} active={active} | {decision} | raw={raw:?} rendered={rendered:?}"
        )
    }

    /// One step of hotkey recording. The policy decides what the keystroke means;
    /// this fills in the macOS key code and stores it.
    ///
    /// Only key-downs that could BE the hotkey are intercepted: a ⌃/⌥ combo
    /// (without ⌘) is captured; Escape cancels. Every other key — plain typing,
    /// shifted letters, all ⌘ shortcuts (⌘Q, ⌘Tab, ⌘S…) — passes through
    /// untouched, so an armed-and-forgotten recording can never lock the keyboard.
    fn capture_hotkey(&self, key: &KeyEvent) -> Decision {
        match hotkey::capture(key) {
            HotkeyCapture::Passthrough => Decision::Passthrough,
            HotkeyCapture::Cancel => {
                self.cancel_hotkey_recording();
                Decision::Consume
            }
            // ⌃⇧E and ⌃⇧W are GlowKey's own; recording either would shadow that
            // feature with no warning. Swallow and keep waiting.
            HotkeyCapture::Reserved { reason } => {
                crate::log::log(reason);
                Decision::Consume
            }
            HotkeyCapture::Captured {
                control,
                shift,
                option,
                key_char,
            } => {
                let preset = HotkeyPreset::Custom {
                    control,
                    shift,
                    option,
                    key_char,
                    // Recorded here, so this is the platform whose code we know.
                    raw_code: Some(key.raw_code),
                };
                let Ok(mut prefs) = self.prefs.try_borrow_mut() else {
                    // Could not store the combo — stay armed rather than silently
                    // ending the recording with the old hotkey still in effect.
                    return Decision::Consume;
                };
                prefs.toggle_hotkey = preset;
                drop(prefs);
                *self.recording_hotkey.borrow_mut() = false;
                // Persistence happens in handle_key_down (decide stays disk-free
                // for the tests).
                crate::log::log(&format!("HOTKEY recorded {preset:?}"));
                crate::prefs::hotkey_recording_done();
                Decision::Consume
            }
        }
    }
}
