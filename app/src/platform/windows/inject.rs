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
    send(&edit_inputs(backspaces, text, app));
}

/// The key sequence one edit becomes.
///
/// Separate from [`emit_edit`] so it can be asserted on without sending anything
/// into the live session: what this returns is exactly what the machine would
/// receive, and the thing most worth pinning about an input method is that it
/// never contains a key the user did not ask for.
fn edit_inputs(backspaces: usize, text: &str, app: Option<&str>) -> Vec<INPUT> {
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
    // it deletes nothing.
    //
    // **It took two attempts to get the condition right, and the wrong one is
    // worth remembering.** From 2026-09-05 this fired for *every* Chromium
    // application, on the argument that reaching a mid-field caret required
    // moving it without flushing. That was wrong: a mouse click *does* flush and
    // *does* leave the caret mid-field, so the next edit carrying backspaces sent
    // a forward-delete into ordinary text. `is_chromium_app` matches Electron
    // too, so that was Chrome, Edge, Slack, VS Code and Discord — and the failure
    // was silent, which makes it worse than the visible mis-render it prevented.
    // It was disabled the same day.
    //
    // macOS could always afford the missing half: an accessibility read tells it
    // whether a selection exists. The Windows equivalent is a UI Automation call
    // — cross-process, COM — which `decisions/0008` forbids on this path and
    // `LowLevelHooksTimeout` punishes by removing the hook.
    //
    // So the answer is resolved *elsewhere*: `omnibox` watches focus changes on a
    // worker thread and stores one `bool`, and reading it here costs an atomic
    // load. Every focus change clears that flag before the resolver runs, so a
    // stale answer can only ever be `false` — the mis-render comes back, and a
    // character is never deleted in a page body. The failure direction is the
    // whole design.
    if needs_omnibox_guard(backspaces, app, super::omnibox::focus_is_omnibox()) {
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
    inputs
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

/// Whether this edit needs the Chromium address-bar forward-delete.
///
/// Both conditions, and both are load-bearing:
///
/// - a Chromium application with backspaces to send — without it the guard is
///   pointless, since no other host keeps an inline-autocomplete selection;
/// - **the focused element is the address bar** — without it the guard deletes
///   the character to the right of the caret in every page body, which is what
///   it did until 2026-09-05 and why it was switched off.
///
/// `focus_is_omnibox` is passed in rather than read here so the rule is testable
/// without a browser, a focus change or a UI Automation call.
#[must_use]
pub fn needs_omnibox_guard(backspaces: usize, app: Option<&str>, focus_is_omnibox: bool) -> bool {
    focus_is_omnibox && backspaces > 0 && app.is_some_and(crate::default_exclusions::is_chromium_app)
}

#[cfg(test)]
mod tests {
    use super::*;
    // Only the test that proves it is never sent still names it.
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_DELETE;

    /// The virtual-key code an entry carries, or `None` for a Unicode entry.
    fn vk_of(input: &INPUT) -> Option<u16> {
        // SAFETY: every entry this module builds is a KEYBDINPUT — `key_input`
        // and `unicode_input` are the only constructors — so reading `ki` is
        // reading the variant that is there.
        let vk = unsafe { input.Anonymous.ki.wVk };
        (vk != 0).then_some(vk)
    }

    /// **A Chromium edit sends no forward-delete while focus is elsewhere.**
    ///
    /// The regression this pins is the user's text disappearing, so it is
    /// asserted against the actual input sequence rather than trusted to a call
    /// site. `VK_DELETE` here removes the character after the caret, and in a
    /// page body there is always one.
    ///
    /// Runs through `edit_inputs`, which reads the live focus flag — false in a
    /// test process, since nothing has ever focused an address bar here. That is
    /// the same default the flag holds at startup and after every focus change,
    /// so this also pins that the *default* is the safe one.
    #[test]
    fn a_chromium_edit_sends_no_forward_delete_without_omnibox_focus() {
        assert!(
            !super::super::omnibox::focus_is_omnibox(),
            "the flag must default to false, or this test proves nothing"
        );
        for app in ["chrome.exe", "msedge.exe", "slack.exe", "code.exe"] {
            let inputs = edit_inputs(2, "ồ", Some(app));
            assert!(
                !inputs.iter().any(|i| vk_of(i) == Some(VK_DELETE)),
                "{app}: a forward-delete deletes the character after the caret"
            );
            assert!(
                inputs.iter().any(|i| vk_of(i) == Some(VK_BACK)),
                "{app}: the edit still deletes what it meant to"
            );
        }
    }

    /// **`hoongf` emits exactly three backspaces and `ồng`, in that order.**
    ///
    /// This is the diff the address-bar defect is reported against, pinned at
    /// the layer that builds the key sequence — the last point where the bug
    /// could still be ours. It cannot fail and the browser still be at fault, so
    /// when the next `hoồng` report arrives this test decides, in one run, which
    /// side of the boundary to look at.
    ///
    /// Deliberately asserted as a whole sequence rather than a count. The blind
    /// diff model's invariant is that the host receives the deletions *and then*
    /// the insertion, in one ordered batch (`emit_edit`'s doc comment); a test
    /// that only counted keys would pass on a batch that interleaved them, and
    /// interleaving is precisely what produced `hoồng` on macOS before the
    /// single-source rule.
    #[test]
    fn the_hong_diff_is_three_backspaces_then_the_text() {
        // The state after `hoong`: the screen reads `hông`, and the `f` makes it
        // `hồng` — a common prefix of `h`, so three code units go and three come
        // back.
        let inputs = edit_inputs(3, "ồng", Some("chrome.exe"));

        // Down/up per key, so six entries of backspace then six of text.
        let vks: Vec<Option<u16>> = inputs.iter().map(vk_of).collect();
        assert_eq!(
            vks,
            vec![
                Some(VK_BACK),
                Some(VK_BACK),
                Some(VK_BACK),
                Some(VK_BACK),
                Some(VK_BACK),
                Some(VK_BACK),
                None,
                None,
                None,
                None,
                None,
                None,
            ],
            "the deletions must all precede the insertion, and nothing else may \
             be in the batch"
        );

        // And the inserted units are the text itself, not a normalisation of it:
        // `ồ` is one code unit here, so a decomposed render would show up as
        // three entries rather than one and this would fail.
        let units: Vec<u16> = inputs
            .iter()
            .filter(|i| vk_of(i).is_none())
            .step_by(2)
            // SAFETY: every entry is a KEYBDINPUT; see `vk_of`.
            .map(|i| unsafe { i.Anonymous.ki.wScan })
            .collect();
        assert_eq!(units, "ồng".encode_utf16().collect::<Vec<_>>());
    }

    /// The guard fires for a Chromium edit **with focus in the address bar**, and
    /// for nothing else.
    ///
    /// Every negative here is a character the user would otherwise lose. The
    /// focus condition is the one that was missing between 2026-09-05 and
    /// 2026-09-06: with only the first two, a forward-delete went into every
    /// Chromium page body, silently.
    #[test]
    fn the_omnibox_guard_needs_focus_in_the_address_bar() {
        // All three conditions.
        assert!(needs_omnibox_guard(1, Some("msedge.exe"), true));
        assert!(needs_omnibox_guard(3, Some("chrome.exe"), true));

        // **A Chromium edit with focus anywhere else.** This is the page body,
        // and it is the case that ate characters in Gmail and Slack.
        assert!(!needs_omnibox_guard(1, Some("msedge.exe"), false));
        assert!(!needs_omnibox_guard(3, Some("chrome.exe"), false));
        assert!(!needs_omnibox_guard(1, Some("slack.exe"), false));

        // No backspaces: nothing to protect, and a forward-delete would be pure
        // risk for no benefit.
        assert!(!needs_omnibox_guard(0, Some("msedge.exe"), true));

        // Not a browser — an address bar cannot be focused in Notepad, but the
        // app condition is checked rather than assumed.
        assert!(!needs_omnibox_guard(1, Some("notepad.exe"), true));

        // No application resolved yet: fail safe, and do not delete anything.
        assert!(!needs_omnibox_guard(1, None, true));
    }

    /// The guard reads the shipped Chromium table rather than its own list, so a
    /// browser added there is covered without a second edit here — and none of
    /// them is guarded without focus in the address bar.
    #[test]
    fn the_guard_uses_the_shipped_chromium_table() {
        for app in crate::default_exclusions::CHROMIUM_APP_PREFIXES {
            assert!(
                needs_omnibox_guard(1, Some(app), true),
                "{app} is in the shipped table and must be guarded"
            );
            assert!(
                !needs_omnibox_guard(1, Some(app), false),
                "{app} must not be guarded when focus is not the address bar"
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
