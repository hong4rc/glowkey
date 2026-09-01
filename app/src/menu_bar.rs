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
    NSApplication, NSMenu, NSMenuDelegate, NSMenuItem, NSStatusBar, NSStatusItem,
    NSVariableStatusItemLength, NSWorkspace,
};
use objc2_foundation::{MainThreadMarker, NSString};

use std::cell::RefCell;

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

        #[unsafe(method(openSettings:))]
        fn open_settings(&self, _sender: Option<&AnyObject>) {
            let mtm = MainThreadMarker::from(self);
            crate::prefs_window::show(self.ivars().state, mtm);
        }

        #[unsafe(method(quit:))]
        fn quit(&self, _sender: Option<&AnyObject>) {
            let mtm = MainThreadMarker::from(self);
            NSApplication::sharedApplication(mtm).terminate(None);
        }
    }
);

impl MenuController {
    fn state(&self) -> &TapState {
        // Safe: the pointer is to a leaked, program-lifetime TapState, and this
        // runs on the main thread where nothing frees it.
        unsafe { &*self.ivars().state }
    }

    /// Refreshes the menu bar glyph to reflect whether Vietnamese is active for the
    /// frontmost app: `VN` when on, `EN` when off (English mode or excluded app).
    fn update_glyph(&self) {
        let title = if self.state().is_active() { "VN" } else { "EN" };
        let mtm = MainThreadMarker::from(self);
        if let Some(item) = self.ivars().status_item.borrow().as_ref() {
            if let Some(button) = item.button(mtm) {
                button.setTitle(&NSString::from_str(title));
            }
        }
    }

    /// Rebuilds the menu items from current state.
    fn rebuild(&self, menu: &NSMenu) {
        menu.removeAllItems();
        let mtm = MainThreadMarker::from(self);

        let (app_name, bundle_id) =
            crate::app_info::frontmost().unwrap_or_else(|| ("this app".to_string(), String::new()));
        let (mode, auto_fix, excluded) = self.state().menu_state(&bundle_id);

        // Header: current state.
        let header = match (excluded, mode) {
            (true, _) => format!("Excluded in {app_name}"),
            (false, glowkey_engine::InputMode::Vietnamese) => "Vietnamese".to_string(),
            (false, glowkey_engine::InputMode::English) => "English".to_string(),
        };
        self.add_disabled(menu, &header, mtm);
        self.add_separator(menu, mtm);

        // Enable/Disable Vietnamese for the current app (the quick per-app switch).
        let toggle_label = if excluded {
            format!("Enable for “{app_name}”")
        } else {
            format!("Disable for “{app_name}”")
        };
        self.add_item(menu, &toggle_label, sel!(toggleCurrentApp:), false, "", mtm);

        self.add_separator(menu, mtm);

        // VN/EN mode toggle. The ⌃⇧Space shortcut is handled by the tap, not the
        // menu (GlowKey is a background agent), so it is shown as title text for
        // discoverability rather than a real menu key equivalent.
        let mode_on = matches!(mode, glowkey_engine::InputMode::Vietnamese);
        self.add_item(
            menu,
            "Vietnamese input (⌃⇧Space)",
            sel!(toggleMode:),
            mode_on,
            "",
            mtm,
        );

        // Auto-fix toggle.
        self.add_item(
            menu,
            "Auto-fix English words",
            sel!(toggleAutoFix:),
            auto_fix,
            "",
            mtm,
        );

        self.add_separator(menu, mtm);
        self.add_item(menu, "Settings…", sel!(openSettings:), false, ",", mtm);
        self.add_item(menu, "Quit GlowKey", sel!(quit:), false, "q", mtm);
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
    (item, controller)
}
