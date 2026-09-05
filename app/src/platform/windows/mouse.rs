//! The mouse hook, which exists for exactly one reason: a click moves the caret.
//!
//! GlowKey is blind. Its one invariant is that what the engine believes it
//! rendered is the text tail at the caret, and nothing verifies that — it holds
//! only because the session is flushed on every event that could move the caret.
//! Arrow keys, Home, End and Page keys reach the ladder as `Key::CaretMove` and
//! flush there. **A mouse click does not go through the keyboard hook at all.**
//!
//! So without this module: type `hoong`, click somewhere else, type `f` — and the
//! engine emits three backspaces against text it no longer has any relationship
//! with, deleting three characters the user typed themselves. That is the failure
//! this project fears most, and it is not a subtle one to reproduce.
//!
//! The macOS tap has always watched `LeftMouseDown`/`RightMouseDown` for this.
//! The Windows port did not, which was an omission rather than a decision.
//!
//! # The rule still applies
//!
//! This callback is on the same footing as the keyboard one: a low-level mouse
//! hook is called for every mouse event on the machine, and a slow one is removed
//! by `LowLevelHooksTimeout` exactly the same way. It does the least possible
//! work — a button test, then a flush that touches only in-memory session state —
//! and never logs, queries or allocates.

use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, WH_MOUSE_LL, WM_LBUTTONDOWN,
    WM_MBUTTONDOWN, WM_NCLBUTTONDOWN, WM_NCMBUTTONDOWN, WM_NCRBUTTONDOWN, WM_RBUTTONDOWN,
};

/// The installed hook, so it can be removed on the way out.
static HOOK: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Installs the mouse hook on the calling thread.
///
/// Must be the same thread that runs the message loop, for the same reason the
/// keyboard hook must be: a low-level hook is delivered through that thread's
/// message queue.
///
/// Returns whether it installed. A failure is not fatal — GlowKey still types
/// correctly, it just stops being safe to click mid-word — so the caller logs it
/// and carries on rather than refusing to start.
pub fn install() -> bool {
    // SAFETY: the callback matches HOOKPROC and lives for the program. Same
    // module-handle requirement as the keyboard hook: a null `hmod` is accepted
    // and then never calls back.
    let module =
        unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null()) };
    // SAFETY: a global hook (zero thread id), which is what "any window the user
    // clicks in" requires.
    let hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_callback), module, 0) };
    if hook.is_null() {
        crate::log::log("MOUSE FAILED to install WH_MOUSE_LL — clicking mid-word will not flush");
        return false;
    }
    HOOK.store(hook as isize, std::sync::atomic::Ordering::Relaxed);
    crate::log::log("MOUSE installed WH_MOUSE_LL");
    true
}

/// Removes the hook. Idempotent.
pub fn uninstall() {
    let raw = HOOK.swap(0, std::sync::atomic::Ordering::Relaxed);
    if raw != 0 {
        // SAFETY: a handle this module installed and has not yet removed.
        unsafe { UnhookWindowsHookEx(raw as HHOOK) };
    }
}

/// The callback. Never consumes an event — a mouse hook that swallowed a click
/// would be a far worse bug than the one it is here to prevent.
unsafe extern "system" fn mouse_callback(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // HC_ACTION is 0. Anything else passes through unexamined.
    if code == 0 && is_button_down(wparam as u32) {
        // Wrapped, because a panic must not unwind into Win32's C frames.
        let _ = std::panic::catch_unwind(super::hook::flush_session);
    }
    // SAFETY: the documented chaining call. Always chained: this hook observes,
    // it never decides.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

/// Whether this message is a button going down.
///
/// Button *down* rather than up: the caret moves on the press, and the next
/// keystroke can arrive before the release. Non-client variants included — a
/// click on a title bar or scrollbar changes focus just as effectively.
///
/// Movement and wheel are deliberately not here. They are by far the most
/// frequent mouse messages and neither moves a text caret, so treating them as
/// flushes would mean flushing constantly — every stray cursor movement over the
/// window would destroy a composition mid-word.
const fn is_button_down(message: u32) -> bool {
    matches!(
        message,
        WM_LBUTTONDOWN
            | WM_RBUTTONDOWN
            | WM_MBUTTONDOWN
            | WM_NCLBUTTONDOWN
            | WM_NCRBUTTONDOWN
            | WM_NCMBUTTONDOWN
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONUP,
    };

    /// Every button press flushes, including the non-client ones: a click on a
    /// title bar or a scrollbar moves focus exactly as well as one in the text.
    #[test]
    fn every_button_press_flushes() {
        for message in [
            WM_LBUTTONDOWN,
            WM_RBUTTONDOWN,
            WM_MBUTTONDOWN,
            WM_NCLBUTTONDOWN,
            WM_NCRBUTTONDOWN,
            WM_NCMBUTTONDOWN,
        ] {
            assert!(is_button_down(message), "{message:#x} must flush");
        }
    }

    /// Movement and the wheel must not.
    ///
    /// They are the overwhelming majority of mouse messages, and flushing on them
    /// would destroy a composition every time the pointer drifted across the
    /// window — which is worse than the bug this file fixes, and constant.
    #[test]
    fn movement_and_the_wheel_do_not_flush() {
        for message in [WM_MOUSEMOVE, WM_MOUSEWHEEL] {
            assert!(
                !is_button_down(message),
                "{message:#x} does not move a text caret and must not flush"
            );
        }
    }

    /// Button *up* does not flush either — the down already did, and flushing
    /// twice for one click is wasted work on a hot path.
    #[test]
    fn button_release_does_not_flush_again() {
        for message in [WM_LBUTTONUP, WM_RBUTTONUP] {
            assert!(!is_button_down(message));
        }
    }
}
