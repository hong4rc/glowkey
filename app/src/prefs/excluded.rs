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
    NSBackingStoreType, NSButton, NSImageView, NSStackView, NSUserInterfaceLayoutOrientation,
    NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSEdgeInsets, NSPoint, NSRect, NSSize, NSString};

use crate::strings::t;

use super::PrefsController;

impl PrefsController {
    /// Builds the separate "Excluded Apps" window on first use: a caption, the
    /// "Add App…" picker, and the app list.
    pub(super) fn build_excluded_window(&self, mtm: MainThreadMarker) {
        let content = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(420.0, 380.0));
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            // Resizable because these windows hold lists of unknown length.
            // Every one of them was fixed-size, which turned "too many rows"
            // into "rows you cannot see".
            | NSWindowStyleMask::Resizable;
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
        // Scrolled, not placed bare: the fourteen shipped exclusions overflow the window on a clean install,
        // and rows past the window's bottom edge were unreachable.
        let scroll = self.scrollable(&list, 200.0, mtm);
        root.addArrangedSubview(&scroll);
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

        // Resolve each identifier to the app's real name and icon, then sort by
        // that name. The list used to show the last segment of the bundle id with
        // its first letter capitalized, sorted by the id — so it read `Wezterm`,
        // `Iterm2`, `Intellij`, ordered by reverse-DNS in a window whose whole job
        // is to let someone find an app they recognise. `ui-design.md` specified
        // icons and localized names from the start; this is that, finally built.
        //
        // The lookups are Launch Services round-trips, which is why they happen
        // here — building a window — and never anywhere near the tap
        // (`docs/decisions/0008`).
        let mut resolved: Vec<(String, crate::app_info::AppDisplay)> = self
            .state()
            .exclusion_ids()
            .into_iter()
            .map(|id| {
                let display = crate::app_info::describe(&id);
                (id, display)
            })
            .collect();
        resolved.sort_by_key(|(_, app)| app.name.to_lowercase());

        // The Remove buttons carry their row index as a tag, so the stored order
        // must be the order on screen — not the order the engine returned.
        let ids: Vec<String> = resolved.iter().map(|(id, _)| id.clone()).collect();
        *self.ivars().apps.borrow_mut() = ids.clone();

        if ids.is_empty() {
            list.addArrangedSubview(
                &self.caption(t("No apps excluded.", "Chưa có ứng dụng nào."), mtm),
            );
            return;
        }

        for (index, (_, app)) in resolved.iter().enumerate() {
            let row = NSStackView::new(mtm);
            row.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
            row.setSpacing(8.0);

            // A 16pt icon, the standard size for an app in a list. The view is
            // added even when there is no icon so the names stay in one column
            // whether or not every app is installed.
            let icon_view = NSImageView::new(mtm);
            if let Some(icon) = app.icon.as_ref() {
                icon_view.setImage(Some(icon));
            }
            icon_view
                .widthAnchor()
                .constraintEqualToConstant(16.0)
                .setActive(true);
            icon_view
                .heightAnchor()
                .constraintEqualToConstant(16.0)
                .setActive(true);
            row.addArrangedSubview(&icon_view);

            // App name in a fixed-width column so the Remove buttons line up.
            let label_text = if app.installed {
                app.name.clone()
            } else {
                // Named as missing rather than dropped: the exclusion is still the
                // user's choice, and an app can be absent for a night (an external
                // disk, a reinstall) without that choice becoming wrong.
                format!("{} {}", app.name, t("(not installed)", "(chưa cài đặt)"))
            };
            let name = self.make_label(&label_text, mtm);
            if !app.installed {
                name.setTextColor(Some(&objc2_app_kit::NSColor::secondaryLabelColor()));
            }
            let width = name.widthAnchor().constraintEqualToConstant(234.0);
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
