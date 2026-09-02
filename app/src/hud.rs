//! A brief on-screen confirmation ("VN" / "EN") shown when the input state is
//! toggled by a hotkey, where no menu is open to give feedback. A small borderless
//! panel appears centred, then hides itself after a short delay.
//!
//! [`flash`] is a no-op off the main thread (it needs a `MainThreadMarker`), which
//! also makes it safe to call from the engine's decision path during tests — those
//! run on worker threads, so no window is ever created there.

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{define_class, msg_send, sel, ClassType, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSFont, NSTextAlignment, NSTextField, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};

use std::cell::RefCell;

/// How long the confirmation stays on screen before hiding.
const HUD_SECONDS: f64 = 0.7;

/// `NSStatusWindowLevel` — above normal windows, so the HUD is always visible.
const STATUS_WINDOW_LEVEL: isize = 25;

pub struct HudIvars {
    window: RefCell<Option<Retained<NSWindow>>>,
    label: RefCell<Option<Retained<NSTextField>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "GlowKeyHudController"]
    #[ivars = HudIvars]
    pub struct HudController;

    unsafe impl NSObjectProtocol for HudController {}

    impl HudController {
        /// Hides the HUD; fired by the delayed `performSelector` scheduled in `show`.
        #[unsafe(method(hideHud:))]
        fn hide_hud(&self, _sender: Option<&AnyObject>) {
            if let Some(window) = self.ivars().window.borrow().as_ref() {
                window.orderOut(None);
            }
        }
    }
);

impl HudController {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(HudIvars {
            window: RefCell::new(None),
            label: RefCell::new(None),
        });
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        this.build_window(mtm);
        this
    }

    fn build_window(&self, mtm: MainThreadMarker) {
        let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(160.0, 120.0));
        let window: Retained<NSWindow> = unsafe {
            let alloc = NSWindow::alloc(mtm);
            msg_send![
                alloc,
                initWithContentRect: rect,
                styleMask: NSWindowStyleMask::Borderless,
                backing: NSBackingStoreType::Buffered,
                defer: false,
            ]
        };
        unsafe {
            let _: () = msg_send![&window, setLevel: STATUS_WINDOW_LEVEL];
            window.setIgnoresMouseEvents(true);
        }

        let label = NSTextField::labelWithString(&NSString::from_str(""), mtm);
        label.setAlignment(NSTextAlignment::Center);
        let font = NSFont::systemFontOfSize(64.0);
        label.setFont(Some(&font));
        label.setFrame(rect);
        window.setContentView(Some(&label));

        *self.ivars().label.borrow_mut() = Some(label);
        *self.ivars().window.borrow_mut() = Some(window);
    }

    fn show(&self, text: &str) {
        if let Some(label) = self.ivars().label.borrow().as_ref() {
            label.setStringValue(&NSString::from_str(text));
            // The panel is fixed-width; longer texts ("VI ⚠") need a smaller font
            // than the two-letter "VI"/"EN" to fit.
            let size = if text.chars().count() <= 2 { 64.0 } else { 36.0 };
            label.setFont(Some(&NSFont::systemFontOfSize(size)));
        }
        if let Some(window) = self.ivars().window.borrow().as_ref() {
            window.center();
            window.orderFrontRegardless();
        }
        // Reset the hide timer: cancel a pending hide, then schedule a fresh one, so
        // rapid toggles keep the HUD up rather than blinking.
        unsafe {
            let class = Self::class();
            let _: () = msg_send![class, cancelPreviousPerformRequestsWithTarget: self];
            let _: () = msg_send![
                self,
                performSelector: sel!(hideHud:),
                withObject: Option::<&AnyObject>::None,
                afterDelay: HUD_SECONDS,
            ];
        }
    }
}

thread_local! {
    /// The single reused HUD controller; created on first flash, main-thread only.
    static HUD: RefCell<Option<Retained<HudController>>> = const { RefCell::new(None) };
}

/// Flashes `text` (e.g. "VN" / "EN") briefly on screen. A no-op off the main thread
/// (including in tests), which is exactly where the engine's decision path runs.
pub fn flash(text: &str) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    HUD.with(|slot| {
        let mut slot = slot.borrow_mut();
        let controller = slot.get_or_insert_with(|| HudController::new(mtm));
        controller.show(text);
    });
}
