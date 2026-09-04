//! The application main menu, which an agent app never draws but still needs.
//!
//! GlowKey is `LSUIElement`, so it owns no menu bar on screen and this menu is
//! invisible. It is installed anyway because **Cocoa dispatches every ⌘-key
//! equivalent through `NSApp.mainMenu` before the responder chain**. With no main
//! menu there is no Cut, Copy, Paste, Select All or Undo in any text field the app
//! owns, and ⌘W closes nothing.
//!
//! That was not a theoretical gap. The Macros window exists so someone can carry a
//! UniKey shortcut table across, and the expansion field is where `Việt Nam` goes
//! — a string most people paste rather than type. Personal Words has the same
//! problem. The app that types Vietnamese for a living could not paste Vietnamese
//! into itself.
//!
//! Every item here is a standard AppKit action sent to `nil`, which means the
//! responder chain resolves it against whatever is focused — the text field, then
//! the window. Nothing is wired to GlowKey's own code except the three items that
//! have to be (About, Settings, Quit), which reuse the status-menu controller so
//! there is exactly one implementation of each.

use objc2::rc::Retained;
use objc2::runtime::Sel;
use objc2::{sel, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSEventModifierFlags, NSMenu, NSMenuItem};
use objc2_foundation::{MainThreadMarker, NSString};

use crate::menu_bar::MenuController;
use crate::strings::t;

/// Builds and installs the main menu. Call once at startup, after the status item
/// exists (the App submenu targets its controller).
pub fn install(controller: &MenuController, mtm: MainThreadMarker) {
    let main = NSMenu::new(mtm);
    main.addItem(&submenu(app_menu(controller, mtm), "GlowKey", mtm));
    main.addItem(&submenu(edit_menu(mtm), t("Edit", "Sửa"), mtm));
    main.addItem(&submenu(window_menu(mtm), t("Window", "Cửa sổ"), mtm));
    NSApplication::sharedApplication(mtm).setMainMenu(Some(&main));
}

/// Wraps a menu in the item that carries it. A submenu's *item* holds the title;
/// the menu's own title is what a torn-off menu would show, so both are set.
fn submenu(menu: Retained<NSMenu>, title: &str, mtm: MainThreadMarker) -> Retained<NSMenuItem> {
    let title = NSString::from_str(title);
    menu.setTitle(&title);
    let item = NSMenuItem::new(mtm);
    item.setTitle(&title);
    item.setSubmenu(Some(&menu));
    item
}

/// About, Settings and Quit — the three that need GlowKey's own code.
///
/// They are duplicated from the status menu on purpose. The status menu's key
/// equivalents only fire while that menu is *open*, so ⌘, and ⌘Q did nothing
/// while a GlowKey window was focused, which is precisely when someone reaches
/// for them.
fn app_menu(controller: &MenuController, mtm: MainThreadMarker) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    targeted(
        &menu,
        controller,
        t("About GlowKey", "Giới thiệu GlowKey"),
        sel!(aboutGlowKey:),
        "",
        mtm,
    );
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    targeted(
        &menu,
        controller,
        t("Settings…", "Cài đặt…"),
        sel!(openSettings:),
        ",",
        mtm,
    );
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    // `hide:` and `hideOtherApplications:` are deliberately absent: hiding an app
    // with no windows and no Dock icon is a way to lose it.
    targeted(
        &menu,
        controller,
        t("Quit GlowKey", "Thoát GlowKey"),
        sel!(quit:),
        "q",
        mtm,
    );
    menu
}

/// The reason this module exists. All standard responder-chain actions, so a
/// focused `NSTextField` handles them with no code of ours involved.
fn edit_menu(mtm: MainThreadMarker) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    standard(&menu, t("Undo", "Hoàn tác"), sel!(undo:), "z", mtm);
    let redo = standard(&menu, t("Redo", "Làm lại"), sel!(redo:), "z", mtm);
    // ⇧⌘Z. `setKeyEquivalentModifierMask` replaces the mask outright, so Command
    // has to be named again alongside Shift.
    redo.setKeyEquivalentModifierMask(NSEventModifierFlags::Shift | NSEventModifierFlags::Command);
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    standard(&menu, t("Cut", "Cắt"), sel!(cut:), "x", mtm);
    standard(&menu, t("Copy", "Sao chép"), sel!(copy:), "c", mtm);
    standard(&menu, t("Paste", "Dán"), sel!(paste:), "v", mtm);
    standard(&menu, t("Delete", "Xoá"), sel!(delete:), "", mtm);
    standard(
        &menu,
        t("Select All", "Chọn tất cả"),
        sel!(selectAll:),
        "a",
        mtm,
    );
    menu
}

/// ⌘W and ⌘M. Without these a GlowKey window can only be closed by its red
/// button, which is a surprise in an app whose windows are otherwise ordinary.
fn window_menu(mtm: MainThreadMarker) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    standard(&menu, t("Close", "Đóng"), sel!(performClose:), "w", mtm);
    standard(
        &menu,
        t("Minimize", "Thu nhỏ"),
        sel!(performMiniaturize:),
        "m",
        mtm,
    );
    menu
}

/// An item sent to `nil`, so the responder chain decides who handles it — and
/// disables it automatically when nobody can.
fn standard(
    menu: &NSMenu,
    title: &str,
    action: Sel,
    key: &str,
    mtm: MainThreadMarker,
) -> Retained<NSMenuItem> {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            Some(action),
            &NSString::from_str(key),
        )
    };
    menu.addItem(&item);
    item
}

/// An item aimed at the menu-bar controller, for the actions that are GlowKey's
/// own rather than the responder chain's.
fn targeted(
    menu: &NSMenu,
    controller: &MenuController,
    title: &str,
    action: Sel,
    key: &str,
    mtm: MainThreadMarker,
) {
    let item = standard(menu, title, action, key, mtm);
    unsafe { item.setTarget(Some(controller)) };
}
