//! One native key event in, one consumed-or-passed-through out.
//!
//! This used to be `decide.rs` and it used to hold GlowKey's decision ladder.
//! The ladder now lives in `glowkey-input`, where it can be tested off any
//! operating system and where Windows and Linux will run the same copy. What is
//! left here is everything that is genuinely macOS:
//!
//! - translating the `CGEvent` (`super::adapt`) and calling the policy;
//! - hotkey **recording**, because the value it produces is a macOS key code;
//! - carrying out a [`Decision`] with `CGEventPost`, including the replay of a
//!   boundary key that must land *after* the edit rather than racing it;
//! - the log line, and performing the [`Effects`] the policy asked for.
//!
//! [`TapState::decide`] stays free of disk side effects, which is what lets the
//! tests in `super::tests` drive it with real `CGEvent`s against the user's real
//! settings without writing to them.

use std::ptr::NonNull;

use glowkey_engine::{ExclusionToggle, HotkeyPreset, InputMode};
use glowkey_input::hotkey::{self, HotkeyCapture};
use glowkey_input::{Ctx, Decision, Effects, KeyEvent};
use objc2_core_graphics::{CGEvent, CGEventField};

use super::adapt::{integer_field, modifier_names, unicode_char};
use super::emit::post_key_with_flags;
use super::TapState;

impl TapState {
    /// Processes one key-down event and applies the result. Returns `true` to
    /// consume the event (suppress the original), or `false` to let it through.
    pub(super) fn handle_key_down(&self, event: NonNull<CGEvent>) -> bool {
        self.refresh_frontmost_at_word_start();
        let was_recording = *self.recording_hotkey.borrow();
        let decision = self.decide(event);
        // A finished hotkey recording changed the session's hotkey — persist it
        // here rather than in decide(), which stays free of disk side effects.
        if was_recording && !*self.recording_hotkey.borrow() {
            self.save_settings();
        }
        // Anything else the policy changed that has to survive a quit — today, a
        // word decision the correction hotkey recorded. Saved here rather than in
        // `decide` so that function stays free of disk side effects, which is
        // what lets the tests drive it against the user's real settings file
        // without writing to it.
        if self.pending_save.replace(false) {
            self.save_settings();
        }
        // Logged before the decision is carried out, deliberately. The lines the
        // arms write themselves (TOGGLE, OMNIBOX, RUNAWAY) must come *after* the
        // KEY line that caused them or the log reads backwards, and the KEY line
        // for a key whose emit path panics — the callback is `catch_unwind`-
        // wrapped, so the app survives and typing continues — has to be on disk
        // already. Emit timing is recorded separately by `emit_edit`.
        crate::log::log(&self.key_log_line(event, &decision));
        self.carry_out(event, decision)
    }

    /// Decides what to do with one key-down event.
    ///
    /// Two macOS-only steps bracket the call into the policy: hotkey recording
    /// ahead of it, in the position the recording branch has always occupied, and
    /// the effects afterwards. In between, the ladder.
    pub(super) fn decide(&self, event: NonNull<CGEvent>) -> Decision {
        let key = super::adapt::key_event(event);

        // Hotkey recording (started from Settings): capture the next ⌃/⌥ combo as
        // the custom toggle hotkey; Escape cancels. Not part of the policy,
        // because what it produces is a macOS virtual key code.
        if *self.recording_hotkey.borrow() {
            return self.capture_hotkey(&key);
        }

        // Resolving the preset needs a shared borrow only, and it happens before
        // the mutable one so the two never overlap.
        let preset = self
            .session
            .try_borrow()
            .map(|s| s.toggle_hotkey())
            .unwrap_or(HotkeyPreset::CtrlShiftSpace);
        let toggle_hotkey = hotkey::resolve(preset, preset.macos_keycode());
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
            return Decision::Passthrough;
        };
        let mut effects = Effects::default();
        let decision = glowkey_input::decide(&mut session, &key, &ctx, &mut effects);
        // Before the effects, every one of which may reach back into the session:
        // `refresh_glyph` asks whether Vietnamese is active, and would read
        // `false` off a failed borrow and paint the wrong glyph.
        drop(session);
        self.carry_out_effects(effects);
        decision
    }

    /// Performs what the policy asked for, in the order the fields are declared —
    /// which is the order these lines have always reached the log in.
    fn carry_out_effects(&self, effects: Effects) {
        if let Some(mode) = effects.mode_toggled {
            eprintln!("GlowKey: {mode:?} mode");
            crate::log::log(&format!("TOGGLE mode -> {mode:?}"));
            // Brief on-screen confirmation for the hotkey (no menu is open).
            crate::hud::flash(if matches!(mode, InputMode::Vietnamese) {
                "VI"
            } else {
                "EN"
            });
        }
        if effects.personal_words_changed {
            crate::prefs::personal_words_changed();
        }
        if let Some((was, becomes)) = effects.corrected {
            crate::log::log(&format!(
                "CORRECT {was:?} -> {becomes:?} — swapped and remembered"
            ));
            crate::hud::flash(&format!("{was} → {becomes}"));
        }
        if effects.refresh_glyph {
            crate::menu_bar::refresh_glyph();
        }
        if effects.save_settings {
            // Not written here: `handle_key_down` owns the write, so `decide`
            // stays disk-free for the tests.
            self.pending_save.set(true);
        }
    }

    /// Carries out a decision: suppress, pass through, or emit the edit. Separated
    /// from [`handle_key_down`](Self::handle_key_down) so the decision and the
    /// work of applying it read as the two steps they are.
    fn carry_out(&self, event: NonNull<CGEvent>, decision: Decision) -> bool {
        match decision {
            Decision::Passthrough => false,
            Decision::Consume => true, // suppress, emit nothing (e.g. toggle hotkey)
            Decision::ToggleApp => {
                // Resolve the frontmost app *now* (not a cached value) so ⌃⇧E always
                // toggles the app you are actually in, even before you have typed.
                if let Some((name, bundle_id)) = crate::app_info::frontmost() {
                    let outcome = self
                        .session
                        .try_borrow_mut()
                        .map(|mut s| s.toggle_app_exclusion(&bundle_id))
                        .unwrap_or(ExclusionToggle::Excluded);
                    // A session-only suspension changes nothing persisted — by
                    // design the snapshot still excludes the terminal.
                    if outcome != ExclusionToggle::EnabledSessionOnly {
                        self.save_settings();
                    }
                    let described = match outcome {
                        ExclusionToggle::Excluded => "disabled",
                        ExclusionToggle::Enabled => "enabled",
                        // A terminal re-enabled by hotkey mangles Vietnamese (a PTY
                        // ignores synthetic backspaces) — allow it, but only until
                        // restart, and warn.
                        ExclusionToggle::EnabledSessionOnly => {
                            "enabled UNTIL RESTART (terminal: Vietnamese will mangle in a PTY)"
                        }
                    };
                    eprintln!("GlowKey: {described} Vietnamese for “{name}”");
                    crate::log::log(&format!("TOGGLE app {name:?} -> {described}"));
                    // Brief on-screen confirmation for the hotkey (no menu is open);
                    // the warning variant marks the risky terminal case.
                    crate::hud::flash(match outcome {
                        ExclusionToggle::Excluded => "EN",
                        ExclusionToggle::Enabled => "VI",
                        ExclusionToggle::EnabledSessionOnly => "VI ⚠",
                    });
                }
                // Keep the persistent menu-bar glyph in sync with the per-app toggle.
                crate::menu_bar::refresh_glyph();
                true
            }
            Decision::Emit(response) => {
                self.emit_edit(&response);
                true
            }
            Decision::EmitThenReplayKey(response) => {
                self.emit_edit(&response);
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
                true
            }
        }
    }

    /// Builds the log line for one handled key and its decision.
    ///
    /// Split from the write so the line can be composed **before** the decision is
    /// carried out and written **after**, with the elapsed time appended. The
    /// snapshot has to be taken here: `ToggleApp` changes the exclusion state in
    /// its match arm, and logging afterwards would report the new `active=` value
    /// for the keystroke that caused the change, which reads backwards when
    /// tracing a bug.
    fn key_log_line(&self, event: NonNull<CGEvent>, decision: &Decision) -> String {
        let ch = unicode_char(event);
        let code = integer_field(event, CGEventField::KeyboardEventKeycode);
        let mods = modifier_names(unsafe { CGEvent::flags(Some(event.as_ref())) });
        let app = self.last_bundle_id.borrow().clone().unwrap_or_default();
        let (raw, rendered, mode, active) = match self.session.try_borrow() {
            Ok(session) => session.debug_state(),
            Err(_) => (
                String::new(),
                String::new(),
                glowkey_engine::InputMode::Vietnamese,
                false,
            ),
        };
        let decision = match decision {
            Decision::Passthrough => "Passthrough".to_string(),
            Decision::Consume => "Consume".to_string(),
            Decision::ToggleApp => "ToggleApp".to_string(),
            Decision::Emit(r) => format!("Emit bs={} ins={:?}", r.backspaces, r.insert),
            Decision::EmitThenReplayKey(r) => {
                format!("EmitThenReplayKey bs={} ins={:?}", r.backspaces, r.insert)
            }
        };
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
                    macos_keycode: Some(key.raw_code),
                    windows_vk: None,
                };
                let Ok(mut session) = self.session.try_borrow_mut() else {
                    // Could not store the combo — stay armed rather than silently
                    // ending the recording with the old hotkey still in effect.
                    return Decision::Consume;
                };
                session.set_toggle_hotkey(preset);
                drop(session);
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
