//! The Settings window itself: four tabs, built once on first open.
//!
//! Split out because it is by far the largest thing in this module and it is all
//! one shape — make a control, set its state from the session, add it to a stack.
//! The four tabs exist because a single column had grown past 800 points, taller
//! than the screen it had to fit on; each tab's title now carries the grouping
//! the section headers used to.

use objc2::rc::Retained;
use objc2::{msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSButton, NSControlStateValueOff, NSControlStateValueOn,
    NSSegmentSwitchTracking, NSSegmentedControl, NSStackView, NSTabView, NSTabViewItem,
    NSUserInterfaceLayoutOrientation, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSArray, NSPoint, NSRect, NSSize, NSString};

use glowkey_engine::{InputMethod, Language, PlacementStyle};

use crate::strings::t;

use super::widgets::LABEL_COLUMN_WIDTH;
use super::PrefsController;

impl PrefsController {
    /// Constructs the window and its static controls once.
    pub(super) fn build_window(&self, mtm: MainThreadMarker) {
        // Sized for the full stack (this grew with the English-restore caption and
        // the hotkey recorder row) — an NSStackView compresses silently if short.
        let content = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(460.0, 540.0));
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
        window.setTitle(&NSString::from_str(t(
            "GlowKey Settings",
            "Cài đặt GlowKey",
        )));
        unsafe { window.setReleasedWhenClosed(false) };

        // One vertical stack per tab. Every option used to live in a single
        // scrolling column, which had grown past 800 points tall — a wall of
        // checkboxes with no shape. Four tabs keep each pane short enough to read
        // at a glance, and the tab title carries the grouping that section
        // headers used to.
        let general = self.tab_stack(mtm);
        let typing = self.tab_stack(mtm);
        let corrections = self.tab_stack(mtm);
        let apps = self.tab_stack(mtm);

        // ===== General =====

        // Interface language — first, because it changes everything below it.
        let language_labels = NSArray::from_retained_slice(&[
            NSString::from_str(t("System", "Hệ thống")),
            NSString::from_str("Tiếng Việt"),
            NSString::from_str("English"),
        ]);
        let language_seg: Retained<NSSegmentedControl> = unsafe {
            NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(
                &language_labels,
                NSSegmentSwitchTracking::SelectOne,
                Some(self.as_ref()),
                Some(sel!(languageChanged:)),
                mtm,
            )
        };
        language_seg.setSelectedSegment(match self.state().language() {
            Language::System => 0,
            Language::Vietnamese => 1,
            Language::English => 2,
        });
        general.addArrangedSubview(&self.form_row(t("Language", "Ngôn ngữ"), &language_seg, mtm));

        let launch_at_login: Retained<NSButton> = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &NSString::from_str(t("Launch GlowKey at login", "Khởi động GlowKey cùng máy")),
                Some(self.as_ref()),
                Some(sel!(launchAtLoginChanged:)),
                mtm,
            )
        };
        launch_at_login.setState(if crate::login_item::is_enabled() {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        general.addArrangedSubview(&launch_at_login);

        let open_at_launch: Retained<NSButton> = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &NSString::from_str(t(
                    "Open this window at launch",
                    "Mở cửa sổ này khi khởi động",
                )),
                Some(self.as_ref()),
                Some(sel!(openAtLaunchChanged:)),
                mtm,
            )
        };
        open_at_launch.setState(if self.state().open_settings_at_launch() {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        general.addArrangedSubview(&open_at_launch);
        unsafe {
            let _: () = msg_send![&general, setCustomSpacing: 22.0f64, afterView: &*open_at_launch];
        }

        // ===== Typing =====

        // Input method — Telex / VNI.
        let method_labels = NSArray::from_retained_slice(&[
            NSString::from_str("Telex"),
            NSString::from_str("VNI"),
            NSString::from_str(t("Simple Telex", "Telex đơn giản")),
        ]);
        let method_seg: Retained<NSSegmentedControl> = unsafe {
            NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(
                &method_labels,
                NSSegmentSwitchTracking::SelectOne,
                Some(self.as_ref()),
                Some(sel!(inputMethodChanged:)),
                mtm,
            )
        };
        method_seg.setSelectedSegment(match self.state().input_method() {
            InputMethod::Telex => 0,
            InputMethod::Vni => 1,
            InputMethod::SimpleTelex => 2,
        });
        typing.addArrangedSubview(&self.form_row(t("Input method", "Kiểu gõ"), &method_seg, mtm));

        // Tone marks — aligned label + segmented control.
        let labels = NSArray::from_retained_slice(&[
            NSString::from_str(t("Modern  hoà", "Kiểu mới  hoà")),
            NSString::from_str(t("Classic  hòa", "Kiểu cũ  hòa")),
        ]);
        let seg: Retained<NSSegmentedControl> = unsafe {
            NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(
                &labels,
                NSSegmentSwitchTracking::SelectOne,
                Some(self.as_ref()),
                Some(sel!(toneChanged:)),
                mtm,
            )
        };
        seg.setSelectedSegment(if self.state().style() == PlacementStyle::New {
            0
        } else {
            1
        });
        typing.addArrangedSubview(&self.form_row(t("Tone marks", "Dấu thanh"), &seg, mtm));

        // Quick Telex — doubled-consonant shortcuts, as EVKey and later UniKey
        // releases offer. (Not present in the 2015 UniKey source, so the idea is
        // credited loosely rather than to a specific implementation.)
        let quick_telex: Retained<NSButton> = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &NSString::from_str(t("Quick Telex", "Gõ tắt phụ âm")),
                Some(self.as_ref()),
                Some(sel!(quickTelexChanged:)),
                mtm,
            )
        };
        quick_telex.setState(if self.state().quick_telex() {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        typing.addArrangedSubview(&quick_telex);
        typing.addArrangedSubview(&self.caption(
            t(
                "A doubled consonant at the start of a syllable types its digraph:\ncc→ch, gg→gi, kk→kh, nn→ng, pp→ph, qq→qu, tt→th, uu→ư.",
                "Phụ âm gõ đôi ở đầu âm tiết cho ra phụ âm ghép:\ncc→ch, gg→gi, kk→kh, nn→ng, pp→ph, qq→qu, tt→th, uu→ư.",
            ),
            mtm,
        ));

        // Telex bracket shortcuts — UniKey's `[`/`]` vowel keys.
        let brackets: Retained<NSButton> = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &NSString::from_str(t("Telex bracket shortcuts", "Phím ngoặc kiểu Telex")),
                Some(self.as_ref()),
                Some(sel!(telexBracketsChanged:)),
                mtm,
            )
        };
        brackets.setState(if self.state().telex_brackets() {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        typing.addArrangedSubview(&brackets);
        typing.addArrangedSubview(&self.caption(
            t(
                "[ → ơ, ] → ư, { → Ơ, } → Ư while typing Telex. These four keys stop\nreaching the app entirely, including where they are shortcuts.",
                "[ → ơ, ] → ư, { → Ơ, } → Ư khi gõ Telex. Bốn phím này sẽ không đến\nứng dụng nữa, kể cả khi chúng là phím tắt.",
            ),
            mtm,
        ));

        // Auto-fix — a full-width checkbox with a secondary caption beneath it.
        let checkbox: Retained<NSButton> = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &NSString::from_str(t(
                    "Auto-fix non-Vietnamese words",
                    "Tự động khôi phục từ không phải tiếng Việt",
                )),
                Some(self.as_ref()),
                Some(sel!(autoFixChanged:)),
                mtm,
            )
        };
        checkbox.setState(if self.state().auto_fix() {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        corrections.addArrangedSubview(&checkbox);
        corrections.addArrangedSubview(&self.caption(
            t(
                "Restores the raw keys at the space when the result isn’t valid\nVietnamese — types “exit”, not “eĩt”.",
                "Khôi phục phím gốc ở dấu cách khi kết quả không phải tiếng Việt —\ngõ ra “exit”, không phải “eĩt”.",
            ),
            mtm,
        ));

        // Mid-word spell check — UniKey's second, separate spell-check option.
        let strict: Retained<NSButton> = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &NSString::from_str(t(
                    "Fix as I type, not at the space",
                    "Sửa ngay khi gõ, không đợi dấu cách",
                )),
                Some(self.as_ref()),
                Some(sel!(strictSpellCheckChanged:)),
                mtm,
            )
        };
        strict.setState(if self.state().strict_spell_check() {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        corrections.addArrangedSubview(&strict);
        corrections.addArrangedSubview(&self.caption(
            t(
                "Restores the raw keys the moment a word stops being possible\nVietnamese — “exit” repairs at the x, not at the space.",
                "Khôi phục phím gốc ngay khi từ không còn là tiếng Việt hợp lệ —\n“exit” được sửa ngay ở chữ x, không đợi dấu cách.",
            ),
            mtm,
        ));

        // Auto-capitalize — a full-width checkbox with a secondary caption.
        let capitalize: Retained<NSButton> = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &NSString::from_str(t(
                    "Auto-capitalize first letter of each sentence",
                    "Tự động viết hoa chữ đầu câu",
                )),
                Some(self.as_ref()),
                Some(sel!(autoCapitalizeChanged:)),
                mtm,
            )
        };
        capitalize.setState(if self.state().auto_capitalize() {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        corrections.addArrangedSubview(&capitalize);

        // English word restore — opt-in resolution of the Telex/English ambiguity.
        let english: Retained<NSButton> = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &NSString::from_str(t(
                    "Restore common English words",
                    "Khôi phục từ tiếng Anh thông dụng",
                )),
                Some(self.as_ref()),
                Some(sel!(englishRestoreChanged:)),
                mtm,
            )
        };
        english.setState(if self.state().restore_english_words() {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        corrections.addArrangedSubview(&english);
        corrections.addArrangedSubview(&self.caption(
            t(
                "The blunt version: “was” stays “was”, but every syllable sharing keys with a\nlisted word (á→as, í→is, cát→cats, cả→car, hải→hair) then needs a different\nkey order. Personal Words below decides one word at a time instead, and wins\nover this.",
                "Cách thô: “was” giữ nguyên “was”, nhưng mọi âm tiết trùng phím với từ trong\ndanh sách (á→as, í→is, cát→cats, cả→car, hải→hair) sẽ phải gõ theo thứ tự\nkhác. “Từ riêng” bên dưới quyết định từng từ một, và được ưu tiên hơn.",
            ),
            mtm,
        ));

        // Personal Words — the per-word answer, directly under the global switch
        // it supersedes. Anywhere else and a user who found one would not find
        // the other.
        let words_button: Retained<NSButton> = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str(t("Personal Words…", "Từ riêng…")),
                Some(self.as_ref()),
                Some(sel!(managePersonalWords:)),
                mtm,
            )
        };
        corrections.addArrangedSubview(&words_button);
        corrections.addArrangedSubview(&self.caption(
            t(
                "Decide a single word and it stays decided — or press ⌃⇧W right after typing\none to fix it and remember the choice.",
                "Quyết định một từ và nó được giữ nguyên — hoặc bấm ⌃⇧W ngay sau khi gõ một\ntừ để sửa và ghi nhớ lựa chọn.",
            ),
            mtm,
        ));

        // Toggle hotkey — presets plus "Custom…", which arms the recorder (the
        // tap captures the next ⌃/⌥ combo; Esc, a click, or an app switch cancel).
        let hotkey_labels = NSArray::from_retained_slice(&[
            NSString::from_str("⌃⇧Space"),
            NSString::from_str("⌃Space"),
            NSString::from_str("⌥Space"),
            NSString::from_str("⌃⇧Z"),
            NSString::from_str(t("Custom…", "Tùy chọn…")),
        ]);
        let hotkey_seg: Retained<NSSegmentedControl> = unsafe {
            NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(
                &hotkey_labels,
                NSSegmentSwitchTracking::SelectOne,
                Some(self.as_ref()),
                Some(sel!(hotkeyChanged:)),
                mtm,
            )
        };
        general.addArrangedSubview(&self.form_row(
            t("Toggle key", "Phím chuyển"),
            &hotkey_seg,
            mtm,
        ));

        // Status row under the picker: "Current: ⌃⇧Space" / the recording prompt.
        let record_row = NSStackView::new(mtm);
        record_row.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
        record_row.setSpacing(8.0);
        let spacer = self.make_label("", mtm);
        let spacer_width = spacer
            .widthAnchor()
            .constraintEqualToConstant(LABEL_COLUMN_WIDTH);
        spacer_width.setActive(true);
        let hotkey_label = self.caption("", mtm);
        record_row.addArrangedSubview(&spacer);
        record_row.addArrangedSubview(&hotkey_label);
        general.addArrangedSubview(&record_row);
        *self.ivars().hotkey_seg.borrow_mut() = Some(hotkey_seg);
        *self.ivars().hotkey_label.borrow_mut() = Some(hotkey_label);
        self.refresh_hotkey_ui();

        // Group separation: a larger gap before the next section header.
        unsafe {
            let _: () = msg_send![&general, setCustomSpacing: 22.0f64, afterView: &*record_row];
        }

        // ===== Excluded apps =====
        // The list itself lives in its own window (advanced/rare) so it does not
        // clutter the everyday settings; this is just the entry point.
        apps.addArrangedSubview(&self.caption(
            t(
                "Apps where GlowKey stays off — terminals & editors by default, so it never\nmangles commands. Toggle the current app anytime with ⌃⇧E.",
                "Những ứng dụng GlowKey luôn tắt — mặc định là terminal và trình soạn thảo, để\nkhông làm hỏng câu lệnh. Bật tắt ứng dụng hiện tại bất cứ lúc nào bằng ⌃⇧E.",
            ),
            mtm,
        ));
        let manage_button: Retained<NSButton> = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str(t("Manage Excluded Apps…", "Quản lý ứng dụng loại trừ…")),
                Some(self.as_ref()),
                Some(sel!(manageExcludedApps:)),
                mtm,
            )
        };
        apps.addArrangedSubview(&manage_button);
        unsafe {
            let _: () = msg_send![&apps, setCustomSpacing: 22.0f64, afterView: &*manage_button];
        }

        // ===== Macros =====
        apps.addArrangedSubview(&self.caption(
            t(
                "Text expansion (gõ tắt): type a shortcut then a space to expand it.",
                "Gõ tắt: gõ chữ viết tắt rồi dấu cách để bung ra.",
            ),
            mtm,
        ));
        let macros_button: Retained<NSButton> = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str(t("Manage Macros…", "Quản lý gõ tắt…")),
                Some(self.as_ref()),
                Some(sel!(manageMacros:)),
                mtm,
            )
        };
        apps.addArrangedSubview(&macros_button);

        let always_macro: Retained<NSButton> = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &NSString::from_str(t(
                    "Expand macros even when Vietnamese is off",
                    "Bung gõ tắt cả khi đã tắt tiếng Việt",
                )),
                Some(self.as_ref()),
                Some(sel!(alwaysMacroChanged:)),
                mtm,
            )
        };
        always_macro.setState(if self.state().always_macro() {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        apps.addArrangedSubview(&always_macro);
        apps.addArrangedSubview(&self.caption(
            t(
                "Never in an excluded app.",
                "Không áp dụng trong ứng dụng đã loại trừ.",
            ),
            mtm,
        ));

        let tabs = NSTabView::new(mtm);
        for (title, view) in [
            (t("General", "Chung"), &general),
            (t("Typing", "Gõ phím"), &typing),
            (t("Corrections", "Sửa lỗi"), &corrections),
            (t("Apps & macros", "Ứng dụng & gõ tắt"), &apps),
        ] {
            let item = NSTabViewItem::new();
            item.setLabel(&NSString::from_str(title));
            item.setView(Some(view));
            tabs.addTabViewItem(&item);
        }
        window.setContentView(Some(&tabs));
        *self.ivars().window.borrow_mut() = Some(window);
    }
}
