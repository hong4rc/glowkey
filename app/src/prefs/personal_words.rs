//! The Personal Words window: per-word answers to the one question no rule can
//! answer.
//!
//! `docs/handoff.md` §6.3 records the English/Telex ambiguity as inherent — the
//! same keystrokes are legitimate Vietnamese and legitimate English, so `was` is
//! `ứa` and `cats` is `cát`. The old answer was a single global switch whose
//! trade-off made a dozen ordinary Vietnamese words untypeable in their natural
//! key order, which is why it shipped off. The ambiguity is per word; the switch
//! was global; this window is where that mismatch is resolved.
//!
//! It exists **before** the correction hotkey that writes to this list, on
//! purpose. A writer without a viewer would mean a file the user cannot inspect
//! quietly accumulating decisions on their behalf.
//!
//! Modelled closely on `macros_window.rs` rather than improved upon: four
//! windows now need the same close-and-reopen fix and the same bilingual
//! treatment, and divergence between them is worse than duplication among them.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSButton, NSStackView, NSUserInterfaceLayoutOrientation, NSWindow,
    NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSEdgeInsets, NSPoint, NSRect, NSSize, NSString};

use glowkey_engine::WordPreference;

use super::PrefsController;
use crate::strings::t;

impl PrefsController {
    /// Builds the Personal Words window on first use.
    pub(super) fn build_personal_words_window(&self, mtm: MainThreadMarker) {
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
        window.setTitle(&NSString::from_str(t("Personal Words", "Từ riêng")));
        // Without this macOS frees the window on close and it cannot reopen.
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
                "Words you have decided about. “Typed” keeps the keys as typed \
                 (was → was); “Vietnamese” keeps the accented form (cats → cát). \
                 These win over auto-fix and over the English-word setting.",
                "Những từ bạn đã quyết định. “Như đã gõ” giữ nguyên các phím \
                 (was → was); “Tiếng Việt” giữ dạng có dấu (cats → cát). Chúng \
                 được ưu tiên hơn tự động sửa và hơn tùy chọn từ tiếng Anh.",
            ),
            mtm,
        ));

        // Input row: [keys] [Add as typed] [Add as Vietnamese]
        //
        // Two Add buttons rather than a field plus a segmented control: the
        // verdict *is* the action here, and a row that reads "type the word, then
        // say which way you want it" needs no third control to explain it.
        let row = NSStackView::new(mtm);
        row.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
        row.setSpacing(8.0);
        let keys = self.input_field(t("word as typed", "từ như đã gõ"), 140.0, mtm);
        row.addArrangedSubview(&keys);
        for (title, action) in [
            (t("Keep typed", "Giữ như gõ"), sel!(addWordAsTyped:)),
            (
                t("Keep Vietnamese", "Giữ tiếng Việt"),
                sel!(addWordAsVietnamese:),
            ),
        ] {
            let button: Retained<NSButton> = unsafe {
                NSButton::buttonWithTitle_target_action(
                    &NSString::from_str(title),
                    Some(self.as_ref()),
                    Some(action),
                    mtm,
                )
            };
            row.addArrangedSubview(&button);
        }
        root.addArrangedSubview(&row);
        *self.ivars().word_keys.borrow_mut() = Some(keys);

        let list = NSStackView::new(mtm);
        list.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
        list.setSpacing(4.0);
        unsafe {
            let _: () = msg_send![&list, setAlignment: 5isize];
        }
        root.addArrangedSubview(&list);
        *self.ivars().words_list.borrow_mut() = Some(list);

        window.setContentView(Some(&root));
        *self.ivars().words_window.borrow_mut() = Some(window);
    }

    /// Rebuilds the row list from the session. Called after every mutation, so a
    /// removed or flipped row cannot leave a stale index behind on a button tag.
    pub(super) fn refresh_words(&self) {
        let mtm = MainThreadMarker::from(self);
        let Some(list) = self.ivars().words_list.borrow().clone() else {
            return;
        };
        for view in list.arrangedSubviews().iter() {
            list.removeArrangedSubview(&view);
            view.removeFromSuperview();
        }
        let words = self.state().word_overrides();
        // The ordered keys, so a button's integer tag resolves back to a word: a
        // button cannot carry a string, and rebuilding invalidates any index
        // captured earlier.
        *self.ivars().word_order.borrow_mut() = words.iter().map(|o| o.keys.clone()).collect();
        if words.is_empty() {
            list.addArrangedSubview(&self.caption(
                t(
                    "Nothing yet. Add a word above, or press ⌃⇧W right after \
                     typing one to fix it and remember the choice.",
                    "Chưa có gì. Thêm một từ ở trên, hoặc bấm ⌃⇧W ngay sau khi \
                     gõ một từ để sửa và ghi nhớ lựa chọn.",
                ),
                mtm,
            ));
            return;
        }
        for (index, entry) in words.iter().enumerate() {
            let row = NSStackView::new(mtm);
            row.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
            row.setSpacing(8.0);
            let verdict = match entry.prefer {
                WordPreference::Raw => t("as typed", "như đã gõ"),
                WordPreference::Vietnamese => t("Vietnamese", "tiếng Việt"),
            };
            let label = self.make_label(&format!("{}  —  {}", entry.keys, verdict), mtm);
            let width = label.widthAnchor().constraintEqualToConstant(250.0);
            width.setActive(true);
            row.addArrangedSubview(&label);
            // Flip, because changing your mind about a word is the common case,
            // not an edge case worth a delete-and-retype.
            for (title, action) in [
                (t("Flip", "Đổi"), sel!(flipWord:)),
                (t("Remove", "Xóa"), sel!(removeWord:)),
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
                    let _: () = msg_send![&button, setTag: index as isize];
                    let _: () = msg_send![&button, setControlSize: 1usize];
                }
                row.addArrangedSubview(&button);
            }
            list.addArrangedSubview(&row);
        }
    }

    /// Resolves a tagged button back to the word its row shows.
    pub(super) fn word_at_tag(&self, sender: Option<&AnyObject>) -> Option<String> {
        let sender = sender?;
        let tag: isize = unsafe { msg_send![sender, tag] };
        self.ivars().word_order.borrow().get(tag as usize).cloned()
    }
}
