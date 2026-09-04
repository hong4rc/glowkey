//! Launch at login: one value under `HKCU\...\Run`.
//!
//! The analogue of `SMAppService` on macOS, and it carries the same obligation:
//! **disabling it must remove the entry, not merely stop honouring it.** A
//! background process that leaves a registry value behind after the user turns it
//! off is indistinguishable, to anyone auditing their own machine, from one that
//! is lying about it — and an input method is exactly the kind of program people
//! audit.
//!
//! `HKCU`, never `HKLM`: per-user, no administrator rights, and nothing to clean
//! up if the user deletes their profile. A `Run` entry also does not start
//! elevated, which is correct and permanent — see `docs/decisions/0009` on why
//! GlowKey does not want elevation.

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_SZ,
};

/// The key every user-level startup entry lives under.
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Our value name. Stable: renaming it would strand the old entry, which is the
/// "left something behind" failure this module exists to avoid.
const VALUE_NAME: &str = "GlowKey";

/// Whether GlowKey is registered to start at login.
#[must_use]
pub fn is_enabled() -> bool {
    let Some(key) = open(KEY_READ) else {
        return false;
    };
    let name = wide(VALUE_NAME);
    let mut size: u32 = 0;
    // SAFETY: a size query — null buffer with a valid size out-pointer is the
    // documented way to ask whether a value exists and how big it is.
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        )
    };
    status == ERROR_SUCCESS
}

/// Adds or removes the startup entry.
///
/// Returns whether the registry now matches what was asked for, so a settings
/// checkbox can reflect reality instead of intent. A checkbox that stays ticked
/// after the write failed is the same class of lie as an indicator over a dead
/// hook.
pub fn set_enabled(enabled: bool) -> bool {
    let Some(key) = open(KEY_READ | KEY_WRITE) else {
        return false;
    };
    let name = wide(VALUE_NAME);

    if !enabled {
        // SAFETY: deleting a value we own by name.
        let status = unsafe { RegDeleteValueW(key.0, name.as_ptr()) };
        // Already absent is success: the caller asked for "not registered", and
        // it is not registered.
        let ok = status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND;
        crate::log::log(&format!("STARTUP disabled (ok={ok})"));
        return ok;
    }

    let Some(command) = launch_command() else {
        crate::log::log("STARTUP cannot register — the executable path is unknown");
        return false;
    };
    let value = wide(&command);
    // Byte length including the terminating NUL, which `RegSetValueExW` wants for
    // REG_SZ — omitting it leaves a value other tools read as unterminated.
    let bytes = std::mem::size_of_val(&value[..]) as u32;
    // SAFETY: `value` is a NUL-terminated wide string and `bytes` is its length.
    let status = unsafe {
        RegSetValueExW(
            key.0,
            name.as_ptr(),
            0,
            REG_SZ,
            value.as_ptr().cast(),
            bytes,
        )
    };
    let ok = status == ERROR_SUCCESS;
    crate::log::log(&format!("STARTUP enabled (ok={ok})"));
    ok
}

/// The command line the `Run` entry stores.
///
/// Quoted, because the path routinely contains spaces (`C:\Program Files\…`) and
/// an unquoted `Run` value with a space in it is both broken and a known
/// unquoted-service-path hazard.
fn launch_command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\"", exe.display()))
}

/// An open registry key that closes itself.
struct OpenKey(HKEY);

impl Drop for OpenKey {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: opened by `open` and not used after this.
            unsafe { RegCloseKey(self.0) };
        }
    }
}

fn open(access: u32) -> Option<OpenKey> {
    let path = wide(RUN_KEY);
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: `path` is NUL-terminated and `key` is a valid out-pointer.
    let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, access, &mut key) };
    (status == ERROR_SUCCESS).then_some(OpenKey(key))
}

/// A NUL-terminated UTF-16 string, which is what every `…W` entry point takes.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_strings_are_nul_terminated() {
        let w = wide("GlowKey");
        assert_eq!(w.last(), Some(&0), "a W entry point reads until the NUL");
        assert_eq!(w.len(), "GlowKey".len() + 1);
    }

    /// The path is quoted. An unquoted `Run` value breaks the moment GlowKey is
    /// installed under `C:\Program Files\`, which is where it will be.
    #[test]
    fn the_launch_command_is_quoted() {
        let command = launch_command().expect("the test binary has a path");
        assert!(command.starts_with('"') && command.ends_with('"'));
        assert!(command.len() > 2);
    }

    /// Reading the current state must not panic or alter it, whatever the
    /// machine's registry looks like.
    #[test]
    fn reading_the_state_is_safe_and_repeatable() {
        let first = is_enabled();
        assert_eq!(first, is_enabled(), "a read must not change what it reads");
    }
}
