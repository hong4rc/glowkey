//! Everything that writes to the outside world: `SendInput`, and the tag that
//! keeps what it writes out of our own hook.
//!
//! Every mutation GlowKey makes on Windows goes through here, in one ordered
//! `SendInput` batch per edit. That is the same invariant the macOS side keeps
//! with its single tagged `CGEventPost` queue, and it exists for the same
//! reason: a synthesized backspace must never overtake the character it deletes.
//! Mixing a natively-typed character with a synthesized edit posted a moment
//! later is what produced `hoongf` → `hoồng` in multiprocess applications, and
//! suppressing every handled key and re-emitting it from one queue is what fixed
//! it.

use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VK_BACK,
    VK_DELETE,
};

/// Whether the current run of injection refusals has already been reported.
///
/// Reset by the first batch that succeeds, so a later episode is reported again
/// rather than being swallowed for the life of the process.
static REFUSAL_REPORTED: AtomicBool = AtomicBool::new(false);

/// The tag GlowKey stamps on every event it injects, read back in the hook's
/// first statement.
///
/// `dwExtraInfo` is the analogue of the tagged `CGEventSource` on macOS: an
/// application-defined value that travels with a synthesized event and comes
/// back unchanged in the hook. **Without this the hook reprocesses its own
/// injection**, each pass generating more input than the last, and the app melts
/// down — the failure mode that makes every other behaviour in this backend
/// untestable.
///
/// The value is "GLOW" in ASCII, matching `GLOWKEY_TAG` on the macOS side. It is
/// not a security boundary — any process can set the same value — it is a
/// self-identification, and the thing it defends against is us.
pub const GLOWKEY_INJECTED: usize = 0x_47_4C_4F_57;

/// Whether this event carries GlowKey's own tag and must be passed straight
/// through.
///
/// A free function over a plain integer rather than a method on an event: it is
/// the one part of the hook path whose correctness can be established without
/// Windows, a live hook or a focused window, and the tests at the bottom of this
/// file are that establishment.
#[must_use]
pub fn is_own_event(extra_info: usize) -> bool {
    extra_info == GLOWKEY_INJECTED
}

/// One synthesized edit: delete `backspaces` UTF-16 code units back from the
/// caret, then insert `text`.
///
/// **`backspaces` is a count of UTF-16 code units, and that is already
/// `SendInput`'s unit.** The engine counts them that way because that is what
/// the platforms it targets take, and the alignment here is lucky rather than
/// designed — but it is real, so do not "fix" this to `char`s. A character
/// outside the basic plane is two code units, two `VK_BACK` presses, and two
/// `KEYEVENTF_UNICODE` entries; converting to `char`s would delete half as far
/// as the engine intended.
///
/// One `SendInput` call for the whole batch, never a call per key. The array is
/// delivered in order and cannot be interleaved with real input midway, which is
/// the ordering guarantee the blind diff model needs.
pub fn emit_edit(backspaces: usize, text: &str, app: Option<&str>) {
    let mut inputs: Vec<INPUT> = Vec::with_capacity(backspaces * 2 + text.len() * 2 + 2);

    // ── The Chromium address-bar guard ──────────────────────────────────────
    //
    // A Chromium omnibox keeps a **trailing inline-autocomplete selection**:
    // type `hoo` and the field holds `hoo` plus a selected completion. The first
    // synthetic Backspace then deletes that selection instead of a character, so
    // the edit lands one character short and every edit after it compounds the
    // error:
    //
    //     hoongf  ->  hoồng     (observed in Edge, 2026-09-05)
    //
    // which is the same defect `docs/decisions/0003-omnibox-ax-guard.md` records
    // on macOS, reproducing here. The port plan listed "does this happen on
    // Windows?" as an open question; it does.
    //
    // A forward-delete first clears the selection. With no selection and the
    // caret at the end of the text — GlowKey's normal position while composing —
    // it deletes nothing, so it is a no-op in the ordinary case.
    //
    // **The trade-off, stated rather than hidden.** macOS gates this on an
    // accessibility read of whether a selection actually exists. The Windows
    // equivalent is a UI Automation call: cross-process, COM, and on the
    // keystroke path — which `decisions/0008` forbids and `LowLevelHooksTimeout`
    // punishes by removing the hook. So this fires unconditionally for Chromium
    // applications instead. The cost is one wasted key event per edit there, and
    // one real risk: if the caret is mid-field *while composing*, the
    // forward-delete eats the character after it. Reaching that state requires
    // moving the caret without flushing, and the ladder flushes on every arrow
    // key and the mouse hook flushes on every click — so it is narrow, and it is
    // written down here rather than discovered later.
    if needs_omnibox_guard(backspaces, app) {
        inputs.push(key_input(VK_DELETE, false));
        inputs.push(key_input(VK_DELETE, true));
    }

    for _ in 0..backspaces {
        inputs.push(key_input(VK_BACK, false));
        inputs.push(key_input(VK_BACK, true));
    }
    for unit in text.encode_utf16() {
        // Surrogate pairs go through as two entries, which is what makes the
        // code-unit accounting above hold in both directions.
        inputs.push(unicode_input(unit, false));
        inputs.push(unicode_input(unit, true));
    }
    send(&inputs);
}

/// Replays one key by its virtual-key code, from our own queue.
///
/// Used for the boundary key after a restore. Letting the original through
/// instead loses the race — it is the event being dispatched right now, so the
/// host applies it *before* the backspaces the edit just queued, and the edit
/// then eats the boundary key rather than the word it meant to replace
/// (`ddc`␣ → `đddc`, the space swallowed). Replaying puts it at the tail of the
/// same ordered batch.
pub fn replay_key(vk: u16) {
    send(&[key_input(vk, false), key_input(vk, true)]);
}

/// Sends a batch, tagged. Empty batches are skipped rather than sent: `SendInput`
/// with a zero count is a no-op that still costs a syscall, and this runs on the
/// keystroke path.
fn send(inputs: &[INPUT]) {
    if inputs.is_empty() {
        return;
    }
    // SAFETY: `inputs` is a valid slice of correctly-sized INPUT structures, and
    // the size argument is `size_of::<INPUT>()` as the API requires.
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if sent as usize != inputs.len() {
        // The overwhelmingly likely cause is UIPI: the foreground window belongs
        // to a higher integrity level and the whole batch was refused. That is a
        // known, permanent limitation rather than a bug, but it must never be
        // silent — `elevation` turns it into a visible indicator state, and this
        // line is what puts it in the log.
        //
        // Two things about how it is logged, both load-bearing:
        //
        // `hook_log`, never `crate::log::log`. This function runs inside the hook
        // callback, and the direct logger takes a global lock, writes, flushes,
        // and sometimes renames the file. That is the archetypal blocking call
        // `decisions/0008` forbids here, and it would fire on a machine already in
        // a degraded state — costing the hook itself, whose loss then reads as a
        // *second*, unrelated fault.
        //
        // Once per run of refusals, not once per key. Injection into an elevated
        // window fails for **every** keystroke while it is in front, so an
        // unguarded line here is a per-keystroke write. The flag resets when a
        // batch succeeds, so a later episode is reported again.
        if !REFUSAL_REPORTED.swap(true, Ordering::Relaxed) {
            super::hook_log::log(format!(
                "INJECT REFUSED {sent}/{} events — the foreground window is likely elevated \
                 (UIPI). Reported once; further refusals are silent until injection succeeds.",
                inputs.len()
            ));
        }
    } else {
        REFUSAL_REPORTED.store(false, Ordering::Relaxed);
    }
}

/// One virtual-key event, tagged as ours.
fn key_input(vk: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: GLOWKEY_INJECTED,
            },
        },
    }
}

/// One UTF-16 code unit as a literal character, tagged as ours.
///
/// `KEYEVENTF_UNICODE` bypasses the keyboard layout entirely, which is what lets
/// GlowKey insert `ồ` on a US layout that has no key for it. `wVk` must be zero:
/// with a virtual key set the unit is ignored and the layout runs instead.
fn unicode_input(unit: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
            ki: KEYBDINPUT {
                wVk: 0,
                wScan: unit,
                dwFlags: if up {
                    KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                } else {
                    KEYEVENTF_UNICODE
                },
                time: 0,
                dwExtraInfo: GLOWKEY_INJECTED,
            },
        },
    }
}

/// Whether an edit into `app` needs the Chromium address-bar guard.
///
/// Split out so the rule is testable without injecting anything.
#[must_use]
pub fn needs_omnibox_guard(backspaces: usize, app: Option<&str>) -> bool {
    backspaces > 0 && app.is_some_and(crate::default_exclusions::is_chromium_app)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The guard fires for a Chromium browser with backspaces to send, and for
    /// nothing else.
    ///
    /// Both halves matter. Missing it in Edge is the `hoongf` -> `hoồng` bug;
    /// firing it anywhere else spends a forward-delete on a field that never had
    /// a selection, which in a normal editor deletes a real character.
    #[test]
    fn the_omnibox_guard_fires_only_for_chromium_edits() {
        assert!(needs_omnibox_guard(1, Some("msedge.exe")));
        assert!(needs_omnibox_guard(3, Some("chrome.exe")));

        // No backspaces: nothing to protect, and a forward-delete would be pure
        // risk for no benefit.
        assert!(!needs_omnibox_guard(0, Some("msedge.exe")));

        // Not a browser.
        assert!(!needs_omnibox_guard(1, Some("notepad.exe")));
        assert!(!needs_omnibox_guard(1, Some("code.exe")));

        // Not yet resolved: fail safe, and do not delete anything.
        assert!(!needs_omnibox_guard(1, None));
    }

    /// The guard reads the shipped Chromium table rather than its own list, so a
    /// browser added there is covered without a second edit here.
    #[test]
    fn the_guard_uses_the_shipped_chromium_table() {
        for app in crate::default_exclusions::CHROMIUM_APP_PREFIXES {
            assert!(
                needs_omnibox_guard(1, Some(app)),
                "{app} is in the shipped table and must be guarded"
            );
        }
    }

    /// The guard, established without a hook, a window or a keystroke.
    ///
    /// This is the one property in the Windows backend that can be proven by a
    /// plain unit test, and it is also the one whose failure makes every other
    /// property unobservable — a hook feeding on its own output produces runaway
    /// input, not a wrong diacritic. So it is proven here rather than left to the
    /// manual pass.
    #[test]
    fn our_own_events_are_recognized() {
        assert!(is_own_event(GLOWKEY_INJECTED));
    }

    #[test]
    fn everything_else_is_not_ours() {
        // Real input carries zero unless another tool set something.
        assert!(!is_own_event(0));
        // A neighbouring value must not match: the check is equality, and a
        // range or a mask here would swallow other tools' tags as ours and pass
        // their synthetic input through untransformed.
        assert!(!is_own_event(GLOWKEY_INJECTED - 1));
        assert!(!is_own_event(GLOWKEY_INJECTED + 1));
        // Another automation tool's tag, and the all-ones value a buggy caller
        // might leave behind.
        assert!(!is_own_event(0xDEAD_BEEF));
        assert!(!is_own_event(usize::MAX));
    }

    #[test]
    fn the_tag_spells_glow() {
        // Same value as the macOS `GLOWKEY_TAG`, and readable in a hex dump of a
        // log or a debugger, which is where it is looked at.
        assert_eq!(
            GLOWKEY_INJECTED.to_be_bytes()[std::mem::size_of::<usize>() - 4..],
            *b"GLOW"
        );
    }
}
