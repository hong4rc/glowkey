//! The low-level keyboard hook: interception, suppression, and the message loop
//! that keeps it alive.
//!
//! # The one rule
//!
//! **Nothing in the callback may block.** `docs/decisions/0008` was written from
//! an incident where a blocking call inside the macOS tap callback froze an
//! entire machine; Windows has the same failure shape with a shorter fuse and no
//! warning. A callback that takes longer than `LowLevelHooksTimeout` (100 ms by
//! default) is not retried, not logged and not reported — the system removes the
//! hook, and GlowKey goes silently dead while its indicator still says `VI`.
//!
//! So the callback: reads the tag, translates the event, calls the policy, and
//! carries out the answer with one `SendInput`. It never resolves the foreground
//! application (that is `foreground`'s notification), never touches the settings
//! file (that is queued through `Effects`), and never waits on a lock another
//! thread holds for long.
//!
//! # The other one rule
//!
//! **Every handled key is suppressed and re-emitted**, including a plain letter
//! that only appends. There is no path where the original character lands *and* a
//! replacement is injected. On macOS, mixing native passthrough with synthesized
//! edits raced in multiprocess applications and produced `hoongf` → `hoồng`;
//! routing every mutation through one ordered queue is what fixed it, and that
//! fix is carried here rather than rediscovered.

use std::cell::{Cell, RefCell};
use std::panic::AssertUnwindSafe;
use std::time::Instant;

use glowkey_engine::Session;
use glowkey_input::hotkey;
use glowkey_input::{Ctx, Decision, Effects};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

use super::{adapt, foreground, hook_log, inject};

thread_local! {
    /// The hook's state. Thread-local rather than a global with a lock: the
    /// callback is only ever called on the thread that installed the hook, so a
    /// `RefCell` is sufficient and — more to the point — there is no lock here
    /// that another thread could be holding when a keystroke arrives.
    static STATE: RefCell<Option<HookState>> = const { RefCell::new(None) };
}

/// The installed hook handle, so it can be removed and reinstalled.
static HOOK: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Everything one keystroke needs, and nothing that needs the outside world.
pub struct HookState {
    session: Session,
    /// The application the session was last told about, so a change is only
    /// pushed when there is one.
    last_app: Option<String>,
    /// Set by the policy when something must reach the settings file. Written
    /// after the callback returns, never inside it.
    pending_save: Cell<bool>,
    /// Set when the tray no longer matches the truth. Repainted after the
    /// callback returns, for the same reason the save is deferred: painting it
    /// means `Shell_NotifyIcon`, which is a `SendMessage` to the taskbar and can
    /// wait on another process. That is not a call to make from a hook callback.
    pending_refresh: Cell<bool>,
    /// Whether the log already carries the note that the custom toggle hotkey was
    /// recorded on another platform. Once per run: it is resolved on every
    /// keystroke, and a line per keystroke is not a warning, it is a way of
    /// hiding one.
    warned_hotkey_fallback: Cell<bool>,
    /// The worst callback duration seen this run, in microseconds.
    ///
    /// The number that says whether `LowLevelHooksTimeout` is in play. A maximum
    /// in the tens of milliseconds means something that can block got into the
    /// callback, and it is the first thing to look at when the hook stops firing.
    worst_micros: Cell<u128>,
}

impl HookState {
    fn new(settings: &glowkey_engine::Settings) -> Self {
        Self {
            session: Session::from_settings(settings),
            last_app: None,
            pending_save: Cell::new(false),
            pending_refresh: Cell::new(false),
            warned_hotkey_fallback: Cell::new(false),
            worst_micros: Cell::new(0),
        }
    }
}

/// Installs the hook on the calling thread.
///
/// `WH_KEYBOARD_LL` is global but delivered to the installing thread's message
/// queue, which is why [`run_message_loop`] must follow on this same thread: with
/// no message pump the callback is never invoked and the hook is removed as
/// unresponsive.
pub fn install() -> bool {
    // Our own module handle, not null.
    //
    // A low-level hook lives in the installing process rather than a DLL, so the
    // documentation reads as though `hmod` may be null — and `SetWindowsHookExW`
    // accepts null and returns a valid handle either way. It just never calls the
    // callback. That is the worst possible failure shape for this: installation
    // reports success, the indicator says the hook is live, the WinEvent hook on
    // the same thread and the same message pump keeps working, and nothing
    // anywhere says a keystroke was missed.
    //
    // Found by a smoke test that typed into Notepad and got no KEY line at all —
    // which is exactly why `HOOK first callback received` exists below.
    // SAFETY: a handle to our own image, which is valid for the process lifetime.
    let module =
        unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null()) };
    // SAFETY: the callback matches HOOKPROC and lives for the program; a zero
    // thread id makes it global, which is what an input method needs.
    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_callback), module, 0) };
    if hook.is_null() {
        crate::log::log("HOOK FAILED to install WH_KEYBOARD_LL");
        return false;
    }
    HOOK.store(hook as isize, std::sync::atomic::Ordering::Relaxed);
    crate::log::log("HOOK installed WH_KEYBOARD_LL");
    true
}

/// Removes the hook. Idempotent.
pub fn uninstall() {
    let raw = HOOK.swap(0, std::sync::atomic::Ordering::Relaxed);
    if raw != 0 {
        // SAFETY: a handle this module installed and has not yet removed.
        unsafe { UnhookWindowsHookEx(raw as HHOOK) };
    }
}

/// Seeds the state. Called once, before the loop starts.
pub fn set_state(settings: &glowkey_engine::Settings) {
    STATE.with(|s| *s.borrow_mut() = Some(HookState::new(settings)));
}

/// Whether the hook is currently installed.
///
/// **Not proof that it is being called.** Windows can remove a slow hook without
/// telling us, and this still reports `true` afterwards — which is why the
/// indicator pairs it with a liveness check rather than trusting it alone.
#[must_use]
pub fn is_installed() -> bool {
    HOOK.load(std::sync::atomic::Ordering::Relaxed) != 0
}

/// Reads the session. For the tray and the settings window, both of which run on
/// this same thread.
///
/// Returns `None` rather than blocking if the state is already borrowed, which on
/// one thread means re-entry — a menu handler invoked from inside the callback.
/// That should not happen, and answering "I cannot tell you" is safer than a
/// panic in a message handler.
pub fn with_session<T>(f: impl FnOnce(&mut Session) -> T) -> Option<T> {
    STATE.with(|state| {
        let mut borrowed = state.try_borrow_mut().ok()?;
        let state = borrowed.as_mut()?;
        Some(f(&mut state.session))
    })
}

/// Flushes the composing word, because something moved the caret.
///
/// Called from the mouse hook. GlowKey is blind: the engine's belief about what
/// it rendered is only true while the caret has not moved, and a click moves it
/// with no keyboard event at all. Without this, typing `hoong`, clicking
/// elsewhere and typing `f` emits three backspaces against unrelated text and
/// deletes three characters the user typed themselves.
///
/// Touches only in-memory session state — no allocation, no syscall, no lock a
/// non-hook thread holds — because it runs inside a low-level hook callback,
/// where `decisions/0008` applies exactly as it does to the keyboard one.
pub fn flush_session() {
    STATE.with(|state| {
        if let Ok(mut borrowed) = state.try_borrow_mut() {
            if let Some(state) = borrowed.as_mut() {
                state.session.flush();
            }
        }
    });
}

/// Marks the settings dirty from outside the callback — a menu toggle, a settings
/// window save.
pub fn mark_dirty() {
    STATE.with(|state| {
        if let Ok(borrowed) = state.try_borrow() {
            if let Some(state) = borrowed.as_ref() {
                request_save(state);
            }
        }
    });
}

/// Runs the message loop that drives both this hook and the foreground
/// notification.
///
/// Blocks for the life of the process.
pub fn run_message_loop() {
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    // SAFETY: a standard message pump. GetMessageW returns 0 on WM_QUIT and -1
    // on error; both end the loop.
    while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        // The hook callback runs on this thread, driven by the pump above, so by
        // here it has returned and the file write is off the keystroke path —
        // which is the whole reason `Effects::save_settings` is a flag rather
        // than a write. Draining it here rather than inside the callback is what
        // keeps `decisions/0008` true: a disk write is the archetypal thing that
        // can block, and a blocked callback loses the hook.
        if let Some(settings) = take_pending_save() {
            crate::settings_store::save(&settings);
        }
        if take_pending_refresh() {
            super::shell::refresh_indicator();
        }
    }
    // Once more after the loop. `WM_QUIT` ends it without running the body, so a
    // save requested by the very keystroke that led to quitting would otherwise
    // be dropped on the way out — which is the same class of loss the wake above
    // exists to prevent, at the one moment there is no next message to rely on.
    if let Some(settings) = take_pending_save() {
        crate::settings_store::save(&settings);
    }
}

/// Whether the tray needs repainting, clearing the flag.
fn take_pending_refresh() -> bool {
    STATE.with(|state| {
        state
            .try_borrow()
            .ok()
            .and_then(|borrowed| borrowed.as_ref().map(|s| s.pending_refresh.replace(false)))
            .unwrap_or(false)
    })
}

/// The hook callback.
///
/// Wrapped in `catch_unwind` because a panic must not unwind into Win32's C
/// frames; on panic the key passes through unchanged, which is the same
/// conservative answer the macOS tap gives.
unsafe extern "system" fn hook_callback(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // The first callback of the run, recorded once, before every filter.
    //
    // This is the difference between "GlowKey decided not to transform" and
    // "GlowKey never saw the key", which are indistinguishable in a log that
    // simply has no KEY lines in it. A hook that installs but is never called is
    // a real and specific failure — a missing message pump, the wrong thread, the
    // system having removed it — and it looks exactly like a working hook on a
    // user who is not typing. Placed above the HC_ACTION check on purpose: a
    // callback arriving with an unexpected code is also worth being able to see.
    static FIRST_CALL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !FIRST_CALL.swap(true, std::sync::atomic::Ordering::Relaxed) {
        hook_log::log(format!(
            "HOOK first callback received (code={code}) — the hook is live"
        ));
    }

    let handled = std::panic::catch_unwind(AssertUnwindSafe(|| dispatch(code, wparam, lparam)))
        .unwrap_or(false);
    if handled {
        // Non-zero: swallow the key. The replacement has already been queued.
        return 1;
    }
    // SAFETY: the documented chaining call. A low-level hook passes a null handle.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

/// The callback's body. Returns `true` to suppress the key.
fn dispatch(code: i32, wparam: WPARAM, lparam: LPARAM) -> bool {
    // HC_ACTION is 0. Anything else must be passed on untouched and unexamined.
    if code != 0 {
        return false;
    }

    // SAFETY: for HC_ACTION on a keyboard hook, lparam is a KBDLLHOOKSTRUCT.
    let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };

    // ── The first act, before anything else ─────────────────────────────────
    //
    // Our own injected events, passed straight back out. Without this the hook
    // reprocesses its own injection, each pass producing more input than the
    // last, and the app melts down. Everything below this line assumes the event
    // came from a human.
    if inject::is_own_event(info.dwExtraInfo) {
        return false;
    }

    // Key-down only. WM_SYSKEYDOWN is the Alt-held variant and carries the same
    // structure; excluding it would make every Alt combination invisible to the
    // shortcut filter, which flushes.
    if wparam as u32 != WM_KEYDOWN && wparam as u32 != WM_SYSKEYDOWN {
        return false;
    }

    let started = Instant::now();
    let handled = STATE.with(|state| {
        let Ok(mut borrowed) = state.try_borrow_mut() else {
            // Re-entered. Never expected on a single thread, and passing the key
            // through is the answer that cannot corrupt the document.
            return false;
        };
        let Some(state) = borrowed.as_mut() else {
            return false;
        };
        handle_key(state, info)
    });
    record_timing(started);
    handled
}

/// One key, from translation to injection.
fn handle_key(state: &mut HookState, info: &KBDLLHOOKSTRUCT) -> bool {
    // The frontmost application, read from the cache the notification fills.
    // Never resolved here: that is a cross-process query and this is the callback.
    //
    // The notification already pushed this into the session (see
    // `shell::foreground_changed`), so this is normally a string compare that
    // changes nothing. It is kept as the backstop for the case where the
    // notification could not be installed at all, which `run()` logs and carries
    // on from — without it, GlowKey would transform in whatever application
    // happened to be in front at launch, forever.
    if let Some(app) = foreground::current() {
        if state.last_app.as_deref() != Some(app.as_str()) {
            state.session.set_frontmost_app(&app);
            state.last_app = Some(app);
        }
    }

    let key = adapt::key_event(info);

    let preset = state.session.toggle_hotkey();
    let toggle_hotkey = hotkey::resolve(preset, preset.windows_vk().map(i64::from));
    if toggle_hotkey.is_char_fallback() && !state.warned_hotkey_fallback.replace(true) {
        // Recorded on another platform: there is no Windows virtual-key code to
        // match, so it falls back to the character, which is only right while the
        // user stays on the layout they recorded it with. Said once, not once per
        // keystroke.
        hook_log::log(
            "HOTKEY the custom toggle was recorded on another platform — matching by \
             character, which depends on the keyboard layout. Re-record it here to fix."
                .into(),
        );
    }

    let mut effects = Effects::default();
    let decision = glowkey_input::decide(
        &mut state.session,
        &key,
        &Ctx { toggle_hotkey },
        &mut effects,
    );

    hook_log::log(format!(
        "KEY {:?} vk={} mods={} app={} | {}",
        key.ch,
        key.raw_code,
        adapt::modifier_names(&key.mods),
        state.last_app.as_deref().unwrap_or(""),
        describe(&decision),
    ));

    carry_out_effects(state, effects);
    carry_out(state, &decision, info)
}

/// Performs what the policy asked for.
///
/// Nothing here writes to disk. `save_settings` sets a flag that the shell
/// consumes after the callback has returned — the whole point of `Effects` being
/// plain data is that the policy can ask for a file write without the keystroke
/// path performing one.
/// The message the callback posts to wake the loop when there is a save waiting.
///
/// The hook callback is invoked by the system *during* the thread's message
/// retrieval — it does not hand `GetMessageW` a message to return. So without
/// this the drain after `DispatchMessageW` runs only when some unrelated message
/// happens along, and on a run where the user types, presses ⌃⇧W, and never
/// switches window, the corrected word is never written: the feature looks like
/// it learned, and a restart has forgotten.
///
/// `PostThreadMessageW` is a non-blocking enqueue — it does not wait on the
/// receiver — so it is one of the few Win32 calls that may be made from here.
const WM_GLOWKEY_SAVE: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 1;

/// Marks the tray out of date and wakes the loop so it gets repainted.
///
/// Deferred rather than painted here for the same reason the save is: repainting
/// calls `Shell_NotifyIcon`, which is a `SendMessage` to the taskbar and waits on
/// another process. Inside a hook callback that is a way to lose the hook.
fn request_refresh(state: &HookState) {
    state.pending_refresh.set(true);
    wake();
}

/// Marks the settings dirty and wakes the loop so the write actually happens.
fn request_save(state: &HookState) {
    state.pending_save.set(true);
    wake();
}

/// Nudges the message loop so its post-dispatch work runs.
fn wake() {
    // SAFETY: posting to our own thread. Non-blocking: it enqueues and returns
    // without waiting for the receiver.
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::PostThreadMessageW(
            windows_sys::Win32::System::Threading::GetCurrentThreadId(),
            WM_GLOWKEY_SAVE,
            0,
            0,
        );
    }
}

fn carry_out_effects(state: &HookState, effects: Effects) {
    if let Some(mode) = effects.mode_toggled {
        hook_log::log(format!("TOGGLE mode -> {mode:?}"));
    }
    if let Some((was, becomes)) = effects.corrected {
        hook_log::log(format!(
            "CORRECT {was:?} -> {becomes:?} — swapped and remembered"
        ));
    }
    if effects.save_settings {
        request_save(state);
    }
    if effects.refresh_glyph {
        // The policy says the indicator is now wrong. Without this, toggling the
        // mode with the hotkey left the tray claiming the old one until something
        // unrelated repainted it — the menu path refreshed and the hotkey path
        // did not, which is the indicator lying about the one thing the user just
        // did deliberately.
        request_refresh(state);
    }
}

/// Carries out a decision. Returns `true` to suppress the original key.
fn carry_out(state: &mut HookState, decision: &Decision, info: &KBDLLHOOKSTRUCT) -> bool {
    match decision {
        Decision::Passthrough => false,
        Decision::Consume => true,
        Decision::ToggleApp => {
            match state.last_app.clone() {
                Some(app) => {
                    let outcome = state.session.toggle_app_exclusion(&app);
                    hook_log::log(format!("TOGGLE app {app:?} -> {outcome:?}"));
                    if outcome != glowkey_engine::ExclusionToggle::EnabledSessionOnly {
                        request_save(state);
                    }
                    // The ladder returns `ToggleApp` without setting
                    // `refresh_glyph` — it cannot know the platform has an
                    // indicator — so the repaint is asked for here.
                    request_refresh(state);
                }
                // No application resolved yet — the notification has not arrived
                // and nothing has been typed. The key is still consumed (it is
                // ours), but saying nothing would make ⌃⇧E look broken rather
                // than early. macOS re-resolves at this point instead; here that
                // would be a cross-process query in the callback, which
                // `decisions/0008` forbids, so the honest answer is the log line.
                None => hook_log::log(
                    "TOGGLE app ignored — no foreground application resolved yet".into(),
                ),
            }
            true
        }
        Decision::Emit(response) => {
            inject::emit_edit(
                response.backspaces,
                &response.insert,
                state.last_app.as_deref(),
            );
            true
        }
        Decision::EmitThenReplayKey(response) => {
            inject::emit_edit(
                response.backspaces,
                &response.insert,
                state.last_app.as_deref(),
            );
            // Replayed from our own queue rather than passed through. Letting the
            // original through loses the race: it is the event being dispatched
            // right now, so the host applies it *before* the backspaces just
            // queued, and the edit eats the boundary key instead of the word it
            // meant to replace.
            inject::replay_key(info.vkCode as u16);
            true
        }
    }
}

/// Whether the settings file needs writing, clearing the flag.
///
/// Called by the shell after the callback has returned, which is what keeps the
/// file write off the keystroke path.
fn take_pending_save() -> Option<glowkey_engine::Settings> {
    STATE.with(|state| {
        let borrowed = state.try_borrow().ok()?;
        let state = borrowed.as_ref()?;
        if state.pending_save.replace(false) {
            Some(state.session.snapshot())
        } else {
            None
        }
    })
}

/// Records how long a callback took, and says so when it is slow enough to
/// matter.
///
/// Matches the macOS `EMIT took=` line. The threshold is deliberately far below
/// `LowLevelHooksTimeout`: by the time a callback actually hits 100 ms the hook is
/// already gone, so the useful warning is the one that fires while there is still
/// margin.
fn record_timing(started: Instant) {
    let micros = started.elapsed().as_micros();
    STATE.with(|state| {
        let Ok(borrowed) = state.try_borrow() else {
            return;
        };
        let Some(state) = borrowed.as_ref() else {
            return;
        };
        if micros > state.worst_micros.get() {
            state.worst_micros.set(micros);
            // Only a new worst case is logged. A line per keystroke would be
            // noise, and the number that matters is the maximum — an average
            // hides exactly the one slow call that loses the hook.
            if micros > 10_000 {
                hook_log::log(format!(
                    "EMIT took={micros}µs — a new worst case; LowLevelHooksTimeout is 100ms \
                     and a hook that reaches it is removed without warning"
                ));
            }
        }
    });
}

/// A decision, for the log.
fn describe(decision: &Decision) -> String {
    match decision {
        Decision::Passthrough => "Passthrough".to_string(),
        Decision::Consume => "Consume".to_string(),
        Decision::ToggleApp => "ToggleApp".to_string(),
        Decision::Emit(r) => format!("Emit bs={} ins={:?}", r.backspaces, r.insert),
        Decision::EmitThenReplayKey(r) => {
            format!("EmitThenReplayKey bs={} ins={:?}", r.backspaces, r.insert)
        }
    }
}
