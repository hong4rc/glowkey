//! Launch at login, via `SMAppService` (macOS 13+). GlowKey registers itself as a
//! login item so it starts with the session — essential for an input method, which
//! is otherwise inert until manually launched.
//!
//! Registration succeeds only for a signed app bundle in a stable location; when it
//! fails (e.g. an ad-hoc build run from the source tree) the error is logged and the
//! menu simply reflects the real `status()`.

use objc2_service_management::{SMAppService, SMAppServiceStatus};

/// Whether GlowKey is currently registered to launch at login.
pub fn is_enabled() -> bool {
    // Safe: reads the main app's login-item status; no arguments, no mutation.
    unsafe { SMAppService::mainAppService().status() == SMAppServiceStatus::Enabled }
}

/// Registers or unregisters GlowKey as a login item. Logs on failure (the caller
/// re-reads `is_enabled()` for the menu checkmark, so a failed toggle shows as
/// unchanged rather than a false "on").
pub fn set_enabled(on: bool) {
    let service = unsafe { SMAppService::mainAppService() };
    let result = if on {
        unsafe { service.registerAndReturnError() }
    } else {
        unsafe { service.unregisterAndReturnError() }
    };
    if let Err(err) = result {
        eprintln!(
            "GlowKey: launch-at-login {} failed: {err}",
            if on { "register" } else { "unregister" }
        );
    }
}
