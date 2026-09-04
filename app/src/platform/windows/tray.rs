//! The tray icon and its menu — the always-resident half of the shell.
//!
//! Raw Win32, deliberately, and not `egui`. This lives for the whole run of a
//! background process that must never be the reason a machine feels slow, so it
//! is a window class, a notify-icon, and a popup menu, with no renderer behind
//! it. The settings window is the part that gets a UI toolkit, and it is created
//! on demand and destroyed on close.
//!
//! The glyph is drawn with GDI rather than loaded from four `.ico` files. Four
//! icons that must stay in sync with four states is four chances for the picture
//! to disagree with the truth, and `docs/decisions/0007` is about exactly that
//! disagreement. Drawing from the state means the tray cannot show a glyph the
//! state does not currently justify.

use std::cell::RefCell;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject,
    DrawTextW, GdiFlush, GetDC, ReleaseDC, SelectObject, SetBkMode, SetTextColor, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, DT_CENTER, DT_SINGLELINE, DT_VCENTER, FW_BOLD,
    TRANSPARENT,
};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon,
    DestroyMenu, DestroyWindow, GetCursorPos, PostQuitMessage, RegisterClassW,
    RegisterWindowMessageW, SetForegroundWindow, TrackPopupMenu, HICON, ICONINFO, MF_CHECKED,
    MF_SEPARATOR, MF_STRING, TPM_BOTTOMALIGN, TPM_RIGHTALIGN, WM_APP, WM_COMMAND, WM_DESTROY,
    WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPED,
};

use crate::strings::t;

use super::indicator::{Breakage, Indicator};

/// The message the notify-icon sends us for mouse activity.
const WM_TRAY: u32 = WM_APP + 100;

/// Menu command ids. Explicit numbers rather than an enum discriminant cast,
/// because these cross a C boundary and a silent renumbering would rewire the
/// menu without changing a line that looks like it does.
mod cmd {
    pub const TOGGLE_MODE: usize = 1;
    pub const TOGGLE_APP: usize = 2;
    pub const AUTO_FIX: usize = 3;
    pub const START_AT_LOGIN: usize = 4;
    pub const CLIPBOARD_REMOVE_TONES: usize = 5;
    pub const CLIPBOARD_UPPER: usize = 6;
    pub const CLIPBOARD_LOWER: usize = 7;
    pub const REVEAL_LOG: usize = 8;
    pub const SETTINGS: usize = 9;
    pub const REINSTALL_HOOK: usize = 10;
    pub const QUIT: usize = 11;
}

thread_local! {
    /// The tray's own window and the last state it painted.
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
}

struct Tray {
    hwnd: HWND,
    icon: HICON,
    shown: Indicator,
    /// The application named in the tooltip.
    ///
    /// Kept alongside the state because the two are not redundant: switching from
    /// one elevated window to another leaves the state at
    /// `Broken(ElevatedWindow)` while the *right answer* changes, and the tooltip
    /// is the only place the user learns **which** window cannot be typed into.
    /// Comparing state alone would leave it naming the first one forever.
    shown_app: Option<String>,
}

/// The broadcast Explorer sends when the notification area is rebuilt.
///
/// Registered once and cached; the id is per-session, not a constant.
fn taskbar_created_message() -> u32 {
    use std::sync::OnceLock;
    static MSG: OnceLock<u32> = OnceLock::new();
    *MSG.get_or_init(|| {
        let name = wide("TaskbarCreated");
        // SAFETY: a plain registration of a well-known message name.
        unsafe { RegisterWindowMessageW(name.as_ptr()) }
    })
}

/// Re-adds the icon after Explorer has restarted.
fn readd() {
    let (hwnd, icon, state, app) = TRAY.with(|t| {
        t.borrow()
            .as_ref()
            .map(|tray| (tray.hwnd, tray.icon, tray.shown, tray.shown_app.clone()))
            .unwrap_or((
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                Indicator::English,
                None,
            ))
    });
    if hwnd.is_null() {
        return;
    }
    let data = notify_data(hwnd, icon, state, app.as_deref());
    // SAFETY: re-adding an icon for a window this module owns.
    unsafe { Shell_NotifyIconW(NIM_ADD, &data) };
}

/// Creates the tray icon. Must run on the message-loop thread.
pub fn install() -> bool {
    let class = wide("GlowKeyTray");
    let mut wc: WNDCLASSW = unsafe { std::mem::zeroed() };
    wc.lpfnWndProc = Some(wnd_proc);
    wc.lpszClassName = class.as_ptr();
    // SAFETY: `wc` is fully initialised and `class` outlives the call.
    unsafe { RegisterClassW(&wc) };

    // A top-level window that is never shown — deliberately **not** a
    // message-only (`HWND_MESSAGE`) window, even though that would otherwise be
    // the natural choice for something that exists only to receive messages.
    //
    // Message-only windows do not receive broadcasts, and `TaskbarCreated` is a
    // broadcast. Using one would mean the tray icon silently never comes back
    // after Explorer restarts, which is the state `decisions/0007` forbids. So:
    // top-level, zero-sized, and never given `WS_VISIBLE`.
    // SAFETY: a standard window creation with a registered class.
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            wide("GlowKey").as_ptr(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if hwnd.is_null() {
        crate::log::log("TRAY FAILED to create the message window");
        return false;
    }

    let state = Indicator::English;
    let icon = draw_glyph(state);
    let data = notify_data(hwnd, icon, state, None);
    // SAFETY: `data` is fully initialised with its `cbSize` set.
    let ok = unsafe { Shell_NotifyIconW(NIM_ADD, &data) } != 0;
    if !ok {
        crate::log::log("TRAY FAILED to add the notify icon");
        // Released rather than leaked: this path is reachable (the shell can be
        // mid-restart) and the caller treats it as non-fatal, so the process
        // carries on and would keep both handles for its whole life.
        // SAFETY: created above and referenced by nothing else.
        unsafe {
            DestroyIcon(icon);
            DestroyWindow(hwnd);
        }
        return false;
    }
    TRAY.with(|t| {
        *t.borrow_mut() = Some(Tray {
            hwnd,
            icon,
            shown: state,
            shown_app: None,
        });
    });
    true
}

/// Repaints the tray to match `state`, if it has changed.
///
/// Cheap to call on every foreground change and every mode toggle: an unchanged
/// state does no work, and a changed one is one icon and one tooltip.
pub fn refresh(state: Indicator, app: Option<&str>) {
    // Everything is decided and the borrow released before any Win32 call.
    // `Shell_NotifyIcon` is a SendMessage to the taskbar and dispatches sent
    // messages to this thread while it waits, so holding a RefCell borrow across
    // it is one such message away from a BorrowMutError — inside a window
    // procedure, where a panic is a process abort.
    let plan = TRAY.with(|t| {
        let borrowed = t.borrow();
        let tray = borrowed.as_ref()?;
        // Compared on the named application as well as the state: the two
        // breakages share a glyph and separate only in the tooltip, so a tooltip
        // that goes stale is the indicator quietly lying again.
        if tray.shown == state && tray.shown_app.as_deref() == app {
            return None;
        }
        Some((tray.hwnd, tray.icon))
    });
    let Some((hwnd, old_icon)) = plan else {
        return;
    };

    let icon = draw_glyph(state);
    let data = notify_data(hwnd, icon, state, app);
    // SAFETY: as in `install`.
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
    // What the tray is now claiming, recorded.
    //
    // `decisions/0007` is about the indicator not lying; this is what makes the
    // claim checkable after the fact. A report of "it said VI and did nothing"
    // is otherwise impossible to separate from "it said VI correctly and
    // something else was wrong", and those have different causes.
    crate::log::log(&format!("INDICATOR {state:?} — {}", state.describe(app)));
    // The old icon is replaced, so it can go. Leaking one per state change would
    // be a slow GDI-handle leak in a process that runs for days.
    // SAFETY: created by `draw_glyph` and no longer referenced.
    unsafe { DestroyIcon(old_icon) };

    TRAY.with(|t| {
        if let Some(tray) = t.borrow_mut().as_mut() {
            tray.icon = icon;
            tray.shown = state;
            tray.shown_app = app.map(str::to_owned);
        }
    });
}

/// Removes the tray icon. Without this the ghost stays in the notification area
/// until the user hovers over it.
pub fn remove() {
    TRAY.with(|t| {
        let Some(tray) = t.borrow_mut().take() else {
            return;
        };
        let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = tray.hwnd;
        data.uID = 1;
        // SAFETY: removing an icon this module added.
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &data);
            DestroyIcon(tray.icon);
            DestroyWindow(tray.hwnd);
        }
    });
}

/// Fills in the notify-icon structure for a state.
fn notify_data(hwnd: HWND, icon: HICON, state: Indicator, app: Option<&str>) -> NOTIFYICONDATAW {
    let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = 1;
    data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    data.uCallbackMessage = WM_TRAY;
    data.hIcon = icon;
    // The tooltip is where the two breakages actually become distinguishable to
    // a user — the glyph is the same `!` for both.
    let tip = tooltip_units(&state.describe(app), data.szTip.len());
    data.szTip[..tip.len()].copy_from_slice(&tip);
    data
}

/// Fits a tooltip into a fixed `capacity` of UTF-16 units, NUL included.
///
/// Split out from [`notify_data`] so the truncation rule can be tested — the
/// version that lived inline could only be checked by a test that reimplemented
/// it, which is a test that cannot fail.
///
/// Truncation never splits a surrogate pair. An executable name can contain
/// characters outside the basic plane, and half a pair is not a character: it
/// would render as a replacement glyph in the one string whose job is to name
/// the window the user cannot type into.
fn tooltip_units(text: &str, capacity: usize) -> Vec<u16> {
    debug_assert!(capacity > 0, "a tooltip needs room for at least the NUL");
    let limit = capacity - 1;
    let mut units: Vec<u16> = text.encode_utf16().take(limit).collect();
    // A high surrogate in the last slot lost its partner to the truncation.
    if units.last().is_some_and(|u| (0xD800..0xDC00).contains(u)) {
        units.pop();
    }
    units.push(0);
    units
}

/// Draws the state's glyph into an icon.
///
/// Two colours only: full ink for an active state, grey for the dimmed
/// excluded-app one, and red for a breakage. The dimming is the entire visual
/// difference between `VI` and excluded-`VI`, which is right — they are the same
/// mode, and one of them is simply not in effect here.
fn draw_glyph(state: Indicator) -> HICON {
    const SIZE: i32 = 16;
    let text = wide(state.glyph());
    let (r, g, b) = match state {
        Indicator::Broken(_) => (0xCCu8, 0x20u8, 0x20u8), // red
        _ if state.dimmed() => (0x80, 0x80, 0x80),        // grey: on, but not here
        _ => (0xF0, 0xF0, 0xF0),                          // near-white, for a dark taskbar
    };

    // A 32-bit DIB section with an explicit alpha channel, not a
    // `CreateCompatibleBitmap`.
    //
    // Two things go wrong with the obvious version, and both are invisible on the
    // machine that wrote it:
    //
    // - `CreateCompatibleBitmap` returns **uninitialised** bits, and `ICONINFO`'s
    //   mask must be a *monochrome* bitmap. Handing the shell a screen-format
    //   colour bitmap as an AND mask is not a supported input, so what renders is
    //   decided by whatever happened to be in that memory.
    // - GDI text drawing does not touch the alpha channel. A black `FillRect`
    //   writes alpha 0 and `DrawTextW` leaves it at 0, so every pixel of a 32-bit
    //   icon is fully transparent — a blank square wherever the shell honours
    //   alpha.
    //
    // So the pixels are composed directly: opaque where the glyph covers,
    // transparent elsewhere, with an all-zero monochrome mask, which for a 32-bit
    // colour icon means "keep every pixel and use the alpha".
    // SAFETY: a standard offscreen GDI composition. Every object is released
    // before returning, and both bitmaps are copied by `CreateIconIndirect`.
    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        let dc = CreateCompatibleDC(screen);

        let mut header: BITMAPINFO = std::mem::zeroed();
        header.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        header.bmiHeader.biWidth = SIZE;
        // Negative: a top-down DIB, so row 0 is the top and the pixel maths below
        // reads the way it looks.
        header.bmiHeader.biHeight = -SIZE;
        header.bmiHeader.biPlanes = 1;
        header.bmiHeader.biBitCount = 32;
        header.bmiHeader.biCompression = BI_RGB;

        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let colour_bmp = CreateDIBSection(
            dc,
            &header,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        );
        if colour_bmp.is_null() || bits.is_null() {
            DeleteDC(dc);
            ReleaseDC(std::ptr::null_mut(), screen);
            crate::log::log("TRAY FAILED to create the icon bitmap");
            return std::ptr::null_mut();
        }
        let old = SelectObject(dc, colour_bmp.cast());

        // Draw the glyph in pure white on the zeroed (black, transparent)
        // surface. GDI antialiases, so each pixel's brightness is how much of the
        // glyph covers it — which is exactly the alpha the icon needs.
        let font = CreateFontW(
            12,
            0,
            0,
            0,
            FW_BOLD as i32,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            wide("Segoe UI").as_ptr(),
        );
        let old_font = SelectObject(dc, font.cast());
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, 0x00FF_FFFF);
        let mut rect = windows_sys::Win32::Foundation::RECT {
            left: 0,
            top: 0,
            right: SIZE,
            bottom: SIZE,
        };
        DrawTextW(
            dc,
            text.as_ptr(),
            -1,
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        SelectObject(dc, old_font);
        DeleteObject(font.cast());
        // GDI batches, so the drawing above may not have reached the buffer yet.
        // Reading the bits without this can produce an empty icon on a fast path.
        GdiFlush();

        // Turn the coverage map into premultiplied BGRA, in place.
        let pixels = bits.cast::<u32>();
        for i in 0..(SIZE * SIZE) as usize {
            // Any channel carries the coverage — the text was white on black.
            let coverage = *pixels.add(i) & 0xFF;
            // Premultiplied, which is what a 32-bit alpha icon must be.
            let pr = (u32::from(r) * coverage) / 255;
            let pg = (u32::from(g) * coverage) / 255;
            let pb = (u32::from(b) * coverage) / 255;
            *pixels.add(i) = (coverage << 24) | (pr << 16) | (pg << 8) | pb;
        }

        SelectObject(dc, old);
        DeleteDC(dc);
        ReleaseDC(std::ptr::null_mut(), screen);

        // A real monochrome mask, all zero: "keep every pixel", leaving the alpha
        // channel to decide.
        let mask_bmp = CreateBitmap(SIZE, SIZE, 1, 1, std::ptr::null());

        let mut info: ICONINFO = std::mem::zeroed();
        info.fIcon = 1;
        info.hbmMask = mask_bmp;
        info.hbmColor = colour_bmp;
        let icon = CreateIconIndirect(&info);
        // `CreateIconIndirect` copies the bitmaps, so ours are ours to free.
        DeleteObject(mask_bmp.cast());
        DeleteObject(colour_bmp.cast());
        icon
    }
}

/// The tray window's message handler.
///
/// Wrapped in `catch_unwind` for the same reason the hook callback is, and with
/// more at stake: this one reaches the whole settings-window stack, so a panic
/// anywhere in it would unwind out of an `extern "system"` function across
/// `DispatchMessageW`, which Rust defines as a process abort. GlowKey would
/// vanish — no log line, no saved settings, and the tray icon left behind as a
/// ghost until someone hovers over it.
unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let handled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        dispatch_message(hwnd, msg, wparam, lparam)
    }));
    match handled {
        Ok(result) => result,
        Err(payload) => {
            let what = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "a non-string panic".to_string());
            crate::log::log(&format!("TRAY panic in the window procedure: {what}"));
            // SAFETY: the documented default handler; the safe answer for a
            // message we failed to process.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }
}

/// The handler's body, separated so it can be wrapped above.
fn dispatch_message(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // Explorer restarting rebuilds the notification area and every icon has to
    // add itself back. Without this the tray silently disappears for the rest of
    // the run while the hook keeps transforming keys — working software with no
    // visible truth about its state, which is what `decisions/0007` forbids.
    if msg == taskbar_created_message() {
        crate::log::log("TRAY the taskbar restarted — re-adding the icon");
        readd();
        return 0;
    }
    match msg {
        WM_TRAY if lparam as u32 == WM_RBUTTONUP => {
            show_menu(hwnd);
            0
        }
        WM_COMMAND => {
            handle_command(wparam & 0xFFFF);
            0
        }
        WM_DESTROY => {
            // SAFETY: ending our own message loop.
            unsafe { PostQuitMessage(0) };
            0
        }
        // SAFETY: the documented default handler.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Builds and shows the popup menu, mirroring the macOS one.
fn show_menu(hwnd: HWND) {
    // `TrackPopupMenu` runs a nested message loop, so a second right-click while
    // the menu is open re-enters this function and stacks another menu on top of
    // the first. Guarded rather than tolerated: the stacked menus have to be
    // dismissed one at a time, which reads as the tray being stuck.
    thread_local! {
        static SHOWING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if SHOWING.with(|s| s.replace(true)) {
        return;
    }
    // Cleared on every path out, including the early return below.
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            SHOWING.with(|s| s.set(false));
        }
    }
    let _guard = Guard;

    let Some(snapshot) = super::shell::snapshot() else {
        return;
    };
    // SAFETY: a menu created, shown and destroyed within this function.
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }

        // The breakage, first and unmissable, when there is one. A user whose
        // GlowKey is dead should not have to read past four toggles to find out.
        if let Indicator::Broken(cause) = snapshot.indicator {
            item(
                menu,
                MF_STRING,
                0,
                &snapshot.indicator.describe(snapshot.app.as_deref()),
            );
            if cause == Breakage::HookGone {
                item(
                    menu,
                    MF_STRING,
                    cmd::REINSTALL_HOOK,
                    t("Reinstall the keyboard hook", "Cài lại bộ bắt phím"),
                );
            }
            separator(menu);
        }

        item(
            menu,
            if snapshot.mode_is_vietnamese {
                MF_CHECKED
            } else {
                MF_STRING
            },
            cmd::TOGGLE_MODE,
            t("Vietnamese input", "Gõ tiếng Việt"),
        );
        item(
            menu,
            if snapshot.app_excluded {
                MF_STRING
            } else {
                MF_CHECKED
            },
            cmd::TOGGLE_APP,
            &match snapshot.app.as_deref() {
                Some(app) => t("Vietnamese in {}", "Gõ tiếng Việt trong {}").replace("{}", app),
                None => t("Vietnamese in this app", "Gõ tiếng Việt trong ứng dụng này").to_string(),
            },
        );
        item(
            menu,
            if snapshot.auto_fix {
                MF_CHECKED
            } else {
                MF_STRING
            },
            cmd::AUTO_FIX,
            t("Auto-fix English words", "Tự động sửa từ tiếng Anh"),
        );
        separator(menu);
        item(
            menu,
            if super::startup::is_enabled() {
                MF_CHECKED
            } else {
                MF_STRING
            },
            cmd::START_AT_LOGIN,
            t("Start at login", "Khởi động cùng máy"),
        );
        separator(menu);
        item(
            menu,
            MF_STRING,
            cmd::CLIPBOARD_REMOVE_TONES,
            t("Clipboard: remove tones", "Clipboard: bỏ dấu"),
        );
        item(
            menu,
            MF_STRING,
            cmd::CLIPBOARD_UPPER,
            t("Clipboard: UPPERCASE", "Clipboard: CHỮ HOA"),
        );
        item(
            menu,
            MF_STRING,
            cmd::CLIPBOARD_LOWER,
            t("Clipboard: lowercase", "Clipboard: chữ thường"),
        );
        separator(menu);
        item(
            menu,
            MF_STRING,
            cmd::REVEAL_LOG,
            t("Show log folder", "Mở thư mục nhật ký"),
        );
        item(menu, MF_STRING, cmd::SETTINGS, t("Settings…", "Cài đặt…"));
        item(
            menu,
            MF_STRING,
            cmd::QUIT,
            t("Quit GlowKey", "Thoát GlowKey"),
        );

        let mut point = POINT { x: 0, y: 0 };
        GetCursorPos(&mut point);
        // Required before TrackPopupMenu, or the menu does not dismiss when the
        // user clicks elsewhere — it just sits there.
        SetForegroundWindow(hwnd);
        TrackPopupMenu(
            menu,
            TPM_RIGHTALIGN | TPM_BOTTOMALIGN,
            point.x,
            point.y,
            0,
            hwnd,
            std::ptr::null(),
        );
        DestroyMenu(menu);
    }
}

unsafe fn item(
    menu: windows_sys::Win32::UI::WindowsAndMessaging::HMENU,
    flags: u32,
    id: usize,
    text: &str,
) {
    let text = wide(text);
    // SAFETY: `menu` is a live menu and `text` outlives the call.
    unsafe { AppendMenuW(menu, MF_STRING | flags, id, text.as_ptr()) };
}

unsafe fn separator(menu: windows_sys::Win32::UI::WindowsAndMessaging::HMENU) {
    // SAFETY: as above.
    unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null()) };
}

/// Runs one menu command.
fn handle_command(id: usize) {
    match id {
        cmd::TOGGLE_MODE => super::shell::toggle_mode(),
        cmd::TOGGLE_APP => super::shell::toggle_current_app(),
        cmd::AUTO_FIX => super::shell::toggle_auto_fix(),
        cmd::START_AT_LOGIN => {
            let now = super::startup::is_enabled();
            super::startup::set_enabled(!now);
        }
        cmd::CLIPBOARD_REMOVE_TONES => {
            super::clipboard::remove_tones();
        }
        cmd::CLIPBOARD_UPPER => {
            super::clipboard::uppercase();
        }
        cmd::CLIPBOARD_LOWER => {
            super::clipboard::lowercase();
        }
        cmd::REVEAL_LOG => super::shell::reveal_log(),
        cmd::SETTINGS => super::shell::open_settings(),
        cmd::REINSTALL_HOOK => super::shell::reinstall_hook(),
        cmd::QUIT => {
            remove();
            // SAFETY: ending our own message loop.
            unsafe { PostQuitMessage(0) };
        }
        _ => {}
    }
}

/// A NUL-terminated UTF-16 string, which is what every `…W` entry point takes.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every command id is distinct. They cross a C boundary as plain integers,
    /// so a duplicate would silently wire two menu entries to one action and the
    /// compiler would say nothing.
    #[test]
    fn every_command_id_is_unique() {
        let ids = [
            cmd::TOGGLE_MODE,
            cmd::TOGGLE_APP,
            cmd::AUTO_FIX,
            cmd::START_AT_LOGIN,
            cmd::CLIPBOARD_REMOVE_TONES,
            cmd::CLIPBOARD_UPPER,
            cmd::CLIPBOARD_LOWER,
            cmd::REVEAL_LOG,
            cmd::SETTINGS,
            cmd::REINSTALL_HOOK,
            cmd::QUIT,
        ];
        let mut sorted = ids.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "two menu entries share an id");
        // Zero is the "no command" value the informational breakage line uses, so
        // nothing real may claim it.
        assert!(
            !ids.contains(&0),
            "0 is reserved for the non-clickable line"
        );
    }

    /// The tooltip has a fixed 128-unit field. A long executable name in the
    /// elevated-window message must be truncated rather than overrun it.
    /// The tooltip has a fixed field, and an over-long executable name must be
    /// truncated to fit rather than overrun it.
    ///
    /// The previous version of this test built the expected value with the same
    /// `take(127)` the code used and then asserted the result was at most 127 —
    /// a tautology that never touched `notify_data` and could not have caught a
    /// truncation bug. This one calls the real function.
    #[test]
    fn a_long_tooltip_is_truncated_to_fit() {
        let long = "a".repeat(500) + ".exe";
        let described = Indicator::Broken(Breakage::ElevatedWindow).describe(Some(&long));
        assert!(
            described.len() > 128,
            "the case only bites when it overflows"
        );

        let units = tooltip_units(&described, 128);
        assert!(units.len() <= 128, "must fit the field");
        assert_eq!(units.last(), Some(&0), "must stay NUL-terminated");
    }

    /// Truncation must not cut a surrogate pair in half.
    ///
    /// Half a pair is not a character; it renders as a replacement glyph in the
    /// one string whose job is to name the window the user cannot type into.
    #[test]
    fn truncation_never_splits_a_surrogate_pair() {
        // Each emoji is two UTF-16 units, so an odd capacity forces the cut to
        // land mid-pair unless it is handled.
        let text = "\u{1F600}".repeat(10);
        for capacity in 2..=20 {
            let units = tooltip_units(&text, capacity);
            assert!(units.len() <= capacity);
            assert_eq!(units.last(), Some(&0));
            // Everything before the NUL must decode.
            assert!(
                String::from_utf16(&units[..units.len() - 1]).is_ok(),
                "capacity {capacity} left a lone surrogate"
            );
        }
    }

    /// A short tooltip is untouched apart from the terminator.
    #[test]
    fn a_short_tooltip_is_left_alone() {
        let units = tooltip_units("VI", 128);
        assert_eq!(units, vec![u16::from(b'V'), u16::from(b'I'), 0]);
    }
}
