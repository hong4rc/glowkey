//! The menu bar: an `NSStatusItem` with a menu that shows the current state and
//! offers the quick controls — toggle Vietnamese for the frontmost app, flip
//! VN/EN, flip auto-fix, quit.
//!
//! Like the tap, this is objc2 AppKit and can only be verified by running.
//!
//! The menu is rebuilt each time it opens (`menuNeedsUpdate:`), so labels and
//! checkmarks always reflect the live state and the current frontmost app without
//! tracking individual item references.

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSMenu, NSMenuDelegate, NSMenuItem, NSPasteboard, NSPasteboardTypeString,
    NSStatusBar, NSStatusItem, NSVariableStatusItemLength, NSWorkspace,
};
use objc2_foundation::{MainThreadMarker, NSArray, NSString, NSURL};

use std::cell::RefCell;

use crate::strings::t;
use crate::tap::TapState;

/// Ivars for the menu controller: a pointer to the leaked, program-lifetime
/// `TapState` shared with the tap callback (both on the main thread), plus the
/// status item so the glyph can be refreshed to reflect live state.
pub struct ControllerIvars {
    state: *const TapState,
    status_item: RefCell<Option<Retained<NSStatusItem>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "GlowKeyMenuController"]
    #[ivars = ControllerIvars]
    pub struct MenuController;

    unsafe impl NSObjectProtocol for MenuController {}

    unsafe impl NSMenuDelegate for MenuController {
        /// Rebuild the menu with current labels/checkmarks whenever it opens.
        #[unsafe(method(menuNeedsUpdate:))]
        fn menu_needs_update(&self, menu: &NSMenu) {
            self.rebuild(menu);
        }
    }

    impl MenuController {
        /// Fired when a different application comes to the front. Update the
        /// session's current app (so VN/EN state reflects the switch immediately)
        /// and refresh the menu bar glyph.
        #[unsafe(method(appDidActivate:))]
        fn app_did_activate(&self, _notification: &objc2_foundation::NSNotification) {
            if let Some((_, bundle_id)) = crate::app_info::frontmost() {
                // Logged so a "something stole my focus" report can be read
                // straight off the key log: the activation lands between the two
                // keystrokes that bracket it.
                crate::log::log(&format!("FRONTMOST -> {bundle_id}"));
                self.state().set_frontmost_app(&bundle_id);
            }
            self.update_glyph();
        }

        #[unsafe(method(toggleCurrentApp:))]
        fn toggle_current_app(&self, _sender: Option<&AnyObject>) {
            if let Some((_, bundle_id)) = crate::app_info::frontmost() {
                self.state().toggle_app_exclusion_and_save(&bundle_id);
            }
            self.update_glyph();
        }

        #[unsafe(method(toggleMode:))]
        fn toggle_mode(&self, _sender: Option<&AnyObject>) {
            self.state().toggle_mode_and_save();
            self.update_glyph();
        }

        #[unsafe(method(toggleAutoFix:))]
        fn toggle_auto_fix(&self, _sender: Option<&AnyObject>) {
            self.state().toggle_auto_fix_and_save();
        }

        #[unsafe(method(toggleLaunchAtLogin:))]
        fn toggle_launch_at_login(&self, _sender: Option<&AnyObject>) {
            crate::login_item::set_enabled(!crate::login_item::is_enabled());
        }

        #[unsafe(method(resetEngine:))]
        fn reset_engine(&self, _sender: Option<&AnyObject>) {
            self.state().reset();
        }

        #[unsafe(method(clipboardRemoveTones:))]
        fn clipboard_remove_tones(&self, _sender: Option<&AnyObject>) {
            transform_clipboard(glowkey_engine::remove_tones);
        }

        #[unsafe(method(clipboardUppercase:))]
        fn clipboard_uppercase(&self, _sender: Option<&AnyObject>) {
            transform_clipboard(|text| text.to_uppercase());
        }

        #[unsafe(method(clipboardLowercase:))]
        fn clipboard_lowercase(&self, _sender: Option<&AnyObject>) {
            transform_clipboard(|text| text.to_lowercase());
        }

        #[unsafe(method(revealLog:))]
        fn reveal_log(&self, _sender: Option<&AnyObject>) {
            // Reveal the log file in Finder so it is easy to grab when reporting an
            // issue. Selects the file if it exists, else opens its folder.
            let Some(path) = crate::log::path() else { return };
            let workspace = NSWorkspace::sharedWorkspace();
            if path.exists() {
                let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
                let urls = NSArray::from_retained_slice(&[url]);
                workspace.activateFileViewerSelectingURLs(&urls);
            } else if let Some(dir) = path.parent() {
                let url = NSURL::fileURLWithPath(&NSString::from_str(&dir.to_string_lossy()));
                workspace.openURL(&url);
            }
        }

        #[unsafe(method(openAccessibilitySettings:))]
        fn open_accessibility_settings(&self, _sender: Option<&AnyObject>) {
            // Only reachable from the revoked-permission line, which is the one
            // moment the user needs the Accessibility pane rather than GlowKey's
            // own Settings. The tap's health monitor notices the grant coming back
            // and rebuilds the tap by itself, so there is nothing to do here but
            // open the right pane.
            crate::tap::open_accessibility_settings();
        }

        #[unsafe(method(openSettings:))]
        fn open_settings(&self, _sender: Option<&AnyObject>) {
            let mtm = MainThreadMarker::from(self);
            crate::prefs::show(self.ivars().state, mtm);
        }

        #[unsafe(method(quickGuide:))]
        fn quick_guide(&self, _sender: Option<&AnyObject>) {
            // The same panel shown once at first launch. Reopenable on purpose:
            // it is what makes dismissing the welcome a safe action rather than a
            // one-way door.
            let mtm = MainThreadMarker::from(self);
            crate::welcome::show(self.state().toggle_hotkey(), mtm);
        }

        #[unsafe(method(aboutGlowKey:))]
        fn about_glowkey(&self, _sender: Option<&AnyObject>) {
            let mtm = MainThreadMarker::from(self);
            crate::about_window::show(mtm);
        }

        #[unsafe(method(quit:))]
        fn quit(&self, _sender: Option<&AnyObject>) {
            // The only graceful-termination path in the app. Logged so an
            // unexplained disappearance can be told apart from a crash and from
            // something else invoking this action.
            crate::log::log("QUIT requested via the menu item");
            let mtm = MainThreadMarker::from(self);
            NSApplication::sharedApplication(mtm).terminate(None);
        }
    }
);

/// Rewrites the clipboard's text through `transform` — UniKey's "Công cụ"
/// tools, which work on a selection there and on the clipboard here because a
/// background agent has no selection of its own to act on.
///
/// Does nothing when the clipboard holds no text, so a stray click on the menu
/// item cannot destroy an image or a file the user had copied.
fn transform_clipboard(transform: impl FnOnce(&str) -> String) {
    let pasteboard = NSPasteboard::generalPasteboard();
    let Some(text) = (unsafe { pasteboard.stringForType(NSPasteboardTypeString) }) else {
        return;
    };
    let transformed = transform(&text.to_string());
    unsafe {
        pasteboard.clearContents();
        pasteboard.setString_forType(&NSString::from_str(&transformed), NSPasteboardTypeString);
    }
}

impl MenuController {
    fn state(&self) -> &TapState {
        // Safe: the pointer is to a leaked, program-lifetime TapState, and this
        // runs on the main thread where nothing frees it.
        unsafe { &*self.ivars().state }
    }

    /// Refreshes the menu bar glyph to reflect whether Vietnamese is active for the
    /// frontmost app: `VI` when on, `EN` when off (English mode or excluded app).
    fn update_glyph(&self) {
        // A dead tap outranks the mode. GlowKey used to keep showing VI after the
        // Accessibility permission was revoked, which made the one indicator the
        // app has assert that Vietnamese was on while no keystroke was reaching
        // the engine at all.
        //
        // Three states, not two. `EN` used to mean both "you turned Vietnamese
        // off" and "this app is on the ignore list", which collapses the one
        // question the indicator exists to answer — and the ignore list is the
        // feature the app is for. `ui-design.md` specified a dimmed glyph for the
        // excluded case from the start; it was never built.
        let dead = crate::tap::tap_is_dead();
        let vietnamese = self.state().mode_is_vietnamese();
        let suspended = vietnamese && !self.state().is_active();
        let title = if dead {
            "⚠"
        } else if vietnamese {
            "VI"
        } else {
            "EN"
        };
        let mtm = MainThreadMarker::from(self);
        if let Some(item) = self.ivars().status_item.borrow().as_ref() {
            if let Some(button) = item.button(mtm) {
                button.setTitle(&NSString::from_str(title));
                // Dimmed means "Vietnamese is your mode, but not in this app".
                // Alpha rather than a colour so it stays legible against whatever
                // the menu bar is sitting on, light, dark or a wallpaper.
                button.setAlphaValue(if suspended { 0.45 } else { 1.0 });
            }
        }
    }

    /// Rebuilds the menu items from current state.
    fn rebuild(&self, menu: &NSMenu) {
        menu.removeAllItems();
        let mtm = MainThreadMarker::from(self);

        let (app_name, bundle_id) = crate::app_info::frontmost()
            .unwrap_or_else(|| (t("this app", "ứng dụng này").to_string(), String::new()));
        let (mode, auto_fix, excluded) = self.state().menu_state(&bundle_id);

        // A dead tap is the only thing worth saying before anything else: every
        // item below it is inert until the permission comes back.
        if crate::tap::tap_is_dead() {
            self.add_disabled(
                menu,
                t(
                    "⚠ Accessibility permission revoked — Vietnamese is off",
                    "⚠ Đã thu hồi quyền Accessibility — tiếng Việt đang tắt",
                ),
                mtm,
            );
            self.add_item(
                menu,
                t("Open System Settings…", "Mở Cài đặt hệ thống…"),
                sel!(openAccessibilitySettings:),
                false,
                "",
                mtm,
            );
            self.add_separator(menu, mtm);
        }

        // Header: current state.
        let header = match (excluded, mode) {
            (true, _) => t("Excluded in {}", "Đã tắt trong {}").replace("{}", &app_name),
            (false, glowkey_engine::InputMode::Vietnamese) => {
                t("Vietnamese", "Tiếng Việt").to_string()
            }
            (false, glowkey_engine::InputMode::English) => t("English", "Tiếng Anh").to_string(),
        };
        self.add_disabled(menu, &header, mtm);
        self.add_separator(menu, mtm);

        // Enable/Disable Vietnamese for the current app (the quick per-app switch).
        let toggle_label = if excluded {
            t("Enable for “{}”", "Bật cho “{}”").replace("{}", &app_name)
        } else {
            t("Disable for “{}”", "Tắt cho “{}”").replace("{}", &app_name)
        };
        self.add_item(menu, &toggle_label, sel!(toggleCurrentApp:), false, "", mtm);

        self.add_separator(menu, mtm);

        // VN/EN mode toggle. The shortcut is handled by the tap, not the menu
        // (GlowKey is a background agent), so it is shown as title text for
        // discoverability rather than a real menu key equivalent.
        //
        // Read from the session rather than written out: the hotkey is
        // configurable — four presets plus a recorded custom combo — and this line
        // used to say `⌃⇧Space` whatever the user had chosen. The menu rebuilds on
        // every open, so it was a fresh lie each time.
        let mode_on = matches!(mode, glowkey_engine::InputMode::Vietnamese);
        let shortcut = crate::prefs::hotkey_display(self.state().toggle_hotkey());
        let mode_label = t("Vietnamese input ({})", "Gõ tiếng Việt ({})").replace("{}", &shortcut);
        self.add_item(menu, &mode_label, sel!(toggleMode:), mode_on, "", mtm);

        // Auto-fix toggle.
        self.add_item(
            menu,
            t("Auto-fix English words", "Tự động sửa từ tiếng Anh"),
            sel!(toggleAutoFix:),
            auto_fix,
            "",
            mtm,
        );

        self.add_separator(menu, mtm);

        // Launch at login (checkmark reflects the real SMAppService status) and a
        // safety-valve reset for the (human-unreachable) circuit breaker.
        self.add_item(
            menu,
            t("Open at login", "Khởi động cùng máy"),
            sel!(toggleLaunchAtLogin:),
            crate::login_item::is_enabled(),
            "",
            mtm,
        );
        self.add_item(
            menu,
            t("Reset input (if stuck)", "Đặt lại bộ gõ (nếu bị kẹt)"),
            sel!(resetEngine:),
            false,
            "",
            mtm,
        );
        // Clipboard tools — UniKey's "Công cụ": transform whatever is on the
        // clipboard in place. Copy, pick one, paste.
        self.add_item(
            menu,
            t("Clipboard: remove tones", "Clipboard: bỏ dấu"),
            sel!(clipboardRemoveTones:),
            false,
            "",
            mtm,
        );
        self.add_item(
            menu,
            t("Clipboard: UPPERCASE", "Clipboard: CHỮ HOA"),
            sel!(clipboardUppercase:),
            false,
            "",
            mtm,
        );
        self.add_item(
            menu,
            t("Clipboard: lowercase", "Clipboard: chữ thường"),
            sel!(clipboardLowercase:),
            false,
            "",
            mtm,
        );

        self.add_separator(menu, mtm);

        self.add_item(
            menu,
            t("Reveal Log in Finder", "Mở nhật ký trong Finder"),
            sel!(revealLog:),
            false,
            "",
            mtm,
        );

        self.add_separator(menu, mtm);
        self.add_item(
            menu,
            t("Settings…", "Cài đặt…"),
            sel!(openSettings:),
            false,
            ",",
            mtm,
        );
        self.add_item(
            menu,
            t("Quick Guide…", "Hướng dẫn nhanh…"),
            sel!(quickGuide:),
            false,
            "",
            mtm,
        );
        self.add_item(
            menu,
            t("About GlowKey", "Giới thiệu GlowKey"),
            sel!(aboutGlowKey:),
            false,
            "",
            mtm,
        );
        self.add_item(
            menu,
            t("Quit GlowKey", "Thoát GlowKey"),
            sel!(quit:),
            false,
            "q",
            mtm,
        );
    }

    fn add_item(
        &self,
        menu: &NSMenu,
        title: &str,
        action: objc2::runtime::Sel,
        checked: bool,
        key_equivalent: &str,
        mtm: MainThreadMarker,
    ) -> Retained<NSMenuItem> {
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(title),
                Some(action),
                &NSString::from_str(key_equivalent),
            )
        };
        unsafe { item.setTarget(Some(self)) };
        if checked {
            // NSControlStateValueOn = 1
            item.setState(1);
        }
        menu.addItem(&item);
        item
    }

    fn add_disabled(&self, menu: &NSMenu, title: &str, mtm: MainThreadMarker) {
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(title),
                None,
                &NSString::from_str(""),
            )
        };
        item.setEnabled(false);
        menu.addItem(&item);
    }

    fn add_separator(&self, menu: &NSMenu, _mtm: MainThreadMarker) {
        menu.addItem(&NSMenuItem::separatorItem(_mtm));
    }
}

/// Builds the status item and its menu, wiring the controller. Returns the retained
/// status item and controller, which the caller must keep alive for the process
/// lifetime (releasing the status item removes the menu bar icon).
pub fn install(
    state: *const TapState,
    mtm: MainThreadMarker,
) -> (Retained<NSStatusItem>, Retained<MenuController>) {
    let controller: Retained<MenuController> = {
        let this = MenuController::alloc(mtm).set_ivars(ControllerIvars {
            state,
            status_item: RefCell::new(None),
        });
        unsafe { msg_send![super(this), init] }
    };

    let status_bar = NSStatusBar::systemStatusBar();
    let item = status_bar.statusItemWithLength(NSVariableStatusItemLength);
    *controller.ivars().status_item.borrow_mut() = Some(item.clone());

    let menu = NSMenu::new(mtm);
    let delegate = ProtocolObject::from_ref(&*controller);
    menu.setDelegate(Some(delegate));
    controller.rebuild(&menu);
    item.setMenu(Some(&menu));

    // Refresh the glyph whenever the frontmost app changes, so it always shows the
    // state for the app you are in.
    let workspace = NSWorkspace::sharedWorkspace();
    let center = workspace.notificationCenter();
    unsafe {
        center.addObserver_selector_name_object(
            &controller,
            sel!(appDidActivate:),
            Some(objc2_app_kit::NSWorkspaceDidActivateApplicationNotification),
            None,
        );
    }

    controller.update_glyph();
    // Publish the controller so the tap can refresh the glyph after a hotkey
    // toggle (⌃⇧Space / ⌃⇧E), which happens in the tap, not the menu.
    CONTROLLER.with(|slot| *slot.borrow_mut() = Some(controller.clone()));
    (item, controller)
}

thread_local! {
    /// The installed menu controller, so [`refresh_glyph`] can update the menu-bar
    /// glyph from the tap. Main-thread only; empty in tests (no menu is installed).
    static CONTROLLER: RefCell<Option<Retained<MenuController>>> = const { RefCell::new(None) };
}

/// Refreshes the menu-bar `VN`/`EN` glyph to the live state. Called by the tap after
/// a hotkey toggle so the persistent indicator matches the current mode/app, not
/// only after an app switch or menu click. A no-op before the menu is installed
/// (including under tests).
pub fn refresh_glyph() {
    CONTROLLER.with(|slot| {
        if let Some(controller) = slot.borrow().as_ref() {
            controller.update_glyph();
        }
    });
}
