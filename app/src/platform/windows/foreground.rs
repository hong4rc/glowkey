//! What is in front, learned from a notification rather than asked on every
//! keystroke: the application, its keyboard layout, and whether we can reach it.
//!
//! **This is `docs/decisions/0008` in Windows form, and the notification path is
//! not optional.** The macOS tap once resolved the frontmost application inside
//! its callback, which is a synchronous round-trip to the window server; when the
//! window server was busy the call blocked, the callback did not return, and
//! macOS disabled the tap for timing out. While that happened every keystroke on
//! the machine was waiting on GlowKey — a frozen Mac, not a missing diacritic.
//!
//! Windows offers the identical mistake with a shorter fuse. `GetForegroundWindow`
//! plus `QueryFullProcessImageNameW` is a cross-process query, and a hook callback
//! that takes too long is removed by the system under `LowLevelHooksTimeout`
//! without a warning, an error, or a second chance. So everything expensive
//! happens here, on a `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` notification and
//! once at startup, and the keystroke path only ever reads what this left behind.
//!
//! # The locking rule
//!
//! The hook callback reads [`State`] through a mutex. That is only safe because
//! **nothing inside the critical section can wait**: the cross-process queries
//! run before the lock is taken, and logging happens after it is released.
//! Holding it across a log write would put a file flush between the keyboard and
//! the user, and would make the callback wait on this thread the moment Phase 5
//! adds another.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_INSUFFICIENT_BUFFER, HWND, MAX_PATH};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayout, HKL};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS,
};

use super::elevation::Reach;

/// What the keystroke path is allowed to know about the window in front.
#[derive(Clone)]
struct State {
    /// The lowercased executable file name — `code.exe`.
    app: String,
    /// The foreground thread's keyboard layout, as a raw handle.
    ///
    /// Stored as `isize` rather than `HKL` so the struct stays `Send`; an `HKL`
    /// is a system-owned handle with no thread affinity, and this is only ever
    /// handed back to `ToUnicodeEx`.
    layout: isize,
    /// Whether injection into it will actually arrive.
    reach: Reach,
}

impl Default for State {
    fn default() -> Self {
        Self {
            app: String::new(),
            layout: 0,
            reach: Reach::Unknown,
        }
    }
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

/// Whether the one-time startup resolution has happened.
static BOOTSTRAPPED: AtomicBool = AtomicBool::new(false);

/// Reads the shared state, recovering from a poisoned lock rather than dying.
///
/// A panic while the lock was held would otherwise make every later read fail
/// forever — and because the engine treats an unknown application as "do not
/// transform", GlowKey would quietly stop working and never say why. That is the
/// defect `docs/decisions/0007` exists to forbid, so the lock is recovered and
/// the recovery is reported once.
fn with_state<T>(f: impl FnOnce(&mut Option<State>) -> T) -> T {
    let mut guard = match STATE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            static REPORTED: AtomicBool = AtomicBool::new(false);
            if !REPORTED.swap(true, Ordering::Relaxed) {
                super::hook_log::log(
                    "FOREGROUND recovered a poisoned lock — a panic happened while it was held"
                        .into(),
                );
            }
            poisoned.into_inner()
        }
    };
    f(&mut guard)
}

/// The current foreground application, or `None` before anything is resolved.
///
/// Fail closed: the engine treats an unknown application as "do not transform",
/// which is the right answer for the instant before the first notification
/// arrives. Transforming into an application we have not identified could be
/// transforming into a terminal.
pub fn current() -> Option<String> {
    with_state(|state| {
        state
            .as_ref()
            .filter(|s| !s.app.is_empty())
            .map(|s| s.app.clone())
    })
}

/// The foreground window's keyboard layout.
///
/// Falls back to our own thread's layout before the first notification, which is
/// wrong in the same way it was always wrong — but it is a fallback for the first
/// few milliseconds of a run rather than the standing behaviour.
pub fn keyboard_layout() -> HKL {
    let cached = with_state(|state| state.as_ref().map_or(0, |s| s.layout));
    if cached != 0 {
        return cached as HKL;
    }
    // SAFETY: a plain in-process read.
    unsafe { GetKeyboardLayout(0) }
}

/// Whether injection into the window in front will arrive.
///
/// Resolved on the notification, not here — this is a cached read, safe from the
/// keystroke path. Phase 5's indicator is its consumer.
pub fn reach() -> Reach {
    with_state(|state| state.as_ref().map_or(Reach::Unknown, |s| s.reach))
}

/// Installs the foreground-change notification.
///
/// `WINEVENT_OUTOFCONTEXT` asks for the callback on our own thread through the
/// message queue rather than injecting this process into every other one, which
/// an input method has no business doing. `WINEVENT_SKIPOWNPROCESS` keeps
/// GlowKey's own settings window from counting as an application switch.
///
/// Must be called from the thread that runs the message loop: an out-of-context
/// WinEvent hook is delivered by that thread's message pump, so installing it
/// anywhere else means the notifications are never delivered and the foreground
/// silently never changes.
pub fn install() -> bool {
    // SAFETY: the callback matches WINEVENTPROC and lives for the program.
    let hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            std::ptr::null_mut(),
            Some(win_event_callback),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };
    if hook.is_null() {
        crate::log::log("FOREGROUND FAILED to install the foreground notification");
        return false;
    }
    // The bootstrap: GlowKey can start while an application is already frontmost,
    // so no notification is coming for it. One query, once — the same shape the
    // macOS side settled on.
    bootstrap();
    true
}

/// Resolves the foreground application once at startup.
fn bootstrap() {
    if BOOTSTRAPPED.swap(true, Ordering::Relaxed) {
        return;
    }
    // SAFETY: a plain query, called from the startup path and never from the hook.
    let hwnd = unsafe { GetForegroundWindow() };
    if !hwnd.is_null() {
        update(hwnd);
    }
}

/// The WinEvent callback. Runs on the message-loop thread, never on the keystroke
/// path.
unsafe extern "system" fn win_event_callback(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    // OBJID_WINDOW is 0. A foreground event for a child object is not an
    // application switch, and resolving one would replace a correct answer with a
    // less correct one.
    if event != EVENT_SYSTEM_FOREGROUND || id_object != 0 || hwnd.is_null() {
        return;
    }
    update(hwnd);
}

/// Resolves a window and records what the keystroke path needs.
///
/// Ordered deliberately: **resolve, then lock, then release, then log.** Every
/// expensive call is above the lock and every log write is below it, so the
/// critical section contains nothing that can wait. See the module note.
fn update(hwnd: HWND) {
    let Some(app) = executable_name(hwnd) else {
        return;
    };
    let mut thread = 0u32;
    // SAFETY: a valid out-pointer; the return is the thread id.
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, &mut thread) };
    // SAFETY: a plain query against another thread's layout.
    let layout = unsafe { GetKeyboardLayout(thread_id) } as isize;
    let reach = super::elevation::foreground_reach(hwnd);

    let next = State { app, layout, reach };
    let changed = with_state(|state| {
        // Compared on reach as well as name, because switching from an ordinary
        // shell to an elevated one of the same executable is a real change and
        // the one the indicator most needs to notice.
        let same = state
            .as_ref()
            .is_some_and(|s| s.app == next.app && s.reach == next.reach && s.layout == next.layout);
        if same {
            return None;
        }
        let previous = state.replace(next.clone());
        Some((previous, next))
    });

    // Outside the lock, on purpose.
    let Some((_, next)) = changed else {
        return;
    };
    super::hook_log::log(format!("FOREGROUND -> {} ({:?})", next.app, next.reach));
    if next.reach != Reach::Ok {
        // Phase 5 turns this into the `⚠` tray state and a menu line naming the
        // window. Until then it is at least in the log, because the alternative
        // is the failure being wholly invisible — the defect `decisions/0007`
        // exists to forbid.
        super::hook_log::log(format!(
            "REACH {} cannot receive injected input — typing there will silently do nothing",
            next.app
        ));
    }
}

/// The lowercased executable file name behind a window — `code.exe`, not the full
/// path.
///
/// Lowercased because that is how the shipped exclusion table spells them and
/// Windows paths are case-insensitive, so comparing them any other way would make
/// the ignore list depend on how a shortcut happened to be capitalised.
///
/// The file name rather than the Application User Model ID, and rather than the
/// full path: the AUMID is absent for a great many applications and changes
/// across installs, and a full path changes when the user moves the program. An
/// ignore list that quietly stops matching after an update is, for the one
/// feature that keeps Vietnamese out of a terminal, the worst possible failure.
pub fn executable_name(hwnd: HWND) -> Option<String> {
    let mut pid: u32 = 0;
    // SAFETY: `pid` is a valid out-pointer.
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid == 0 {
        return None;
    }

    // PROCESS_QUERY_LIMITED_INFORMATION rather than the full query right: it is
    // the one that succeeds against a process at a higher integrity level, which
    // is precisely the case we need to be able to name (see `elevation`). Asking
    // for more than is needed would turn the elevated-window case from "reported
    // honestly" back into "silently unknown".
    // SAFETY: a handle closed below on every path.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let path = full_image_path(handle);
    // SAFETY: from OpenProcess above, not used after this.
    unsafe { CloseHandle(handle) };
    Some(file_name_of(&path?))
}

/// The full image path of a process, growing the buffer if `MAX_PATH` is not
/// enough.
///
/// The retry is not theoretical. Store applications install under deep
/// `WindowsApps` paths that exceed 260 units, and without it the resolution
/// simply fails — after which the foreground stays stale and, because the engine
/// fails closed on an unknown application, GlowKey quietly stops transforming
/// with nothing in the log to say why.
fn full_image_path(handle: windows_sys::Win32::Foundation::HANDLE) -> Option<String> {
    let mut capacity = MAX_PATH as usize;
    // Two attempts: the common case, then one grown buffer. Extended-length paths
    // top out around 32767 units, so a second try at that size cannot fail for
    // want of room and there is no reason to loop further.
    for _ in 0..2 {
        let mut buf = vec![0u16; capacity];
        let mut len = capacity as u32;
        // SAFETY: `buf`/`len` are a matched buffer and capacity; `len` is updated
        // to the number of units written.
        let ok = unsafe {
            QueryFullProcessImageNameW(
                handle,
                0, // PROCESS_NAME_WIN32: a drive-letter path, not an NT device path.
                buf.as_mut_ptr(),
                &mut len,
            )
        };
        if ok != 0 {
            return Some(String::from_utf16_lossy(&buf[..len as usize]));
        }
        // SAFETY: a plain read of the calling thread's last error.
        let err = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        if err != ERROR_INSUFFICIENT_BUFFER {
            return None;
        }
        capacity = 32_768;
    }
    None
}

/// The lowercased final component of a Windows path.
///
/// Split out from the Win32 call so the rule can be tested without a process to
/// point it at. Both separators are handled: a path can come back with either,
/// and `\\?\`-prefixed long paths use backslashes throughout.
fn file_name_of(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_path_becomes_a_lowercased_file_name() {
        assert_eq!(
            file_name_of(r"C:\Program Files\Microsoft VS Code\Code.exe"),
            "code.exe"
        );
        assert_eq!(
            file_name_of(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"),
            "powershell.exe"
        );
    }

    /// The names produced here are compared against the shipped exclusion table,
    /// so the two spellings have to agree. A capitalised `Code.exe` reaching the
    /// list as-is would silently stop excluding VS Code.
    #[test]
    fn the_result_matches_how_the_shipped_table_spells_it() {
        let resolved = file_name_of(r"C:\Program Files\WindowsApps\WindowsTerminal.exe");
        assert!(
            glowkey_engine::exclusion::is_terminal(&resolved),
            "{resolved} must match the shipped terminal table"
        );
    }

    #[test]
    fn a_long_path_prefix_does_not_confuse_it() {
        assert_eq!(file_name_of(r"\\?\C:\Windows\System32\cmd.exe"), "cmd.exe");
    }

    #[test]
    fn a_bare_name_survives() {
        assert_eq!(file_name_of("Notepad.exe"), "notepad.exe");
        assert_eq!(file_name_of(""), "");
    }

    /// Before any notification the callback must get "unknown", not a wrong
    /// answer — the engine fails closed on unknown, which is what keeps
    /// Vietnamese out of a terminal GlowKey has not identified yet.
    #[test]
    fn an_empty_app_name_reads_as_unknown() {
        with_state(|state| {
            let saved = state.take();
            *state = Some(State::default());
            drop(saved);
        });
        assert_eq!(current(), None);
    }

    /// The layout falls back rather than returning a null handle, which
    /// `ToUnicodeEx` would reject.
    #[test]
    fn the_layout_falls_back_before_the_first_notification() {
        with_state(|state| *state = Some(State::default()));
        assert!(!keyboard_layout().is_null());
    }
}
