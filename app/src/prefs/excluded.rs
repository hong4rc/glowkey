//! The Excluded Apps window — the user-facing surface of GlowKey's primary
//! feature, the per-application ignore list.
//!
//! A separate window rather than a Settings tab because the list is unbounded and
//! needs its own scroll region, and because removing an entry here is the *only*
//! way to drop a shipped default permanently (`docs/decisions/0004`): the ⌃⇧E
//! hotkey in a known terminal is deliberately session-only.

use objc2::rc::Retained;
use objc2::{msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSButton, NSStackView, NSUserInterfaceLayoutOrientation, NSWindow,
    NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSEdgeInsets, NSPoint, NSRect, NSSize, NSString};

use crate::strings::t;

use super::widgets::display_name;
use super::PrefsController;

impl PrefsController {
    /// Builds the separate "Excluded Apps" window on first use: a caption, the
    /// "Add App…" picker, and the app list.
    pub(super) fn build_excluded_window(&self, mtm: MainThreadMarker) {
        let content = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(420.0, 380.0));
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable;
        let window: Retained<NSWindow> = unsafe {
            let alloc = NSWindow::alloc(mtm);
            msg_send![
                alloc,
                initWithContentRect: content,
                styleMask: style,
                backing: NSBackingStoreType::Buffered,
                defer: false,
            ]
        };
        window.setTitle(&NSString::from_str(t("Excluded Apps", "Ứng dụng loại trừ")));
        unsafe { window.setReleasedWhenClosed(false) };

        let root = NSStackView::new(mtm);
        root.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
        root.setSpacing(8.0);
        root.setEdgeInsets(NSEdgeInsets {
            top: 20.0,
            left: 20.0,
            bottom: 20.0,
            right: 20.0,
        });
        unsafe {
            let _: () = msg_send![&root, setAlignment: 5isize];
        }

        root.addArrangedSubview(&self.caption(
            t(
                "GlowKey types plain keys in these apps. Add one below, from the menu bar\n(“Disable for …”), or with ⌃⇧E while in the app.",
                "GlowKey gõ phím thường trong các ứng dụng này. Thêm ở dưới, từ thanh menu\n(“Tắt cho …”), hoặc bằng ⌃⇧E khi đang ở trong ứng dụng.",
            ),
            mtm,
        ));
        let add_button: Retained<NSButton> = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str(t("Add App…", "Thêm ứng dụng…")),
                Some(self.as_ref()),
                Some(sel!(addApp:)),
                mtm,
            )
        };
        root.addArrangedSubview(&add_button);

        let list = NSStackView::new(mtm);
        list.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
        list.setSpacing(2.0);
        unsafe {
            let _: () = msg_send![&list, setAlignment: 5isize];
        }
        root.addArrangedSubview(&list);
        *self.ivars().list_stack.borrow_mut() = Some(list);

        window.setContentView(Some(&root));
        *self.ivars().excluded_window.borrow_mut() = Some(window);
    }

    /// Rebuilds the excluded-app rows from the live ignore list.
    pub(super) fn refresh_list(&self) {
        let mtm = MainThreadMarker::from(self);
        let Some(list) = self.ivars().list_stack.borrow().clone() else {
            return;
        };
        // Clear existing rows.
        let existing = list.arrangedSubviews();
        for view in existing.iter() {
            {
                list.removeArrangedSubview(&view);
                view.removeFromSuperview();
            }
        }

        let ids = self.state().exclusion_ids();
        *self.ivars().apps.borrow_mut() = ids.clone();

        if ids.is_empty() {
            list.addArrangedSubview(
                &self.caption(t("No apps excluded.", "Chưa có ứng dụng nào."), mtm),
            );
            return;
        }

        for (index, id) in ids.iter().enumerate() {
            let row = NSStackView::new(mtm);
            row.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
            row.setSpacing(8.0);

            // App name in a fixed-width column so the Remove buttons line up.
            let name = self.make_label(&display_name(id), mtm);
            let width = name.widthAnchor().constraintEqualToConstant(250.0);
            width.setActive(true);

            let remove: Retained<NSButton> = unsafe {
                NSButton::buttonWithTitle_target_action(
                    &NSString::from_str(t("Remove", "Xóa")),
                    Some(self.as_ref()),
                    Some(sel!(removeApp:)),
                    mtm,
                )
            };
            unsafe {
                let _: () = msg_send![&remove, setTag: index as isize];
                // NSControlSizeSmall == 1 — a compact secondary button.
                let _: () = msg_send![&remove, setControlSize: 1usize];
            }
            row.addArrangedSubview(&name);
            row.addArrangedSubview(&remove);
            list.addArrangedSubview(&row);
        }
    }
}
