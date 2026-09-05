//! Whether Windows is in its light or dark theme.
//!
//! Asked of the registry rather than of the UI toolkit. `winit`'s theme
//! detection does not resolve reliably here, and egui's fallback when it cannot
//! tell is **dark** — so a settings window on a light system came up black, with
//! a black Done button, on a machine whose apps are set to light. Guessing wrong
//! in the dark direction is the worst of the two, because a light system is the
//! Windows default.
//!
//! # Two settings, not one
//!
//! Windows keeps these independently and users really do mix them — a dark
//! taskbar with light apps is a common combination:
//!
//! - `SystemUsesLightTheme` — the taskbar, the tray, system chrome. What the
//!   tray glyph has to contrast against.
//! - `AppsUseLightTheme` — application windows. What the settings window should
//!   match.
//!
//! Using one for both is how the tray icon ends up invisible on a taskbar whose
//! theme nobody checked.

use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, REG_VALUE_TYPE,
};

/// Where Windows keeps both values.
const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

/// Whether the **taskbar** is light. For the tray glyph.
#[must_use]
pub fn taskbar_is_light() -> bool {
    light_theme_value("SystemUsesLightTheme")
}

/// Whether **application windows** are light. For the settings window.
#[must_use]
pub fn apps_are_light() -> bool {
    light_theme_value("AppsUseLightTheme")
}

/// Reads one of the two theme flags.
///
/// Read fresh rather than cached: the user can change the theme while GlowKey is
/// running, and a window or an icon that only suits the theme it started under is
/// as wrong as one that never matched. It is a registry read on a repaint, never
/// on a keystroke.
///
/// Absent means light. The value is missing only on a system where the setting
/// has never been touched, and light is the Windows default — so the absent case
/// and the default case agree, which is what makes the fallback safe rather than
/// arbitrary.
fn light_theme_value(name: &str) -> bool {
    let path = wide(PERSONALIZE);
    let name = wide(name);
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: `path` is NUL-terminated and `key` is a valid out-pointer.
    let opened = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_READ, &mut key) };
    if opened != 0 {
        return true;
    }

    let mut value: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let mut kind: REG_VALUE_TYPE = 0;
    // SAFETY: `value`/`size` describe a matched buffer for the REG_DWORD this is.
    let read = unsafe {
        RegQueryValueExW(
            key,
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut kind,
            std::ptr::addr_of_mut!(value).cast(),
            &mut size,
        )
    };
    // SAFETY: opened above, not used after this.
    unsafe { RegCloseKey(key) };

    if read != 0 {
        return true;
    }
    value == 1
}

/// A NUL-terminated UTF-16 string, which is what every `…W` entry point takes.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both reads must be stable and must not panic, whatever the machine's
    /// registry looks like.
    #[test]
    fn the_theme_reads_are_stable() {
        assert_eq!(taskbar_is_light(), taskbar_is_light());
        assert_eq!(apps_are_light(), apps_are_light());
    }

    /// A missing value reads as light rather than dark.
    ///
    /// The direction matters: light is the Windows default, so an absent value
    /// and an untouched system agree. Falling back to dark is what produced a
    /// black settings window on a light machine.
    #[test]
    fn an_absent_value_reads_as_light() {
        assert!(
            light_theme_value("GlowKeyNoSuchValue"),
            "an unreadable theme flag must fall back to light"
        );
    }
}
