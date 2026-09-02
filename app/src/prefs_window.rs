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
    NSApplication, NSBackingStoreType, NSButton, NSColor, NSControlStateValueOff,
    NSControlStateValueOn, NSFont, NSModalResponseOK, NSOpenPanel, NSSegmentSwitchTracking,
    NSSegmentedControl, NSStackView, NSTextAlignment, NSTextField,
    NSUserInterfaceLayoutOrientation, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{
    MainThreadMarker, NSArray, NSBundle, NSEdgeInsets, NSPoint, NSRect, NSSize, NSString, NSURL,
};

use std::cell::RefCell;

use glowkey_engine::{HotkeyPreset, InputMethod, PlacementStyle};

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

        /// Input method changed on the segmented control (0 = Telex, 1 = VNI).
        #[unsafe(method(inputMethodChanged:))]
        fn input_method_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let seg: isize = unsafe { msg_send![sender, selectedSegment] };
            let method = if seg == 0 {
                InputMethod::Telex
            } else {
                InputMethod::Vni
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
                                "Press a ⌃/⌥ combo… (Esc cancels)",
                            ));
                        }
                    }
                    return;
                }
            };
            self.state().set_toggle_hotkey_and_save(preset);
            self.refresh_hotkey_ui();
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
                "Choose apps to disable Vietnamese in.",
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
        window.setTitle(&NSString::from_str("GlowKey Settings"));
        unsafe { window.setReleasedWhenClosed(false) };

        // Outer vertical stack fills the content view. Tight rhythm within a group;
        // larger custom gaps separate the two groups (set below).
        let root = NSStackView::new(mtm);
        root.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
        root.setSpacing(6.0);
        root.setEdgeInsets(NSEdgeInsets {
            top: 20.0,
            left: 20.0,
            bottom: 20.0,
            right: 20.0,
        });
        // Leading-align arranged subviews (NSLayoutAttribute::Leading == 5).
        unsafe {
            let _: () = msg_send![&root, setAlignment: 5isize];
        }

        // ===== General =====
        root.addArrangedSubview(&self.header("General", mtm));

        let launch_at_login: Retained<NSButton> = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &NSString::from_str("Launch GlowKey at login"),
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
        root.addArrangedSubview(&launch_at_login);

        let open_at_launch: Retained<NSButton> = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &NSString::from_str("Open this window at launch"),
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
        root.addArrangedSubview(&open_at_launch);
        unsafe {
            let _: () = msg_send![&root, setCustomSpacing: 22.0f64, afterView: &*open_at_launch];
        }

        // ===== Typing =====
        root.addArrangedSubview(&self.header("Typing", mtm));

        // Input method — Telex / VNI.
        let method_labels =
            NSArray::from_retained_slice(&[NSString::from_str("Telex"), NSString::from_str("VNI")]);
        let method_seg: Retained<NSSegmentedControl> = unsafe {
            NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(
                &method_labels,
                NSSegmentSwitchTracking::SelectOne,
                Some(self.as_ref()),
                Some(sel!(inputMethodChanged:)),
                mtm,
            )
        };
        method_seg.setSelectedSegment(if self.state().input_method() == InputMethod::Telex {
            0
        } else {
            1
        });
        root.addArrangedSubview(&self.form_row("Input method", &method_seg, mtm));

        // Tone marks — aligned label + segmented control.
        let labels = NSArray::from_retained_slice(&[
            NSString::from_str("Modern  hoà"),
            NSString::from_str("Classic  hòa"),
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
        root.addArrangedSubview(&self.form_row("Tone marks", &seg, mtm));

        // Auto-fix — a full-width checkbox with a secondary caption beneath it.
        let checkbox: Retained<NSButton> = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &NSString::from_str("Auto-fix non-Vietnamese words"),
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
        root.addArrangedSubview(&checkbox);
        root.addArrangedSubview(&self.caption(
            "Restores the raw keys when the result isn’t valid Vietnamese — types “exit”, not “eĩt”.",
            mtm,
        ));

        // Auto-capitalize — a full-width checkbox with a secondary caption.
        let capitalize: Retained<NSButton> = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &NSString::from_str("Auto-capitalize first letter of each sentence"),
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
        root.addArrangedSubview(&capitalize);

        // English word restore — opt-in resolution of the Telex/English ambiguity.
        let english: Retained<NSButton> = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &NSString::from_str("Restore common English words"),
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
        root.addArrangedSubview(&english);
        root.addArrangedSubview(&self.caption(
            "For mixed English typing: “was” stays “was”, not “ứa”. Trade-off: syllables\nsharing keys with listed words (á→as, í→is, cát→cats, cả→car, hải→hair)\nthen need a different key order or the EN toggle.",
            mtm,
        ));

        // Toggle hotkey — presets plus "Custom…", which arms the recorder (the
        // tap captures the next ⌃/⌥ combo; Esc, a click, or an app switch cancel).
        let hotkey_labels = NSArray::from_retained_slice(&[
            NSString::from_str("⌃⇧Space"),
            NSString::from_str("⌃Space"),
            NSString::from_str("⌥Space"),
            NSString::from_str("⌃⇧Z"),
            NSString::from_str("Custom…"),
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
        root.addArrangedSubview(&self.form_row("Toggle key", &hotkey_seg, mtm));

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
        root.addArrangedSubview(&record_row);
        *self.ivars().hotkey_seg.borrow_mut() = Some(hotkey_seg);
        *self.ivars().hotkey_label.borrow_mut() = Some(hotkey_label);
        self.refresh_hotkey_ui();

        // Group separation: a larger gap before the next section header.
        unsafe {
            let _: () = msg_send![&root, setCustomSpacing: 22.0f64, afterView: &*record_row];
        }

        // ===== Excluded apps =====
        // The list itself lives in its own window (advanced/rare) so it does not
        // clutter the everyday settings; this is just the entry point.
        root.addArrangedSubview(&self.header("Excluded apps", mtm));
        root.addArrangedSubview(&self.caption(
            "Apps where GlowKey stays off — terminals & editors by default, so it never\nmangles commands. Toggle the current app anytime with ⌃⇧E.",
            mtm,
        ));
        let manage_button: Retained<NSButton> = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str("Manage Excluded Apps…"),
                Some(self.as_ref()),
                Some(sel!(manageExcludedApps:)),
                mtm,
            )
        };
        root.addArrangedSubview(&manage_button);
        unsafe {
            let _: () = msg_send![&root, setCustomSpacing: 22.0f64, afterView: &*manage_button];
        }

        // ===== Macros =====
        root.addArrangedSubview(&self.header("Macros", mtm));
        root.addArrangedSubview(&self.caption(
            "Text expansion (gõ tắt): type a shortcut then a space to expand it.",
            mtm,
        ));
        let macros_button: Retained<NSButton> = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str("Manage Macros…"),
                Some(self.as_ref()),
                Some(sel!(manageMacros:)),
                mtm,
            )
        };
        root.addArrangedSubview(&macros_button);

        window.setContentView(Some(&root));
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
        window.setTitle(&NSString::from_str("Excluded Apps"));
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
            "GlowKey types plain keys in these apps. Add one below, from the menu bar\n(“Disable for …”), or with ⌃⇧E while in the app.",
            mtm,
        ));
        let add_button: Retained<NSButton> = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str("Add App…"),
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
            list.addArrangedSubview(&self.caption("No apps excluded.", mtm));
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
                    &NSString::from_str("Remove"),
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
        window.setTitle(&NSString::from_str("Macros"));
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
            "Type the shortcut then a space to expand it — e.g. “vn” → “Việt Nam”.",
            mtm,
        ));

        // Input row: [shortcut] [expansion] [Add]
        let row = NSStackView::new(mtm);
        row.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
        row.setSpacing(8.0);
        let shortcut = self.input_field("shortcut", 90.0, mtm);
        let expansion = self.input_field("expansion", 210.0, mtm);
        let add: Retained<NSButton> = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str("Add"),
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
            list.addArrangedSubview(&self.caption("No macros yet.", mtm));
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
                    &NSString::from_str("Remove"),
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
            label.setStringValue(&NSString::from_str(&format!(
                "Current: {}",
                hotkey_display(preset)
            )));
        }
    }

    // --- small view helpers (type hierarchy: header / label / caption) ---

    /// A plain primary-color label.
    fn make_label(&self, text: &str, mtm: MainThreadMarker) -> Retained<NSTextField> {
        NSTextField::labelWithString(&NSString::from_str(text), mtm)
    }

    /// A bold group header (e.g. "Typing").
    fn header(&self, text: &str, mtm: MainThreadMarker) -> Retained<NSTextField> {
        let label = self.make_label(text, mtm);
        label.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
        label
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
