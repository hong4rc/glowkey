//! The tap health monitor: noticing that the tap has stopped working, and
//! recovering when it can.
//!
//! See `docs/decisions/0007-tap-health-monitor.md` for why this polls rather than
//! observes — macOS exposes no way to watch an Accessibility grant change, and a
//! revoked tap delivers no events at all, so the callback cannot be the detector.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use objc2_core_foundation::{kCFRunLoopCommonModes, CFRunLoop};
use objc2_core_graphics::{CGEvent, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement};

use super::permission::accessibility_trusted;
use super::{tap_callback, TapContext};

// ---------------------------------------------------------------------------
// Tap health
//
// A tap can stop working while the process keeps running, and until this existed
// nothing noticed. Revoking the Accessibility permission is the case that
// matters: the permission is checked once at startup and never again, so the tap
// simply died, the menu bar kept showing VI, the log said nothing, and
// re-granting did not help because nothing re-entered the gate. The status glyph
// asserting VI over a dead tap is the part that makes it a defect rather than a
// limitation — the app's one indicator was lying.
//
// The other cause is the system disabling a tap under load or on timeout. The
// callback already re-enables that in place, but silently; now it is counted and
// logged, so a tap that flaps is visible in the log instead of invisible.
// ---------------------------------------------------------------------------

/// How often the tap's health is checked. Two seconds is chosen against human
/// reaction, not machine precision: the user needs to learn the tap died within a
/// few seconds of trying to type, and this is a background agent that runs all
/// day, so the check must stay cheap. It is one `CGEventTapIsEnabled` call —
/// cheaper than a single keystroke, which this same run loop already handles
/// twenty times a second.
const HEALTH_CHECK_SECONDS: f64 = 2.0;

/// Skip the health check entirely if a keystroke arrived more recently than
/// this. Typing proves the tap works, and asking the window server to confirm it
/// costs a round-trip on the thread that must stay responsive.
const HEALTH_SKIP_AFTER_KEYSTROKE: Duration = Duration::from_secs(3);

/// Consecutive failed checks before the glyph changes. A tap disabled under load
/// is usually re-enabled on the next tick, and a glyph that flickers is worse
/// than one that is briefly wrong.
const HEALTH_FAILURES_BEFORE_WARNING: u32 = 2;

/// Consecutive failures of the re-enable-in-place remedy before GlowKey stops
/// claiming to work. Thirty checks is a minute of a tap that will not come back.
const HEALTH_FAILURES_BEFORE_GIVING_UP: u32 = 30;

/// Log the re-enable attempt on the first failure and then every this-many, so a
/// tap flapping for hours leaves a trail without filling the file.
const HEALTH_FAILURES_PER_LOG_LINE: u32 = 30;

/// Flushes the engine after any gap in the tap's grip on the keystream.
///
/// This is not housekeeping; it is the blind model's one invariant
/// (`docs/handoff.md` §5): *rendered == the text tail at the caret*. GlowKey
/// never reads the document, so the engine's idea of the current word is only
/// true while every keystroke passes through the tap. A dead or disabled tap is
/// the strongest possible break in that: the user's keys reach the document
/// **natively, unsuppressed**, while `Session` still holds the raw log and render
/// from before the gap.
///
/// Concretely, without this: type `hoo` (render `hô`), lose the permission
/// mid-word, type `ngf` — which lands literally, so the document reads `hôngf` —
/// then re-grant. The next letter is diffed against the stale render `hô`, and
/// the emitted backspaces delete characters the user typed themselves.
///
/// The usual safety net cannot help here either. Everything else that moves the
/// caret behind GlowKey's back is caught by a flush on mouse-down or a caret
/// key — but those arrive *through the tap*, and the tap is precisely what was
/// dead. Nothing else was ever going to notice.
fn flush_after_gap(ctx: &TapContext, why: &str) {
    ctx.state.flush();
    crate::log::log(&format!(
        "HEALTH {why} — flushed the composing word (keys typed while the tap was \
         down reached the document without us)"
    ));
}

/// Whether the tap is known to be dead. Read by the menu bar so the glyph can
/// tell the truth (`docs/handoff.md` §6).
static TAP_DEAD: AtomicBool = AtomicBool::new(false);

/// Whether GlowKey currently has no working event tap — no Vietnamese, whatever
/// the mode says. The menu bar shows a warning glyph while this holds.
#[must_use]
pub fn tap_is_dead() -> bool {
    TAP_DEAD.load(Ordering::Relaxed)
}

/// Creates the event tap, attaches it to the current run loop, and stores both in
/// `ctx`. Returns whether it succeeded.
///
/// Shared by startup and by the health monitor's recovery path, so a rebuilt tap
/// is created exactly like the original — a second code path here would be a
/// place for the two to drift apart.
pub(super) fn create_tap(ctx: &TapContext, ctx_ptr: *mut c_void) -> bool {
    // Retire the previous tap first, and **refuse to continue if we cannot**.
    //
    // This is a `let … else` rather than an `if let` on purpose. Skipping the
    // teardown and carrying on would attach a second tap to the same run loop
    // with the same context, so every keystroke would be processed twice — and
    // for a blind engine that means every edit applied twice, which is the
    // failure `docs/handoff.md` §8 keeps two app identities to avoid. Worse, the
    // stores below would drop the only handles able to remove the old source
    // (`CFRunLoop` retains what it is given), making the state unrecoverable
    // without a restart.
    //
    // Refusing costs two seconds: the health timer tries again on the next tick.
    // A borrow can only fail here if the tap callback is somehow mid-flight, and
    // waiting for that to finish is exactly the right response.
    let (Ok(mut old_source), Ok(mut old_port)) =
        (ctx.source.try_borrow_mut(), ctx.port.try_borrow_mut())
    else {
        crate::log::log(
            "HEALTH cannot retire the previous tap right now — refusing to create a second one",
        );
        return false;
    };
    if let (Some(run_loop), Some(source)) = (CFRunLoop::current(), old_source.take()) {
        run_loop.remove_source(Some(&source), unsafe { kCFRunLoopCommonModes });
    }
    if let Some(port) = old_port.take() {
        CGEvent::tap_enable(&port, false);
    }
    // Both slots are empty now and the borrows end here, so the stores below
    // cannot fail for want of an available borrow.
    drop(old_source);
    drop(old_port);

    let port = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            ctx.mask.get(),
            Some(tap_callback),
            ctx_ptr,
        )
    };
    let Some(port) = port else {
        return false;
    };
    let source = objc2_core_foundation::CFMachPort::new_run_loop_source(None, Some(&port), 0);
    let (Some(run_loop), Some(source)) = (CFRunLoop::current(), source) else {
        return false;
    };
    run_loop.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });

    // Store the handles **before** enabling, and treat a failed store as a failed
    // install. Reporting success while `ctx` holds no handle to a tap that is
    // live and attached would be the worst of both worlds: the health check would
    // read `port == None`, conclude "not enabled" on every tick forever, and
    // never be able to retire the tap it cannot see.
    let (Ok(mut port_slot), Ok(mut source_slot)) =
        (ctx.port.try_borrow_mut(), ctx.source.try_borrow_mut())
    else {
        crate::log::log("HEALTH created a tap but could not store it — tearing it back down");
        run_loop.remove_source(Some(&source), unsafe { kCFRunLoopCommonModes });
        return false;
    };
    port_slot.replace(port);
    source_slot.replace(source);
    if let Some(port) = port_slot.as_ref() {
        CGEvent::tap_enable(port, true);
    }
    true
}

/// Schedules the repeating health check on the current run loop.
pub(super) fn install_health_timer(ctx_ptr: *mut c_void) {
    let mut context = objc2_core_foundation::CFRunLoopTimerContext {
        version: 0,
        info: ctx_ptr,
        retain: None,
        release: None,
        copyDescription: None,
    };
    let first_fire = objc2_core_foundation::CFAbsoluteTimeGetCurrent() + HEALTH_CHECK_SECONDS;
    let timer = unsafe {
        objc2_core_foundation::CFRunLoopTimer::new(
            None,
            first_fire,
            HEALTH_CHECK_SECONDS,
            0,
            0,
            Some(health_timer_callback),
            &mut context,
        )
    };
    let (Some(run_loop), Some(timer)) = (CFRunLoop::current(), timer) else {
        crate::log::log("HEALTH failed to schedule the tap health check");
        return;
    };
    run_loop.add_timer(Some(&timer), unsafe { kCFRunLoopCommonModes });
    // Leaked deliberately: the timer must live for the whole process, like the
    // tap's own run-loop source.
    std::mem::forget(timer);
}

/// The C timer callback. Wrapped in `catch_unwind` for the same reason the tap
/// callback is: a panic must not unwind into CoreFoundation's C frames.
extern "C-unwind" fn health_timer_callback(
    _timer: *mut objc2_core_foundation::CFRunLoopTimer,
    info: *mut c_void,
) {
    let _ = std::panic::catch_unwind(|| {
        if info.is_null() {
            return;
        }
        check_tap_health(unsafe { &*(info as *const TapContext) }, info);
    });
}

/// One health check. Four states, because "trusted again" and "trusted all
/// along" need different remedies:
///
/// | enabled | trusted | warned already | meaning | action |
/// |---|---|---|---|---|
/// | yes | — | — | healthy | clear any warning, flush if we had been dead |
/// | no | yes | no | disabled by timeout or load | re-enable the same port in place |
/// | no | yes | **yes** | permission came back after a revocation | **rebuild** the tap — the old port was created under a grant that no longer exists, so re-enabling it does nothing |
/// | no | no | — | permission revoked | warn after two consecutive checks |
///
/// The "warned already" column is `tap_is_dead()`, and it is what separates rows
/// two and three. Without it a returning grant would only ever re-enable a port
/// the system has already invalidated, and the app would sit there looking
/// healthy and typing nothing.
pub(super) fn check_tap_health(ctx: &TapContext, ctx_ptr: *mut c_void) {
    // **A recent keystroke is proof the tap is alive**, so do not ask the system.
    //
    // `CGEventTapIsEnabled` is a round-trip to the window server, and this timer
    // runs on the same run loop as the tap callback — so a slow answer here
    // delays the callback, and a delayed callback is how macOS comes to disable
    // the tap for timing out, which freezes input system-wide. The one thread
    // that must never block should not be making periodic IPC calls for
    // information that just arrived for free.
    if let Some(at) = ctx.state.last_key_at.get() {
        if at.elapsed() < HEALTH_SKIP_AFTER_KEYSTROKE {
            return;
        }
    }
    let enabled = match ctx.port.try_borrow() {
        Ok(slot) => slot
            .as_ref()
            .is_some_and(|port| CGEvent::tap_is_enabled(port)),
        // The callback holds the borrow: it is mid-keystroke, so it is alive.
        Err(_) => return,
    };

    if enabled {
        ctx.health_failures.set(0);
        // While we are here and idle, re-check which application is in front.
        //
        // The authoritative source is
        // `NSWorkspaceDidActivateApplicationNotification`, which `menu_bar`
        // observes. This is the belt-and-braces that used to live in the tap
        // callback, where it cost a blocking window-server round-trip on every
        // keystroke; here it costs one on an idle timer instead. Same safety net
        // for the per-app ignore list — a stale frontmost app means Vietnamese
        // firing in a terminal — off the one thread that must never block.
        ctx.state.refresh_frontmost_if_idle();
        if TAP_DEAD.swap(false, Ordering::Relaxed) {
            // Coming back from dead **must** flush. See `flush_after_gap`.
            flush_after_gap(ctx, "tap is working again");
            crate::menu_bar::refresh_glyph();
        }
        return;
    }

    let failures = ctx.health_failures.get().saturating_add(1);
    ctx.health_failures.set(failures);
    let trusted = accessibility_trusted();

    if trusted && !tap_is_dead() {
        // The system disabled the tap but the permission is intact: a timeout or
        // a load spike. Re-enabling the same port is the documented remedy.
        let mut re_enabled = false;
        if let Ok(slot) = ctx.port.try_borrow() {
            if let Some(port) = slot.as_ref() {
                CGEvent::tap_enable(port, true);
                re_enabled = true;
            }
        }
        // Logged on the transition, not on every tick. At two seconds apart an
        // unconditional line here would be ~43,000 lines a day, and the log's
        // size cap is evaluated once per process — so a long-running agent would
        // grow the file without bound. The count still shows how long it lasted.
        if failures == 1 || failures.is_multiple_of(HEALTH_FAILURES_PER_LOG_LINE) {
            crate::log::log(&format!(
                "HEALTH tap disabled while still trusted — {} ({failures} consecutive)",
                if re_enabled {
                    "re-enabled"
                } else {
                    "no port to re-enable"
                }
            ));
        }
        // A tap the system keeps disabling while the permission is intact is not
        // recoverable by re-enabling, and the glyph must not keep claiming VI
        // over it — the lie this whole module exists to end.
        if failures >= HEALTH_FAILURES_BEFORE_GIVING_UP && !TAP_DEAD.swap(true, Ordering::Relaxed) {
            crate::log::log(
                "HEALTH giving up on re-enabling in place — the tap stays disabled while trusted",
            );
            flush_after_gap(ctx, "tap is stuck disabled");
            crate::menu_bar::refresh_glyph();
        }
        return;
    }

    if trusted {
        // Permission came back after having been revoked. The old port was created
        // under a grant that no longer exists, so re-enabling it is not enough —
        // build a new tap.
        if create_tap(ctx, ctx_ptr) {
            ctx.health_failures.set(0);
            TAP_DEAD.store(false, Ordering::Relaxed);
            crate::log::log("HEALTH Accessibility restored — event tap rebuilt, no restart needed");
            eprintln!("GlowKey: Accessibility restored — typing Vietnamese again.");
            flush_after_gap(ctx, "tap rebuilt");
            crate::menu_bar::refresh_glyph();
        } else {
            crate::log::log("HEALTH Accessibility is granted but the tap could not be rebuilt");
        }
        return;
    }

    if failures >= HEALTH_FAILURES_BEFORE_WARNING && !TAP_DEAD.swap(true, Ordering::Relaxed) {
        crate::log::log(
            "HEALTH the Accessibility permission was revoked — no keystrokes are being \
             processed. Re-enable GlowKey in System Settings → Privacy & Security → \
             Accessibility and it recovers by itself.",
        );
        eprintln!("GlowKey: the Accessibility permission was revoked — Vietnamese is off.");
        crate::menu_bar::refresh_glyph();
    }
}
