//! Where GlowKey's files live, asked of the system rather than interpolated.
//!
//! `%APPDATA%` is an environment variable and can be absent, stale, or pointing
//! somewhere else entirely — a redirected profile, a service context, a process
//! launched with a scrubbed environment. `SHGetKnownFolderPath` is the system's
//! own answer to the same question and is right in all of those cases.

use windows_sys::Win32::Foundation::S_OK;
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_RoamingAppData, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
};

use std::path::PathBuf;

/// `%APPDATA%\GlowKey` — settings, which roam with the user's profile.
pub fn settings_dir() -> Option<PathBuf> {
    known_folder(&FOLDERID_RoamingAppData).map(|mut p| {
        p.push("GlowKey");
        p
    })
}

/// `%LOCALAPPDATA%\GlowKey\Logs` — the log, which does not roam.
///
/// Local rather than roaming on purpose: the log records the text you type (that
/// is what it is for), it is bounded but not small, and copying it between
/// machines on a domain profile is neither useful nor something a user would
/// expect an input method to do.
pub fn log_dir() -> Option<PathBuf> {
    known_folder(&FOLDERID_LocalAppData).map(|mut p| {
        p.push("GlowKey");
        p.push("Logs");
        p
    })
}

/// One known folder, as a path.
fn known_folder(id: &windows_sys::core::GUID) -> Option<PathBuf> {
    let mut raw: *mut u16 = std::ptr::null_mut();
    // SAFETY: `id` is a valid known-folder GUID; `raw` receives an owned string
    // that is freed below on every path.
    let hr =
        unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT as u32, std::ptr::null_mut(), &mut raw) };
    if hr != S_OK || raw.is_null() {
        return None;
    }
    // SAFETY: a NUL-terminated wide string owned by the shell allocator.
    let len = unsafe {
        let mut len = 0;
        while *raw.add(len) != 0 {
            len += 1;
        }
        len
    };
    let path = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(raw, len) });
    // SAFETY: allocated by SHGetKnownFolderPath, which requires this free.
    unsafe { CoTaskMemFree(raw.cast()) };
    Some(PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both folders resolve on any Windows machine, and they are not the same
    /// place — settings roam, the log does not.
    #[test]
    fn the_known_folders_resolve_and_differ() {
        let settings = settings_dir().expect("RoamingAppData always resolves");
        let logs = log_dir().expect("LocalAppData always resolves");
        assert!(settings.is_absolute());
        assert!(logs.is_absolute());
        assert_ne!(settings, logs);
        assert!(logs.ends_with("Logs"));
    }
}
