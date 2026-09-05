//! One GlowKey at a time.
//!
//! Two running at once is not a cosmetic problem. Each installs its own
//! `WH_KEYBOARD_LL` hook, each keeps its own engine session, each injects — and
//! because both stamp the *same* `dwExtraInfo` tag, each treats the other's
//! injection as its own and passes it through. So they do not fight in a way that
//! announces itself. They produce two tray icons, two sets of log lines
//! interleaved in one file with two independent sequence counters, and behaviour
//! that is a function of which hook the system happened to call first.
//!
//! That was observed on a real machine: a log with `#361, #383, #363, #384…`
//! running through it, and typing that could not be reasoned about from it.
//! Diagnosing anything with two instances up is guesswork, which is why this
//! exists — a wrong answer that is reproducible is worth more than a right one
//! that is not.
//!
//! A named mutex rather than a lock file: the kernel releases it when the process
//! ends however it ends, including a crash or a `taskkill`. A lock file has to be
//! cleaned up by the very process that just died.

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, HANDLE};
use windows_sys::Win32::System::Threading::CreateMutexW;

/// The mutex name.
///
/// `Local\` rather than `Global\`: the scope that matters is one desktop session.
/// Two users logged in at once each get their own GlowKey, which is correct —
/// they have their own keyboards, their own settings file and their own tray.
/// `Global\` would let the first user to log in silently prevent the second from
/// running at all.
const MUTEX_NAME: &str = r"Local\GlowKey-SingleInstance-8f3a1c";

/// Holds the claim for the life of the process.
pub struct InstanceGuard(HANDLE);

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: a handle this module created and has not closed.
            unsafe { CloseHandle(self.0) };
        }
    }
}

/// Claims the single-instance slot.
///
/// `Some` means this process is the one GlowKey. `None` means another is already
/// running and this one should exit — quietly, because a user who double-clicks
/// the icon twice has not done anything wrong and does not need an error.
#[must_use]
pub fn claim() -> Option<InstanceGuard> {
    let name: Vec<u16> = MUTEX_NAME
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `name` is NUL-terminated and outlives the call. A default security
    // descriptor is what we want — this is per-session and per-user.
    let handle = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
    if handle.is_null() {
        // The claim could not be made at all. Allowed to run rather than refused:
        // failing to start because a *mutex* could not be created would be a worse
        // bug than the one this prevents.
        crate::log::log("STARTUP could not create the single-instance mutex — continuing");
        return Some(InstanceGuard(std::ptr::null_mut()));
    }

    // SAFETY: a plain read of the calling thread's last error, which
    // `CreateMutexW` sets to `ERROR_ALREADY_EXISTS` when the mutex was already
    // there. The handle is still valid and still ours to close either way.
    let existed = unsafe { windows_sys::Win32::Foundation::GetLastError() } == ERROR_ALREADY_EXISTS;
    if existed {
        // SAFETY: created above; released because we are about to give up.
        unsafe { CloseHandle(handle) };
        return None;
    }
    Some(InstanceGuard(handle))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first claim succeeds and the second, while the first is held, does not.
    ///
    /// This is the whole contract, and it is checkable in-process because the
    /// mutex is per-session rather than per-process.
    #[test]
    fn a_second_claim_is_refused_while_the_first_is_held() {
        let first = claim().expect("nothing else holds it in a test run");
        assert!(
            claim().is_none(),
            "a second instance must not be allowed to start"
        );
        drop(first);
    }

    /// Dropping the guard releases the slot, so a restart works.
    ///
    /// Without this a crash would lock the user out of their own input method
    /// until they logged out — which is the failure mode a lock *file* has and
    /// this is chosen to avoid.
    #[test]
    fn releasing_the_guard_frees_the_slot() {
        let first = claim().expect("free at the start of this test");
        drop(first);
        let second = claim().expect("the slot must be free again after a drop");
        drop(second);
    }
}
