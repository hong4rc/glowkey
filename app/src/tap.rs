//! The CGEventTap shell: GlowKey as a background agent that wraps the active
//! keyboard layout with Vietnamese Telex, like EVKey and OpenKey.
//!
//! ## How it works
//!
//! A `CGEventTap` intercepts key-down events *after* the system keyboard layout has
//! mapped them, so the user's Colemak/US layout stays in effect and GlowKey sees
//! the already-mapped character. For a plain keystroke that the engine does not
//! change, the original event passes through untouched. When the engine transforms
//! (a diacritic or tone appears, moving earlier characters), GlowKey **suppresses**
//! the keystroke and re-emits the result: it posts N backspaces to delete the
//! characters already on screen, then inserts the new Vietnamese text — the same
//! `(backspaces, insert)` diff the engine produces. There is no marked text: every
//! keystroke is written straight to the document.
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

use glowkey_engine::{KeyResponse, Session};
use objc2_app_kit::NSWorkspace;
use objc2_core_foundation::{kCFRunLoopCommonModes, CFRetained, CFRunLoop};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventMask, CGEventSource, CGEventSourceStateID,
    CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy, CGEventType,
};

/// macOS virtual key code for Delete/Backspace.
const KEY_CODE_DELETE: i64 = 51;

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
    /// start). Called by the menu controller's app-activation observer.
    pub fn set_frontmost_app(&self, bundle_id: &str) {
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
            eprintln!("GlowKey: runaway detected — transformation disabled. Restart to re-enable.");
            return false;
        }
        true
    }

    /// Flushes the in-progress word — the engine's edits assume the composing word
    /// is still the document tail, so this must run when the caret may have moved
    /// (a mouse click).
    fn flush(&self) {
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.flush();
        }
    }

    /// Processes one key-down event and applies the result. Returns `true` to
    /// consume the event (suppress the original), or `false` to let it through.
    fn handle_key_down(&self, event: NonNull<CGEvent>) -> bool {
        self.refresh_frontmost_at_word_start();
        match self.decide(event) {
            Decision::Passthrough => false,
            Decision::Consume => true, // suppress, emit nothing (e.g. toggle hotkey)
            Decision::ToggleApp => {
                // Resolve the frontmost app *now* (not a cached value) so ⌃⇧E always
                // toggles the app you are actually in, even before you have typed.
                if let Some((name, bundle_id)) = crate::app_info::frontmost() {
                    let excluded = self
                        .session
                        .try_borrow_mut()
                        .map(|mut s| s.toggle_app_exclusion(&bundle_id))
                        .unwrap_or(false);
                    self.save_settings();
                    eprintln!(
                        "GlowKey: {} Vietnamese for “{name}”",
                        if excluded { "disabled" } else { "enabled" }
                    );
                }
                true
            }
            Decision::Emit(response) => {
                self.emit_edit(&response);
                true
            }
            Decision::EmitThenPassthrough(response) => {
                self.emit_edit(&response);
                false // the boundary key still passes through to the host
            }
        }
    }

    /// Emits one edit through the session-posting path, honoring the circuit breaker
    /// and debug logging.
    fn emit_edit(&self, response: &KeyResponse) {
        if !self.circuit_ok() {
            return;
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

        // VN/EN toggle hotkey (⌃⇧Space): flip mode and consume the key so it does
        // not type a space. Checked before the shortcut filter, since it is one.
        if is_toggle_hotkey(flags, keycode) {
            if let Ok(mut session) = self.session.try_borrow_mut() {
                let mode = session.toggle_mode();
                eprintln!("GlowKey: {mode:?} mode");
            }
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
            // Delete the last visible character like a normal editor (hồng →
            // hồn) and stop composing, rather than undoing the last keystroke.
            // The host performs the delete; we just re-sync.
            session.flush();
            return Decision::Passthrough;
        }

        match unicode_char(event) {
            Some(ch) if ch.is_ascii_alphabetic() => {
                let response = session.process_key(ch);
                if !response.handled {
                    return Decision::Passthrough;
                }
                // Let a plain append (no diacritic, no reordering) pass through the
                // normal input path. This is essential, not just an optimization:
                // a passed-through key is committed by the OS *synchronously*
                // before the next key is processed, so when a later transform posts
                // a backspace, the character it deletes is already on screen.
                // Suppressing and re-injecting every key instead makes the injected
                // character asynchronous, so the first transform's backspace can
                // fire before that character lands — producing an extra letter
                // (`exit` → `eexit`, `hoongf` → `hoồng`).
                if response.backspaces == 0 && response.insert == ch.to_string() {
                    return Decision::Passthrough;
                }
                Decision::Emit(response)
            }
            // A word boundary (space, digit, punctuation): commit the word. If
            // auto-fix restores an invalid result to its raw keys, emit that edit
            // and still let the boundary key through afterward; otherwise the word
            // is already on screen and the boundary key just passes through.
            Some(_) => match session.commit() {
                Some(restore) => Decision::EmitThenPassthrough(restore),
                None => Decision::Passthrough,
            },
            None => Decision::Passthrough,
        }
    }
}

/// The outcome of processing one key event.
enum Decision {
    /// Let the original keystroke through unchanged.
    Passthrough,
    /// Suppress the original with no output (e.g. the VN/EN toggle hotkey).
    Consume,
    /// Toggle the current app's ignore-list membership, then consume the key.
    ToggleApp,
    /// Suppress the original and apply this edit (backspaces + insert).
    Emit(KeyResponse),
    /// Apply this edit (e.g. an auto-fix restore) and then let the original key
    /// through — the boundary key that triggered the commit still types.
    EmitThenPassthrough(KeyResponse),
}

/// macOS virtual key code for Space.
const KEY_CODE_SPACE: i64 = 49;
/// macOS virtual key code for the letter E.
const KEY_CODE_E: i64 = 14;

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

/// The VN/EN toggle hotkey: ⌃⇧Space.
fn is_toggle_hotkey(flags: CGEventFlags, keycode: i64) -> bool {
    is_ctrl_shift(flags, keycode, KEY_CODE_SPACE)
}

/// The per-app enable/disable hotkey: ⌃⇧E.
fn is_app_toggle_hotkey(flags: CGEventFlags, keycode: i64) -> bool {
    is_ctrl_shift(flags, keycode, KEY_CODE_E)
}

/// True when a shortcut modifier is held — Command, Control, or Option. Shift is
/// excluded (it produces uppercase letters).
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
    if let Some(event) = CGEvent::new_keyboard_event(Some(source), keycode, key_down) {
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
        while !accessibility_trusted() {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    eprintln!("GlowKey: Accessibility granted — starting.");

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
        let (item, controller) = crate::menu_bar::install(unsafe { &(*ctx).state }, mtm);
        std::mem::forget(item);
        std::mem::forget(controller);
        let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
        app.run();
    } else {
        CFRunLoop::run();
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
                Decision::EmitThenPassthrough(r) => {
                    apply(&mut screen, &r);
                    screen.push(ch); // the boundary key still types
                }
            }
        }
        screen
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
    }

    #[test]
    fn real_events_backspace_deletes_last_visible_char() {
        let state = active_state();
        assert_eq!(type_via_tap(&state, "hoongf"), "hồng");
        assert!(state.session.borrow().is_composing());

        // Backspace passes through (the host deletes the last visible character,
        // hồng → hồn) and the engine stops composing so it re-syncs.
        let bs = backspace_event(&state.source);
        assert!(matches!(
            state.decide(NonNull::from(&*bs)),
            Decision::Passthrough
        ));
        assert!(!state.session.borrow().is_composing());
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
            .toggle_app_exclusion("com.apple.TextEdit"));
        assert_eq!(type_via_tap(&state, "hoongf"), "hoongf");
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
