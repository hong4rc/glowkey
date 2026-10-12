//! Telling an installed web app apart from the browser that hosts it.
//!
//! An app installed from Edge or Chrome ("Install this site as an app") runs
//! inside the browser's own process: same `msedge.exe`, same process id as the
//! browser window next to it. The executable name cannot separate them, so a
//! setting made for one lands on the other.
//!
//! What does differ is the window's Application User Model ID, which Windows
//! keeps in the window's property store. Measured on a running Edge: the browser
//! window reports `MSEdge`, an installed app reports something like
//! `monkeytype.com-D841B8E7_c12ap3q39px74!App`, and the value is the same every
//! time the app is reopened. That makes it a stable identity for the app.
//!
//! Only used to *refine* an identity that is already a Chromium browser. The
//! executable name stays the identity of everything else, for the reason
//! `foreground::executable_name` gives.

use std::cell::Cell;
use std::ffi::c_void;

use windows_sys::core::{GUID, HRESULT};
use windows_sys::Win32::Foundation::{HWND, PROPERTYKEY};
use windows_sys::Win32::System::Com::StructuredStorage::{PropVariantClear, PROPVARIANT};
use windows_sys::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows_sys::Win32::UI::Shell::PropertiesSystem::SHGetPropertyStoreForWindow;

/// `IID_IPropertyStore`.
const IID_PROPERTY_STORE: GUID = GUID::from_u128(0x886d8eeb_8cf2_4446_8d02_cdba1dbdcf99);

/// `PKEY_AppUserModel_ID`: format id `9F4C2855-…`, property 5.
const PKEY_APP_USER_MODEL_ID: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
    pid: 5,
};

/// `VT_LPWSTR`.
const VT_LPWSTR: u16 = 31;

/// The part of `IPropertyStore` this needs. `windows-sys` carries no COM
/// interfaces, so the vtable is spelled out; only `GetValue` and `Release` are
/// called, the rest are placeholders that keep their slots.
#[repr(C)]
struct PropertyStoreVtbl {
    query_interface: usize,
    add_ref: usize,
    release: unsafe extern "system" fn(*mut PropertyStore) -> u32,
    get_count: usize,
    get_at: usize,
    get_value: unsafe extern "system" fn(
        *mut PropertyStore,
        *const PROPERTYKEY,
        *mut PROPVARIANT,
    ) -> HRESULT,
    set_value: usize,
    commit: usize,
}

#[repr(C)]
struct PropertyStore {
    vtbl: *const PropertyStoreVtbl,
}

thread_local! {
    /// COM is initialised once per calling thread. The property-store call
    /// marshals through the shell, which needs an apartment; the caller is the
    /// message-loop thread, which pumps messages, so a single-threaded one fits.
    static COM_READY: Cell<bool> = const { Cell::new(false) };
}

/// The window's Application User Model ID, if it has one.
///
/// A cross-process query: call it from the foreground notification path, never
/// from the keyboard hook callback.
#[must_use]
pub fn app_user_model_id(hwnd: HWND) -> Option<String> {
    COM_READY.with(|ready| {
        if !ready.get() {
            // SAFETY: initialising COM for this thread. A failure because the
            // thread already chose another model is harmless: COM is usable.
            unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
            ready.set(true);
        }
    });

    let mut store: *mut c_void = std::ptr::null_mut();
    // SAFETY: a valid out-pointer; on success it owns one reference, released
    // below on every path.
    let hr = unsafe { SHGetPropertyStoreForWindow(hwnd, &IID_PROPERTY_STORE, &mut store) };
    if hr < 0 || store.is_null() {
        return None;
    }
    let store = store.cast::<PropertyStore>();

    let mut value = PROPVARIANT::default();
    // SAFETY: `store` is a live IPropertyStore with the vtable layout above;
    // `value` is zeroed and cleared after use.
    let id = unsafe {
        let vtbl = &*(*store).vtbl;
        let got = (vtbl.get_value)(store, &PKEY_APP_USER_MODEL_ID, &mut value) >= 0;
        let inner = value.Anonymous.Anonymous;
        let id = (got && inner.vt == VT_LPWSTR && !inner.Anonymous.pwszVal.is_null())
            .then(|| wide_to_string(inner.Anonymous.pwszVal));
        PropVariantClear(&mut value);
        (vtbl.release)(store);
        id
    };
    id.filter(|s| !s.is_empty())
}

/// # Safety
/// `p` must point at a NUL-terminated UTF-16 string.
unsafe fn wide_to_string(p: *const u16) -> String {
    let mut len = 0;
    // SAFETY: the caller guarantees termination.
    while unsafe { *p.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: `len` units were just read.
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, len) })
}

/// Whether an Application User Model ID names an installed web app rather than
/// the browser itself.
///
/// Deliberately narrow, because the cost of a false positive is an ordinary
/// browser window splitting off from its own setting. Edge's apps end in `!App`
/// (observed); Chromium names an app's id after `_crx_` (from its source, not
/// yet observed on a Chrome install).
#[must_use]
pub fn names_installed_web_app(aumid: &str) -> bool {
    let lower = aumid.to_ascii_lowercase();
    lower.ends_with("!app") || lower.contains("_crx_")
}

/// A short form of an identity for display: `msedge.exe#monkeytype.com-d841…!app`
/// reads as `msedge.exe · monkeytype.com`. Display only; the full identity is
/// still what is stored and matched.
#[must_use]
pub fn display_name(app_id: &str) -> String {
    let Some((exe, id)) = app_id.split_once('#') else {
        return app_id.to_owned();
    };
    // Edge names an app `<site>-<profile hash>_<package>!App`; the site is
    // everything before the last `-`.
    let site = id.rsplit_once('-').map_or(id, |(site, _)| site);
    format!("{exe} · {site}")
}

/// The identity to use for a window of `exe`, given its AUMID.
///
/// `msedge.exe` stays `msedge.exe`; an installed app becomes
/// `msedge.exe#<aumid>`. The `#` suffix keeps `is_chromium_app`'s prefix match
/// true and leaves every existing `msedge.exe` entry in a settings file meaning
/// the browser.
#[must_use]
pub fn refine(exe: &str, aumid: Option<&str>) -> String {
    match aumid {
        Some(id)
            if crate::default_exclusions::is_chromium_app(exe) && names_installed_web_app(id) =>
        {
            format!("{exe}#{}", id.to_ascii_lowercase())
        }
        _ => exe.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_installed_app_is_told_apart_from_its_browser() {
        let app = "monkeytype.com-D841B8E7_c12ap3q39px74!App";
        assert_eq!(
            refine("msedge.exe", Some(app)),
            "msedge.exe#monkeytype.com-d841b8e7_c12ap3q39px74!app"
        );
        assert_eq!(refine("msedge.exe", Some("MSEdge")), "msedge.exe");
    }

    #[test]
    fn display_name_shortens_only_a_refined_identity() {
        assert_eq!(
            display_name("msedge.exe#monkeytype.com-d841b8e7_c12ap3q39px74!app"),
            "msedge.exe · monkeytype.com"
        );
        assert_eq!(display_name("code.exe"), "code.exe");
    }

    #[test]
    fn a_missing_id_leaves_the_executable_name() {
        assert_eq!(refine("msedge.exe", None), "msedge.exe");
    }

    #[test]
    fn only_chromium_browsers_are_refined() {
        assert_eq!(refine("notepad.exe", Some("x!App")), "notepad.exe");
    }

    #[test]
    fn a_refined_identity_is_still_a_chromium_app() {
        assert!(crate::default_exclusions::is_chromium_app(&refine(
            "msedge.exe",
            Some("a!App")
        )));
    }
}
