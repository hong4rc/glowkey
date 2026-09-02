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

use glowkey_engine::PlacementStyle;

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

        /// Auto-fix checkbox toggled.
        #[unsafe(method(autoFixChanged:))]
        fn auto_fix_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let state: isize = unsafe { msg_send![sender, state] };
            self.state().set_auto_fix_and_save(state == NSControlStateValueOn);
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
        let content = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(460.0, 420.0));
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

        // Shortcut — read-only, in the aligned form.
        let shortcut = self.value_label("⌃⇧Space   ·   turn Vietnamese on or off", mtm);
        let typing_last = self.form_row("Shortcut", &shortcut, mtm);
        root.addArrangedSubview(&typing_last);

        // Group separation: a larger gap before the next section header.
        unsafe {
            let _: () = msg_send![&root, setCustomSpacing: 22.0f64, afterView: &*typing_last];
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

    /// A regular value label (e.g. the read-only shortcut text).
    fn value_label(&self, text: &str, mtm: MainThreadMarker) -> Retained<NSTextField> {
        self.make_label(text, mtm)
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
