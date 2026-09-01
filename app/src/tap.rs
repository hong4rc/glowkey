//! The CGEventTap shell: GlowKey as a background agent that wraps the active
//! keyboard layout with Vietnamese Telex, like EVKey and OpenKey.
//!
//! ## How it works
//!
//! A `CGEventTap` intercepts key-down events *after* the system keyboard layout has
//! mapped them, so the user's Colemak/US layout stays in effect and GlowKey sees
//! the already-mapped character. For each key it asks the engine what edit to make
//! and, when the engine transforms, **suppresses the original keystroke** and
//! re-emits the result: it posts N backspaces to delete the characters already on
//! screen, then inserts the new Vietnamese text. This is the same `(backspaces,
//! insert)` diff the engine produces — there is no marked text and no separate
//! commit step, because every keystroke is written straight to the document.
//!
//! Synthesized events are tagged (via the event's user-data field) so the tap
//! ignores its own output and never feeds back on itself.
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
use std::ptr::NonNull;

use glowkey_engine::{ExclusionList, KeyResponse, PlacementStyle, Session};
use objc2_app_kit::NSWorkspace;
use objc2_core_foundation::{kCFRunLoopCommonModes, CFRetained, CFRunLoop};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventMask, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventTapProxy, CGEventType,
};

/// Tag written into synthesized events' user-data field so the tap can recognize
/// and skip its own output (otherwise it would feed back on itself).
const GLOWKEY_TAG: i64 = 0x47_4C_4F_57; // "GLOW"

/// macOS virtual key code for Delete/Backspace.
const KEY_CODE_DELETE: i64 = 51;

/// Long-lived shell state, referenced from the C tap callback via a raw pointer.
/// The callback runs on the main run loop thread, so a `RefCell` is sufficient —
/// no cross-thread access.
struct TapState {
    session: RefCell<Session>,
    last_bundle_id: RefCell<Option<String>>,
}

impl TapState {
    fn new() -> Self {
        Self {
            session: RefCell::new(Session::new(
                PlacementStyle::New,
                ExclusionList::with_defaults(),
            )),
            last_bundle_id: RefCell::new(None),
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

        // Keep the ignore list honest: resolve the frontmost app and, on a change,
        // tell the session (which flushes and re-evaluates exclusion).
        if let Some(bundle_id) = frontmost_bundle_id() {
            let mut last = self.last_bundle_id.borrow_mut();
            if last.as_deref() != Some(bundle_id.as_str()) {
                session.set_frontmost_app(bundle_id.clone());
                *last = Some(bundle_id);
            }
        }

        if !session.is_active() {
            return false;
        }

        let keycode = integer_field(event, CGEventField::KeyboardEventKeycode);
        let response = if keycode == KEY_CODE_DELETE {
            if !session.is_composing() {
                return false; // nothing composing — let the host delete
            }
            session.backspace()
        } else {
            match unicode_char(event) {
                Some(ch) if ch.is_ascii_alphabetic() => session.process_key(ch),
                // A word boundary (space, digit, punctuation): the engine resets
                // and reports the key unhandled. The characters already on screen
                // are the finished word, so just let the boundary key through.
                Some(_) => {
                    session.process_key('\u{20}'); // any non-letter flushes the word
                    return false;
                }
                None => return false,
            }
        };

        if !response.handled {
            return false;
        }
        // Release the borrow before synthesizing events, which re-enter the tap.
        drop(session);
        emit(&response);
        true
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
    let len = actual as usize;
    String::from_utf16(&buf[..len.min(buf.len())])
        .ok()
        .and_then(|s| s.chars().next())
}

/// Emits the engine's edit: `backspaces` deletions, then the inserted text.
fn emit(response: &KeyResponse) {
    for _ in 0..response.backspaces {
        post_key(KEY_CODE_DELETE as u16, true);
        post_key(KEY_CODE_DELETE as u16, false);
    }
    if !response.insert.is_empty() {
        post_string(&response.insert);
    }
}

/// Posts a synthetic keystroke by virtual key code, tagged as our own.
fn post_key(keycode: u16, key_down: bool) {
    let Some(event) = CGEvent::new_keyboard_event(None, keycode, key_down) else {
        return;
    };
    tag_and_post(&event);
}

/// Posts a synthetic key event carrying a Unicode string, tagged as our own.
fn post_string(text: &str) {
    let utf16: Vec<u16> = text.encode_utf16().collect();
    // Key-down carries the string; a matching key-up keeps the event pair balanced.
    for key_down in [true, false] {
        let Some(event) = CGEvent::new_keyboard_event(None, 0, key_down) else {
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
        tag_and_post(&event);
    }
}

/// Tags an event as GlowKey's own output and posts it to the session tap.
fn tag_and_post(event: &CGEvent) {
    CGEvent::set_integer_value_field(Some(event), CGEventField::EventSourceUserData, GLOWKEY_TAG);
    CGEvent::post(CGEventTapLocation::SessionEventTap, Some(event));
}

/// Bundle identifier of the frontmost application, for the ignore list.
fn frontmost_bundle_id() -> Option<String> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;
    app.bundleIdentifier().map(|s| s.to_string())
}

/// The C tap callback. Skips its own tagged output and non-key events, re-enables
/// the tap if the system disables it, and routes key-downs to [`TapState`].
unsafe extern "C-unwind" fn tap_callback(
    _proxy: CGEventTapProxy,
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
        if let Some(port) = ctx.port.borrow().as_ref() {
            CGEvent::tap_enable(port, true);
        }
        return event.as_ptr();
    }

    // Ignore our own synthesized events.
    if integer_field(event, CGEventField::EventSourceUserData) == GLOWKEY_TAG {
        return event.as_ptr();
    }

    if event_type != CGEventType::KeyDown {
        return event.as_ptr();
    }

    if ctx.state.handle_key_down(event) {
        // Consumed: suppress the original event.
        std::ptr::null_mut()
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

    let mask: CGEventMask = 1 << (CGEventType::KeyDown.0 as u64);

    // The context must outlive the run loop; leak it deliberately.
    let ctx: *mut TapContext = Box::into_raw(Box::new(TapContext {
        state: TapState::new(),
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

    unsafe {
        let source = objc2_core_foundation::CFMachPort::new_run_loop_source(None, Some(&port), 0);
        if let (Some(run_loop), Some(source)) = (CFRunLoop::current(), source) {
            run_loop.add_source(Some(&source), kCFRunLoopCommonModes);
            CGEvent::tap_enable(&port, true);
            CFRunLoop::run();
        }
    }
}

/// Whether this process is trusted for Accessibility (required for the tap).
fn accessibility_trusted() -> bool {
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    unsafe { AXIsProcessTrusted() }
}
