//! The Settings window: a single non-resizable pane for the typing preferences
//! (tone-mark style, auto-fix) and the "Excluded apps" list, per `docs/ui-design.md`.
//!
//! Like the menu bar and tap, this is objc2 AppKit and can only be verified by
//! running. All controls are native (`NSSegmentedControl`, `NSButton`,
//! `NSTextField`, `NSStackView`), so focus, keyboard navigation, and light/dark
//! parity come for free. Every change applies to the live session and is persisted
//! immediately — there is no Apply/OK button, matching macOS Settings behaviour.
//!
//! Apps can be added to the ignore list three ways: the "Add App…" picker here, the
//! menu bar's "Disable for <App>", or ⌃⇧E while in the app. This window lists what
//! is excluded, lets you add/remove entries, and adjusts the typing options.

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSAlert, NSSavePanel, NSTabView, NSTabViewItem,
    NSApplication, NSBackingStoreType, NSButton, NSColor, NSControlStateValueOff,
    NSControlStateValueOn, NSFont, NSModalResponseOK, NSOpenPanel, NSSegmentSwitchTracking,
    NSSegmentedControl, NSStackView, NSTextAlignment, NSTextField,
    NSUserInterfaceLayoutOrientation, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{
    MainThreadMarker, NSArray, NSBundle, NSEdgeInsets, NSPoint, NSRect, NSSize, NSString, NSURL,
};

use std::cell::RefCell;

use glowkey_engine::{HotkeyPreset, InputMethod, Language, PlacementStyle};

use crate::strings::t;
use crate::tap::TapState;

/// Ivars for the Settings window controller: the shared `TapState`, the window
/// (built lazily on first open), the vertical stack that holds the excluded-app
/// rows, and the ordered bundle-id list that maps a row's "Remove" button (by its
/// integer tag) back to the app it removes.
pub struct PrefsIvars {
    state: *const TapState,
    window: RefCell<Option<Retained<NSWindow>>>,
    /// Separate window holding the excluded-app list, so it stays off the main
    /// Settings pane (advanced/rare, opened via "Manage Excluded Apps…").
    excluded_window: RefCell<Option<Retained<NSWindow>>>,
    /// Windows replaced by a language change, held until the next rebuild.
    /// Releasing one inside the action of a control it owns would free the view
    /// tree under the AppKit frames still unwinding that click.
    retired_windows: RefCell<Vec<Retained<NSWindow>>>,
    list_stack: RefCell<Option<Retained<NSStackView>>>,
    apps: RefCell<Vec<String>>,
    /// Separate window for text-expansion macros (gõ tắt), with its input fields,
    /// list stack, and the ordered shortcuts (for remove-by-tag).
    macros_window: RefCell<Option<Retained<NSWindow>>>,
    macro_shortcut: RefCell<Option<Retained<NSTextField>>>,
    macro_expansion: RefCell<Option<Retained<NSTextField>>>,
    macros_list: RefCell<Option<Retained<NSStackView>>>,
    macro_count: RefCell<usize>,
    /// Toggle-hotkey controls, kept so the recorder can reflect a captured combo:
    /// the preset segmented control and the "Current: …" label.
    hotkey_seg: RefCell<Option<Retained<NSSegmentedControl>>>,
    hotkey_label: RefCell<Option<Retained<NSTextField>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "GlowKeyPrefsController"]
    #[ivars = PrefsIvars]
    pub struct PrefsController;

    unsafe impl NSObjectProtocol for PrefsController {}

    impl PrefsController {
        /// Tone-mark style changed on the segmented control (0 = Modern, 1 = Classic).
        #[unsafe(method(toneChanged:))]
        fn tone_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let seg: isize = unsafe { msg_send![sender, selectedSegment] };
            let style = if seg == 0 {
                PlacementStyle::New
            } else {
                PlacementStyle::Old
            };
            self.state().set_style_and_save(style);
        }

        /// Input method changed (0 = Telex, 1 = VNI, 2 = Simple Telex).
        #[unsafe(method(inputMethodChanged:))]
        fn input_method_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let seg: isize = unsafe { msg_send![sender, selectedSegment] };
            let method = match seg {
                1 => InputMethod::Vni,
                2 => InputMethod::SimpleTelex,
                _ => InputMethod::Telex,
            };
            self.state().set_input_method_and_save(method);
        }

        /// Toggle-hotkey segment clicked (0..=3 presets; 4 = "Custom…" arms the
        /// recorder — the tap captures the next ⌃/⌥ combo, Esc cancels).
        #[unsafe(method(hotkeyChanged:))]
        fn hotkey_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let seg: isize = unsafe { msg_send![sender, selectedSegment] };
            let preset = match seg {
                0 => HotkeyPreset::CtrlShiftSpace,
                1 => HotkeyPreset::CtrlSpace,
                2 => HotkeyPreset::OptionSpace,
                3 => HotkeyPreset::CtrlShiftZ,
                _ => {
                    // "Custom…": arm the recorder. The label is the prompt; the
                    // segment snaps to the real state when recording ends.
                    if !self.state().is_recording_hotkey() {
                        self.state().begin_hotkey_recording();
                        if let Some(label) = self.ivars().hotkey_label.borrow().as_ref() {
                            label.setStringValue(&NSString::from_str(
                                t("Press a ⌃/⌥ combo… (Esc cancels)", "Bấm tổ hợp ⌃/⌥… (Esc để hủy)"),
                            ));
                        }
                    }
                    return;
                }
            };
            self.state().set_toggle_hotkey_and_save(preset);
            self.refresh_hotkey_ui();
        }

        /// Interface language segment clicked (0 = System, 1 = Vietnamese, 2 = English).
        /// Rebuilds the windows, since every label in them is now the wrong language.
        #[unsafe(method(languageChanged:))]
        fn language_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let seg: isize = unsafe { msg_send![sender, selectedSegment] };
            let language = match seg {
                1 => Language::Vietnamese,
                2 => Language::English,
                _ => Language::System,
            };
            self.state().set_language_and_save(language);
            self.rebuild_windows();
        }

        /// Quick Telex checkbox toggled.
        #[unsafe(method(quickTelexChanged:))]
        fn quick_telex_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let state: isize = unsafe { msg_send![sender, state] };
            self.state()
                .set_quick_telex_and_save(state == NSControlStateValueOn);
        }

        /// "Import…" — read a macro table and merge it into the list.
        #[unsafe(method(importMacros:))]
        fn import_macros(&self, _sender: Option<&AnyObject>) {
            let mtm = MainThreadMarker::from(self);
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
                self.notify(
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
                self.notify(
                    t("Could not read that file.", "Không đọc được tệp đó."),
                    "",
                    mtm,
                );
                return;
            };
            if glowkey_engine::Macro::table_is_legacy_viqr(&text) {
                self.notify(
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
                self.notify(
                    t("No macros in that file.", "Tệp đó không có gõ tắt nào."),
                    detail,
                    mtm,
                );
                return;
            }
            let Some((added, skipped)) = self.state().import_macros_and_save(&imported) else {
                self.notify(
                    t("Could not import right now.", "Chưa nhập được lúc này."),
                    t("Try again in a moment.", "Thử lại sau một lát."),
                    mtm,
                );
                return;
            };
            self.refresh_macros();
            let detail = if skipped == 0 {
                String::new()
            } else {
                t(
                    "{} skipped — those shortcuts already exist.",
                    "Bỏ qua {} — các chữ viết tắt đó đã có.",
                )
                .replace("{}", &skipped.to_string())
            };
            self.notify(
                &t("Imported {} macros.", "Đã nhập {} gõ tắt.").replace("{}", &added.to_string()),
                &detail,
                mtm,
            );
        }

        /// "Export…" — write the current macro table to a file.
        #[unsafe(method(exportMacros:))]
        fn export_macros(&self, _sender: Option<&AnyObject>) {
            let mtm = MainThreadMarker::from(self);
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
            let macros = self.state().macros();
            let text = glowkey_engine::Macro::format_table(&macros);
            if std::fs::write(path.to_string(), text).is_err() {
                self.notify(
                    t("Could not write that file.", "Không ghi được tệp đó."),
                    "",
                    mtm,
                );
                return;
            }
            // Silence after a save reads as "nothing happened", and an empty table
            // writes an empty file, which is worth saying out loud.
            self.notify(
                &t("Exported {} macros.", "Đã xuất {} gõ tắt.")
                    .replace("{}", &macros.len().to_string()),
                if macros.is_empty() {
                    t("The list is empty, so the file is too.", "Danh sách trống nên tệp cũng trống.")
                } else {
                    ""
                },
                mtm,
            );
        }

        /// "Expand macros even when Vietnamese is off" checkbox toggled.
        #[unsafe(method(alwaysMacroChanged:))]
        fn always_macro_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let state: isize = unsafe { msg_send![sender, state] };
            self.state()
                .set_always_macro_and_save(state == NSControlStateValueOn);
        }

        /// Mid-word spell check checkbox toggled.
        #[unsafe(method(strictSpellCheckChanged:))]
        fn strict_spell_check_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let state: isize = unsafe { msg_send![sender, state] };
            self.state()
                .set_strict_spell_check_and_save(state == NSControlStateValueOn);
        }

        /// Telex bracket shortcuts checkbox toggled.
        #[unsafe(method(telexBracketsChanged:))]
        fn telex_brackets_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let state: isize = unsafe { msg_send![sender, state] };
            self.state()
                .set_telex_brackets_and_save(state == NSControlStateValueOn);
        }

        /// "Restore common English words" checkbox toggled.
        #[unsafe(method(englishRestoreChanged:))]
        fn english_restore_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let state: isize = unsafe { msg_send![sender, state] };
            self.state()
                .set_restore_english_words_and_save(state == NSControlStateValueOn);
        }

        /// Auto-fix checkbox toggled.
        #[unsafe(method(autoFixChanged:))]
        fn auto_fix_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let state: isize = unsafe { msg_send![sender, state] };
            self.state().set_auto_fix_and_save(state == NSControlStateValueOn);
        }

        /// Auto-capitalize checkbox toggled.
        #[unsafe(method(autoCapitalizeChanged:))]
        fn auto_capitalize_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let state: isize = unsafe { msg_send![sender, state] };
            self.state()
                .set_auto_capitalize_and_save(state == NSControlStateValueOn);
        }

        /// "Open at launch" checkbox toggled.
        #[unsafe(method(openAtLaunchChanged:))]
        fn open_at_launch_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let state: isize = unsafe { msg_send![sender, state] };
            self.state()
                .set_open_settings_at_launch_and_save(state == NSControlStateValueOn);
        }

        /// "Launch at login" checkbox toggled (mirrors the menu item).
        #[unsafe(method(launchAtLoginChanged:))]
        fn launch_at_login_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let state: isize = unsafe { msg_send![sender, state] };
            crate::login_item::set_enabled(state == NSControlStateValueOn);
        }

        /// Remove-app button clicked; its tag indexes the current `apps` list.
        #[unsafe(method(removeApp:))]
        fn remove_app(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let tag: isize = unsafe { msg_send![sender, tag] };
            let bundle_id = self
                .ivars()
                .apps
                .borrow()
                .get(tag as usize)
                .cloned();
            if let Some(bundle_id) = bundle_id {
                self.state().remove_exclusion_and_save(&bundle_id);
                self.refresh_list();
            }
        }

        /// Opens the separate Excluded-Apps window (built on first use).
        #[unsafe(method(manageExcludedApps:))]
        fn manage_excluded_apps(&self, _sender: Option<&AnyObject>) {
            let mtm = MainThreadMarker::from(self);
            if self.ivars().excluded_window.borrow().is_none() {
                self.build_excluded_window(mtm);
            }
            self.refresh_list();
            if let Some(window) = self.ivars().excluded_window.borrow().as_ref() {
                window.center();
                window.makeKeyAndOrderFront(None);
            }
            NSApplication::sharedApplication(mtm).activate();
        }

        /// "Add App…" — open a file picker on /Applications and disable Vietnamese
        /// for each app chosen (resolving its bundle id).
        #[unsafe(method(addApp:))]
        fn add_app(&self, _sender: Option<&AnyObject>) {
            let mtm = MainThreadMarker::from(self);
            let panel = NSOpenPanel::openPanel(mtm);
            let apps_dir = NSURL::fileURLWithPath(&NSString::from_str("/Applications"));
            panel.setCanChooseFiles(true); // .app bundles are packages (file-like)
            panel.setCanChooseDirectories(false);
            panel.setAllowsMultipleSelection(true);
            panel.setDirectoryURL(Some(&apps_dir));
            panel.setMessage(Some(&NSString::from_str(
                t("Choose apps to disable Vietnamese in.", "Chọn ứng dụng để tắt tiếng Việt."),
            )));
            let response = panel.runModal();
            if response != NSModalResponseOK {
                return;
            }
            let urls = panel.URLs();
            for url in urls.iter() {
                let bundle_id = NSBundle::bundleWithURL(&url)
                    .and_then(|b| b.bundleIdentifier())
                    .map(|s| s.to_string());
                if let Some(bundle_id) = bundle_id {
                    self.state().add_exclusion_and_save(&bundle_id);
                }
            }
            self.refresh_list();
        }

        /// Opens the separate Macros window (built on first use).
        #[unsafe(method(manageMacros:))]
        fn manage_macros(&self, _sender: Option<&AnyObject>) {
            let mtm = MainThreadMarker::from(self);
            if self.ivars().macros_window.borrow().is_none() {
                self.build_macros_window(mtm);
            }
            self.refresh_macros();
            if let Some(window) = self.ivars().macros_window.borrow().as_ref() {
                window.center();
                window.makeKeyAndOrderFront(None);
            }
            NSApplication::sharedApplication(mtm).activate();
        }

        /// "Add" — read the two macro fields, store the macro, clear the fields.
        #[unsafe(method(addMacro:))]
        fn add_macro(&self, _sender: Option<&AnyObject>) {
            let shortcut = self
                .ivars()
                .macro_shortcut
                .borrow()
                .as_ref()
                .map(|f| f.stringValue().to_string())
                .unwrap_or_default();
            let expansion = self
                .ivars()
                .macro_expansion
                .borrow()
                .as_ref()
                .map(|f| f.stringValue().to_string())
                .unwrap_or_default();
            if self.state().add_macro_and_save(&shortcut, &expansion) {
                let empty = NSString::from_str("");
                if let Some(f) = self.ivars().macro_shortcut.borrow().as_ref() {
                    f.setStringValue(&empty);
                }
                if let Some(f) = self.ivars().macro_expansion.borrow().as_ref() {
                    f.setStringValue(&empty);
                }
                self.refresh_macros();
            }
        }

        /// Remove-macro button clicked; its tag is the macro's index.
        #[unsafe(method(removeMacro:))]
        fn remove_macro(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let tag: isize = unsafe { msg_send![sender, tag] };
            self.state().remove_macro_and_save(tag as usize);
            self.refresh_macros();
        }
    }
);

impl PrefsController {
    fn state(&self) -> &TapState {
        // Safe: the pointer is to a leaked, program-lifetime TapState on the main
        // thread, the same one the tap and menu use.
        unsafe { &*self.ivars().state }
    }

    fn new(state: *const TapState, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(PrefsIvars {
            state,
            window: RefCell::new(None),
            excluded_window: RefCell::new(None),
            retired_windows: RefCell::new(Vec::new()),
            list_stack: RefCell::new(None),
            apps: RefCell::new(Vec::new()),
            macros_window: RefCell::new(None),
            macro_shortcut: RefCell::new(None),
            macro_expansion: RefCell::new(None),
            macros_list: RefCell::new(None),
            macro_count: RefCell::new(0),
            hotkey_seg: RefCell::new(None),
            hotkey_label: RefCell::new(None),
        });
        unsafe { msg_send![super(this), init] }
    }

    /// A short modal report — the only feedback an import or a failed write gets.
    /// Silence after a file operation reads as "nothing happened".
    fn notify(&self, message: &str, detail: &str, mtm: MainThreadMarker) {
        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str(message));
        if !detail.is_empty() {
            alert.setInformativeText(&NSString::from_str(detail));
        }
        alert.runModal();
    }

    /// Drops every built window so the next open rebuilds it in the current
    /// language, then reopens Settings. Labels are baked in at build time — the
    /// alternative, holding a reference to each one to re-set its title, would be
    /// a field per control for a preference changed approximately once.
    fn rebuild_windows(&self) {
        let mtm = MainThreadMarker::from(self);
        // Safe now: nothing from that generation can be mid-click.
        self.ivars().retired_windows.borrow_mut().clear();

        // Take the guards in a `let` first. Temporaries in a `for` head live for
        // the whole loop body, which would hold three `RefMut`s across `close()`
        // — and `close()` runs arbitrary AppKit code (notifications, key-window
        // transfer) that could borrow them again and panic, in frames with no
        // unwind guard. Every other borrow in this file is fallible for the same
        // reason.
        let windows = [
            self.ivars().window.borrow_mut().take(),
            self.ivars().excluded_window.borrow_mut().take(),
            self.ivars().macros_window.borrow_mut().take(),
        ];

        // Reopen whatever the user had open, rather than only Settings.
        let reopen_excluded = windows[1].as_ref().is_some_and(|w| w.isVisible());
        let reopen_macros = windows[2].as_ref().is_some_and(|w| w.isVisible());

        // Controls from the discarded windows must not be reachable any more.
        self.ivars().list_stack.replace(None);
        self.ivars().macros_list.replace(None);
        self.ivars().macro_shortcut.replace(None);
        self.ivars().macro_expansion.replace(None);
        self.ivars().hotkey_seg.replace(None);
        self.ivars().hotkey_label.replace(None);

        // Retire rather than release. This runs *from the action of a control
        // inside the window being torn down*, so dropping the last reference here
        // would free the view tree — sender and all — underneath the AppKit frames
        // still unwinding that click. `setReleasedWhenClosed(false)` opts out of
        // AppKit's own deferral, so the deferral has to be ours: the previous
        // generation was already released at the top of this function, during some
        // later event, when nothing of theirs is in flight.
        {
            let mut retired = self.ivars().retired_windows.borrow_mut();
            for window in windows.into_iter().flatten() {
                window.orderOut(None);
                retired.push(window);
            }
        }

        self.build_window(mtm);
        self.show_window();
        if reopen_excluded {
            self.manage_excluded_apps(sel!(manageExcludedApps:), None);
        }
        if reopen_macros {
            self.manage_macros(sel!(manageMacros:), None);
        }
        // The About window is built once and cached elsewhere; it would otherwise
        // keep whichever language it was first opened in.
        crate::about_window::invalidate();
    }

    /// Builds the window on first call, then refreshes the list and brings it front.
    fn show_window(&self) {
        let mtm = MainThreadMarker::from(self);
        if self.ivars().window.borrow().is_none() {
            self.build_window(mtm);
        }
        // The excluded-app list lives in its own window now, so nothing to refresh
        // here — the main pane holds only the everyday settings.
        if let Some(window) = self.ivars().window.borrow().as_ref() {
            window.center();
            window.makeKeyAndOrderFront(None);
        }
        NSApplication::sharedApplication(mtm).activate();
    }

    /// Constructs the window and its static controls once.
    fn build_window(&self, mtm: MainThreadMarker) {
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
        window.setTitle(&NSString::from_str(t("GlowKey Settings", "Cài đặt GlowKey")));
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
                &NSString::from_str(t("Open this window at launch", "Mở cửa sổ này khi khởi động")),
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
                &NSString::from_str(t("Auto-fix non-Vietnamese words", "Tự động khôi phục từ không phải tiếng Việt")),
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
                &NSString::from_str(t("Restore common English words", "Khôi phục từ tiếng Anh thông dụng")),
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
                "For mixed English typing: “was” stays “was”, not “ứa”. Trade-off: syllables\nsharing keys with listed words (á→as, í→is, cát→cats, cả→car, hải→hair)\nthen need a different key order or the EN toggle.",
                "Khi gõ lẫn tiếng Anh: “was” giữ nguyên “was”, không thành “ứa”. Đánh đổi: các\nâm tiết trùng phím với từ trong danh sách (á→as, í→is, cát→cats, cả→car,\nhải→hair) phải gõ theo thứ tự khác hoặc chuyển sang EN.",
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
        general.addArrangedSubview(&self.form_row(t("Toggle key", "Phím chuyển"), &hotkey_seg, mtm));

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

    /// Builds the separate "Excluded Apps" window on first use: a caption, the
    /// "Add App…" picker, and the app list.
    fn build_excluded_window(&self, mtm: MainThreadMarker) {
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
    fn refresh_list(&self) {
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
            list.addArrangedSubview(&self.caption(t("No apps excluded.", "Chưa có ứng dụng nào."), mtm));
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

    /// Builds the Macros window on first use: a shortcut/expansion input row and
    /// the list of existing macros.
    fn build_macros_window(&self, mtm: MainThreadMarker) {
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
    fn refresh_macros(&self) {
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
    fn input_field(
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

    /// Syncs the toggle-hotkey controls to the live preset: selects the matching
    /// segment (or clears the selection for a custom combo) and updates the
    /// "Current: …" label. Called after a preset click and after a recording ends.
    fn refresh_hotkey_ui(&self) {
        let preset = self.state().toggle_hotkey();
        if let Some(seg) = self.ivars().hotkey_seg.borrow().as_ref() {
            // Always a real segment — a SelectOne segmented control does not
            // reliably support clearing the selection with -1.
            let selected: isize = match preset {
                HotkeyPreset::CtrlShiftSpace => 0,
                HotkeyPreset::CtrlSpace => 1,
                HotkeyPreset::OptionSpace => 2,
                HotkeyPreset::CtrlShiftZ => 3,
                HotkeyPreset::Custom { .. } => 4,
            };
            seg.setSelectedSegment(selected);
        }
        if let Some(label) = self.ivars().hotkey_label.borrow().as_ref() {
            label.setStringValue(&NSString::from_str(
                &t("Current: {}", "Hiện tại: {}").replace("{}", &hotkey_display(preset)),
            ));
        }
    }

    // --- small view helpers (type hierarchy: header / label / caption) ---

    /// One tab's content stack: vertical, leading-aligned, inset from the pane.
    fn tab_stack(&self, mtm: MainThreadMarker) -> Retained<NSStackView> {
        let stack = NSStackView::new(mtm);
        stack.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
        stack.setSpacing(6.0);
        stack.setEdgeInsets(NSEdgeInsets {
            top: 18.0,
            left: 18.0,
            bottom: 18.0,
            right: 18.0,
        });
        // Leading-align arranged subviews (NSLayoutAttribute::Leading == 5).
        unsafe {
            let _: () = msg_send![&stack, setAlignment: 5isize];
        }
        stack
    }

    /// A plain primary-color label.
    fn make_label(&self, text: &str, mtm: MainThreadMarker) -> Retained<NSTextField> {
        NSTextField::labelWithString(&NSString::from_str(text), mtm)
    }

    /// A smaller secondary-color caption for explanatory text.
    fn caption(&self, text: &str, mtm: MainThreadMarker) -> Retained<NSTextField> {
        let label = self.make_label(text, mtm);
        label.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        label.setTextColor(Some(&NSColor::secondaryLabelColor()));
        label
    }

    /// One aligned form row: a fixed-width, right-aligned label followed by its
    /// control — the two-column macOS settings form. The fixed label width lines the
    /// controls up across rows.
    fn form_row(
        &self,
        title: &str,
        control: &NSView,
        mtm: MainThreadMarker,
    ) -> Retained<NSStackView> {
        let row = NSStackView::new(mtm);
        row.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
        row.setSpacing(8.0);

        let label = self.make_label(title, mtm);
        label.setAlignment(NSTextAlignment::Right);
        let width = label
            .widthAnchor()
            .constraintEqualToConstant(LABEL_COLUMN_WIDTH);
        width.setActive(true);

        row.addArrangedSubview(&label);
        row.addArrangedSubview(control);
        row
    }
}

/// Width of the right-aligned label column in the aligned form rows.
const LABEL_COLUMN_WIDTH: f64 = 92.0;

/// A human-readable rendering of a toggle-hotkey preset ("⌃⇧Space", "⌃⌥K").
fn hotkey_display(preset: HotkeyPreset) -> String {
    match preset {
        HotkeyPreset::CtrlShiftSpace => "⌃⇧Space".to_string(),
        HotkeyPreset::CtrlSpace => "⌃Space".to_string(),
        HotkeyPreset::OptionSpace => "⌥Space".to_string(),
        HotkeyPreset::CtrlShiftZ => "⌃⇧Z".to_string(),
        HotkeyPreset::Custom {
            control,
            shift,
            option,
            key_char,
            ..
        } => {
            let mut out = String::new();
            if control {
                out.push('⌃');
            }
            if option {
                out.push('⌥');
            }
            if shift {
                out.push('⇧');
            }
            if key_char == ' ' {
                out.push_str("Space");
            } else {
                out.push(key_char);
            }
            out
        }
    }
}

/// A readable name for a bundle id: the last dotted component, title-cased, since
/// GlowKey does not persist display names — only bundle ids — in settings.
fn display_name(bundle_id: &str) -> String {
    let leaf = bundle_id.rsplit('.').next().unwrap_or(bundle_id);
    if leaf.is_empty() {
        return bundle_id.to_string();
    }
    let mut chars = leaf.chars();
    let first = chars.next().unwrap();
    format!("{}{}", first.to_uppercase(), chars.as_str())
}

thread_local! {
    /// The single Settings controller, created on first open and reused after
    /// (the window is hidden, not destroyed, on close). Main-thread only.
    static CONTROLLER: RefCell<Option<Retained<PrefsController>>> = const { RefCell::new(None) };
}

/// Opens (creating on first call) the Settings window. Called from the menu bar's
/// "Settings…" item.
pub fn show(state: *const TapState, mtm: MainThreadMarker) {
    CONTROLLER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let controller = slot.get_or_insert_with(|| PrefsController::new(state, mtm));
        controller.show_window();
    });
}

/// Called by the tap when a hotkey recording ends (captured or cancelled), so the
/// Settings controls reflect the new combo. A no-op before the window exists
/// (including under tests, where no controller is installed).
pub fn hotkey_recording_done() {
    CONTROLLER.with(|slot| {
        // try_borrow: this can be reached re-entrantly if AppKit pumps the run
        // loop while show() still holds the slot mutably.
        if let Ok(slot) = slot.try_borrow() {
            if let Some(controller) = slot.as_ref() {
                controller.refresh_hotkey_ui();
            }
        }
    });
}
