//! The decision: what to do with one key-down event.
//!
//! [`TapState::decide`] is a pure function of the event and the session — no
//! event synthesis, no workspace query, no disk — which is what lets the tests in
//! `super::tests` drive it with real `CGEvent`s and no Accessibility grant. That
//! purity is worth protecting: it is the only part of this shell that can be
//! proved headless, and every reported typing bug has been traced through it.

use std::ptr::NonNull;

use glowkey_engine::{ExclusionToggle, HotkeyPreset, KeyResponse};
use objc2_core_graphics::{CGEvent, CGEventField, CGEventFlags};

use super::emit::post_key_with_flags;
use super::keys::{
    integer_field, is_app_toggle_hotkey, is_caret_move, is_correction_hotkey, is_shortcut,
    is_toggle_hotkey, modifier_names, unicode_char, KEY_CODE_DELETE, KEY_CODE_ESCAPE,
    KEY_CODE_SPACE,
};
use super::TapState;

/// The outcome of processing one key event.
#[derive(Debug)]
pub(super) enum Decision {
    /// Let the original keystroke through unchanged.
    Passthrough,
    /// Suppress the original with no output (e.g. the VN/EN toggle hotkey).
    Consume,
    /// Toggle the current app's ignore-list membership, then consume the key.
    ToggleApp,
    /// Suppress the original and apply this edit (backspaces + insert).
    Emit(KeyResponse),
    /// Apply this edit (e.g. an auto-fix restore) and then replay the original
    /// key from GlowKey's own source, so the boundary key that triggered the
    /// commit still types — but lands *after* the edit rather than racing it.
    EmitThenReplayKey(KeyResponse),
}

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
        // Anything else `decide` changed that has to survive a quit — today, a
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
    /// Decides what to do with one key-down event: pass it through, suppress it, or
    /// suppress it and emit an edit. Pure with respect to the OS (no event
    /// synthesis, no workspace query), so it can be driven by real `CGEvent`s in
    /// tests.
    pub(super) fn decide(&self, event: NonNull<CGEvent>) -> Decision {
        let flags = unsafe { CGEvent::flags(Some(event.as_ref())) };
        let keycode = integer_field(event, CGEventField::KeyboardEventKeycode);

        // Hotkey recording (started from Settings): capture the next ⌃/⌥ combo as
        // the custom toggle hotkey; Escape cancels. All key-downs are consumed
        // while recording so nothing leaks into the focused app.
        if *self.recording_hotkey.borrow() {
            return self.capture_hotkey(event, flags, keycode);
        }

        // VN/EN toggle hotkey (user-configurable preset): flip mode and consume the
        // key. Checked before the shortcut filter, since it is one.
        let toggle_preset = self
            .session
            .try_borrow()
            .map(|s| s.toggle_hotkey())
            .unwrap_or(HotkeyPreset::CtrlShiftSpace);
        if is_toggle_hotkey(flags, keycode, toggle_preset) {
            if let Ok(mut session) = self.session.try_borrow_mut() {
                let mode = session.toggle_mode();
                eprintln!("GlowKey: {mode:?} mode");
                crate::log::log(&format!("TOGGLE mode -> {mode:?}"));
                // Brief on-screen confirmation for the hotkey (no menu is open).
                let on = matches!(mode, glowkey_engine::InputMode::Vietnamese);
                crate::hud::flash(if on { "VI" } else { "EN" });
            }
            // Update the persistent menu-bar glyph too (the toggle happened here in
            // the tap, not via the menu), so it reflects the current state.
            crate::menu_bar::refresh_glyph();
            return Decision::Consume;
        }

        // Per-app toggle hotkey (⌃⇧E): enable/disable Vietnamese for the current
        // app in one keystroke, without opening the menu.
        if is_app_toggle_hotkey(flags, keycode) {
            return Decision::ToggleApp;
        }

        // ⌃⇧W: correct the word just typed and remember the decision. Checked
        // here, with the other ⌃⇧ hotkeys, because `is_shortcut` below would
        // otherwise flush and pass it through — and a flush is exactly what
        // destroys the memory this needs.
        if is_correction_hotkey(flags, keycode) {
            let Ok(mut session) = self.session.try_borrow_mut() else {
                return Decision::Passthrough;
            };
            // Never in an excluded app: excluded means hands off, and this edit
            // rewrites text that is already on screen.
            if !session.is_active() {
                return Decision::Passthrough;
            }
            let described = session.correctable_word();
            return match session.correct_last_word() {
                Some(edit) => {
                    // The decision is now in memory only; `handle_key_down` writes
                    // it to disk. Saving here would put a file write in a function
                    // the tests drive directly.
                    self.pending_save.set(true);
                    crate::prefs::personal_words_changed();
                    if let Some((was, becomes)) = described {
                        crate::log::log(&format!(
                            "CORRECT {was:?} -> {becomes:?} — swapped and remembered"
                        ));
                        crate::hud::flash(&format!("{was} → {becomes}"));
                    }
                    Decision::Emit(edit)
                }
                // Nothing to correct: no word remembered, or the caret has moved
                // since. Consumed either way, which is a real trade-off rather
                // than a free choice: a Control-modified key inserts no text in
                // Cocoa, so passing it through would not put a stray `W` in the
                // document — it would hand ⌃⇧W to the focused app. Swallowing it
                // everywhere GlowKey is active is the price of the hotkey being
                // fixed rather than configurable.
                None => Decision::Consume,
            };
        }

        if is_shortcut(flags) {
            // A shortcut may move the caret or change the selection (⌘A select-all,
            // ⌘V paste, ⌘←). Flush so a later edit is not computed against a stale
            // baseline, then let it through.
            if let Ok(mut session) = self.session.try_borrow_mut() {
                session.flush();
            }
            return Decision::Passthrough;
        }

        let Ok(mut session) = self.session.try_borrow_mut() else {
            return Decision::Passthrough;
        };
        // Normally an inactive session means hands off entirely. The exception is
        // UniKey's always-macro: Vietnamese is off, but a shortcut should still
        // expand, which needs the keys to reach the engine.
        if !session.is_active() && !session.macros_active() {
            return Decision::Passthrough;
        }

        if keycode == KEY_CODE_DELETE {
            // Usually the host performs the delete and we only re-sync the engine
            // to whatever the screen will then show — but not always: undoing a
            // spell-check escape suppresses the key and rewrites the word
            // instead. Four cases, in order:
            //   - deleting the boundary right after a committed word re-composes it
            //     so the next keys keep editing it (hồng␣⌫z → hông);
            //   - mid-word, shrink the composition by one visible character and
            //     stay composed, so the next key is still a Telex key rather than a
            //     literal (hoongf⌫z → hôn, not hồnz);
            //   - undoing a spell-check escape rewrites the word in one edit and
            //     swallows the keystroke (`hoongfa`⌫ → `hồng`);
            //   - if the engine cannot stay in step, flush and stop composing.
            if session.recompose_after_boundary_backspace() {
                return Decision::Passthrough;
            }
            // Exhaustive on purpose — no catch-all arm. A future outcome falling
            // through as a plain delete is the failure this path is most exposed
            // to, and the compiler is the only thing that reliably stops it.
            return match session.backspace_visible_char() {
                // The escape lifted: the word transforms again. Suppress the
                // keystroke and emit the whole repair ourselves — the user's
                // delete is accounted for inside the edit. Letting the host
                // delete and posting this afterwards would mix a native
                // keystroke with a synthesized edit, which is exactly the race
                // the full-suppression model exists to remove (see the module
                // docs on `super`).
                glowkey_engine::BackspaceOutcome::Repair(edit) => Decision::Emit(edit),
                glowkey_engine::BackspaceOutcome::InStep => Decision::Passthrough,
                glowkey_engine::BackspaceOutcome::Flush => {
                    session.flush();
                    Decision::Passthrough
                }
            };
        }

        if is_caret_move(keycode) {
            // Arrow / Home / End / Page keys move the caret without our knowledge,
            // so the engine's diff baseline (and any re-composition memory) is now
            // stale. Flush and let the key through — same contract as a mouse click.
            session.flush();
            return Decision::Passthrough;
        }

        // A word-extending character is a letter always, plus a digit in VNI (where
        // digits carry tone/diacritic marks — `viet65` → việt). Everything else is a
        // word boundary.
        let is_word_char = |ch: char| {
            ch.is_ascii_alphabetic()
                || (ch.is_ascii_digit()
                    && session.input_method() == glowkey_engine::InputMethod::Vni)
                // With the bracket shortcuts on these are vowel keys, so they must
                // reach the engine instead of committing the word. Off (the
                // default) they stay ordinary punctuation and `[` types a bracket.
                || (session.telex_brackets()
                    && session.input_method() == glowkey_engine::InputMethod::Telex
                    && matches!(ch, '[' | ']' | '{' | '}'))
        };
        match unicode_char(event) {
            Some(ch) if is_word_char(ch) => {
                let response = session.process_key(ch);
                if !response.handled {
                    return Decision::Passthrough;
                }
                // Suppress the key and synthesize the edit — for EVERY letter,
                // including a plain append (`{backspaces:0, insert:ch}`). This is
                // the crux of correctness: mixing native passthrough with
                // synthesized edits races, because a natively-typed character and a
                // synthesized backspace posted a moment later reach the document out
                // of order (the app→renderer path in multiprocess apps like Chrome
                // is asynchronous). The symptom is the first transform after a
                // letter landing wrong: `aa` → `aâ`, `hoongf` → `hoồng`.
                //
                // With every letter suppressed and re-emitted from the one tagged
                // `CGEventSource`, all document mutations flow through a single
                // ordered `CGEventPost` queue, so a backspace can never overtake the
                // character it deletes. This is how EVKey/OpenKey drive the document.
                Decision::Emit(response)
            }
            // A word boundary (space, punctuation, Telex digit): commit the word. If
            // auto-fix restores an invalid result to its raw keys, emit that edit
            // and replay the boundary key after it; otherwise the word is already on
            // screen and the boundary key just passes through.
            Some(ch) => {
                let restore = session.commit();
                // Sentence-ending punctuation primes the next word for capitalization.
                session.note_boundary(ch);
                match restore {
                    Some(restore) => Decision::EmitThenReplayKey(restore),
                    None => Decision::Passthrough,
                }
            }
            None => Decision::Passthrough,
        }
    }
    /// One step of hotkey recording. Only key-downs that could BE the hotkey are
    /// intercepted: a ⌃/⌥ combo (without ⌘) is captured; Escape cancels. Every
    /// other key — plain typing, shifted letters, all ⌘ shortcuts (⌘Q, ⌘Tab,
    /// ⌘S…) — passes through untouched, so an armed-and-forgotten recording can
    /// never lock the keyboard. ⌃⇧E is rejected: it is the per-app toggle.
    fn capture_hotkey(
        &self,
        event: NonNull<CGEvent>,
        flags: CGEventFlags,
        keycode: i64,
    ) -> Decision {
        if keycode == KEY_CODE_ESCAPE {
            self.cancel_hotkey_recording();
            return Decision::Consume;
        }
        let control = flags.0 & CGEventFlags::MaskControl.0 != 0;
        let shift = flags.0 & CGEventFlags::MaskShift.0 != 0;
        let option = flags.0 & CGEventFlags::MaskAlternate.0 != 0;
        let command = flags.0 & CGEventFlags::MaskCommand.0 != 0;
        if command || (!control && !option) {
            // Not a candidate combo: let it through so typing and every ⌘
            // shortcut keep working while the recorder is armed.
            return Decision::Passthrough;
        }
        if is_app_toggle_hotkey(flags, keycode) {
            // ⌃⇧E is the built-in per-app toggle; recording it would shadow that
            // feature with no warning. Swallow and keep waiting.
            crate::log::log("HOTKEY ⌃⇧E is reserved (per-app toggle) — pick another combo");
            return Decision::Consume;
        }
        if is_correction_hotkey(flags, keycode) {
            // Same reason: ⌃⇧W corrects the last word, and recording it as the
            // VN/EN toggle would silently cost the user both features.
            crate::log::log("HOTKEY ⌃⇧W is reserved (correct last word) — pick another combo");
            return Decision::Consume;
        }
        // Display character: with Control held the event's Unicode string is a
        // control code (⌃A → U+0001), so map it back to its letter; Space by
        // keycode (its char can be NUL under modifiers).
        let key_char = if keycode == KEY_CODE_SPACE {
            ' '
        } else {
            match unicode_char(event) {
                Some(c) if ('\x01'..='\x1a').contains(&c) => ((c as u8 - 1) + b'A') as char,
                Some(c) if !c.is_control() => c.to_ascii_uppercase(),
                _ => '?',
            }
        };
        let preset = HotkeyPreset::Custom {
            control,
            shift,
            option,
            keycode,
            key_char,
        };
        let Ok(mut session) = self.session.try_borrow_mut() else {
            // Could not store the combo — stay armed rather than silently ending
            // the recording with the old hotkey still in effect.
            return Decision::Consume;
        };
        session.set_toggle_hotkey(preset);
        drop(session);
        *self.recording_hotkey.borrow_mut() = false;
        // Persistence happens in handle_key_down (decide stays disk-free for tests).
        crate::log::log(&format!("HOTKEY recorded {preset:?}"));
        crate::prefs::hotkey_recording_done();
        Decision::Consume
    }
}
