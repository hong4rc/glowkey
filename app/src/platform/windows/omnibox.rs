//! Whether the focused element is a Chromium address bar.
//!
//! # Why this exists
//!
//! A Chromium omnibox keeps a **trailing inline-autocomplete selection**: type
//! `hoo` and the field holds `hoo` plus a selected completion. The first
//! synthetic Backspace of an edit then deletes that selection instead of a
//! character, so the edit lands one short and every edit after it compounds the
//! error — `hoongf` becomes `hoồng`. A forward-delete before the edit clears the
//! selection and fixes it.
//!
//! Sending that forward-delete unconditionally is worse than the bug. In a page
//! body there is no selection, so it deletes the character to the right of the
//! caret — silently, in Gmail, Slack, VS Code and every other Electron app,
//! because `is_chromium_app` matches them all. That is why the guard was
//! disabled on 2026-09-05.
//!
//! So the guard needs a second condition: fire only when focus is actually the
//! address bar.
//!
//! # Why it is shaped like this
//!
//! **The question cannot be asked on the keystroke path.** Answering it means UI
//! Automation, which is a cross-process call that can block for as long as the
//! other process feels like taking. `docs/decisions/0008` forbids that inside the
//! hook callback, and Windows enforces it: a callback slower than
//! `LowLevelHooksTimeout` is not retried or reported — the hook is removed and
//! GlowKey goes silently dead.
//!
//! **Nor can it be asked from the WinEvent callback**, which is the trap worth
//! naming. `WINEVENT_OUTOFCONTEXT` delivers that callback on *our own thread*,
//! through the same message loop that serves the keyboard hook. Blocking there
//! blocks the hook just as surely as blocking in the hook itself.
//!
//! So a focus change only *signals* a worker thread, which does the UIA call and
//! stores one `bool`. The keystroke path reads an atomic and never calls
//! anything.
//!
//! # The failure direction is chosen
//!
//! Every focus change sets the flag **false** before the worker runs. A stale
//! answer can therefore only ever mean "guard did not fire", which costs a
//! visible mis-render in the address bar. It can never mean "guard fired in a
//! page body", which costs the user a character they cannot see go missing. That
//! asymmetry is the whole reason the flag exists, so it is enforced here rather
//! than hoped for.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::OnceLock;

use windows_sys::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED, SAFEARRAY};
use windows_sys::Win32::System::Ole::{SafeArrayDestroy, SafeArrayGetElement};
use windows_sys::Win32::System::Variant::{VariantClear, VARIANT, VT_BSTR};
use windows_sys::Win32::UI::Accessibility::{
    UiaNodeFromFocus, AutomationElementMode_Full, ConditionType_True, TreeScope_Element,
    UiaCacheRequest, UiaCondition, UIA_ClassNamePropertyId,
};

/// Chromium's own C++ class for the omnibox text field.
///
/// Chosen over the element's name ("Address and search bar") because that is
/// localized and differs between Chrome and Edge, and over its automation id
/// (`view_1021`) because that is generated and moves between releases. Captured
/// from a live Edge and a live page with `scripts/probe-omnibox-identity.ps1`:
/// the omnibox reports this, while a text field inside a page reports the web
/// page's CSS class list and Windows Terminal reports `TermControl`. Nothing in
/// a document can collide with it.
const OMNIBOX_CLASS: &str = "OmniboxViewViews";

/// Whether the element with keyboard focus is a Chromium address bar.
///
/// Read from the keystroke path, so it is a relaxed atomic load: the cost is a
/// register read, and being one focus change out of date is exactly the stale
/// case the module header accounts for.
static FOCUS_IS_OMNIBOX: AtomicBool = AtomicBool::new(false);

/// Wakes the worker. Bounded at one: a burst of focus events only needs the
/// worker to look once more, and dropping the extras is the point.
static WAKE: OnceLock<SyncSender<()>> = OnceLock::new();

/// Whether the focused element is a Chromium address bar, as of the last focus
/// change the worker has caught up with.
#[must_use]
pub fn focus_is_omnibox() -> bool {
    FOCUS_IS_OMNIBOX.load(Ordering::Relaxed)
}

/// A focus change happened. Called from the WinEvent callback.
///
/// Does two things and neither of them can block: clears the flag, so the
/// keystroke path fails safe from this instant, and pokes the worker. A full
/// channel means the worker already has a look pending, which is all this would
/// have asked for.
pub fn note_focus_changed() {
    FOCUS_IS_OMNIBOX.store(false, Ordering::Relaxed);
    if let Some(tx) = WAKE.get() {
        match tx.try_send(()) {
            Ok(()) | Err(TrySendError::Full(())) => {}
            Err(TrySendError::Disconnected(())) => {}
        }
    }
}

/// Starts the resolver thread. Called once, at startup.
///
/// Failure is not fatal: without it `focus_is_omnibox` stays false forever, the
/// guard never fires, and the address bar keeps the mis-render it has had since
/// the guard was disabled. That is the same failure direction as everything else
/// here.
pub fn start() {
    let (tx, rx) = sync_channel::<()>(1);
    if WAKE.set(tx).is_err() {
        return; // already started
    }
    let spawned = std::thread::Builder::new()
        .name("glowkey-omnibox".into())
        .spawn(move || {
            // SAFETY: initialising COM for this thread, which is required before
            // any UIA call and must happen on the thread that makes them.
            // Multithreaded apartment: nothing here owns a window or pumps
            // messages, and an STA that never pumps deadlocks cross-process
            // calls.
            unsafe { CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32) };
            while rx.recv().is_ok() {
                let is_omnibox = focused_class_is(OMNIBOX_CLASS);
                let was = FOCUS_IS_OMNIBOX.swap(is_omnibox, Ordering::Relaxed);
                // Only the edges, and only into the address bar. Focus changes
                // constantly — a line per change would be the noise the flush
                // logging was careful to avoid — but "the guard is armed" is the
                // one fact that says whether any of this works at all, and
                // without it a wrong answer looks exactly like a right one.
                if is_omnibox && !was {
                    crate::log::log("OMNIBOX focus is the address bar — guard armed");
                }
            }
        });
    match spawned {
        Ok(_) => crate::log::log("OMNIBOX resolver started"),
        Err(e) => crate::log::log(&format!(
            "OMNIBOX resolver FAILED to start ({e}) — the address-bar guard will never fire"
        )),
    }
}

/// Whether the focused element's UIA class name is `class`.
///
/// The class name is asked for **in the cache request**, so the answer arrives
/// inside the returned array and no second call is needed. That is also what
/// keeps the memory handling small enough to reason about: the node itself is
/// never extracted from the array, so it is released by `SafeArrayDestroy` along
/// with everything else, and the only thing this function owns is one copied
/// `VARIANT`.
fn focused_class_is(class: &str) -> bool {
    let mut condition = UiaCondition {
        ConditionType: ConditionType_True,
    };
    let mut property = UIA_ClassNamePropertyId;
    let mut request = UiaCacheRequest {
        pViewCondition: std::ptr::addr_of_mut!(condition),
        Scope: TreeScope_Element,
        pProperties: std::ptr::addr_of_mut!(property),
        cProperties: 1,
        pPatterns: std::ptr::null_mut(),
        cPatterns: 0,
        automationElementMode: AutomationElementMode_Full,
    };

    let mut results: *mut SAFEARRAY = std::ptr::null_mut();
    let mut tree: windows_sys::core::BSTR = std::ptr::null_mut();

    // SAFETY: `request` and its two pointees outlive the call; the two out
    // parameters are owned by us afterwards and freed on every path below.
    let hr = unsafe {
        UiaNodeFromFocus(
            std::ptr::addr_of_mut!(request),
            std::ptr::addr_of_mut!(results),
            std::ptr::addr_of_mut!(tree),
        )
    };

    let matched = if hr >= 0 && !results.is_null() {
        read_class(results).is_some_and(|name| name == class)
    } else {
        false
    };

    if !tree.is_null() {
        // SAFETY: a BSTR the call above allocated for us.
        unsafe { windows_sys::Win32::Foundation::SysFreeString(tree) };
    }
    if !results.is_null() {
        // SAFETY: the array we were given. Destroying it clears every element,
        // which releases the focused node we deliberately never took out of it.
        unsafe { SafeArrayDestroy(results) };
    }
    matched
}

/// The cached class name, from `[0][1]` of the returned array.
///
/// The layout is fixed by the API: one row per element in scope (one here, since
/// the request asks for `TreeScope_Element`), and within it the node followed by
/// each requested property in order.
fn read_class(results: *mut SAFEARRAY) -> Option<String> {
    let indices: [i32; 2] = [0, 1];
    let mut value = std::mem::MaybeUninit::<VARIANT>::zeroed();

    // SAFETY: `results` is the array UIA just handed us and `indices` addresses
    // the property slot the request asked for. `SafeArrayGetElement` copies the
    // element into `value`, so what comes back is ours to clear.
    let hr = unsafe {
        SafeArrayGetElement(
            results,
            indices.as_ptr(),
            value.as_mut_ptr().cast::<core::ffi::c_void>(),
        )
    };
    if hr < 0 {
        return None;
    }
    // SAFETY: the call above initialised it.
    let mut value = unsafe { value.assume_init() };

    // SAFETY: reading the tag of a VARIANT we own. An element that has no class
    // name comes back as something other than VT_BSTR, which is not an error.
    let text = unsafe {
        let anonymous = &value.Anonymous.Anonymous;
        if anonymous.vt == VT_BSTR {
            let bstr = anonymous.Anonymous.bstrVal;
            if bstr.is_null() {
                None
            } else {
                Some(bstr_to_string(bstr))
            }
        } else {
            None
        }
    };

    // SAFETY: our copy, freed exactly once.
    unsafe { VariantClear(std::ptr::addr_of_mut!(value)) };
    text
}

/// A BSTR as a `String`. Its length prefix is authoritative — a BSTR may contain
/// interior NULs — so this reads the prefix rather than scanning for a
/// terminator.
///
/// # Safety
///
/// `bstr` must be a valid, non-null BSTR.
unsafe fn bstr_to_string(bstr: windows_sys::core::BSTR) -> String {
    let len = unsafe { windows_sys::Win32::Foundation::SysStringLen(bstr) } as usize;
    let slice = unsafe { std::slice::from_raw_parts(bstr, len) };
    String::from_utf16_lossy(slice)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The flag starts false and a focus change returns it to false.
    ///
    /// This is the safety property, not a formality: a stale `true` is the one
    /// state that can delete a character the user typed, so focus changing must
    /// clear it before anything else has a chance to read it.
    #[test]
    fn a_focus_change_fails_safe() {
        FOCUS_IS_OMNIBOX.store(true, Ordering::Relaxed);
        note_focus_changed();
        assert!(
            !focus_is_omnibox(),
            "a focus change must clear the flag before the worker re-resolves it"
        );
    }

    /// The class name is the one the probe captured from a live browser.
    ///
    /// Pinned as a literal because it is the entire discriminator, and because
    /// the alternatives that look equally good — the localized name, the
    /// generated automation id — are the ones a future edit would reach for.
    #[test]
    fn the_omnibox_class_is_the_chromium_view_name() {
        assert_eq!(OMNIBOX_CLASS, "OmniboxViewViews");
    }
}
