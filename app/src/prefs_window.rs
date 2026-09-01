//! The Settings window: a single non-resizable pane for the typing preferences
//! (tone-mark style, auto-fix) and the "Excluded apps" list, per `docs/ui-design.md`.
//!
//! Like the menu bar and tap, this is objc2 AppKit and can only be verified by
//! running. All controls are native (`NSSegmentedControl`, `NSButton`,
//! `NSTextField`, `NSStackView`), so focus, keyboard navigation, and light/dark
//! parity come for free. Every change applies to the live session and is persisted
//! immediately — there is no Apply/OK button, matching macOS Settings behaviour.
//!
//! Adding an app to the ignore list is done from the app itself (menu bar →
//! "Disable for <App>", or ⌃⇧E) — the natural place, since GlowKey already knows
//! the frontmost app there. This window lists what is excluded and lets you remove
//! entries and adjust the typing options.

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSButton, NSControlStateValueOff, NSControlStateValueOn,
    NSSegmentSwitchTracking, NSSegmentedControl, NSStackView, NSTextField,
    NSUserInterfaceLayoutOrientation, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{
    MainThreadMarker, NSArray, NSEdgeInsets, NSPoint, NSRect, NSSize, NSString,
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
        self.refresh_list();
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

        // Outer vertical stack fills the content view.
        let root = NSStackView::new(mtm);
        {
            root.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
            root.setSpacing(10.0);
            root.setEdgeInsets(NSEdgeInsets {
                top: 20.0,
                left: 20.0,
                bottom: 20.0,
                right: 20.0,
            });
        }
        // Left-align arranged subviews (NSLayoutAttribute::Leading == 5).
        unsafe {
            let _: () = msg_send![&root, setAlignment: 5isize];
        }

        // --- Typing section ---
        self.add_section_header(&root, "Typing", mtm);

        // Tone marks: Modern (hoà) / Classic (hòa).
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
        let selected = if self.state().style() == PlacementStyle::New {
            0
        } else {
            1
        };
        seg.setSelectedSegment(selected);
        self.add_row(&root, "Tone marks", &seg, mtm);

        // Auto-fix checkbox with help text.
        let checkbox: Retained<NSButton> = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &NSString::from_str("Auto-fix words that aren’t Vietnamese"),
                Some(self.as_ref()),
                Some(sel!(autoFixChanged:)),
                mtm,
            )
        };
        let state = if self.state().auto_fix() {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        };
        checkbox.setState(state);
        root.addArrangedSubview(&checkbox);
        self.add_help(&root, "Types “exit” instead of “eĩt”.", mtm);

        self.add_help(&root, "Toggle Vietnamese / English:  ⌃⇧Space", mtm);

        // --- Excluded apps section ---
        self.add_section_header(&root, "Excluded apps", mtm);
        self.add_help(
            &root,
            "GlowKey won’t type Vietnamese in these apps. Add one from its menu bar\nitem (“Disable for …”) or with ⌃⇧E while in the app.",
            mtm,
        );

        let list = NSStackView::new(mtm);
        unsafe {
            list.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
            list.setSpacing(4.0);
            let _: () = msg_send![&list, setAlignment: 5isize];
            root.addArrangedSubview(&list);
        }
        *self.ivars().list_stack.borrow_mut() = Some(list);

        window.setContentView(Some(&root));
        *self.ivars().window.borrow_mut() = Some(window);
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
            let empty = self.make_label("No apps excluded.", mtm);
            list.addArrangedSubview(&empty);
            return;
        }

        for (index, id) in ids.iter().enumerate() {
            let row = NSStackView::new(mtm);
            {
                row.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
                row.setSpacing(8.0);
            }
            let name = self.make_label(&display_name(id), mtm);
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
                row.addArrangedSubview(&remove);
                row.addArrangedSubview(&name);
                list.addArrangedSubview(&row);
            }
        }
    }

    // --- small view helpers ---

    fn make_label(&self, text: &str, mtm: MainThreadMarker) -> Retained<NSTextField> {
        NSTextField::labelWithString(&NSString::from_str(text), mtm)
    }

    fn add_section_header(&self, stack: &NSStackView, text: &str, mtm: MainThreadMarker) {
        let label = self.make_label(text, mtm);
        stack.addArrangedSubview(&label);
    }

    fn add_help(&self, stack: &NSStackView, text: &str, mtm: MainThreadMarker) {
        let label = self.make_label(text, mtm);
        stack.addArrangedSubview(&label);
    }

    fn add_row(&self, stack: &NSStackView, title: &str, control: &NSView, mtm: MainThreadMarker) {
        let row = NSStackView::new(mtm);
        {
            row.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
            row.setSpacing(8.0);
        }
        let label = self.make_label(title, mtm);
        {
            row.addArrangedSubview(&label);
            row.addArrangedSubview(control);
            stack.addArrangedSubview(&row);
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
    format!("{}{}  ({bundle_id})", first.to_uppercase(), chars.as_str())
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
