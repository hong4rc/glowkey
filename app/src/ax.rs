//! Minimal Accessibility (AX) read-back for the Chromium omnibox fix.
//!
//! The omnibox's inline autocomplete keeps a **trailing selection** after each
//! keystroke, so the first synthetic Backspace deletes that selection instead of a
//! character (`hoongf`→`hoồng`). This module answers one question — "does the
//! focused element have a non-empty text selection right now?" — so the tap can
//! clear it (one forward-delete) before emitting backspaces. In a normal field the
//! selection is empty and nothing changes, which is what makes the fix safe.
//!
//! Deliberately narrow: read-only, called only when an edit with backspaces is
//! about to be emitted into a Chromium browser, with a short messaging timeout so
//! a stalled AX server cannot add typing latency. Any failure reads as "no
//! selection", i.e. no behavior change. This does NOT contradict GlowKey's blind
//! model (no host-text read-back for composing) — it never reads text content,
//! only whether a selection exists.

use std::ffi::c_void;
use std::ptr;

use objc2_core_foundation::CFString;

/// AXError success (`kAXErrorSuccess`).
const AX_SUCCESS: i32 = 0;

/// How long to wait for the focused app's AX server before giving up. Keystroke
/// latency budget: failure is just "no selection".
const AX_TIMEOUT_SECONDS: f32 = 0.05;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> *mut c_void;
    fn AXUIElementCopyAttributeValue(
        element: *mut c_void,
        attribute: *const c_void, // CFStringRef
        value: *mut *mut c_void,
    ) -> i32;
    fn AXUIElementSetMessagingTimeout(element: *mut c_void, timeout_in_seconds: f32) -> i32;
    fn CFGetTypeID(cf: *const c_void) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFStringGetLength(string: *const c_void) -> isize;
    fn CFRelease(cf: *const c_void);
}

extern "C" {
    fn CFEqual(a: *const c_void, b: *const c_void) -> bool;
}

/// The process-wide system AX element, created once. The messaging timeout set on
/// it is the process-global default, which copied elements without their own
/// timeout inherit — so one call bounds every query this module makes. Stored as
/// a raw address (main-thread use only; never released — process lifetime).
fn system_element() -> *mut c_void {
    use std::sync::OnceLock;
    static SYSTEM: OnceLock<usize> = OnceLock::new();
    *SYSTEM.get_or_init(|| unsafe {
        let system = AXUIElementCreateSystemWide();
        if !system.is_null() {
            AXUIElementSetMessagingTimeout(system, AX_TIMEOUT_SECONDS);
        }
        system as usize
    }) as *mut c_void
}

/// Logs an AX failure once per process, so a silently-failing guard leaves a
/// diagnostic trail (the guard itself must stay silent-per-keystroke).
fn log_first_failure(context: &str, err: i32) {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        crate::log::log(&format!(
            "AX guard unavailable: {context} (AXError {err}) — omnibox guard inactive"
        ));
    });
}

/// Copies one AX attribute from `element`, returning an owned CF object pointer
/// (the caller must `CFRelease` it) or null.
unsafe fn copy_attribute(element: *mut c_void, name: &str) -> *mut c_void {
    let attribute = CFString::from_str(name);
    let mut value: *mut c_void = ptr::null_mut();
    let err = AXUIElementCopyAttributeValue(
        element,
        (&*attribute as *const CFString).cast::<c_void>(),
        &mut value,
    );
    if err == AX_SUCCESS {
        value
    } else {
        if name == "AXFocusedUIElement" {
            log_first_failure(name, err);
        }
        ptr::null_mut()
    }
}

/// Whether `element` (borrowed) is a plain text field (`AXRole == AXTextField`) —
/// the omnibox's role. Scopes the guard away from web content, contenteditable
/// surfaces, and custom AX implementations, where a reported selection may not be
/// the trailing-autocomplete pattern and a forward-delete could eat real text.
unsafe fn is_text_field(element: *mut c_void) -> bool {
    let role = copy_attribute(element, "AXRole");
    if role.is_null() {
        return false;
    }
    let expected = CFString::from_str("AXTextField");
    let matches = CFGetTypeID(role) == CFStringGetTypeID()
        && CFEqual(
            role.cast_const(),
            (&*expected as *const CFString).cast::<c_void>(),
        );
    CFRelease(role);
    matches
}

/// Whether the focused UI element is a plain text field with a non-empty text
/// selection — the Chromium omnibox inline-autocomplete signature. Returns
/// `false` on any failure (no AX access, no focused element, attribute
/// unsupported) — failure must never change GlowKey's behavior.
pub fn focused_text_field_has_selection() -> bool {
    unsafe {
        let system = system_element();
        if system.is_null() {
            return false;
        }
        let focused = copy_attribute(system, "AXFocusedUIElement");
        if focused.is_null() {
            return false;
        }
        let selected = copy_attribute(focused, "AXSelectedText");
        let non_empty = !selected.is_null()
            && CFGetTypeID(selected) == CFStringGetTypeID()
            && CFStringGetLength(selected.cast_const()) > 0;
        if !selected.is_null() {
            CFRelease(selected);
        }
        // Role checked only when a selection exists (the rare case), keeping the
        // common path at two IPC round-trips.
        let applies = non_empty && is_text_field(focused);
        CFRelease(focused);
        applies
    }
}
