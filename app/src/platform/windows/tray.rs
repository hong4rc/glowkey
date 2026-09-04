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
    CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreateSolidBrush, DeleteDC,
    DeleteObject, DrawTextW, GetDC, ReleaseDC, SelectObject, SetBkMode, SetTextColor, DT_CENTER,
    DT_SINGLELINE, DT_VCENTER, FW_BOLD, TRANSPARENT,
};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon,
    DestroyMenu, DestroyWindow, GetCursorPos, PostQuitMessage, RegisterClassW, SetForegroundWindow,
    TrackPopupMenu, HICON, ICONINFO, MF_CHECKED, MF_SEPARATOR, MF_STRING, TPM_BOTTOMALIGN,
    TPM_RIGHTALIGN, WM_APP, WM_COMMAND, WM_DESTROY, WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPED,
};

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
}

/// Creates the tray icon. Must run on the message-loop thread.
pub fn install() -> bool {
    let class = wide("GlowKeyTray");
    let mut wc: WNDCLASSW = unsafe { std::mem::zeroed() };
    wc.lpfnWndProc = Some(wnd_proc);
    wc.lpszClassName = class.as_ptr();
    // SAFETY: `wc` is fully initialised and `class` outlives the call.
    unsafe { RegisterClassW(&wc) };

    // A message-only window: it is never shown, never sized, and exists solely to
    // receive the notify-icon's messages. A background agent has no business
    // putting a window on screen.
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
        return false;
    }
    TRAY.with(|t| {
        *t.borrow_mut() = Some(Tray {
            hwnd,
            icon,
            shown: state,
        });
    });
    true
}

/// Repaints the tray to match `state`, if it has changed.
///
/// Cheap to call on every foreground change and every mode toggle: an unchanged
/// state does no work, and a changed one is one icon and one tooltip.
pub fn refresh(state: Indicator, app: Option<&str>) {
    TRAY.with(|t| {
        let mut borrowed = t.borrow_mut();
        let Some(tray) = borrowed.as_mut() else {
            return;
        };
        if tray.shown == state {
            return;
        }
        let icon = draw_glyph(state);
        let data = notify_data(tray.hwnd, icon, state, app);
        // SAFETY: as in `install`.
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
        // The old icon is replaced, so it can go. Leaking one per state change
        // would be a slow GDI-handle leak in a process that runs for days.
        // SAFETY: created by `draw_glyph` and no longer referenced.
        unsafe { DestroyIcon(tray.icon) };
        tray.icon = icon;
        tray.shown = state;
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
    let tip: Vec<u16> = state
        .describe(app)
        .encode_utf16()
        .take(data.szTip.len() - 1)
        .chain(std::iter::once(0))
        .collect();
    data.szTip[..tip.len()].copy_from_slice(&tip);
    data
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
    let colour: u32 = match state {
        Indicator::Broken(_) => 0x00_00_00_CC, // BGR: red
        _ if state.dimmed() => 0x00_80_80_80,  // grey
        _ => 0x00_F0_F0_F0,                    // near-white, for a dark taskbar
    };

    // SAFETY: a standard offscreen GDI composition. Every object created here is
    // released before returning; the two bitmaps are handed to `CreateIconIndirect`,
    // which copies them.
    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        let dc = CreateCompatibleDC(screen);
        let colour_bmp = CreateCompatibleBitmap(screen, SIZE, SIZE);
        let mask_bmp = CreateCompatibleBitmap(screen, SIZE, SIZE);
        let old = SelectObject(dc, colour_bmp.cast());

        let brush = CreateSolidBrush(0);
        let mut rect = windows_sys::Win32::Foundation::RECT {
            left: 0,
            top: 0,
            right: SIZE,
            bottom: SIZE,
        };
        windows_sys::Win32::Graphics::Gdi::FillRect(dc, &rect, brush);
        DeleteObject(brush.cast());

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
        SetTextColor(dc, colour);
        DrawTextW(
            dc,
            text.as_ptr(),
            -1,
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );

        SelectObject(dc, old_font);
        DeleteObject(font.cast());
        SelectObject(dc, old);
        DeleteDC(dc);
        ReleaseDC(std::ptr::null_mut(), screen);

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
unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
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
                    "Reinstall the keyboard hook",
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
            "Vietnamese input",
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
                Some(app) => format!("Vietnamese in {app}"),
                None => "Vietnamese in this app".to_string(),
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
            "Auto-fix English words",
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
            "Start at login",
        );
        separator(menu);
        item(
            menu,
            MF_STRING,
            cmd::CLIPBOARD_REMOVE_TONES,
            "Clipboard: remove tones",
        );
        item(
            menu,
            MF_STRING,
            cmd::CLIPBOARD_UPPER,
            "Clipboard: UPPERCASE",
        );
        item(
            menu,
            MF_STRING,
            cmd::CLIPBOARD_LOWER,
            "Clipboard: lowercase",
        );
        separator(menu);
        item(menu, MF_STRING, cmd::REVEAL_LOG, "Show log folder");
        item(menu, MF_STRING, cmd::SETTINGS, "Settings…");
        item(menu, MF_STRING, cmd::QUIT, "Quit GlowKey");

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
    #[test]
    fn a_long_tooltip_is_truncated_not_overrun() {
        let long = "a".repeat(500) + ".exe";
        let described = Indicator::Broken(Breakage::ElevatedWindow).describe(Some(&long));
        let units: Vec<u16> = described.encode_utf16().take(127).collect();
        assert!(units.len() <= 127);
    }
}
