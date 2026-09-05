//! The one-time welcome, shown once after the first successful Accessibility
//! grant and reopenable from the menu.
//!
//! GlowKey is an `LSUIElement` agent: no Dock icon, no window, nothing to click
//! through. Before this existed it asked for a permission, put a two-letter glyph
//! in the menu bar, and then said nothing at all — so ⌃⇧Space, ⌃⇧E and the
//! per-app ignore list, which is the reason the app exists, were left for the
//! user to discover by reading the README.
//!
//! Deliberately an `NSAlert` rather than a first-run Settings window: Settings
//! shows every control but explains none of them, and the one thing a new user
//! needs is the two keystrokes. Shown once, ever — an agent that nags is worse
//! than one that stays quiet — and the menu's "Quick Guide" reopens it, which is
//! what makes dismissing it safe rather than destructive.

use objc2_app_kit::{NSAlert, NSApplication};
use objc2_foundation::{MainThreadMarker, NSString};

use glowkey_input::HotkeyPreset;

use crate::strings::t;

/// Shows the welcome alert, modally, and returns when the user dismisses it.
///
/// `NSAlert::layout()` before `window()` is not needed here because this alert is
/// run with `runModal`, which lays itself out — unlike the startup permission
/// gate, which drives a raw modal session and therefore has to lay out by hand
/// (`docs/handoff.md` §6.5). Worth stating, because the two look alike and the
/// difference is invisible until the panel renders wrong.
///
/// The two calls before it are not optional, and both are lessons the permission
/// gate already paid for (`tap/permission.rs`):
///
/// - **`finishLaunching`** — on the already-trusted path this is the first window
///   AppKit is asked to draw, and it happens *before* `app.run()`. AppKit will
///   not put a window on screen until the app has finished launching, so without
///   this a modal alert can run while drawing nothing. Since the alert is modal
///   and sits ahead of the run loop, that would look exactly like a hang: no
///   panel, an inert status-item menu, and `welcome_shown` never saved, so it
///   would repeat on every launch.
/// - **`activate`** — GlowKey is an `LSUIElement` agent and is never the active
///   application, so an un-activated window opens *behind* whatever the user is
///   already looking at. Every other window in the app activates first; this one
///   was the only exception.
///
/// Calling `finishLaunching` twice is harmless — AppKit ignores the second — so
/// the gate's own call does not make this one redundant.
pub fn show(toggle_hotkey: HotkeyPreset, mtm: MainThreadMarker) {
    let app = NSApplication::sharedApplication(mtm);
    app.finishLaunching();
    app.activate();

    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(t(
        "GlowKey is running",
        "GlowKey đang chạy",
    )));
    // The toggle shortcut is substituted, not written out: it is configurable, and
    // the Quick Guide reopens from the menu long after the user has changed it.
    // The per-app shortcut ⌃⇧E is fixed, so it stays a literal.
    //
    // Separated by an em dash rather than padded into columns. The old text lined
    // the two up with runs of spaces, which never aligned — an NSAlert draws in a
    // proportional font — and a substituted shortcut of a different width would
    // have made the attempt visibly worse.
    let shortcut = crate::prefs::hotkey_display(toggle_hotkey);
    alert.setInformativeText(&NSString::from_str(
        &t(
            "Type Vietnamese anywhere — hoongf becomes hồng, and the tone key can go \
             anywhere in the word.\n\n\
             {} — turn Vietnamese on and off\n\
             ⌃⇧E — turn it off for just the app you are in\n\n\
             Terminals and code editors are excluded already, on purpose: synthetic \
             backspaces mangle text in a terminal. The menu-bar glyph shows VI or EN \
             for the app in front, and everything else lives in its menu.",
            "Gõ tiếng Việt ở mọi nơi — hoongf thành hồng, và dấu có thể đặt ở bất kỳ \
             đâu trong từ.\n\n\
             {} — bật/tắt tiếng Việt\n\
             ⌃⇧E — tắt riêng cho ứng dụng đang dùng\n\n\
             Terminal và trình soạn thảo mã đã được loại trừ sẵn, có chủ đích: phím \
             xoá giả lập làm hỏng văn bản trong terminal. Biểu tượng trên thanh menu \
             hiện VI hoặc EN cho ứng dụng đang ở trước, phần còn lại nằm trong menu đó.",
        )
        .replace("{}", &shortcut),
    ));
    alert.addButtonWithTitle(&NSString::from_str(t("Got it", "Đã hiểu")));
    alert.runModal();
}
