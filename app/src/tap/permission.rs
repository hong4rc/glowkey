//! The Accessibility permission gate, on screen.
//!
//! GlowKey is an `LSUIElement` agent: no Dock icon, and the status item cannot
//! draw before the AppKit loop runs. Waiting for the grant in a bare sleep loop
//! therefore left a launch from Finder with nothing at all to see — no icon, no
//! window, no log line — and the app looked dead while it was in fact waiting.
//! Two details here are load-bearing and both are recorded in
//! `docs/handoff.md` §6.5: `NSAlert::layout()` must be called before `window()`,
//! and the system's own prompt fires from the alert's button rather than at
//! launch.

use std::ffi::c_void;
use std::time::Duration;

use objc2_app_kit::NSWorkspace;

/// Blocks until this process is trusted for Accessibility, keeping an alert on
/// screen for as long as it waits.
///
/// GlowKey is an `LSUIElement` agent: no Dock icon, and the status item cannot
/// draw before the AppKit loop runs. Polling in a bare sleep loop therefore left
/// a launch from Finder or `open` with nothing at all to see — no icon, no
/// window, no log line — and the app looked dead while it was in fact waiting.
/// A modal *session* is the one thing that renders here: it drives the run loop
/// so the alert appears, and it hands control back on every pass, so the wait
/// ends by itself the moment the user flips the switch.
pub(super) fn wait_for_accessibility() {
    use objc2_app_kit::{
        NSAlert, NSAlertFirstButtonReturn, NSApplication, NSModalResponseContinue,
    };
    use objc2_foundation::{MainThreadMarker, NSString};

    let Some(mtm) = MainThreadMarker::new() else {
        while !accessibility_trusted() {
            std::thread::sleep(Duration::from_millis(500));
        }
        return;
    };

    // Name the running bundle rather than the project: "GlowKey" and "GlowKey Dev"
    // are separate entries in the Accessibility list, and the alert has to say
    // which one to switch on.
    let name = bundle_display_name();
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(
        &crate::strings::t("{} needs Accessibility permission", "{} cần quyền Trợ năng")
            .replace("{}", &name),
    ));
    alert.setInformativeText(&NSString::from_str(
        &crate::strings::t(
            "Open System Settings → Privacy & Security → Accessibility and turn {} on. \
             Vietnamese typing starts by itself the moment you do — leave this window open.\n\n\
             Already in the list? The permission is tied to this exact copy of the app, so a \
             rebuild or a move (to /Applications, say) needs a fresh grant: switch {} off \
             and on again, or remove it with “−” and add this copy back.",
            "Mở Cài đặt Hệ thống → Quyền riêng tư & Bảo mật → Trợ năng và bật {} lên. \
             Gõ tiếng Việt sẽ chạy ngay khi bạn bật — cứ để cửa sổ này mở.\n\n\
             Đã có trong danh sách? Quyền gắn với đúng bản sao này của ứng dụng, nên sau khi \
             build lại hoặc di chuyển (ví dụ sang /Applications) phải cấp lại: tắt {} rồi bật \
             lại, hoặc xóa bằng “−” và thêm bản này vào.",
        )
        .replace("{}", &name),
    ));
    alert.addButtonWithTitle(&NSString::from_str(crate::strings::t(
        "Open System Settings",
        "Mở Cài đặt Hệ thống",
    )));
    alert.addButtonWithTitle(&NSString::from_str(
        &crate::strings::t("Quit {}", "Thoát {}").replace("{}", &name),
    ));

    let app = NSApplication::sharedApplication(mtm);
    // The main loop has not started yet, and AppKit will not put a window on
    // screen until the app has finished launching. Without this the modal session
    // runs but draws nothing — the very silence this alert exists to break.
    app.finishLaunching();

    // NSAlert lays itself out inside `runModal`, which a raw modal session never
    // calls. Skipping it left the panel showing its un-laid-out template: 260
    // points wide with both strings truncated, a placeholder "Do not show this
    // message again" checkbox nobody asked for, and a spare untitled button
    // sitting between the two real ones. `layout` is the documented way to
    // prepare the panel when you need its window before running it.
    alert.layout();
    let window = alert.window();

    loop {
        if accessibility_trusted() {
            break;
        }
        // Bring the alert to the front: an agent app is never the active app, so
        // without this the window can open behind whatever the user is using.
        app.activate();
        let session = app.beginModalSessionForWindow(&window);
        let pressed = loop {
            if accessibility_trusted() {
                break None;
            }
            let response = unsafe { app.runModalSession(session) };
            if response != NSModalResponseContinue {
                break Some(response);
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        unsafe { app.endModalSession(session) };
        match pressed {
            // Granted while the alert was up.
            None => break,
            // "Open System Settings" — send them to the right pane, then show the
            // alert again so the app still has a visible presence while it waits.
            Some(response) if response == NSAlertFirstButtonReturn => {
                // Ask the system now rather than at launch. `AXIsProcessTrustedWithOptions`
                // is what registers the app in the Accessibility list — without it
                // there is nothing for the user to switch on — but it also puts up
                // macOS's own dialog, and firing that at startup alongside this one
                // meant two popups at once. Now it follows the user's click.
                prompt_accessibility();
                open_accessibility_settings();
            }
            _ => {
                crate::log::log("STARTUP quit at the Accessibility gate");
                std::process::exit(0);
            }
        }
    }
    window.orderOut(None);
}

/// The running bundle's display name ("GlowKey", "GlowKey Dev"), falling back to
/// the project name when unbundled (tests).
pub(super) fn bundle_display_name() -> String {
    use objc2_foundation::{NSBundle, NSString};

    for key in ["CFBundleDisplayName", "CFBundleName"] {
        if let Some(name) = NSBundle::mainBundle()
            .objectForInfoDictionaryKey(&NSString::from_str(key))
            .and_then(|value| value.downcast::<NSString>().ok())
        {
            return name.to_string();
        }
    }
    "GlowKey".to_string()
}

/// Opens System Settings straight at Privacy & Security → Accessibility.
pub fn open_accessibility_settings() {
    use objc2_foundation::{NSString, NSURL};

    let url = NSURL::URLWithString(&NSString::from_str(
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
    ));
    if let Some(url) = url {
        NSWorkspace::sharedWorkspace().openURL(&url);
    }
}

/// Whether this process is trusted for Accessibility (required for the tap).
pub(super) fn accessibility_trusted() -> bool {
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    unsafe { AXIsProcessTrusted() }
}

/// Shows the system Accessibility prompt and registers GlowKey in the
/// Accessibility list, so the user can grant it with one click. Returns the
/// current trust state.
pub(super) fn prompt_accessibility() -> bool {
    use objc2_core_foundation::{
        kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks, CFDictionary,
    };
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        static kAXTrustedCheckOptionPrompt: *const c_void; // CFStringRef
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    }
    unsafe {
        // Build { kAXTrustedCheckOptionPrompt: true } and ask with a prompt.
        let true_value = objc2_core_foundation::kCFBooleanTrue;
        let key = kAXTrustedCheckOptionPrompt;
        let value = true_value
            .map(|b| (b as *const objc2_core_foundation::CFBoolean).cast::<c_void>())
            .unwrap_or(std::ptr::null());
        let mut keys = [key];
        let mut values = [value];
        let options = CFDictionary::new(
            None,
            keys.as_mut_ptr(),
            values.as_mut_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        let options_ptr = options
            .as_ref()
            .map(|d| (d.as_ref() as *const objc2_core_foundation::CFDictionary).cast::<c_void>())
            .unwrap_or(std::ptr::null());
        AXIsProcessTrustedWithOptions(options_ptr)
    }
}
