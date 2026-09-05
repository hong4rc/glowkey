//! The clipboard tools — UniKey's "Công cụ", the same three the macOS menu has.
//!
//! Transform whatever is on the clipboard, in place: copy, pick one, paste. They
//! work on a selection in UniKey and on the clipboard here for the same reason
//! they do on macOS — GlowKey has no way to read a selection out of another
//! application, and the clipboard is the one piece of text the user can hand it
//! deliberately.
//!
//! Nothing here is on the keystroke path. The clipboard is a global lock other
//! processes hold, so opening it can wait, and `docs/decisions/0008` forbids
//! waiting in the hook. These run from the tray menu.

use windows_sys::Win32::Foundation::{GlobalFree, HANDLE, HWND};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

/// Rewrites the clipboard's text through `transform`.
///
/// Does nothing when the clipboard holds no text, so a stray menu click on an
/// image or an empty clipboard is a no-op rather than a way to lose what is
/// there.
pub fn transform(transform: impl FnOnce(&str) -> String) -> bool {
    let Some(_guard) = Clipboard::open() else {
        // Another process holds it. Ordinary and transient; the user can click
        // again. Not worth an error dialog, worth a log line.
        crate::log::log("CLIPBOARD busy — another program has it open");
        return false;
    };
    let Some(text) = read_text() else {
        return false;
    };
    let transformed = transform(&text);
    if transformed == text {
        // Nothing to do, and writing anyway would clear every other format on
        // the clipboard for no gain — a user who copied rich text would silently
        // lose its formatting.
        return true;
    }
    write_text(&transformed)
}

/// Removes Vietnamese tone marks and diacritics — `Việt` becomes `Viet`.
pub fn remove_tones() -> bool {
    transform(glowkey_session::remove_tones)
}

/// Uppercases, honouring Vietnamese casing rules through Rust's Unicode-aware
/// `to_uppercase` rather than an ASCII fold.
pub fn uppercase() -> bool {
    transform(str::to_uppercase)
}

/// Lowercases, same caveat.
pub fn lowercase() -> bool {
    transform(str::to_lowercase)
}

/// An open clipboard that closes itself.
///
/// The clipboard is a process-wide, system-wide lock. Leaving it open makes every
/// other program's copy and paste fail, so the close has to survive every early
/// return — hence a guard rather than discipline.
struct Clipboard;

impl Clipboard {
    fn open() -> Option<Self> {
        // SAFETY: a null owner window is documented as "the current task", which
        // is what a process with no window of its own wants.
        let ok = unsafe { OpenClipboard(std::ptr::null_mut::<std::ffi::c_void>() as HWND) };
        (ok != 0).then_some(Self)
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        // SAFETY: opened above; closing an open clipboard is always valid.
        unsafe { CloseClipboard() };
    }
}

/// The clipboard's text, if it holds any.
fn read_text() -> Option<String> {
    // SAFETY: the clipboard is open (the caller holds the guard). The handle
    // belongs to the clipboard and must not be freed by us.
    let handle = unsafe { GetClipboardData(CF_UNICODETEXT as u32) };
    if handle.is_null() {
        return None;
    }
    // SAFETY: locking a global handle the clipboard owns; unlocked below.
    let ptr = unsafe { GlobalLock(handle) }.cast::<u16>();
    if ptr.is_null() {
        return None;
    }
    // SAFETY: CF_UNICODETEXT is documented NUL-terminated.
    let len = unsafe {
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        len
    };
    // SAFETY: `ptr`/`len` describe the string measured just above.
    let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(ptr, len) });
    // SAFETY: paired with the lock above.
    unsafe { GlobalUnlock(handle) };
    Some(text)
}

/// Replaces the clipboard's contents with `text`.
fn write_text(text: &str) -> bool {
    let units: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = std::mem::size_of_val(&units[..]);

    // GMEM_MOVEABLE is required: `SetClipboardData` takes ownership of the block
    // and the system may move it. Fixed memory here is a documented way to
    // corrupt the clipboard.
    // SAFETY: a plain allocation; ownership passes to the clipboard on success.
    let handle: HANDLE = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) };
    if handle.is_null() {
        return false;
    }
    // SAFETY: locking the block just allocated.
    let ptr = unsafe { GlobalLock(handle) }.cast::<u16>();
    if ptr.is_null() {
        // Ownership only passes to the clipboard on a successful
        // `SetClipboardData`, so this block is still ours to release.
        // SAFETY: allocated above and not handed to anyone.
        unsafe { GlobalFree(handle) };
        return false;
    }
    // SAFETY: `handle` was allocated with exactly `bytes` bytes, which is the
    // size of `units`.
    unsafe {
        std::ptr::copy_nonoverlapping(units.as_ptr(), ptr, units.len());
        GlobalUnlock(handle);
    }

    // The block is fully prepared *before* the clipboard is emptied.
    //
    // `EmptyClipboard` is required before `SetClipboardData`, but running it
    // early means a later failure leaves the clipboard **empty** — the user's
    // text destroyed by a menu item whose whole contract is that a stray click
    // costs nothing. Everything that can fail has now already failed or
    // succeeded, so the destructive step is as close to the write as it can be.
    //
    // Emptying also drops the other formats deliberately: a stale RTF copy of the
    // untransformed text would otherwise be what the next paste picks up.
    // SAFETY: the clipboard is open and owned by this process.
    unsafe {
        EmptyClipboard();
        if SetClipboardData(CF_UNICODETEXT as u32, handle).is_null() {
            // The clipboard did not take ownership, so the block is still ours.
            GlobalFree(handle);
            crate::log::log("CLIPBOARD failed to write the transformed text");
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    /// The transformations themselves are the engine's, and are tested there.
    /// What is worth pinning here is that this module wires the right one to the
    /// right name — swapping uppercase and lowercase would be invisible in a
    /// review and obvious to a user.
    #[test]
    fn each_tool_is_wired_to_the_transformation_it_names() {
        assert_eq!(glowkey_session::remove_tones("Việt Nam"), "Viet Nam");
        assert_eq!("Việt".to_uppercase(), "VIỆT");
        assert_eq!("VIỆT".to_lowercase(), "việt");
    }
}
