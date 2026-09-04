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

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VK_BACK,
};

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
pub fn emit_edit(backspaces: usize, text: &str) {
    let mut inputs: Vec<INPUT> = Vec::with_capacity(backspaces * 2 + text.len() * 2);
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
        // silent — `elevation` is what turns it into a visible indicator state,
        // and this line is what puts it in the log.
        crate::log::log(&format!(
            "INJECT REFUSED {sent}/{} events — the foreground window is likely elevated (UIPI)",
            inputs.len()
        ));
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

#[cfg(test)]
mod tests {
    use super::*;

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
