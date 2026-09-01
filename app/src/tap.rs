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

    /// Processes one key-down event. Returns `true` to consume it (the engine
    /// transformed and re-emitted the text), or `false` to let it through.
    fn handle_key_down(&self, event: NonNull<CGEvent>) -> bool {
        let flags = unsafe { CGEvent::flags(Some(event.as_ref())) };
        if is_shortcut(flags) {
            return false; // ⌘/⌃/⌥ chords are shortcuts, not text
        }

        let Ok(mut session) = self.session.try_borrow_mut() else {
            return false;
        };

        // Keep the ignore list honest without polling every keystroke: resolve the
        // frontmost app only at a word start (not mid-word), which is cheap and
        // still catches app switches at the boundaries where they matter.
        if !session.is_composing() {
            if let Some(bundle_id) = frontmost_bundle_id() {
                let mut last = self.last_bundle_id.borrow_mut();
                if last.as_deref() != Some(bundle_id.as_str()) {
                    session.set_frontmost_app(bundle_id.clone());
                    *last = Some(bundle_id);
                }
            }
        }

        if !session.is_active() {
            return false;
        }

        let keycode = integer_field(event, CGEventField::KeyboardEventKeycode);
        if keycode == KEY_CODE_DELETE {
            if !session.is_composing() {
                return false; // nothing composing — let the host delete
            }
            let response = session.backspace();
            drop(session);
            emit(&self.source, &response);
            return true;
        }

        match unicode_char(event) {
            Some(ch) if ch.is_ascii_alphabetic() => {
                let response = session.process_key(ch);
                if !response.handled {
                    return false;
                }
                // Fast path: when the engine only appends the character just typed
                // (no diacritic, no reordering), let the original keystroke through
                // untouched. This makes ordinary/English typing zero-overhead and
                // shrinks the window for any event-ordering race.
                if response.backspaces == 0 && response.insert == ch.to_string() {
                    return false;
                }
                drop(session);
                emit(&self.source, &response);
                true
            }
            // A word boundary (space, digit, punctuation): the engine resets and
            // reports the key unhandled. The characters already on screen are the
            // finished word, so let the boundary key through.
            Some(_) => {
                session.process_key('\u{20}'); // any non-letter flushes the word
                false
            }
            None => false,
        }
    }
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
    if !accessibility_trusted() {
        eprintln!(
            "GlowKey needs Accessibility permission. Grant it in System Settings → \
             Privacy & Security → Accessibility, then relaunch."
        );
    }

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
