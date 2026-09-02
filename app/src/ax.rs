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
        ptr::null_mut()
    }
}

/// Whether the focused UI element currently has a non-empty text selection.
/// Returns `false` on any failure (no AX access, no focused element, attribute
/// unsupported) — failure must never change GlowKey's behavior.
pub fn focused_has_selection() -> bool {
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return false;
        }
        AXUIElementSetMessagingTimeout(system, AX_TIMEOUT_SECONDS);
        let focused = copy_attribute(system, "AXFocusedUIElement");
        CFRelease(system);
        if focused.is_null() {
            return false;
        }
        let selected = copy_attribute(focused, "AXSelectedText");
        CFRelease(focused);
        if selected.is_null() {
            return false;
        }
        let non_empty = CFGetTypeID(selected) == CFStringGetTypeID()
            && CFStringGetLength(selected.cast_const()) > 0;
        CFRelease(selected);
        non_empty
    }
}
