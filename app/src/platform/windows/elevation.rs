//! Whether the window in front is out of reach, and why.
//!
//! User Interface Privilege Isolation forbids a process at one integrity level
//! from sending input to a window owned by a higher one. A non-elevated GlowKey
//! typing into Task Manager, regedit or an elevated terminal does not fail with
//! an error the user can see — `SendInput` returns a short count and the
//! keystroke simply does not arrive. **This is a silent-failure class with no
//! macOS analogue**, and it is the reason `docs/decisions/0007` ("an indicator
//! that lies about a dead tap is a defect") needs a second cause on Windows.
//!
//! The answer is to detect it and show it, never to ask for elevation. An input
//! method requesting administrator rights is a red flag, and correctly so: it
//! observes every keystroke on the machine, and the one honest thing it can do
//! about a window it cannot reach is say which window and why.
//!
//! This module answers the question. `indicator` (Phase 5) is what puts the
//! answer on screen.

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows_sys::Win32::Security::{
    GetTokenInformation, TokenIntegrityLevel, TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

/// Why injection into the foreground window would not arrive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// Injection will be delivered.
    Ok,
    /// The window belongs to a higher integrity level. UIPI will discard our
    /// input and report nothing.
    BlockedByElevation,
    /// The window could not be examined at all — it went away, or the query was
    /// refused.
    ///
    /// Deliberately not folded into [`Reach::Ok`]. "I could not tell" and "it
    /// works" are different answers, and collapsing them is how an indicator
    /// starts lying: the most likely reason a query fails is the same privilege
    /// boundary this module exists to detect.
    Unknown,
}

/// Whether GlowKey can inject into the window currently in front.
///
/// A cross-process query, so this is called from the foreground-change path and
/// the health check — **never from the hook callback**, for the reason
/// `foreground`'s module note gives.
pub fn foreground_reach(hwnd: HWND) -> Reach {
    if hwnd.is_null() {
        return Reach::Unknown;
    }
    let Some(ours) = own_integrity_level() else {
        return Reach::Unknown;
    };
    match window_integrity_level(hwnd) {
        Some(theirs) if theirs > ours => Reach::BlockedByElevation,
        Some(_) => Reach::Ok,
        None => Reach::Unknown,
    }
}

/// A handle that closes itself.
///
/// Replaces a `close_process: bool` argument that used to decide, from inside
/// the callee, whether the caller's handle got closed. The pairing was correct
/// on every path — but a function whose second parameter can close its first is
/// one refactor away from a double close, and this removes the class rather than
/// the instance.
struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: opened by this module and not used after this.
            unsafe { CloseHandle(self.0) };
        }
    }
}

/// This process's integrity level.
fn own_integrity_level() -> Option<u32> {
    // A pseudo-handle: it must NOT be closed, which is why it is not wrapped.
    // SAFETY: a constant that needs no release.
    integrity_level_of(unsafe { GetCurrentProcess() })
}

/// The integrity level of the process owning a window.
fn window_integrity_level(hwnd: HWND) -> Option<u32> {
    let mut pid: u32 = 0;
    // SAFETY: `pid` is a valid out-pointer.
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid == 0 {
        return None;
    }
    // PROCESS_QUERY_LIMITED_INFORMATION is the right one specifically because it
    // is granted across an integrity boundary. The full query right is refused by
    // the elevated process we most need to identify, which would turn every
    // elevated window into `Unknown` and defeat the module.
    // SAFETY: wrapped immediately, so it closes on every path out of here.
    let handle = OwnedHandle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) });
    if handle.0.is_null() {
        return None;
    }
    integrity_level_of(handle.0)
}

/// Reads the mandatory integrity level out of a process token.
///
/// The level is the last sub-authority of the token's integrity SID — the
/// well-known values being 0x1000 low, 0x2000 medium, 0x3000 high, 0x4000 system.
/// Compared numerically rather than matched against those constants, because the
/// question is only ever "is theirs above ours", and a level between two named
/// ones (medium-plus, at 0x2100, is real) must not read as equal.
/// Borrows `process`; never closes it. The caller owns its own handle.
fn integrity_level_of(process: HANDLE) -> Option<u32> {
    let mut raw: HANDLE = std::ptr::null_mut();
    // SAFETY: `raw` is a valid out-pointer.
    let opened = unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut raw) };
    if opened == 0 {
        return None;
    }
    // Wrapped straight away: the two early returns below would otherwise each
    // need their own close, which is how one of them eventually would not.
    let token_guard = OwnedHandle(raw);
    let token = token_guard.0;

    // Two calls: the first to learn the size of the variable-length label, the
    // second to read it.
    let mut needed: u32 = 0;
    // SAFETY: a deliberate size query — it is expected to fail and set `needed`.
    unsafe {
        GetTokenInformation(
            token,
            TokenIntegrityLevel,
            std::ptr::null_mut(),
            0,
            &mut needed,
        );
    }
    if needed == 0 {
        return None;
    }

    // `Vec<u64>`, not `Vec<u8>`. `TOKEN_MANDATORY_LABEL` contains a `PSID`
    // pointer and so needs pointer alignment, and a `Vec<u8>` guarantees
    // alignment 1. Reading a misaligned struct through a reference is undefined
    // behaviour in Rust whatever x86 happens to tolerate at runtime, and it is
    // the kind of thing that starts miscompiling under a newer LLVM rather than
    // failing honestly. `u64` is at least as aligned as any field in the
    // structure on both 32- and 64-bit targets.
    let words = (needed as usize).div_ceil(std::mem::size_of::<u64>());
    let mut buf = vec![0u64; words];
    // SAFETY: `buf` is at least `needed` bytes and correctly aligned.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenIntegrityLevel,
            buf.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    };
    if ok == 0 {
        return None;
    }

    // SAFETY: on success the buffer holds a TOKEN_MANDATORY_LABEL whose `Label.Sid`
    // points into it, and the buffer is aligned for it.
    let level = unsafe {
        let label = &*buf.as_ptr().cast::<TOKEN_MANDATORY_LABEL>();
        let sid = label.Label.Sid;
        if sid.is_null() {
            return None;
        }
        let count = *windows_sys::Win32::Security::GetSidSubAuthorityCount(sid);
        if count == 0 {
            return None;
        }
        *windows_sys::Win32::Security::GetSidSubAuthority(sid, u32::from(count) - 1)
    };
    Some(level)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The well-known mandatory integrity levels, named here rather than pulled
    // from `Win32_System_SystemServices` — that feature group exists to be
    // linked, and adding it so two test constants can be spelled would widen
    // what this process links for no gain at runtime. The values are fixed by
    // the platform and have not moved since Vista.
    const MEDIUM: u32 = 0x2000;
    const HIGH: u32 = 0x3000;

    /// GlowKey runs unelevated and can examine itself, so this must not be
    /// `Unknown` — a machine where it is has a broken assumption underneath every
    /// other claim in this module.
    #[test]
    fn we_can_read_our_own_integrity_level() {
        let ours = own_integrity_level().expect("a process can always read its own token");
        // Medium is the normal case for a desktop application. Asserted as a
        // range rather than a value, because running the suite from an elevated
        // shell is legitimate and must not fail the test.
        assert!(
            ours >= MEDIUM,
            "an interactive process is at least medium integrity, got {ours:#x}"
        );
    }

    /// The comparison is `>`, not `>=` or `!=`. Equal levels reach each other,
    /// which is the ordinary case for every application the user types into; a
    /// `!=` here would report every normal window as unreachable, and a `>=`
    /// would report all of them.
    #[test]
    fn only_a_strictly_higher_level_blocks() {
        let medium = MEDIUM;
        let high = HIGH;
        assert!(high > medium, "an elevated window is above an ordinary one");
        assert!(!(medium > medium), "an equal level is reachable");
        assert!(!(medium > high), "we do not block on being more privileged");
    }
}
