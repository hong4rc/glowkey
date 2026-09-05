//! The CGEventTap shell: GlowKey as a background agent that wraps the active
//! keyboard layout with Vietnamese Telex, like EVKey and OpenKey.
//!
//! ## How it works
//!
//! A `CGEventTap` intercepts key-down events *after* the system keyboard layout has
//! mapped them, so the user's Colemak/US layout stays in effect and GlowKey sees
//! the already-mapped character. GlowKey **suppresses every letter it handles** and
//! re-emits the engine's `(backspaces, insert)` diff — a plain append re-emits the
//! character, a transform posts N backspaces then the new Vietnamese text. There is
//! no marked text: every keystroke is written straight to the document.
//!
//! Suppressing *every* letter (rather than passing plain keys through) is what makes
//! the output deterministic. A natively-typed character and a synthesized backspace
//! posted a moment later reach the document out of order in multiprocess apps
//! (Chrome/Edge), so mixing the two races — the first transform after a letter lands
//! wrong (`aa` → `aâ`, `hoongf` → `hoồng`). Routing every mutation through the one
//! tagged source's single `CGEventPost` queue removes the race by construction.
//!
//! Synthesized events are posted at the **session level** (`CGEventPost`), the
//! normal input path, so multi-process apps like Chrome route them to the focused
//! renderer correctly. GlowKey tags its own event source and skips events carrying
//! that tag in the tap, which prevents a feedback loop; a latching circuit breaker
//! caps any runaway (should self-identification ever fail) rather than letting it
//! flood the input system. Set `GLOWKEY_DEBUG=1` to log each emit.
//!
//! ## Constraints (inherent to the event-tap approach, same as EVKey)
//!
//! - Requires an Accessibility permission grant. Without it the tap cannot be
//!   created and GlowKey stays inert.
//! - Does not work in secure input fields (passwords): macOS withholds those
//!   events from all event taps.
//!
//! The decision itself is not here at all: it lives in `glowkey-input`, with no
//! operating system in it, and this module translates into and out of it (see
//! [`adapt`] and [`dispatch`]). The tests below still drive the whole path with
//! real `CGEvent`s, which is what proves the translation.
//! Only the system-level parts — installing the tap, delivering synthesized events
//! to an app — need Accessibility and a live session to verify. See
//! `docs/checkpoint.md`.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::panic::AssertUnwindSafe;
use std::ptr::NonNull;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use glowkey_engine::Session;

use crate::prefs_model::Settings;
use crate::session_adapter::{session_from, settings_from};
use objc2_core_foundation::{CFRetained, CFRunLoop};
use objc2_core_graphics::{
    CGEvent, CGEventMask, CGEventSource, CGEventSourceStateID, CGEventTapProxy, CGEventType,
};

mod adapt;
mod dispatch;
mod emit;
mod health;
mod permission;
mod settings;
#[cfg(test)]
mod tests;

pub use health::tap_is_dead;
pub use permission::open_accessibility_settings;

/// The policy's verdict on one key. Re-exported so the adapter's tests can name
/// it without reaching across crates.
#[cfg(test)]
use glowkey_input::Decision;

use emit::{frontmost_bundle_id, is_own_event, own_bundle_id};
use health::{create_tap, install_health_timer};
use permission::{accessibility_trusted, wait_for_accessibility};

/// User-data tag on GlowKey's own event source. Synthesized events are posted at
/// the session level (so multi-process apps like Chrome route them to the focused
/// renderer correctly); the tap reads each event's source user-data and skips its
/// own, which is the documented way to avoid a feedback loop.
const GLOWKEY_TAG: i64 = 0x47_4C_4F_57; // "GLOW"

/// Latched if emits ever exceed a human typing rate — a runaway (e.g. if self-ID
/// ever fails). Caps a flood at [`RUNAWAY_LIMIT`] events, then stops until restart.
static DISABLED: AtomicBool = AtomicBool::new(false);

/// Circuit-breaker thresholds: more emits than this within the window is a runaway,
/// not human typing (a person tops out around 20 keystrokes/second).
const RUNAWAY_LIMIT: usize = 60;
const RUNAWAY_WINDOW: Duration = Duration::from_millis(1000);

/// Whether `GLOWKEY_DEBUG` is set — enables per-emit logging for diagnosing
/// delivery issues in specific apps.
fn debug_enabled() -> bool {
    use std::sync::OnceLock;
    static DEBUG: OnceLock<bool> = OnceLock::new();
    *DEBUG.get_or_init(|| std::env::var_os("GLOWKEY_DEBUG").is_some())
}

/// Long-lived shell state, referenced from the C tap callback via a raw pointer.
/// The callback runs on the main run loop thread, so a `RefCell` is sufficient —
/// no cross-thread access.
pub(crate) struct TapState {
    session: RefCell<Session>,
    /// The product-only preferences the session does not hold (language,
    /// launch flags, the hotkey preset), kept so a save writes the whole file.
    prefs: RefCell<Settings>,
    last_bundle_id: RefCell<Option<String>>,
    /// The tagged event source all synthesized events are created from.
    source: CFRetained<CGEventSource>,
    /// Recent emit timestamps, for the runaway circuit breaker.
    recent_emits: RefCell<VecDeque<Instant>>,
    /// True while the Settings window is recording a custom toggle hotkey: the
    /// next key-down with ⌃ or ⌥ becomes the hotkey; Escape cancels.
    recording_hotkey: RefCell<bool>,
    /// Set by `decide` when it changed something that must reach the settings
    /// file, and consumed by `handle_key_down`.
    ///
    /// `decide` is deliberately free of disk side effects — that is what lets the
    /// tests drive it with real events without writing to the user's real
    /// settings file — so it cannot save for itself. Without this the correction
    /// hotkey recorded its decision in memory only, and every word the user
    /// taught GlowKey was lost at quit: the feature looked like it had learned,
    /// the Personal Words window even showed the word, and a restart forgot all
    /// of it.
    pending_save: Cell<bool>,
    /// When the tap last handled a key. A keystroke arriving *is* proof the tap
    /// is alive, so the health monitor can skip its window-server round-trip
    /// while someone is typing — see `health::check_tap_health`.
    last_key_at: Cell<Option<Instant>>,
    /// Whether the log already carries the note that the custom toggle hotkey is
    /// being matched by character because it was recorded on another platform.
    /// Once per run: it is resolved on every keystroke, and a line per keystroke
    /// is not a warning, it is a way of hiding one.
    warned_hotkey_fallback: Cell<bool>,
}

impl TapState {
    /// Builds state with default settings (used in tests).
    #[cfg(test)]
    fn new() -> Option<Self> {
        Self::from_settings(&Settings::default())
    }

    /// Builds state with a session configured from persisted settings.
    fn from_settings(settings: &Settings) -> Option<Self> {
        let source = CGEventSource::new(CGEventSourceStateID::Private)?;
        CGEventSource::set_user_data(Some(&source), GLOWKEY_TAG);
        Some(Self {
            session: RefCell::new(session_from(settings)),
            prefs: RefCell::new(settings.clone()),
            last_bundle_id: RefCell::new(None),
            source,
            recent_emits: RefCell::new(VecDeque::new()),
            recording_hotkey: RefCell::new(false),
            pending_save: Cell::new(false),
            last_key_at: Cell::new(None),
            warned_hotkey_fallback: Cell::new(false),
        })
    }

    /// Snapshots the current session and writes it to the settings file. Called
    /// after any user-driven change (menu toggle, preference edit).
    pub fn save_settings(&self) {
        if let Some(settings) = self.snapshot() {
            crate::settings_store::save(&settings);
        }
    }

    /// The whole preferences file as it stands now: the session's state over
    /// the product-only fields. `None` while either is busy.
    pub fn snapshot(&self) -> Option<Settings> {
        let session = self.session.try_borrow().ok()?;
        let prefs = self.prefs.try_borrow().ok()?;
        Some(settings_from(&session, &prefs))
    }

    /// Records the frontmost application on the session, so the ignore list and
    /// VN/EN state reflect an app switch immediately (not only at the next word
    /// start). Called by the menu controller's app-activation observer. Switching
    /// to another app also cancels an armed hotkey recording — the recorder only
    /// makes sense while GlowKey's own Settings window is in front.
    pub fn set_frontmost_app(&self, bundle_id: &str) {
        if own_bundle_id().as_deref() != Some(bundle_id) {
            self.cancel_hotkey_recording();
        }
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.set_frontmost_app(bundle_id);
        }
        *self.last_bundle_id.borrow_mut() = Some(bundle_id.to_string());
    }

    /// Whether Vietnamese is currently active (Vietnamese mode and the frontmost
    /// app not excluded) — drives the menu bar glyph.
    pub fn is_active(&self) -> bool {
        self.session
            .try_borrow()
            .map(|s| s.is_active())
            .unwrap_or(false)
    }

    /// Whether Vietnamese is switched on *as a mode*, regardless of whether the
    /// app in front is excluded.
    ///
    /// [`is_active`](Self::is_active) collapses the two, which is right for
    /// deciding whether to transform a key and wrong for the indicator: the glyph
    /// said `EN` both when the user had turned Vietnamese off and when the app in
    /// front was on the ignore list. Those are the two things a user most needs to
    /// tell apart, and the ignore list is the feature this app exists for.
    pub fn mode_is_vietnamese(&self) -> bool {
        self.session
            .try_borrow()
            .map(|s| s.mode() == glowkey_engine::InputMode::Vietnamese)
            .unwrap_or(false)
    }
    /// Flushes the in-progress word — the engine's edits assume the composing word
    /// is still the document tail, so this must run when the caret may have moved
    /// (a mouse click). A click also cancels an armed hotkey recording: the user
    /// has moved on, and a forgotten recorder must not capture a later ⌃/⌥ combo.
    fn flush(&self) {
        self.cancel_hotkey_recording();
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.flush();
        }
    }
    /// Resolves the frontmost app at a word start (not mid-word) and, on a change,
    /// tells the session — keeping the ignore list honest without a per-keystroke
    /// workspace query. Separated from [`decide`](Self::decide) so the decision
    /// logic is a pure function of the event and session state, and testable.
    /// Re-reads the frontmost application, for the health timer to call when the
    /// user is not typing.
    ///
    /// Deliberately never called from the keystroke path: it is a synchronous
    /// window-server round-trip, and blocking there is what makes macOS disable
    /// the tap for timing out.
    pub(super) fn refresh_frontmost_if_idle(&self) {
        let Some(bundle_id) = frontmost_bundle_id() else {
            return;
        };
        if self.last_bundle_id.borrow().as_deref() == Some(bundle_id.as_str()) {
            return;
        }
        if let Ok(mut session) = self.session.try_borrow_mut() {
            session.set_frontmost_app(bundle_id.clone());
        }
        *self.last_bundle_id.borrow_mut() = Some(bundle_id);
    }

    fn refresh_frontmost_at_word_start(&self) {
        // **Only on the very first keystroke.** After that the frontmost app
        // comes from `NSWorkspaceDidActivateApplicationNotification`, which
        // `menu_bar::install` observes and which calls `set_frontmost_app` for
        // every switch.
        //
        // This used to ask NSWorkspace on every keystroke at a word start, and
        // that is a synchronous round-trip to the window server **inside the tap
        // callback**. When the window server is busy — an authentication sheet,
        // loginwindow, the Accessibility pane being toggled — the call blocks,
        // the callback does not return, and macOS disables the tap with
        // `kCGEventTapDisabledByTimeout`. While that happens every keystroke in
        // the system is waiting on us, which is a frozen Mac, not a missing
        // diacritic.
        //
        // Read straight off a user's log: five timeouts in one run, bracketing
        // `FRONTMOST -> com.apple.systempreferences` and
        // `FRONTMOST -> com.apple.loginwindow`, while they revoked the
        // Accessibility grant. Nothing may block on this thread.
        if self.last_bundle_id.borrow().is_some() {
            return;
        }
        let Ok(mut session) = self.session.try_borrow_mut() else {
            return;
        };
        // The bootstrap: GlowKey can start while an app is already frontmost, so
        // no activation notification is coming for it. One query, once.
        if let Some(bundle_id) = frontmost_bundle_id() {
            session.set_frontmost_app(bundle_id.clone());
            *self.last_bundle_id.borrow_mut() = Some(bundle_id);
        }
    }
    /// Cancels an in-progress hotkey recording (Esc, a mouse click, or an app
    /// switch). No-op when not recording.
    pub fn cancel_hotkey_recording(&self) {
        let mut recording = self.recording_hotkey.borrow_mut();
        if *recording {
            *recording = false;
            drop(recording);
            crate::log::log("HOTKEY recording cancelled");
            crate::prefs::hotkey_recording_done();
        }
    }
}

/// Everything the callback needs: the shell state plus the tap port (to re-enable
/// it). The port is filled in after the tap is created, so it lives behind a
/// `RefCell`. Boxed and leaked for the program's lifetime.
struct TapContext {
    state: TapState,
    port: RefCell<Option<CFRetained<objc2_core_foundation::CFMachPort>>>,
    /// The tap's run-loop source, kept so it can be removed when the tap is
    /// rebuilt. Leaving a stale source attached while adding a second one means
    /// two live taps, and every keystroke would be processed twice — the failure
    /// the two app identities exist to prevent (`docs/handoff.md` §8).
    source: RefCell<Option<CFRetained<objc2_core_foundation::CFRunLoopSource>>>,
    /// The event mask, so a rebuilt tap watches exactly what the first one did.
    mask: Cell<CGEventMask>,
    /// Consecutive failed health checks. Only a run of them changes the glyph:
    /// a tap that flaps under load would otherwise make the indicator flicker,
    /// which is worse than one that is briefly wrong.
    health_failures: Cell<u32>,
    /// How many times the system has disabled the tap this run. A rising count in
    /// the log is the signature of a machine under enough load to drop taps.
    disabled_events: Cell<u32>,
}

/// The C tap callback. Wrapped in `catch_unwind` because a panic must not unwind
/// into CoreFoundation's C frames; on panic the event passes through unchanged.
unsafe extern "C-unwind" fn tap_callback(
    _proxy: CGEventTapProxy,
    event_type: CGEventType,
    event: NonNull<CGEvent>,
    user_info: *mut c_void,
) -> *mut CGEvent {
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        tap_dispatch(event_type, event, user_info)
    }));
    result.unwrap_or(event.as_ptr())
}

/// The actual callback logic, separated so it can be wrapped in `catch_unwind`.
fn tap_dispatch(
    event_type: CGEventType,
    event: NonNull<CGEvent>,
    user_info: *mut c_void,
) -> *mut CGEvent {
    let ctx = unsafe { &*(user_info as *const TapContext) };

    // The system disables the tap on timeout or heavy load; re-enable it.
    //
    // Logged and counted rather than done silently. This used to re-enable with
    // no record at all, which meant a tap flapping under load looked exactly like
    // a tap that was fine — and if the re-enable failed (the permission having
    // gone, say) nothing was left behind to say so. The health monitor covers the
    // case where this event never arrives; this covers the case where it does.
    if matches!(
        event_type,
        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
    ) {
        let cause = if event_type == CGEventType::TapDisabledByTimeout {
            "timeout"
        } else {
            "user input"
        };
        let count = ctx.disabled_events.get().saturating_add(1);
        ctx.disabled_events.set(count);
        if let Ok(port) = ctx.port.try_borrow() {
            if let Some(port) = port.as_ref() {
                CGEvent::tap_enable(port, true);
                crate::log::log(&format!(
                    "TAP disabled by {cause} — re-enabled (#{count} this run)"
                ));
            }
        }
        // Flush: while the tap was disabled the user's keys reached the document
        // natively, so the engine's composing word no longer matches the text at
        // the caret — the blind model's one invariant (§5). Diffing the next
        // keystroke against that stale render would delete characters the user
        // typed themselves.
        ctx.state.flush();
        return event.as_ptr();
    }

    // A mouse click can move the caret without any key event, which would leave the
    // engine's diff baseline stale. Flush the in-progress word on mouse-down.
    if matches!(
        event_type,
        CGEventType::LeftMouseDown | CGEventType::RightMouseDown
    ) {
        ctx.state.flush();
        return event.as_ptr();
    }

    if event_type != CGEventType::KeyDown {
        return event.as_ptr();
    }

    // Skip GlowKey's own synthesized events so they do not feed back into the tap.
    if is_own_event(event) {
        return event.as_ptr();
    }

    ctx.state.last_key_at.set(Some(Instant::now()));
    if ctx.state.handle_key_down(event) {
        std::ptr::null_mut() // consumed: suppress the original event
    } else {
        event.as_ptr()
    }
}

/// Creates the event tap and runs the main loop. Returns without running if the
/// Accessibility permission is missing (the tap cannot be created).
pub fn run() {
    // Settings are loaded before the permission gate because the gate's alert is
    // the first thing the user sees, and it has to speak their language too.
    let settings = crate::settings_store::load();
    crate::strings::set_language(settings.language);

    // Wait for Accessibility instead of exiting, so the app stays alive while the
    // user grants it (add GlowKey.app in System Settings → Privacy & Security →
    // Accessibility). Once granted the tap starts automatically; some macOS
    // versions need a relaunch to pick up the grant, but polling covers the rest.
    if !accessibility_trusted() {
        eprintln!("GlowKey: waiting for Accessibility permission…");
        eprintln!(
            "  A prompt should have appeared. Enable GlowKey in System Settings → \
             Privacy & Security → Accessibility, then it starts automatically."
        );
        crate::log::log("STARTUP waiting for the Accessibility permission");
        wait_for_accessibility();
    }
    eprintln!("GlowKey: Accessibility granted — starting.");
    crate::log::log("STARTUP Accessibility granted — starting");

    // The one-time welcome, shown only from the path where the gate has actually
    // succeeded — never from inside the gate itself, which would put two dialogs
    // on screen at once, the exact bug §6.5 records from the permission prompt.
    let show_welcome = !settings.welcome_shown;

    let Some(state) = TapState::from_settings(&settings) else {
        eprintln!("GlowKey: failed to create the event source.");
        return;
    };

    let key_down = 1u64 << (CGEventType::KeyDown.0 as u64);
    let left_mouse = 1u64 << (CGEventType::LeftMouseDown.0 as u64);
    let right_mouse = 1u64 << (CGEventType::RightMouseDown.0 as u64);
    let mask: CGEventMask = key_down | left_mouse | right_mouse;

    // The context must outlive the run loop; leak it deliberately.
    let ctx: *mut TapContext = Box::into_raw(Box::new(TapContext {
        state,
        port: RefCell::new(None),
        source: RefCell::new(None),
        mask: Cell::new(mask),
        health_failures: Cell::new(0),
        disabled_events: Cell::new(0),
    }));

    if !create_tap(unsafe { &*ctx }, ctx as *mut c_void) {
        crate::log::log("TAP FAILED to create (Accessibility not granted?)");
        eprintln!("GlowKey: failed to create the event tap (Accessibility not granted?).");
        return;
    }

    install_health_timer(ctx as *mut c_void);

    // Install the menu bar (shares the same leaked TapState) and run the AppKit
    // event loop, which drives both the status item and the tap's run-loop source.
    // The status item and controller are leaked so they live for the process.
    if let Some(mtm) = objc2_foundation::MainThreadMarker::new() {
        let state_ptr: *const TapState = unsafe { &(*ctx).state };
        let (item, controller) = crate::menu_bar::install(unsafe { &*state_ptr }, mtm);
        // The main menu is never drawn — GlowKey is an agent — but Cocoa routes
        // every ⌘-key equivalent through it before the responder chain, so
        // without one there is no Copy or Paste in any of GlowKey's own text
        // fields. Installed after the status item because its App submenu targets
        // that controller.
        crate::main_menu::install(&controller, mtm);
        std::mem::forget(item);
        std::mem::forget(controller);
        // After the menu bar exists, so the glyph the welcome talks about is
        // already on screen for the user to look at while reading about it.
        if show_welcome {
            crate::welcome::show(unsafe { (*ctx).state.toggle_hotkey() }, mtm);
            if let Ok(mut prefs) = unsafe { (*ctx).state.prefs.try_borrow_mut() } {
                prefs.welcome_shown = true;
            }
            unsafe { (*ctx).state.save_settings() };
            crate::log::log("STARTUP showed the one-time welcome");
        }
        // Show the Settings window on launch (like EVKey/Unikey opening their
        // control panel), unless the user has turned that off in Settings.
        if unsafe { (*state_ptr).open_settings_at_launch() } {
            crate::prefs::show(state_ptr, mtm);
        }
        let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
        app.run();
    } else {
        CFRunLoop::run();
    }
}
