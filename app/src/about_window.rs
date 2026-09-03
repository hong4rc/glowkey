//! A small About window — name, version, and a one-line description — like the
//! About box Unikey/EVKey ship. Built once and reused; no state, so it needs no
//! controller class.

use objc2::rc::Retained;
use objc2::{msg_send, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSColor, NSFont, NSStackView, NSTextField,
    NSUserInterfaceLayoutOrientation, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{
    MainThreadMarker, NSBundle, NSEdgeInsets, NSPoint, NSRect, NSSize, NSString,
};

use std::cell::RefCell;

/// The app's short version string from Info.plist (e.g. "0.1.0"), or "?" if absent.
fn version_string() -> String {
    NSBundle::mainBundle()
        .objectForInfoDictionaryKey(&NSString::from_str("CFBundleShortVersionString"))
        .and_then(|obj| obj.downcast::<NSString>().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "?".to_string())
}

fn build(mtm: MainThreadMarker) -> Retained<NSWindow> {
    let content = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(340.0, 180.0));
    let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable;
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
    window.setTitle(&NSString::from_str(crate::strings::t("About GlowKey", "Giới thiệu GlowKey")));
    unsafe { window.setReleasedWhenClosed(false) };

    let stack = NSStackView::new(mtm);
    stack.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
    stack.setSpacing(6.0);
    stack.setEdgeInsets(NSEdgeInsets {
        top: 24.0,
        left: 24.0,
        bottom: 24.0,
        right: 24.0,
    });
    // Center the labels (NSLayoutAttribute::CenterX == 9).
    unsafe {
        let _: () = msg_send![&stack, setAlignment: 9isize];
    }

    let name = NSTextField::labelWithString(&NSString::from_str("GlowKey"), mtm);
    name.setFont(Some(&NSFont::boldSystemFontOfSize(22.0)));
    stack.addArrangedSubview(&name);

    let version = NSTextField::labelWithString(
        &NSString::from_str(
            &crate::strings::t("Version {}", "Phiên bản {}").replace("{}", &version_string()),
        ),
        mtm,
    );
    version.setFont(Some(&NSFont::systemFontOfSize(12.0)));
    version.setTextColor(Some(&NSColor::secondaryLabelColor()));
    stack.addArrangedSubview(&version);

    let desc = NSTextField::labelWithString(
        &NSString::from_str(crate::strings::t(
            "Vietnamese Telex & VNI input for macOS",
            "Bộ gõ tiếng Việt Telex & VNI cho macOS",
        )),
        mtm,
    );
    stack.addArrangedSubview(&desc);

    let credit = NSTextField::labelWithString(
        &NSString::from_str(crate::strings::t(
            "An EVKey-style keyboard wrapper, all-Rust.",
            "Bộ gõ kiểu EVKey, viết hoàn toàn bằng Rust.",
        )),
        mtm,
    );
    credit.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    credit.setTextColor(Some(&NSColor::secondaryLabelColor()));
    stack.addArrangedSubview(&credit);

    window.setContentView(Some(&stack));
    window
}

thread_local! {
    /// The single About window, created on first open and reused after.
    static WINDOW: RefCell<Option<Retained<NSWindow>>> = const { RefCell::new(None) };
}

/// Opens (creating on first call) the About window. Called from the menu bar.
/// Discards the cached window so the next open rebuilds it. Its labels are baked
/// in at build time, so a language change would otherwise leave it in whichever
/// language it was first opened in.
pub fn invalidate() {
    WINDOW.with(|cell| {
        if let Some(window) = cell.borrow_mut().take() {
            window.orderOut(None);
        }
    });
}

pub fn show(mtm: MainThreadMarker) {
    WINDOW.with(|slot| {
        let mut slot = slot.borrow_mut();
        let window = slot.get_or_insert_with(|| build(mtm));
        window.center();
        window.makeKeyAndOrderFront(None);
    });
    NSApplication::sharedApplication(mtm).activate();
}
