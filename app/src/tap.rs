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
//! Synthesized events are created from a dedicated [`CGEventSource`] tagged with
//! [`GLOWKEY_TAG`] (its user-data survives the post), so the tap recognizes and
//! skips its own output and never feeds back on itself. An in-flight flag guards
//! synchronous re-entry as a second line of defense.
//!
//! ## Constraints (inherent to the event-tap approach, same as EVKey)
//!
//! - Requires an Accessibility permission grant. Without it the tap cannot be
//!   created and GlowKey stays inert.
//! - Does not work in secure input fields (passwords): macOS withholds those
//!   events from all event taps.
//!
//! NOTE: none of this can be unit-tested — it needs Accessibility granted and a
//! running session. The engine crate carries the tested logic; this layer is
//! verified by granting permission, running, and typing. See `docs/checkpoint.md`.

use std::cell::RefCell;
use std::ffi::c_void;
use std::panic::AssertUnwindSafe;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};

use glowkey_engine::{ExclusionList, KeyResponse, PlacementStyle, Session};
use objc2_app_kit::NSWorkspace;
use objc2_core_foundation::{kCFRunLoopCommonModes, CFRetained, CFRunLoop};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventMask, CGEventSource, CGEventSourceStateID,
    CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy, CGEventType,
};

/// User-data tag on GlowKey's own event source, so the tap can recognize and skip
/// its own synthesized output.
const GLOWKEY_TAG: i64 = 0x47_4C_4F_57; // "GLOW"

/// macOS virtual key code for Delete/Backspace.
const KEY_CODE_DELETE: i64 = 51;

/// Set while GlowKey is posting its own synthesized events. Defense-in-depth
/// against a feedback loop: if a posted event re-enters the callback synchronously,
/// the callback sees this flag and passes it through instead of re-processing it.
/// The tagged event source is the primary guard; this is the belt to its suspenders.
static EMITTING: AtomicBool = AtomicBool::new(false);

/// Long-lived shell state, referenced from the C tap callback via a raw pointer.
/// The callback runs on the main run loop thread, so a `RefCell` is sufficient —
/// no cross-thread access.
struct TapState {
    session: RefCell<Session>,
    last_bundle_id: RefCell<Option<String>>,
    /// The tagged event source all synthesized events are created from.
    source: CFRetained<CGEventSource>,
}

impl TapState {
    fn new() -> Option<Self> {
        let source = CGEventSource::new(CGEventSourceStateID::Private)?;
        CGEventSource::set_user_data(Some(&source), GLOWKEY_TAG);
        Some(Self {
            session: RefCell::new(Session::new(
                PlacementStyle::New,
                ExclusionList::with_defaults(),
            )),
            last_bundle_id: RefCell::new(None),
            source,
        })
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
            Decision::Emit(response) => {
                emit(&self.source, &response);
                true
            }
        }
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

    /// Decides what to do with one key-down event: pass it through, or suppress it
    /// and emit an edit. Pure with respect to the OS (no event synthesis, no
    /// workspace query), so it can be driven by real `CGEvent`s in tests.
    fn decide(&self, event: NonNull<CGEvent>) -> Decision {
        let flags = unsafe { CGEvent::flags(Some(event.as_ref())) };
        if is_shortcut(flags) {
            return Decision::Passthrough; // ⌘/⌃/⌥ chords are shortcuts, not text
        }

        let Ok(mut session) = self.session.try_borrow_mut() else {
            return Decision::Passthrough;
        };
        if !session.is_active() {
            return Decision::Passthrough;
        }

        let keycode = integer_field(event, CGEventField::KeyboardEventKeycode);
        if keycode == KEY_CODE_DELETE {
            if !session.is_composing() {
                return Decision::Passthrough; // nothing composing — let host delete
            }
            return Decision::Emit(session.backspace());
        }

        match unicode_char(event) {
            Some(ch) if ch.is_ascii_alphabetic() => {
                let response = session.process_key(ch);
                if !response.handled {
                    return Decision::Passthrough;
                }
                // Fast path: when the engine only appends the character just typed
                // (no diacritic, no reordering), let the original keystroke through
                // untouched. This makes ordinary/English typing zero-overhead and
                // shrinks the window for any event-ordering race.
                if response.backspaces == 0 && response.insert == ch.to_string() {
                    return Decision::Passthrough;
                }
                Decision::Emit(response)
            }
            // A word boundary (space, digit, punctuation): the engine resets and
            // reports the key unhandled. The characters already on screen are the
            // finished word, so let the boundary key through.
            Some(_) => {
                session.process_key('\u{20}'); // any non-letter flushes the word
                Decision::Passthrough
            }
            None => Decision::Passthrough,
        }
    }
}

/// The outcome of processing one key event.
enum Decision {
    /// Let the original keystroke through unchanged.
    Passthrough,
    /// Suppress the original and apply this edit (backspaces + insert).
    Emit(KeyResponse),
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

/// Emits the engine's edit from GlowKey's tagged source: `backspaces` deletions,
/// then the inserted text. Sets [`EMITTING`] for the duration.
fn emit(source: &CGEventSource, response: &KeyResponse) {
    EMITTING.store(true, Ordering::SeqCst);
    for _ in 0..response.backspaces {
        post_key(source, KEY_CODE_DELETE as u16, true);
        post_key(source, KEY_CODE_DELETE as u16, false);
    }
    if !response.insert.is_empty() {
        post_string(source, &response.insert);
    }
    EMITTING.store(false, Ordering::SeqCst);
}

/// Posts a synthetic keystroke by virtual key code, from GlowKey's tagged source.
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

    // Ignore our own synthesized events — by the in-flight flag (synchronous
    // re-entry) and by the source's user-data tag.
    if EMITTING.load(Ordering::SeqCst)
        || integer_field(event, CGEventField::EventSourceUserData) == GLOWKEY_TAG
    {
        return event.as_ptr();
    }

    if event_type != CGEventType::KeyDown {
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

    let Some(state) = TapState::new() else {
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
    if let (Some(run_loop), Some(source)) = (CFRunLoop::current(), source) {
        run_loop.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
        CGEvent::tap_enable(&port, true);
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
            match state.decide(ptr) {
                Decision::Passthrough => screen.push(ch),
                Decision::Emit(r) => {
                    let units: Vec<u16> = screen.encode_utf16().collect();
                    let keep = units.len().saturating_sub(r.backspaces);
                    screen = String::from_utf16(&units[..keep]).unwrap();
                    screen.push_str(&r.insert);
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
    fn real_events_backspace() {
        let state = active_state();
        let mut screen = String::new();
        for ch in "hoongf".chars() {
            let event = key_event(&state.source, ch);
            if let Decision::Emit(r) = state.decide(NonNull::from(&*event)) {
                let units: Vec<u16> = screen.encode_utf16().collect();
                let keep = units.len().saturating_sub(r.backspaces);
                screen = String::from_utf16(&units[..keep]).unwrap();
                screen.push_str(&r.insert);
            } else {
                screen.push(ch);
            }
        }
        assert_eq!(screen, "hồng");

        // A real Backspace event removes the tone.
        let bs = backspace_event(&state.source);
        if let Decision::Emit(r) = state.decide(NonNull::from(&*bs)) {
            let units: Vec<u16> = screen.encode_utf16().collect();
            let keep = units.len().saturating_sub(r.backspaces);
            screen = String::from_utf16(&units[..keep]).unwrap();
            screen.push_str(&r.insert);
        }
        assert_eq!(screen, "hông");
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
}
