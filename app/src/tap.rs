//! The CGEventTap shell: GlowKey as a background agent that wraps the active
//! keyboard layout with Vietnamese Telex, like EVKey and OpenKey.
//!
//! ## How it works
//!
//! A `CGEventTap` intercepts key-down events *after* the system keyboard layout has
//! mapped them, so the user's Colemak/US layout stays in effect and GlowKey sees
//! the already-mapped character. GlowKey **suppresses every letter it handles** and
//! re-emits the engine's `(backspaces, insert)` diff — a plain append re-emits the
//! character, a transform posts N backspaces then the new Vietnamese text. There is
//! no marked text: every keystroke is written straight to the document.
//!
//! Suppressing *every* letter (rather than passing plain keys through) is what makes
//! the output deterministic. A natively-typed character and a synthesized backspace
//! posted a moment later reach the document out of order in multiprocess apps
//! (Chrome/Edge), so mixing the two races — the first transform after a letter lands
//! wrong (`aa` → `aâ`, `hoongf` → `hoồng`). Routing every mutation through the one
//! tagged source's single `CGEventPost` queue removes the race by construction.
//!
//! Synthesized events are posted at the **session level** (`CGEventPost`), the
//! normal input path, so multi-process apps like Chrome route them to the focused
//! renderer correctly. GlowKey tags its own event source and skips events carrying
//! that tag in the tap, which prevents a feedback loop; a latching circuit breaker
//! caps any runaway (should self-identification ever fail) rather than letting it
//! flood the input system. Set `GLOWKEY_DEBUG=1` to log each emit.
//!
//! ## Constraints (inherent to the event-tap approach, same as EVKey)
//!
//! - Requires an Accessibility permission grant. Without it the tap cannot be
//!   created and GlowKey stays inert.
//! - Does not work in secure input fields (passwords): macOS withholds those
//!   events from all event taps.
//!
//! The decision logic ([`TapState::decide`]) is a pure function of the event and
//! session state and is unit-tested with real `CGEvent`s (see the tests below).
//! Only the system-level parts — installing the tap, delivering synthesized events
//! to an app — need Accessibility and a live session to verify. See
//! `docs/checkpoint.md`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::panic::AssertUnwindSafe;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use glowkey_engine::{ExclusionToggle, HotkeyPreset, KeyResponse, Session};
use objc2_app_kit::NSWorkspace;
use objc2_core_foundation::{kCFRunLoopCommonModes, CFRetained, CFRunLoop};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventMask, CGEventSource, CGEventSourceStateID,
    CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy, CGEventType,
};

/// macOS virtual key code for Delete/Backspace.
const KEY_CODE_DELETE: i64 = 51;
/// macOS virtual key code for Forward Delete (⌦). Used by the omnibox guard: with
/// a trailing selection it deletes the selection; with the caret at the end of the
/// text (GlowKey's normal position) it is a no-op.
const KEY_CODE_FORWARD_DELETE: i64 = 117;
/// macOS virtual key code for Escape — cancels hotkey recording.
const KEY_CODE_ESCAPE: i64 = 53;

/// Chromium-family browsers, matched by bundle-id prefix. Their omnibox keeps an
/// inline-autocomplete **trailing selection** after each keystroke, which a
/// synthetic Backspace deletes instead of a character (`hoongf`→`hoồng`). The
/// omnibox guard (see [`TapState::emit_edit`]) applies only in these apps.
const CHROMIUM_BUNDLE_PREFIXES: &[&str] = &[
    "com.google.Chrome",
    "com.microsoft.edgemac",
    "org.chromium.Chromium",
    "com.brave.Browser",
    "com.vivaldi.Vivaldi",
    "com.operasoftware.Opera",
    "company.thebrowser.Browser", // Arc
];

/// Whether `bundle_id` is a Chromium-family browser (see [`CHROMIUM_BUNDLE_PREFIXES`]).
fn is_chromium_browser(bundle_id: &str) -> bool {
    CHROMIUM_BUNDLE_PREFIXES
        .iter()
        .any(|prefix| bundle_id.starts_with(prefix))
}

/// Whether `GLOWKEY_DEBUG` is set — enables per-emit logging for diagnosing
/// delivery issues in specific apps.
fn debug_enabled() -> bool {
    use std::sync::OnceLock;
    static DEBUG: OnceLock<bool> = OnceLock::new();
    *DEBUG.get_or_init(|| std::env::var_os("GLOWKEY_DEBUG").is_some())
}

/// User-data tag on GlowKey's own event source. Synthesized events are posted at
/// the session level (so multi-process apps like Chrome route them to the focused
/// renderer correctly); the tap reads each event's source user-data and skips its
/// own, which is the documented way to avoid a feedback loop.
const GLOWKEY_TAG: i64 = 0x47_4C_4F_57; // "GLOW"

/// Latched if emits ever exceed a human typing rate — a runaway (e.g. if self-ID
/// ever fails). Caps a flood at [`RUNAWAY_LIMIT`] events, then stops until restart.
static DISABLED: AtomicBool = AtomicBool::new(false);

/// Circuit-breaker thresholds: more emits than this within the window is a runaway,
/// not human typing (a person tops out around 20 keystrokes/second).
const RUNAWAY_LIMIT: usize = 60;
const RUNAWAY_WINDOW: Duration = Duration::from_millis(1000);

/// Long-lived shell state, referenced from the C tap callback via a raw pointer.
/// The callback runs on the main run loop thread, so a `RefCell` is sufficient —
/// no cross-thread access.
pub(crate) struct TapState {
    session: RefCell<Session>,
    last_bundle_id: RefCell<Option<String>>,
    /// The tagged event source all synthesized events are created from.
    source: CFRetained<CGEventSource>,
    /// Recent emit timestamps, for the runaway circuit breaker.
    recent_emits: RefCell<VecDeque<Instant>>,
    /// True while the Settings window is recording a custom toggle hotkey: the
    /// next key-down with ⌃ or ⌥ becomes the hotkey; Escape cancels.
    recording_hotkey: RefCell<bool>,
}

impl TapState {
    /// Builds state with default settings (used in tests).
    #[cfg(test)]
    fn new() -> Option<Self> {
        Self::from_settings(&glowkey_engine::Settings::default())
    }

    /// Builds state with a session configured from persisted settings.
    fn from_settings(settings: &glowkey_engine::Settings) -> Option<Self> {
        let source = CGEventSource::new(CGEventSourceStateID::Private)?;
        CGEventSource::set_user_data(Some(&source), GLOWKEY_TAG);
        Some(Self {
            session: RefCell::new(Session::from_settings(settings)),
            last_bundle_id: RefCell::new(None),
            source,
            recent_emits: RefCell::new(VecDeque::new()),
            recording_hotkey: RefCell::new(false),
        })
    }

    /// Snapshots the current session and writes it to the settings file. Called
    /// after any user-driven change (menu toggle, preference edit).
    pub fn save_settings(&self) {
        if let Ok(session) = self.session.try_borrow() {
            crate::settings_store::save(&session.snapshot());
        }
    }

    /// Records the frontmost application on the session, so the ignore list and
    /// VN/EN state reflect an app switch immediately (not only at the next word
    /// start). Called by the menu controller's app-activation observer. Switching
    /// to another app also cancels an armed hotkey recording — the recorder only
    /// makes sense while GlowKey's own Settings window is in front.
    pub fn set_frontmost_app(&self, bundle_id: &str) {
        if own_bundle_id().as_deref() != Some(bundle_id) {
            self.cancel_hotkey_recording();
        }
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.set_frontmost_app(bundle_id);
        }
        *self.last_bundle_id.borrow_mut() = Some(bundle_id.to_string());
    }

    /// Whether Vietnamese is currently active (Vietnamese mode and the frontmost
    /// app not excluded) — drives the menu bar glyph.
    pub fn is_active(&self) -> bool {
        self.session
            .try_borrow()
            .map(|s| s.is_active())
            .unwrap_or(false)
    }

    /// Toggles VN/EN mode and saves. Used by the menu bar.
    pub fn toggle_mode_and_save(&self) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.toggle_mode();
        }
        self.save_settings();
    }

    /// Toggles auto-fix and saves. Used by the menu bar.
    pub fn toggle_auto_fix_and_save(&self) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            let on = session.auto_fix();
            session.set_auto_fix(!on);
        }
        self.save_settings();
    }

    /// Toggles a specific app in the ignore list and saves. Used by the menu bar's
    /// "Enable/Disable for <App>" action. Per-app and independent.
    pub fn toggle_app_exclusion_and_save(&self, bundle_id: &str) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.toggle_app_exclusion(bundle_id);
        }
        self.save_settings();
    }

    /// Current state for menu labels: (mode, auto-fix on, is `bundle_id` excluded).
    pub fn menu_state(&self, bundle_id: &str) -> (glowkey_engine::InputMode, bool, bool) {
        match self.session.try_borrow() {
            Ok(s) => (
                s.mode(),
                s.auto_fix(),
                s.exclusions().is_excluded(bundle_id),
            ),
            Err(_) => (glowkey_engine::InputMode::Vietnamese, true, false),
        }
    }

    /// The bundle identifiers currently excluded (Vietnamese off), sorted. Drives
    /// the Settings window's "Excluded apps" list.
    pub fn exclusion_ids(&self) -> Vec<String> {
        match self.session.try_borrow() {
            Ok(s) => s.exclusions().ids().map(|s| s.to_string()).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Adds an app to the ignore list (disables Vietnamese there) and saves. Used by
    /// the Settings window's "Add App…" picker. Idempotent if already excluded.
    pub fn add_exclusion_and_save(&self, bundle_id: &str) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.exclusions_mut().add(bundle_id.to_string());
        }
        self.save_settings();
    }

    /// Removes an app from the ignore list (re-enables Vietnamese there) and saves.
    /// Used by the Settings window's per-row "Remove" button.
    pub fn remove_exclusion_and_save(&self, bundle_id: &str) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.exclusions_mut().remove(bundle_id);
        }
        self.save_settings();
    }

    /// Whether auto-fix (restore invalid Vietnamese to the raw keys) is on.
    pub fn auto_fix(&self) -> bool {
        self.session
            .try_borrow()
            .map(|s| s.auto_fix())
            .unwrap_or(true)
    }

    /// Whether auto-capitalize (first letter of each sentence) is on.
    pub fn auto_capitalize(&self) -> bool {
        self.session
            .try_borrow()
            .map(|s| s.auto_capitalize())
            .unwrap_or(false)
    }

    /// Sets auto-capitalize and saves.
    pub fn set_auto_capitalize_and_save(&self, on: bool) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.set_auto_capitalize(on);
        }
        self.save_settings();
    }

    /// Sets auto-fix on/off explicitly and saves. Used by the Settings checkbox.
    pub fn set_auto_fix_and_save(&self, on: bool) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.set_auto_fix(on);
        }
        self.save_settings();
    }

    /// Whether the Settings window should open on launch.
    pub fn open_settings_at_launch(&self) -> bool {
        self.session
            .try_borrow()
            .map(|s| s.open_settings_at_launch())
            .unwrap_or(true)
    }

    /// Sets the "open Settings on launch" preference and saves.
    pub fn set_open_settings_at_launch_and_save(&self, on: bool) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.set_open_settings_at_launch(on);
        }
        self.save_settings();
    }

    /// The current input method (Telex/VNI). Drives the Settings control.
    pub fn input_method(&self) -> glowkey_engine::InputMethod {
        self.session
            .try_borrow()
            .map(|s| s.input_method())
            .unwrap_or(glowkey_engine::InputMethod::Telex)
    }

    /// Sets the input method (Telex/VNI) and saves.
    pub fn set_input_method_and_save(&self, method: glowkey_engine::InputMethod) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.set_input_method(method);
        }
        self.save_settings();
    }

    /// The current toggle-hotkey preset. Drives the Settings control.
    pub fn toggle_hotkey(&self) -> HotkeyPreset {
        self.session
            .try_borrow()
            .map(|s| s.toggle_hotkey())
            .unwrap_or(HotkeyPreset::CtrlShiftSpace)
    }

    /// Sets the toggle-hotkey preset and saves.
    pub fn set_toggle_hotkey_and_save(&self, preset: HotkeyPreset) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.set_toggle_hotkey(preset);
        }
        self.save_settings();
    }

    /// Starts recording a custom toggle hotkey: the next key-down with ⌃ or ⌥
    /// becomes the hotkey (Escape cancels). Driven by the Settings window.
    pub fn begin_hotkey_recording(&self) {
        *self.recording_hotkey.borrow_mut() = true;
    }

    /// Whether a hotkey recording is in progress.
    pub fn is_recording_hotkey(&self) -> bool {
        *self.recording_hotkey.borrow()
    }

    /// Whether the opt-in English word restore is on.
    pub fn restore_english_words(&self) -> bool {
        self.session
            .try_borrow()
            .map(|s| s.restore_english_words())
            .unwrap_or(false)
    }

    /// Sets the English word restore and saves.
    pub fn set_restore_english_words_and_save(&self, on: bool) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.set_restore_english_words(on);
        }
        self.save_settings();
    }

    /// The text-expansion macros, cloned for the Settings list.
    pub fn macros(&self) -> Vec<glowkey_engine::Macro> {
        self.session
            .try_borrow()
            .map(|s| s.macros().to_vec())
            .unwrap_or_default()
    }

    /// Adds (or replaces) a macro and saves. Returns whether it was accepted.
    pub fn add_macro_and_save(&self, shortcut: &str, expansion: &str) -> bool {
        let ok = self
            .session
            .try_borrow_mut()
            .map(|mut s| s.add_macro(shortcut, expansion))
            .unwrap_or(false);
        if ok {
            self.save_settings();
        }
        ok
    }

    /// Removes the macro at `index` and saves.
    pub fn remove_macro_and_save(&self, index: usize) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.remove_macro(index);
        }
        self.save_settings();
    }

    /// The current tone-placement style. Drives the Settings segmented control.
    pub fn style(&self) -> glowkey_engine::PlacementStyle {
        self.session
            .try_borrow()
            .map(|s| s.style())
            .unwrap_or(glowkey_engine::PlacementStyle::New)
    }

    /// Sets the tone-placement style and saves. Used by the Settings segmented control.
    pub fn set_style_and_save(&self, style: glowkey_engine::PlacementStyle) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.set_style(style);
        }
        self.save_settings();
    }

    /// Clears the runaway circuit breaker and any half-typed word, recovering input
    /// if the breaker ever latched (the "Reset input" menu item). Human typing never
    /// trips it, so this is only a safety valve.
    pub fn reset(&self) {
        DISABLED.store(false, Ordering::Relaxed);
        if let Ok(mut emits) = self.recent_emits.try_borrow_mut() {
            emits.clear();
        }
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.flush();
        }
    }

    /// Records an emit and returns false if the rate indicates a runaway; latches
    /// [`DISABLED`] on a trip so a loop is capped rather than sustained. Human
    /// typing never approaches the limit.
    fn circuit_ok(&self) -> bool {
        if DISABLED.load(Ordering::Relaxed) {
            return false;
        }
        let now = Instant::now();
        let mut times = self.recent_emits.borrow_mut();
        while times
            .front()
            .is_some_and(|t| now.duration_since(*t) > RUNAWAY_WINDOW)
        {
            times.pop_front();
        }
        times.push_back(now);
        if times.len() > RUNAWAY_LIMIT {
            DISABLED.store(true, Ordering::Relaxed);
            crate::log::log("RUNAWAY circuit breaker latched — input disabled until reset");
            eprintln!("GlowKey: runaway detected — transformation disabled. Restart to re-enable.");
            return false;
        }
        true
    }

    /// Flushes the in-progress word — the engine's edits assume the composing word
    /// is still the document tail, so this must run when the caret may have moved
    /// (a mouse click). A click also cancels an armed hotkey recording: the user
    /// has moved on, and a forgotten recorder must not capture a later ⌃/⌥ combo.
    fn flush(&self) {
        self.cancel_hotkey_recording();
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.flush();
        }
    }

    /// Processes one key-down event and applies the result. Returns `true` to
    /// consume the event (suppress the original), or `false` to let it through.
    fn handle_key_down(&self, event: NonNull<CGEvent>) -> bool {
        self.refresh_frontmost_at_word_start();
        let was_recording = *self.recording_hotkey.borrow();
        let decision = self.decide(event);
        // A finished hotkey recording changed the session's hotkey — persist it
        // here rather than in decide(), which stays free of disk side effects.
        if was_recording && !*self.recording_hotkey.borrow() {
            self.save_settings();
        }
        self.log_key(event, &decision);
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

    /// Records one handled key and its decision to the log file, so a reported issue
    /// can be traced from the log without a live repro.
    fn log_key(&self, event: NonNull<CGEvent>, decision: &Decision) {
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
        crate::log::log(&format!(
            "KEY {ch:?} code={code} mods={mods} app={app} mode={mode:?} active={active} | {decision} | raw={raw:?} rendered={rendered:?}"
        ));
    }

    /// Emits one edit through the session-posting path, honoring the circuit breaker
    /// and debug logging.
    fn emit_edit(&self, response: &KeyResponse) {
        if !self.circuit_ok() {
            return;
        }
        // Chromium omnibox guard: the omnibox's inline autocomplete keeps a
        // trailing selection, which the first synthetic Backspace would delete
        // instead of a character (`hoongf`→`hoồng`). When an edit with backspaces
        // is about to land in a Chromium browser AND the focused element really
        // has a selection (one cheap AX check), clear the selection first with a
        // forward-delete. In a normal field the selection is empty, so nothing is
        // posted and nothing can regress; forward-delete is also a no-op at the
        // end of the text, GlowKey's normal caret position.
        if response.backspaces > 0 {
            let chromium = self
                .last_bundle_id
                .borrow()
                .as_deref()
                .is_some_and(is_chromium_browser);
            if chromium && crate::ax::focused_text_field_has_selection() {
                crate::log::log("OMNIBOX trailing selection detected — clearing with ⌦");
                post_key(&self.source, KEY_CODE_FORWARD_DELETE as u16, true);
                post_key(&self.source, KEY_CODE_FORWARD_DELETE as u16, false);
            }
        }
        if debug_enabled() {
            eprintln!(
                "GlowKey emit: backspaces={} insert={:?}",
                response.backspaces, response.insert
            );
        }
        emit(&self.source, response);
    }

    /// Resolves the frontmost app at a word start (not mid-word) and, on a change,
    /// tells the session — keeping the ignore list honest without a per-keystroke
    /// workspace query. Separated from [`decide`](Self::decide) so the decision
    /// logic is a pure function of the event and session state, and testable.
    fn refresh_frontmost_at_word_start(&self) {
        let Ok(mut session) = self.session.try_borrow_mut() else {
            return;
        };
        if session.is_composing() {
            return;
        }
        if let Some(bundle_id) = frontmost_bundle_id() {
            let mut last = self.last_bundle_id.borrow_mut();
            if last.as_deref() != Some(bundle_id.as_str()) {
                session.set_frontmost_app(bundle_id.clone());
                *last = Some(bundle_id);
            }
        }
    }

    /// Decides what to do with one key-down event: pass it through, suppress it, or
    /// suppress it and emit an edit. Pure with respect to the OS (no event
    /// synthesis, no workspace query), so it can be driven by real `CGEvent`s in
    /// tests.
    fn decide(&self, event: NonNull<CGEvent>) -> Decision {
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
        if !session.is_active() {
            return Decision::Passthrough;
        }

        if keycode == KEY_CODE_DELETE {
            // The host always performs the delete; we only re-sync the engine to
            // whatever the screen will then show. Three cases, in order:
            //   - deleting the boundary right after a committed word re-composes it
            //     so the next keys keep editing it (hồng␣⌫z → hông);
            //   - mid-word, shrink the composition by one visible character and
            //     stay composed, so the next key is still a Telex key rather than a
            //     literal (hoongf⌫z → hôn, not hồnz);
            //   - if the engine cannot stay in step, flush and stop composing.
            if !session.recompose_after_boundary_backspace() && !session.backspace_visible_char() {
                session.flush();
            }
            return Decision::Passthrough;
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

    /// Cancels an in-progress hotkey recording (Esc, a mouse click, or an app
    /// switch). No-op when not recording.
    pub fn cancel_hotkey_recording(&self) {
        let mut recording = self.recording_hotkey.borrow_mut();
        if *recording {
            *recording = false;
            drop(recording);
            crate::log::log("HOTKEY recording cancelled");
            crate::prefs_window::hotkey_recording_done();
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
        crate::prefs_window::hotkey_recording_done();
        Decision::Consume
    }
}

/// The outcome of processing one key event.
#[derive(Debug)]
enum Decision {
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

/// Whether `keycode` is a caret-navigation key (arrows, Home/End, Page Up/Down).
/// These move the insertion point without any text change, so GlowKey must flush
/// its diff baseline when one is pressed.
fn is_caret_move(keycode: i64) -> bool {
    // Left 123, Right 124, Down 125, Up 126, Home 115, End 119, PgUp 116, PgDn 121.
    matches!(keycode, 123 | 124 | 125 | 126 | 115 | 116 | 119 | 121)
}

/// macOS virtual key code for Space.
const KEY_CODE_SPACE: i64 = 49;
/// macOS virtual key code for the letter E.
const KEY_CODE_E: i64 = 14;
/// macOS virtual key code for the letter Z.
const KEY_CODE_Z: i64 = 6;

/// True when only Control and Shift are held (no Command or Option) and the key is
/// `keycode` — the modifier pattern shared by GlowKey's ⌃⇧ hotkeys.
fn is_ctrl_shift(flags: CGEventFlags, keycode: i64, target: i64) -> bool {
    if keycode != target {
        return false;
    }
    let control = flags.0 & CGEventFlags::MaskControl.0 != 0;
    let shift = flags.0 & CGEventFlags::MaskShift.0 != 0;
    let command = flags.0 & CGEventFlags::MaskCommand.0 != 0;
    let option = flags.0 & CGEventFlags::MaskAlternate.0 != 0;
    control && shift && !command && !option
}

/// The VN/EN toggle hotkey — matches the chosen preset or a recorded custom combo.
fn is_toggle_hotkey(flags: CGEventFlags, keycode: i64, preset: HotkeyPreset) -> bool {
    // (control, shift, option, keycode) for each preset. Command is never allowed.
    let (ctrl, shift, option, target) = match preset {
        HotkeyPreset::CtrlShiftSpace => (true, true, false, KEY_CODE_SPACE),
        HotkeyPreset::CtrlSpace => (true, false, false, KEY_CODE_SPACE),
        HotkeyPreset::OptionSpace => (false, false, true, KEY_CODE_SPACE),
        HotkeyPreset::CtrlShiftZ => (true, true, false, KEY_CODE_Z),
        HotkeyPreset::Custom {
            control,
            shift,
            option,
            keycode,
            ..
        } => (control, shift, option, keycode),
    };
    if keycode != target {
        return false;
    }
    let f_ctrl = flags.0 & CGEventFlags::MaskControl.0 != 0;
    let f_shift = flags.0 & CGEventFlags::MaskShift.0 != 0;
    let f_command = flags.0 & CGEventFlags::MaskCommand.0 != 0;
    let f_option = flags.0 & CGEventFlags::MaskAlternate.0 != 0;
    f_ctrl == ctrl && f_shift == shift && f_option == option && !f_command
}

/// The per-app enable/disable hotkey: ⌃⇧E.
fn is_app_toggle_hotkey(flags: CGEventFlags, keycode: i64) -> bool {
    is_ctrl_shift(flags, keycode, KEY_CODE_E)
}

/// True when a shortcut modifier is held — Command, Control, or Option. Shift is
/// excluded (it produces uppercase letters).
/// Renders the modifier flags of a key event compactly for the log ("⌘⇧", "-").
/// Without this a logged `q` cannot be told apart from ⌘Q, which is the
/// difference between a plain keystroke and a quit.
fn modifier_names(flags: CGEventFlags) -> String {
    let mut names = String::new();
    for (mask, symbol) in [
        (CGEventFlags::MaskCommand, "⌘"),
        (CGEventFlags::MaskControl, "⌃"),
        (CGEventFlags::MaskAlternate, "⌥"),
        (CGEventFlags::MaskShift, "⇧"),
        (CGEventFlags::MaskSecondaryFn, "fn"),
    ] {
        if flags.0 & mask.0 != 0 {
            names.push_str(symbol);
        }
    }
    if names.is_empty() {
        names.push('-');
    }
    names
}

fn is_shortcut(flags: CGEventFlags) -> bool {
    let shortcut =
        CGEventFlags::MaskCommand.0 | CGEventFlags::MaskControl.0 | CGEventFlags::MaskAlternate.0;
    flags.0 & shortcut != 0
}

/// Reads an integer field from an event.
fn integer_field(event: NonNull<CGEvent>, field: CGEventField) -> i64 {
    unsafe { CGEvent::integer_value_field(Some(event.as_ref()), field) }
}

/// Extracts the typed character (already mapped through the active layout).
fn unicode_char(event: NonNull<CGEvent>) -> Option<char> {
    let mut buf = [0u16; 4];
    let mut actual: u64 = 0;
    unsafe {
        CGEvent::keyboard_get_unicode_string(
            Some(event.as_ref()),
            buf.len() as u64,
            &mut actual,
            buf.as_mut_ptr(),
        );
    }
    let len = (actual as usize).min(buf.len());
    String::from_utf16(&buf[..len])
        .ok()
        .and_then(|s| s.chars().next())
}

/// Emits the engine's edit — `backspaces` deletions then the inserted text — at the
/// session level. Session posting goes through the normal input path, which the OS
/// routes to the focused element correctly even for multi-process apps (Chrome's
/// text field lives in a renderer process, not the main one). GlowKey's own events
/// are tagged on their source and skipped by the tap, so they do not feed back.
fn emit(source: &CGEventSource, response: &KeyResponse) {
    for _ in 0..response.backspaces {
        post_key(source, KEY_CODE_DELETE as u16, true);
        post_key(source, KEY_CODE_DELETE as u16, false);
    }
    if !response.insert.is_empty() {
        post_string(source, &response.insert);
    }
}

/// Posts a synthetic keystroke at the session level, from GlowKey's tagged source.
fn post_key(source: &CGEventSource, keycode: u16, key_down: bool) {
    post_key_with_flags(source, keycode, CGEventFlags(0), key_down);
}

/// Posts a synthetic keystroke carrying explicit modifier flags. Replaying a
/// boundary key needs the flags the user actually held, or ⇧1 comes back as `1`
/// instead of `!`.
fn post_key_with_flags(
    source: &CGEventSource,
    keycode: u16,
    flags: CGEventFlags,
    key_down: bool,
) {
    if let Some(event) = CGEvent::new_keyboard_event(Some(source), keycode, key_down) {
        CGEvent::set_flags(Some(&event), flags);
        CGEvent::post(CGEventTapLocation::SessionEventTap, Some(&event));
    }
}

/// Posts a synthetic key event carrying a Unicode string, from GlowKey's source.
fn post_string(source: &CGEventSource, text: &str) {
    let utf16: Vec<u16> = text.encode_utf16().collect();
    // Key-down carries the string; a matching key-up keeps the event pair balanced.
    for key_down in [true, false] {
        let Some(event) = CGEvent::new_keyboard_event(Some(source), 0, key_down) else {
            return;
        };
        if key_down {
            unsafe {
                CGEvent::keyboard_set_unicode_string(
                    Some(&event),
                    utf16.len() as u64,
                    utf16.as_ptr(),
                );
            }
        }
        CGEvent::post(CGEventTapLocation::SessionEventTap, Some(&event));
    }
}

/// GlowKey's own bundle identifier (None when running unbundled, e.g. tests).
fn own_bundle_id() -> Option<String> {
    use std::sync::OnceLock;
    static OWN: OnceLock<Option<String>> = OnceLock::new();
    OWN.get_or_init(|| {
        objc2_foundation::NSBundle::mainBundle()
            .bundleIdentifier()
            .map(|s| s.to_string())
    })
    .clone()
}

/// Bundle identifier of the frontmost application, for the ignore list.
fn frontmost_bundle_id() -> Option<String> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;
    app.bundleIdentifier().map(|s| s.to_string())
}

/// True when the event was synthesized by GlowKey — its source carries our tag.
/// Reading the source from the event is the documented way to recognize our own
/// output and avoid a feedback loop.
fn is_own_event(event: NonNull<CGEvent>) -> bool {
    let Some(source) = CGEvent::new_source_from_event(Some(unsafe { event.as_ref() })) else {
        return false;
    };
    CGEventSource::user_data(Some(&source)) == GLOWKEY_TAG
}

/// The C tap callback. Wrapped in `catch_unwind` because a panic must not unwind
/// into CoreFoundation's C frames; on panic the event passes through unchanged.
unsafe extern "C-unwind" fn tap_callback(
    _proxy: CGEventTapProxy,
    event_type: CGEventType,
    event: NonNull<CGEvent>,
    user_info: *mut c_void,
) -> *mut CGEvent {
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        tap_dispatch(event_type, event, user_info)
    }));
    result.unwrap_or(event.as_ptr())
}

/// The actual callback logic, separated so it can be wrapped in `catch_unwind`.
fn tap_dispatch(
    event_type: CGEventType,
    event: NonNull<CGEvent>,
    user_info: *mut c_void,
) -> *mut CGEvent {
    let ctx = unsafe { &*(user_info as *const TapContext) };

    // The system disables the tap on timeout or heavy load; re-enable it.
    if matches!(
        event_type,
        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
    ) {
        if let Ok(port) = ctx.port.try_borrow() {
            if let Some(port) = port.as_ref() {
                CGEvent::tap_enable(port, true);
            }
        }
        return event.as_ptr();
    }

    // A mouse click can move the caret without any key event, which would leave the
    // engine's diff baseline stale. Flush the in-progress word on mouse-down.
    if matches!(
        event_type,
        CGEventType::LeftMouseDown | CGEventType::RightMouseDown
    ) {
        ctx.state.flush();
        return event.as_ptr();
    }

    if event_type != CGEventType::KeyDown {
        return event.as_ptr();
    }

    // Skip GlowKey's own synthesized events so they do not feed back into the tap.
    if is_own_event(event) {
        return event.as_ptr();
    }

    if ctx.state.handle_key_down(event) {
        std::ptr::null_mut() // consumed: suppress the original event
    } else {
        event.as_ptr()
    }
}

/// Everything the callback needs: the shell state plus the tap port (to re-enable
/// it). The port is filled in after the tap is created, so it lives behind a
/// `RefCell`. Boxed and leaked for the program's lifetime.
struct TapContext {
    state: TapState,
    port: RefCell<Option<CFRetained<objc2_core_foundation::CFMachPort>>>,
}

/// Creates the event tap and runs the main loop. Returns without running if the
/// Accessibility permission is missing (the tap cannot be created).
pub fn run() {
    // Wait for Accessibility instead of exiting, so the app stays alive while the
    // user grants it (add GlowKey.app in System Settings → Privacy & Security →
    // Accessibility). Once granted the tap starts automatically; some macOS
    // versions need a relaunch to pick up the grant, but polling covers the rest.
    if !prompt_accessibility() {
        eprintln!("GlowKey: waiting for Accessibility permission…");
        eprintln!(
            "  A prompt should have appeared. Enable GlowKey in System Settings → \
             Privacy & Security → Accessibility, then it starts automatically."
        );
        crate::log::log("STARTUP waiting for the Accessibility permission");
        wait_for_accessibility();
    }
    eprintln!("GlowKey: Accessibility granted — starting.");
    crate::log::log("STARTUP Accessibility granted — starting");

    let settings = crate::settings_store::load();
    let Some(state) = TapState::from_settings(&settings) else {
        eprintln!("GlowKey: failed to create the event source.");
        return;
    };

    let key_down = 1u64 << (CGEventType::KeyDown.0 as u64);
    let left_mouse = 1u64 << (CGEventType::LeftMouseDown.0 as u64);
    let right_mouse = 1u64 << (CGEventType::RightMouseDown.0 as u64);
    let mask: CGEventMask = key_down | left_mouse | right_mouse;

    // The context must outlive the run loop; leak it deliberately.
    let ctx: *mut TapContext = Box::into_raw(Box::new(TapContext {
        state,
        port: RefCell::new(None),
    }));

    let port = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            mask,
            Some(tap_callback),
            ctx as *mut c_void,
        )
    };

    let Some(port) = port else {
        crate::log::log("TAP FAILED to create (Accessibility not granted?)");
        eprintln!("GlowKey: failed to create the event tap (Accessibility not granted?).");
        return;
    };

    // Link the port back into the context so the callback can re-enable it.
    unsafe { (*ctx).port.borrow_mut().replace(port.clone()) };

    let source = objc2_core_foundation::CFMachPort::new_run_loop_source(None, Some(&port), 0);
    let (Some(run_loop), Some(source)) = (CFRunLoop::current(), source) else {
        return;
    };
    run_loop.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
    CGEvent::tap_enable(&port, true);

    // Install the menu bar (shares the same leaked TapState) and run the AppKit
    // event loop, which drives both the status item and the tap's run-loop source.
    // The status item and controller are leaked so they live for the process.
    if let Some(mtm) = objc2_foundation::MainThreadMarker::new() {
        let state_ptr: *const TapState = unsafe { &(*ctx).state };
        let (item, controller) = crate::menu_bar::install(unsafe { &*state_ptr }, mtm);
        std::mem::forget(item);
        std::mem::forget(controller);
        // Show the Settings window on launch (like EVKey/Unikey opening their
        // control panel), unless the user has turned that off in Settings.
        if unsafe { (*state_ptr).open_settings_at_launch() } {
            crate::prefs_window::show(state_ptr, mtm);
        }
        let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
        app.run();
    } else {
        CFRunLoop::run();
    }
}

/// Blocks until this process is trusted for Accessibility, keeping an alert on
/// screen for as long as it waits.
///
/// GlowKey is an `LSUIElement` agent: no Dock icon, and the status item cannot
/// draw before the AppKit loop runs. Polling in a bare sleep loop therefore left
/// a launch from Finder or `open` with nothing at all to see — no icon, no
/// window, no log line — and the app looked dead while it was in fact waiting.
/// A modal *session* is the one thing that renders here: it drives the run loop
/// so the alert appears, and it hands control back on every pass, so the wait
/// ends by itself the moment the user flips the switch.
fn wait_for_accessibility() {
    use objc2_app_kit::{
        NSAlert, NSAlertFirstButtonReturn, NSApplication, NSModalResponseContinue,
    };
    use objc2_foundation::{MainThreadMarker, NSString};

    let Some(mtm) = MainThreadMarker::new() else {
        while !accessibility_trusted() {
            std::thread::sleep(Duration::from_millis(500));
        }
        return;
    };

    // Name the running bundle rather than the project: "GlowKey" and "GlowKey Dev"
    // are separate entries in the Accessibility list, and the alert has to say
    // which one to switch on.
    let name = bundle_display_name();
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(&format!(
        "{name} needs Accessibility permission"
    )));
    alert.setInformativeText(&NSString::from_str(&format!(
        "Open System Settings → Privacy & Security → Accessibility and turn {name} on. \
         Vietnamese typing starts by itself the moment you do — leave this window open.\n\n\
         Already in the list? The permission is tied to this exact copy of the app, so a \
         rebuild or a move (to /Applications, say) needs a fresh grant: switch {name} off \
         and on again, or remove it with “−” and add this copy back."
    )));
    alert.addButtonWithTitle(&NSString::from_str("Open System Settings"));
    alert.addButtonWithTitle(&NSString::from_str(&format!("Quit {name}")));

    let app = NSApplication::sharedApplication(mtm);
    // The main loop has not started yet, and AppKit will not put a window on
    // screen until the app has finished launching. Without this the modal session
    // runs but draws nothing — the very silence this alert exists to break.
    app.finishLaunching();
    let window = alert.window();

    loop {
        if accessibility_trusted() {
            break;
        }
        // Bring the alert to the front: an agent app is never the active app, so
        // without this the window can open behind whatever the user is using.
        app.activate();
        let session = app.beginModalSessionForWindow(&window);
        let pressed = loop {
            if accessibility_trusted() {
                break None;
            }
            let response = unsafe { app.runModalSession(session) };
            if response != NSModalResponseContinue {
                break Some(response);
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        unsafe { app.endModalSession(session) };
        match pressed {
            // Granted while the alert was up.
            None => break,
            // "Open System Settings" — send them to the right pane, then show the
            // alert again so the app still has a visible presence while it waits.
            Some(response) if response == NSAlertFirstButtonReturn => {
                open_accessibility_settings()
            }
            _ => {
                crate::log::log("STARTUP quit at the Accessibility gate");
                std::process::exit(0);
            }
        }
    }
    window.orderOut(None);
}

/// The running bundle's display name ("GlowKey", "GlowKey Dev"), falling back to
/// the project name when unbundled (tests).
fn bundle_display_name() -> String {
    use objc2_foundation::{NSBundle, NSString};

    for key in ["CFBundleDisplayName", "CFBundleName"] {
        if let Some(name) = NSBundle::mainBundle()
            .objectForInfoDictionaryKey(&NSString::from_str(key))
            .and_then(|value| value.downcast::<NSString>().ok())
        {
            return name.to_string();
        }
    }
    "GlowKey".to_string()
}

/// Opens System Settings straight at Privacy & Security → Accessibility.
fn open_accessibility_settings() {
    use objc2_foundation::{NSString, NSURL};

    let url = NSURL::URLWithString(&NSString::from_str(
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
    ));
    if let Some(url) = url {
        NSWorkspace::sharedWorkspace().openURL(&url);
    }
}

/// Whether this process is trusted for Accessibility (required for the tap).
fn accessibility_trusted() -> bool {
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    unsafe { AXIsProcessTrusted() }
}

/// Shows the system Accessibility prompt and registers GlowKey in the
/// Accessibility list, so the user can grant it with one click. Returns the
/// current trust state.
fn prompt_accessibility() -> bool {
    use objc2_core_foundation::{
        kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks, CFDictionary,
    };
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        static kAXTrustedCheckOptionPrompt: *const c_void; // CFStringRef
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    }
    unsafe {
        // Build { kAXTrustedCheckOptionPrompt: true } and ask with a prompt.
        let true_value = objc2_core_foundation::kCFBooleanTrue;
        let key = kAXTrustedCheckOptionPrompt;
        let value = true_value
            .map(|b| (b as *const objc2_core_foundation::CFBoolean).cast::<c_void>())
            .unwrap_or(std::ptr::null());
        let mut keys = [key];
        let mut values = [value];
        let options = CFDictionary::new(
            None,
            keys.as_mut_ptr(),
            values.as_mut_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        let options_ptr = options
            .as_ref()
            .map(|d| (d.as_ref() as *const objc2_core_foundation::CFDictionary).cast::<c_void>())
            .unwrap_or(std::ptr::null());
        AXIsProcessTrustedWithOptions(options_ptr)
    }
}

#[cfg(test)]
mod real_event_tests {
    //! End-to-end tests driving the real tap decision path with real CoreGraphics
    //! key events (real CGEvent objects, real Unicode decode, real engine). This
    //! covers everything except the system-level tap install and event injection,
    //! which require Accessibility permission a test process cannot grant.

    use super::*;

    /// Builds a real key-down CGEvent from GlowKey's source carrying `ch` as its
    /// Unicode string (keycode 0, no modifiers) — what the tap would see for a
    /// letter typed on the active layout.
    fn key_event(source: &CGEventSource, ch: char) -> CFRetained<CGEvent> {
        let event = CGEvent::new_keyboard_event(Some(source), 0, true).expect("event");
        let utf16: Vec<u16> = ch.to_string().encode_utf16().collect();
        unsafe {
            CGEvent::keyboard_set_unicode_string(Some(&event), utf16.len() as u64, utf16.as_ptr());
        }
        event
    }

    /// Builds a real Backspace key-down event (virtual keycode 51).
    fn backspace_event(source: &CGEventSource) -> CFRetained<CGEvent> {
        CGEvent::new_keyboard_event(Some(source), KEY_CODE_DELETE as u16, true).expect("event")
    }

    /// Builds a real key-down event for a caret-navigation key by virtual keycode
    /// (e.g. Left = 123), with no Unicode string — as the tap sees an arrow key.
    fn nav_event(source: &CGEventSource, keycode: u16) -> CFRetained<CGEvent> {
        CGEvent::new_keyboard_event(Some(source), keycode, true).expect("event")
    }

    /// Types `input` through the real `decide()` path and returns the resulting
    /// on-screen text, applying each Decision exactly as the OS would.
    fn type_via_tap(state: &TapState, input: &str) -> String {
        let mut screen = String::new();
        for ch in input.chars() {
            let event = key_event(&state.source, ch);
            let ptr = NonNull::from(&*event);
            let apply = |screen: &mut String, r: &KeyResponse| {
                let units: Vec<u16> = screen.encode_utf16().collect();
                let keep = units.len().saturating_sub(r.backspaces);
                *screen = String::from_utf16(&units[..keep]).unwrap();
                screen.push_str(&r.insert);
            };
            match state.decide(ptr) {
                Decision::Passthrough => screen.push(ch),
                Decision::Consume | Decision::ToggleApp => {}
                Decision::Emit(r) => apply(&mut screen, &r),
                Decision::EmitThenReplayKey(r) => {
                    apply(&mut screen, &r);
                    screen.push(ch); // the boundary key still types
                }
            }
        }
        screen
    }

    /// The two shapes reported from the field. An auto-fix restore at a boundary
    /// must leave the raw keys followed by the boundary key, in that order. While
    /// the boundary key was passed through natively instead of replayed, the host
    /// applied it before the posted backspaces and the edit ate it: `ddc`␣ came out
    /// `đddc` and `work`␣ came out `ưwork`, both with the space swallowed.
    #[test]
    fn auto_fix_restore_keeps_the_boundary_key() {
        assert_eq!(type_via_tap(&active_state(), "work "), "work ");
        // A leading đ is exempt from auto-fix, so this one commits with no restore
        // — the boundary key must survive that path too.
        assert_eq!(type_via_tap(&active_state(), "ddc "), "đc ");
    }

    /// Pins the mechanism, not just the result: the boundary key that triggers an
    /// auto-fix restore must be suppressed and replayed, never left to race the
    /// edit as a plain passthrough.
    #[test]
    fn auto_fix_boundary_replays_the_key_rather_than_passing_it_through() {
        let state = active_state();
        for ch in "work".chars() {
            let event = key_event(&state.source, ch);
            state.decide(NonNull::from(&*event));
        }
        let space = key_event(&state.source, ' ');
        match state.decide(NonNull::from(&*space)) {
            Decision::EmitThenReplayKey(_) => {}
            other => panic!("boundary key must be replayed, got {other:?}"),
        }
    }

    fn active_state() -> TapState {
        let state = TapState::new().expect("event source");
        // A non-excluded app so transformation is active.
        state
            .session
            .borrow_mut()
            .set_frontmost_app("com.apple.TextEdit");
        state
    }

    #[test]
    fn real_events_free_tone_placement() {
        // The headline: real key events, tone key in any position → hồng.
        assert_eq!(type_via_tap(&active_state(), "hoongf"), "hồng");
        assert_eq!(type_via_tap(&active_state(), "hofong"), "hồng");
        assert_eq!(type_via_tap(&active_state(), "hoonfg"), "hồng");
        // Multi-transform word through the real emit path (w horns uo→ươ, f tones).
        assert_eq!(type_via_tap(&active_state(), "nguoiwf"), "người");
        // The user's second example, exactly as typed:
        assert_eq!(type_via_tap(&active_state(), "hofngo"), "hồng");
    }

    #[test]
    fn real_events_words_and_english() {
        assert_eq!(type_via_tap(&active_state(), "nguyeenx"), "nguyễn");
        assert_eq!(type_via_tap(&active_state(), "dduwowcj"), "được");
        assert_eq!(type_via_tap(&active_state(), "Hoongf"), "Hồng");
        // English passes through untouched (fast path).
        assert_eq!(type_via_tap(&active_state(), "hello"), "hello");
    }

    #[test]
    fn real_events_boundary_commits_word() {
        // Space is a boundary: the word is already on screen, space passes through.
        assert_eq!(type_via_tap(&active_state(), "hoongf "), "hồng ");
        // Without deleting the space, a following key starts a NEW word — z is
        // literal, not a modifier of the previous word.
        assert_eq!(type_via_tap(&active_state(), "hoongf z"), "hồng z");
    }

    #[test]
    fn toggle_hotkey_presets_match_only_their_combo() {
        let ctrl_shift = CGEventFlags(CGEventFlags::MaskControl.0 | CGEventFlags::MaskShift.0);
        let ctrl = CGEventFlags(CGEventFlags::MaskControl.0);
        let option = CGEventFlags(CGEventFlags::MaskAlternate.0);

        assert!(is_toggle_hotkey(
            ctrl_shift,
            KEY_CODE_SPACE,
            HotkeyPreset::CtrlShiftSpace
        ));
        assert!(!is_toggle_hotkey(
            ctrl,
            KEY_CODE_SPACE,
            HotkeyPreset::CtrlShiftSpace
        ));

        assert!(is_toggle_hotkey(
            ctrl,
            KEY_CODE_SPACE,
            HotkeyPreset::CtrlSpace
        ));
        // Shift must NOT be held for the plain ⌃Space preset.
        assert!(!is_toggle_hotkey(
            ctrl_shift,
            KEY_CODE_SPACE,
            HotkeyPreset::CtrlSpace
        ));

        assert!(is_toggle_hotkey(
            option,
            KEY_CODE_SPACE,
            HotkeyPreset::OptionSpace
        ));

        assert!(is_toggle_hotkey(
            ctrl_shift,
            KEY_CODE_Z,
            HotkeyPreset::CtrlShiftZ
        ));
        // Right modifiers, wrong key.
        assert!(!is_toggle_hotkey(
            ctrl_shift,
            KEY_CODE_SPACE,
            HotkeyPreset::CtrlShiftZ
        ));
    }

    #[test]
    fn real_events_arrow_key_flushes_engine() {
        // An arrow key mid-word must flush (so a stale baseline can't corrupt later
        // edits) and pass through — never emit an edit.
        let state = active_state();
        for ch in "hoo".chars() {
            let event = key_event(&state.source, ch);
            let _ = state.decide(NonNull::from(&*event));
        }
        assert!(state.session.borrow().is_composing());

        let left = nav_event(&state.source, 123); // Left arrow
        assert!(matches!(
            state.decide(NonNull::from(&*left)),
            Decision::Passthrough
        ));
        assert!(
            !state.session.borrow().is_composing(),
            "arrow key must flush the composing word"
        );
    }

    #[test]
    fn real_events_recompose_after_space_backspace() {
        // hồng, Space, Backspace (delete the space), then z (Telex tone-clear) must
        // re-compose the previous word: hồng + z → hông.
        let state = active_state();
        let mut screen = String::new();
        let apply = |screen: &mut String, r: &KeyResponse| {
            let units: Vec<u16> = screen.encode_utf16().collect();
            let keep = units.len().saturating_sub(r.backspaces);
            *screen = String::from_utf16(&units[..keep]).unwrap();
            screen.push_str(&r.insert);
        };

        for ch in "hoongf".chars() {
            let event = key_event(&state.source, ch);
            match state.decide(NonNull::from(&*event)) {
                Decision::Passthrough => screen.push(ch),
                Decision::Emit(r) => apply(&mut screen, &r),
                other => panic!("unexpected {other:?} for {ch}"),
            }
        }
        assert_eq!(screen, "hồng");

        // Space — boundary commits the (valid) word and passes through.
        let space = key_event(&state.source, ' ');
        match state.decide(NonNull::from(&*space)) {
            Decision::Passthrough => screen.push(' '),
            Decision::EmitThenReplayKey(r) => {
                apply(&mut screen, &r);
                screen.push(' ');
            }
            other => panic!("unexpected {other:?} for space"),
        }
        assert_eq!(screen, "hồng ");

        // Backspace — passes through (host deletes the space); engine re-composes.
        let backspace = backspace_event(&state.source);
        match state.decide(NonNull::from(&*backspace)) {
            Decision::Passthrough => {
                screen.pop(); // host deletes the trailing space
            }
            other => panic!("backspace should pass through, got {other:?}"),
        }
        assert_eq!(screen, "hồng");

        // z — now edits the re-composed word: hồng → hông.
        let z = key_event(&state.source, 'z');
        match state.decide(NonNull::from(&*z)) {
            Decision::Emit(r) => apply(&mut screen, &r),
            Decision::Passthrough => screen.push('z'),
            other => panic!("unexpected {other:?} for z"),
        }
        assert_eq!(screen, "hông");
    }

    #[test]
    fn real_events_backspace_deletes_last_visible_char() {
        let state = active_state();
        assert_eq!(type_via_tap(&state, "hoongf"), "hồng");
        assert!(state.session.borrow().is_composing());

        // Backspace passes through (the host deletes the last visible character,
        // hồng → hồn) and the engine shrinks with it, staying composed so the next
        // key is still a Telex key: z removes the tone rather than typing a literal.
        let bs = backspace_event(&state.source);
        assert!(matches!(
            state.decide(NonNull::from(&*bs)),
            Decision::Passthrough
        ));
        assert!(state.session.borrow().is_composing());
        let (raw, rendered, _, _) = state.session.borrow().debug_state();
        assert_eq!((raw.as_str(), rendered.as_str()), ("hoonf", "hồn"));

        let z = key_event(&state.source, 'z');
        match state.decide(NonNull::from(&*z)) {
            Decision::Emit(r) => {
                let mut screen = String::from("hồn");
                let units: Vec<u16> = screen.encode_utf16().collect();
                screen = String::from_utf16(&units[..units.len() - r.backspaces]).unwrap();
                screen.push_str(&r.insert);
                assert_eq!(screen, "hôn");
            }
            other => panic!("z after a mid-word backspace must edit the word, got {other:?}"),
        }
    }

    #[test]
    fn real_events_shortcut_flushes_engine() {
        // ⌘A (select-all) changes the selection; the engine must flush so the next
        // keystroke is not diffed against a stale baseline (the select-all → hoồng
        // bug). A ⌘-shortcut passes through and clears composing state.
        let state = active_state();
        assert_eq!(type_via_tap(&state, "hoong"), "hông");
        assert!(state.session.borrow().is_composing());

        let event = CGEvent::new_keyboard_event(Some(&state.source), 0, true).expect("event");
        CGEvent::set_flags(Some(&event), CGEventFlags(CGEventFlags::MaskCommand.0));
        assert!(matches!(
            state.decide(NonNull::from(&*event)),
            Decision::Passthrough
        ));
        assert!(!state.session.borrow().is_composing());
    }

    #[test]
    fn real_events_excluded_app_passes_through() {
        let state = TapState::new().expect("source");
        state
            .session
            .borrow_mut()
            .set_frontmost_app("com.apple.Terminal"); // default exclusion
        assert_eq!(type_via_tap(&state, "hoongf"), "hoongf");
    }

    /// A real ⌃⇧ + `keycode` key event.
    fn ctrl_shift_event(source: &CGEventSource, keycode: u16) -> CFRetained<CGEvent> {
        let event = CGEvent::new_keyboard_event(Some(source), keycode, true).expect("event");
        let flags = CGEventFlags(CGEventFlags::MaskControl.0 | CGEventFlags::MaskShift.0);
        CGEvent::set_flags(Some(&event), flags);
        event
    }

    /// A real Control+Shift+Space key event.
    fn toggle_event(source: &CGEventSource) -> CFRetained<CGEvent> {
        ctrl_shift_event(source, KEY_CODE_SPACE as u16)
    }

    #[test]
    fn real_events_app_toggle_hotkey() {
        // ⌃⇧E toggles the current app's ignore-list membership and consumes the key.
        let state = active_state(); // frontmost = TextEdit, not excluded
        assert_eq!(type_via_tap(&state, "hoongf"), "hồng");

        let ev = ctrl_shift_event(&state.source, KEY_CODE_E as u16);
        assert!(matches!(
            state.decide(NonNull::from(&*ev)),
            Decision::ToggleApp
        ));
        // Applying the toggle (as handle_key_down does) excludes TextEdit.
        assert!(state
            .session
            .borrow_mut()
            .toggle_app_exclusion("com.apple.TextEdit")
            .excluded());
        assert_eq!(type_via_tap(&state, "hoongf"), "hoongf");
    }

    #[test]
    fn chromium_browsers_are_classified_by_prefix() {
        assert!(is_chromium_browser("com.google.Chrome"));
        assert!(is_chromium_browser("com.google.Chrome.canary"));
        assert!(is_chromium_browser("com.microsoft.edgemac"));
        assert!(is_chromium_browser("com.brave.Browser"));
        // The omnibox guard must never run outside Chromium browsers.
        assert!(!is_chromium_browser("com.apple.Safari"));
        assert!(!is_chromium_browser("org.mozilla.firefox"));
        assert!(!is_chromium_browser("com.apple.TextEdit"));
    }

    /// A key event with arbitrary modifier flags.
    fn flagged_event(source: &CGEventSource, keycode: u16, flags: CGEventFlags) -> CFRetained<CGEvent> {
        let event = CGEvent::new_keyboard_event(Some(source), keycode, true).expect("event");
        CGEvent::set_flags(Some(&event), flags);
        event
    }

    #[test]
    fn hotkey_recording_captures_a_custom_combo() {
        let state = active_state();
        state.begin_hotkey_recording();
        assert!(state.is_recording_hotkey());

        // A plain letter passes through (typing must never be blocked by an
        // armed recorder) and recording continues.
        let plain = key_event(&state.source, 'k');
        assert!(matches!(
            state.decide(NonNull::from(&*plain)),
            Decision::Passthrough
        ));
        assert!(state.is_recording_hotkey());

        // ⌘ shortcuts pass through too (⌘Q/⌘Tab must keep working).
        let cmd = flagged_event(&state.source, 12, CGEventFlags(CGEventFlags::MaskCommand.0));
        assert!(matches!(
            state.decide(NonNull::from(&*cmd)),
            Decision::Passthrough
        ));
        assert!(state.is_recording_hotkey());

        // ⌃⇧E is reserved for the per-app toggle: swallowed, still recording.
        let reserved = ctrl_shift_event(&state.source, KEY_CODE_E as u16);
        assert!(matches!(
            state.decide(NonNull::from(&*reserved)),
            Decision::Consume
        ));
        assert!(state.is_recording_hotkey());
        assert_eq!(state.toggle_hotkey(), HotkeyPreset::CtrlShiftSpace);

        // ⌃⌥K (keycode 40) becomes the custom hotkey and ends recording.
        let combo = flagged_event(
            &state.source,
            40,
            CGEventFlags(CGEventFlags::MaskControl.0 | CGEventFlags::MaskAlternate.0),
        );
        assert!(matches!(
            state.decide(NonNull::from(&*combo)),
            Decision::Consume
        ));
        assert!(!state.is_recording_hotkey());
        let recorded = state.toggle_hotkey();
        assert!(matches!(
            recorded,
            HotkeyPreset::Custom {
                control: true,
                shift: false,
                option: true,
                keycode: 40,
                ..
            }
        ));

        // The recorded combo now toggles VN/EN…
        let combo = flagged_event(
            &state.source,
            40,
            CGEventFlags(CGEventFlags::MaskControl.0 | CGEventFlags::MaskAlternate.0),
        );
        assert!(matches!(
            state.decide(NonNull::from(&*combo)),
            Decision::Consume
        ));
        assert_eq!(
            state.session.borrow().mode(),
            glowkey_engine::InputMode::English
        );
        // …and the old default (⌃⇧Space) no longer does.
        let old = toggle_event(&state.source);
        state.decide(NonNull::from(&*old));
        assert_eq!(
            state.session.borrow().mode(),
            glowkey_engine::InputMode::English,
            "the replaced preset must not toggle anymore"
        );
    }

    #[test]
    fn hotkey_recording_escape_cancels() {
        let state = active_state();
        state.begin_hotkey_recording();
        let esc = nav_event(&state.source, KEY_CODE_ESCAPE as u16);
        assert!(matches!(
            state.decide(NonNull::from(&*esc)),
            Decision::Consume
        ));
        assert!(!state.is_recording_hotkey());
        // The preset is unchanged.
        assert_eq!(state.toggle_hotkey(), HotkeyPreset::CtrlShiftSpace);
    }

    #[test]
    fn hotkey_recording_cancelled_by_mouse_click() {
        // A mouse click (the tap's flush path) cancels an armed recorder, so a
        // forgotten recording cannot capture a later ⌃/⌥ combo.
        let state = active_state();
        state.begin_hotkey_recording();
        state.flush();
        assert!(!state.is_recording_hotkey());
        assert_eq!(state.toggle_hotkey(), HotkeyPreset::CtrlShiftSpace);
    }

    #[test]
    fn terminal_toggle_via_hotkey_is_session_only() {
        // ⌃⇧E in a terminal enables Vietnamese for the session, but the snapshot
        // (what gets persisted) still excludes it.
        let state = TapState::new().expect("source");
        state
            .session
            .borrow_mut()
            .set_frontmost_app("com.mitchellh.ghostty");
        assert_eq!(type_via_tap(&state, "hoongf"), "hoongf"); // excluded by default

        let outcome = state
            .session
            .borrow_mut()
            .toggle_app_exclusion("com.mitchellh.ghostty");
        assert_eq!(outcome, ExclusionToggle::EnabledSessionOnly);
        assert_eq!(type_via_tap(&state, "hoongf"), "hồng"); // live for the session
        let snapshot = state.session.borrow().snapshot();
        assert!(
            snapshot
                .exclusions
                .iter()
                .any(|id| id == "com.mitchellh.ghostty"),
            "the persisted exclusion must survive a session-only toggle"
        );
    }

    #[test]
    fn real_events_toggle_hotkey_switches_mode() {
        let state = active_state();
        // Vietnamese by default: transforms.
        assert_eq!(type_via_tap(&state, "hoongf"), "hồng");

        // ⌃⇧Space toggles to English — and is consumed (types nothing).
        let toggle = toggle_event(&state.source);
        assert!(matches!(
            state.decide(NonNull::from(&*toggle)),
            Decision::Consume
        ));
        assert_eq!(
            state.session.borrow().mode(),
            glowkey_engine::InputMode::English
        );

        // Now the same keys pass through untransformed.
        assert_eq!(type_via_tap(&state, "hoongf"), "hoongf");

        // Toggle back to Vietnamese.
        let toggle = toggle_event(&state.source);
        assert!(matches!(
            state.decide(NonNull::from(&*toggle)),
            Decision::Consume
        ));
        assert_eq!(type_via_tap(&state, "hoongf"), "hồng");
    }
}
