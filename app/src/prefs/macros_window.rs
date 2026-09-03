//! The Macros window (UniKey's "gõ tắt") and its import/export.
//!
//! Kept with the window rather than in the controller because the interesting
//! part is the table format, not the UI: import merges and never overwrites, and
//! a real UniKey export needs its byte-order mark stripped and its version line
//! recognised — a version other than 1 means a VIQR body, which is refused with
//! an explanation rather than stored as literal `Vie^.t Nam`.

use objc2::rc::Retained;
use objc2::{msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSButton, NSModalResponseOK, NSOpenPanel, NSSavePanel, NSStackView,
    NSTextField, NSUserInterfaceLayoutOrientation, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSEdgeInsets, NSPoint, NSRect, NSSize, NSString};

use crate::strings::t;

use super::PrefsController;

impl PrefsController {
    /// Builds the Macros window on first use: a shortcut/expansion input row and
    /// the list of existing macros.
    pub(super) fn build_macros_window(&self, mtm: MainThreadMarker) {
        let content = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(460.0, 400.0));
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
        window.setTitle(&NSString::from_str(t("Macros", "Gõ tắt")));
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
                "Type the shortcut then a space to expand it — e.g. “vn” → “Việt Nam”.",
                "Gõ chữ viết tắt rồi dấu cách để bung ra — ví dụ “vn” → “Việt Nam”.",
            ),
            mtm,
        ));

        // Input row: [shortcut] [expansion] [Add]
        let row = NSStackView::new(mtm);
        row.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
        row.setSpacing(8.0);
        let shortcut = self.input_field(t("shortcut", "viết tắt"), 90.0, mtm);
        let expansion = self.input_field(t("expansion", "nội dung"), 210.0, mtm);
        let add: Retained<NSButton> = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str(t("Add", "Thêm")),
                Some(self.as_ref()),
                Some(sel!(addMacro:)),
                mtm,
            )
        };
        row.addArrangedSubview(&shortcut);
        row.addArrangedSubview(&expansion);
        row.addArrangedSubview(&add);
        root.addArrangedSubview(&row);
        *self.ivars().macro_shortcut.borrow_mut() = Some(shortcut);
        *self.ivars().macro_expansion.borrow_mut() = Some(expansion);

        // Import/export — the migration path for someone arriving from Unikey or
        // EVKey with a table they have curated for years.
        let file_row = NSStackView::new(mtm);
        file_row.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
        file_row.setSpacing(8.0);
        for (title, action) in [
            (t("Import…", "Nhập…"), sel!(importMacros:)),
            (t("Export…", "Xuất…"), sel!(exportMacros:)),
        ] {
            let button: Retained<NSButton> = unsafe {
                NSButton::buttonWithTitle_target_action(
                    &NSString::from_str(title),
                    Some(self.as_ref()),
                    Some(action),
                    mtm,
                )
            };
            unsafe {
                // NSControlSizeSmall == 1 — secondary to Add.
                let _: () = msg_send![&button, setControlSize: 1usize];
            }
            file_row.addArrangedSubview(&button);
        }
        root.addArrangedSubview(&file_row);

        let list = NSStackView::new(mtm);
        list.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
        list.setSpacing(2.0);
        unsafe {
            let _: () = msg_send![&list, setAlignment: 5isize];
        }
        root.addArrangedSubview(&list);
        *self.ivars().macros_list.borrow_mut() = Some(list);

        window.setContentView(Some(&root));
        *self.ivars().macros_window.borrow_mut() = Some(window);
    }

    /// Rebuilds the macro rows from the live list.
    pub(super) fn refresh_macros(&self) {
        let mtm = MainThreadMarker::from(self);
        let Some(list) = self.ivars().macros_list.borrow().clone() else {
            return;
        };
        for view in list.arrangedSubviews().iter() {
            list.removeArrangedSubview(&view);
            view.removeFromSuperview();
        }
        let macros = self.state().macros();
        *self.ivars().macro_count.borrow_mut() = macros.len();
        if macros.is_empty() {
            list.addArrangedSubview(&self.caption(t("No macros yet.", "Chưa có gõ tắt nào."), mtm));
            return;
        }
        for (index, m) in macros.iter().enumerate() {
            let row = NSStackView::new(mtm);
            row.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
            row.setSpacing(8.0);
            let label = self.make_label(&format!("{}  →  {}", m.shortcut, m.expansion), mtm);
            let width = label.widthAnchor().constraintEqualToConstant(320.0);
            width.setActive(true);
            let remove: Retained<NSButton> = unsafe {
                NSButton::buttonWithTitle_target_action(
                    &NSString::from_str(t("Remove", "Xóa")),
                    Some(self.as_ref()),
                    Some(sel!(removeMacro:)),
                    mtm,
                )
            };
            unsafe {
                let _: () = msg_send![&remove, setTag: index as isize];
                let _: () = msg_send![&remove, setControlSize: 1usize];
            }
            row.addArrangedSubview(&label);
            row.addArrangedSubview(&remove);
            list.addArrangedSubview(&row);
        }
    }

    /// An editable single-line text field with a placeholder and fixed width.
    pub(super) fn input_field(
        &self,
        placeholder: &str,
        width: f64,
        mtm: MainThreadMarker,
    ) -> Retained<NSTextField> {
        let field = NSTextField::new(mtm);
        field.setPlaceholderString(Some(&NSString::from_str(placeholder)));
        let constraint = field.widthAnchor().constraintEqualToConstant(width);
        constraint.setActive(true);
        field
    }
}

/// "Import…" — read a macro table and merge it into the list.
///
/// A free function rather than the action method's body: at a hundred lines of
/// file dialog, table parsing and merge reporting it was the largest thing inside
/// `define_class!`, where it sat between forty four-line toggles and made the
/// class definition unreadable. The action method calls straight through.
pub(super) fn import_macros(controller: &PrefsController) {
    let mtm = MainThreadMarker::from(controller);
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(false);
    panel.setAllowsMultipleSelection(false);
    panel.setMessage(Some(&NSString::from_str(t(
        "Choose a macro table to import.",
        "Chọn tệp gõ tắt để nhập.",
    ))));
    if panel.runModal() != NSModalResponseOK {
        return;
    }
    let Some(path) = panel.URLs().iter().next().and_then(|url| url.path()) else {
        return;
    };
    // Cap the read: this runs on the main thread, and stalling it long
    // enough gets the event tap disabled by timeout, which stops typing
    // everywhere. A real macro table is kilobytes.
    const MAX_TABLE_BYTES: u64 = 4 * 1024 * 1024;
    let path = path.to_string();
    if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > MAX_TABLE_BYTES) {
        controller.notify(
            t("That file is too large.", "Tệp đó quá lớn."),
            t(
                "A macro table is normally a few kilobytes.",
                "Bảng gõ tắt thường chỉ vài kilobyte.",
            ),
            mtm,
        );
        return;
    }
    let Ok(text) = std::fs::read_to_string(&path) else {
        controller.notify(
            t("Could not read that file.", "Không đọc được tệp đó."),
            "",
            mtm,
        );
        return;
    };
    if glowkey_engine::Macro::table_is_legacy_viqr(&text) {
        controller.notify(
            t(
                "That is an old UniKey table.",
                "Đó là bảng gõ tắt UniKey cũ.",
            ),
            t(
                "Its text is VIQR-encoded, which GlowKey does not read. Open it in \
                         UniKey and save it again to convert it to Unicode.",
                "Nội dung mã VIQR, GlowKey không đọc được. Mở lại bằng UniKey và lưu \
                         lại để chuyển sang Unicode.",
            ),
            mtm,
        );
        return;
    }
    let imported = glowkey_engine::Macro::parse_table(&text);
    if imported.is_empty() {
        let detail = if text.trim_start().starts_with('[') {
            t(
                "That JSON table could not be read.",
                "Không đọc được bảng JSON đó.",
            )
        } else {
            t(
                "Expected lines of the form shortcut:expansion.",
                "Cần các dòng dạng viếttắt:nội dung.",
            )
        };
        controller.notify(
            t("No macros in that file.", "Tệp đó không có gõ tắt nào."),
            detail,
            mtm,
        );
        return;
    }
    let Some((added, skipped)) = controller.state().import_macros_and_save(&imported) else {
        controller.notify(
            t("Could not import right now.", "Chưa nhập được lúc này."),
            t("Try again in a moment.", "Thử lại sau một lát."),
            mtm,
        );
        return;
    };
    controller.refresh_macros();
    let detail = if skipped == 0 {
        String::new()
    } else {
        t(
            "{} skipped — those shortcuts already exist.",
            "Bỏ qua {} — các chữ viết tắt đó đã có.",
        )
        .replace("{}", &skipped.to_string())
    };
    controller.notify(
        &t("Imported {} macros.", "Đã nhập {} gõ tắt.").replace("{}", &added.to_string()),
        &detail,
        mtm,
    );
}

/// "Export…" — write the current macro table to a file.
pub(super) fn export_macros(controller: &PrefsController) {
    let mtm = MainThreadMarker::from(controller);
    let panel = NSSavePanel::savePanel(mtm);
    panel.setNameFieldStringValue(&NSString::from_str("glowkey-macros.txt"));
    panel.setMessage(Some(&NSString::from_str(t(
        "Save the macro table.",
        "Lưu bảng gõ tắt.",
    ))));
    if panel.runModal() != NSModalResponseOK {
        return;
    }
    let Some(path) = panel.URL().and_then(|url| url.path()) else {
        return;
    };
    let macros = controller.state().macros();
    let text = glowkey_engine::Macro::format_table(&macros);
    if std::fs::write(path.to_string(), text).is_err() {
        controller.notify(
            t("Could not write that file.", "Không ghi được tệp đó."),
            "",
            mtm,
        );
        return;
    }
    // Silence after a save reads as "nothing happened", and an empty table
    // writes an empty file, which is worth saying out loud.
    controller.notify(
        &t("Exported {} macros.", "Đã xuất {} gõ tắt.").replace("{}", &macros.len().to_string()),
        if macros.is_empty() {
            t(
                "The list is empty, so the file is too.",
                "Danh sách trống nên tệp cũng trống.",
            )
        } else {
            ""
        },
        mtm,
    );
}
