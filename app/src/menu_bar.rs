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
    NSVariableStatusItemLength,
};
use objc2_foundation::{MainThreadMarker, NSString};

use crate::tap::TapState;

/// Ivars for the menu controller: a pointer to the leaked, program-lifetime
/// `TapState` shared with the tap callback (both on the main thread).
pub struct ControllerIvars {
    state: *const TapState,
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

        #[unsafe(method(toggleCurrentApp:))]
        fn toggle_current_app(&self, _sender: Option<&AnyObject>) {
            if let Some((_, bundle_id)) = crate::app_info::frontmost() {
                self.state().toggle_app_exclusion_and_save(&bundle_id);
            }
        }

        #[unsafe(method(toggleMode:))]
        fn toggle_mode(&self, _sender: Option<&AnyObject>) {
            self.state().toggle_mode_and_save();
        }

        #[unsafe(method(toggleAutoFix:))]
        fn toggle_auto_fix(&self, _sender: Option<&AnyObject>) {
            self.state().toggle_auto_fix_and_save();
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

        // Enable/Disable for the current app.
        let toggle_label = if excluded {
            format!("Enable Vietnamese for {app_name}")
        } else {
            format!("Disable Vietnamese for {app_name}")
        };
        self.add_item(menu, &toggle_label, sel!(toggleCurrentApp:), false, mtm);

        // VN/EN mode toggle.
        let mode_on = matches!(mode, glowkey_engine::InputMode::Vietnamese);
        self.add_item(
            menu,
            "Vietnamese mode (⌃⇧Space)",
            sel!(toggleMode:),
            mode_on,
            mtm,
        );

        // Auto-fix toggle.
        self.add_item(
            menu,
            "Auto-fix invalid words",
            sel!(toggleAutoFix:),
            auto_fix,
            mtm,
        );

        self.add_separator(menu, mtm);
        self.add_item(menu, "Quit GlowKey", sel!(quit:), false, mtm);
    }

    fn add_item(
        &self,
        menu: &NSMenu,
        title: &str,
        action: objc2::runtime::Sel,
        checked: bool,
        mtm: MainThreadMarker,
    ) {
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(title),
                Some(action),
                &NSString::from_str(""),
            )
        };
        unsafe { item.setTarget(Some(self)) };
        if checked {
            // NSControlStateValueOn = 1
            item.setState(1);
        }
        menu.addItem(&item);
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
        let this = MenuController::alloc(mtm).set_ivars(ControllerIvars { state });
        unsafe { msg_send![super(this), init] }
    };

    let status_bar = NSStatusBar::systemStatusBar();
    let item = status_bar.statusItemWithLength(NSVariableStatusItemLength);
    if let Some(button) = item.button(mtm) {
        button.setTitle(&NSString::from_str("VN"));
    }

    let menu = NSMenu::new(mtm);
    let delegate = ProtocolObject::from_ref(&*controller);
    menu.setDelegate(Some(delegate));
    controller.rebuild(&menu);
    item.setMenu(Some(&menu));

    (item, controller)
}
