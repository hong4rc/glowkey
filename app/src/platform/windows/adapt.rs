//! Translation, and only translation: a `KBDLLHOOKSTRUCT` in, a neutral
//! [`KeyEvent`] out.
//!
//! This is the whole of what Windows contributes to the decision. Everything the
//! ladder branches on — is this Backspace, is this a caret move, which modifiers
//! are held, what character did the layout produce — is answered here from Win32
//! and then handed over as plain data. No session, no side effects, and nothing
//! that can wait.
//!
//! The virtual-key codes below are the *physical* ones in the sense that matters
//! for hotkeys: `VK_Z` is the key where Z sits, whatever a Colemak user's layout
//! makes it type. The character is carried separately, in [`KeyEvent::ch`], and
//! that one *is* the layout's answer. Same split as the macOS adapter, for the
//! same reason.

use glowkey_input::{Key, KeyEvent, Modifiers};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ToUnicodeEx, HKL, VK_BACK, VK_CAPITAL, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END,
    VK_ESCAPE, VK_HOME, VK_LEFT, VK_LWIN, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT,
    VK_RMENU, VK_RWIN, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::KBDLLHOOKSTRUCT;

/// Which key this virtual-key code is, in the ladder's vocabulary.
///
/// The caret-move group is one class on purpose: every key in it moves the
/// insertion point with no text change, so GlowKey's diff baseline is stale
/// either way and the ladder flushes. Splitting them would invite someone to
/// treat one as safe.
fn key_for(vk: u16) -> Key {
    match vk {
        VK_BACK => Key::Backspace,
        VK_DELETE => Key::ForwardDelete,
        VK_ESCAPE => Key::Escape,
        VK_SPACE => Key::Space,
        VK_RETURN => Key::Return,
        VK_TAB => Key::Tab,
        VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN | VK_HOME | VK_END | VK_PRIOR | VK_NEXT => {
            Key::CaretMove
        }
        // Windows assigns the ASCII values of 'A'..='Z' to the letter keys, so
        // the table the macOS adapter needs is arithmetic here. Lowercased to
        // match `Key::Letter`'s contract.
        0x41..=0x5A => Key::Letter((vk as u8 as char).to_ascii_lowercase()),
        _ => Key::Other,
    }
}

/// Which modifiers are held.
///
/// Read with `GetKeyState` rather than `GetAsyncKeyState`: inside a
/// `WH_KEYBOARD_LL` callback the synchronous state is the one consistent with
/// the event being delivered, while the async state updates on a different
/// schedule and can disagree. It is also a cheap in-process read rather than a
/// cross-thread query, which is what the callback needs.
///
/// The Windows key maps to `command`. The ladder uses that field for "the
/// system's own shortcut modifier", which is ⌘ on macOS and the Windows key
/// here; keeping the mapping means `Modifiers::is_shortcut` keeps meaning what
/// it meant, and the shortcut filter keeps flushing where it always did.
fn modifiers() -> (Modifiers, bool) {
    // AltGr is not a modifier at all, for the ladder's purposes. It is part of
    // the *layout*: on many European keyboards AltGr+key is simply how an
    // ordinary letter or punctuation mark is typed, and the character it
    // produces is what `ToUnicodeEx` resolves from the state array.
    //
    // Windows delivers it as Right Alt **plus a synthetic Left Control**, so a
    // naive read reports Ctrl+Alt. The ladder's shortcut filter is
    // `control || option || command`, so reporting *either* half fires it:
    // flush, pass through, composition destroyed — on every AltGr keystroke.
    //
    // So both halves are withheld, and the fact of AltGr is returned separately
    // for `unicode_char` to put into the state array, which is where it belongs.
    // macOS has no equivalent case (⌥ there really is a command modifier), so
    // this is not inherited from the tap; it is new, and untreated it is the kind
    // of thing reported as "Vietnamese does not work on my keyboard".
    let altgr = is_down(VK_RMENU);
    let mods = Modifiers {
        control: is_down(VK_CONTROL) && !altgr,
        shift: is_down(VK_SHIFT),
        // Alt. Named `option` because the neutral crate was lifted off macOS;
        // it is the same physical position and the same role.
        option: is_down(VK_MENU) && !altgr,
        command: is_down(VK_LWIN) || is_down(VK_RWIN),
    };
    (mods, altgr)
}

/// Whether a key is currently held down.
fn is_down(vk: u16) -> bool {
    // SAFETY: a plain read of the calling thread's keyboard state.
    // The high bit is "currently down"; the low bit is the toggle state, which
    // is why this is not a plain `!= 0` — Caps Lock being on would otherwise
    // read as Shift being held.
    (unsafe { GetKeyState(vk as i32) } as u16 & 0x8000) != 0
}

/// Whether a *toggle* key is currently on — Caps Lock, as opposed to held.
///
/// A separate read from [`is_down`] because the two live in different bits and
/// mean different things: the high bit is "held", the low bit is "toggled on".
/// Conflating them is how Caps Lock would read as Shift.
fn is_toggled(vk: u16) -> bool {
    // SAFETY: as above.
    (unsafe { GetKeyState(vk as i32) } & 1) != 0
}

/// Reads one key-down event into the neutral form the policy takes.
pub fn key_event(info: &KBDLLHOOKSTRUCT) -> KeyEvent {
    let vk = info.vkCode as u16;
    let (mods, altgr) = modifiers();
    KeyEvent {
        ch: unicode_char(vk, info.scanCode, &mods, altgr),
        key: key_for(vk),
        mods,
        // Carried, not interpreted: it is how a hotkey the user recorded on this
        // machine is recognised again. `windows_vk` in the settings file.
        raw_code: i64::from(vk),
    }
}

/// The character this key produces under the **foreground window's** layout.
///
/// # The layout has to be the foreground window's, not ours
///
/// Windows tracks the input language per window. `GetKeyboardLayout(0)` returns
/// the layout of the *calling* thread — GlowKey's message loop, which never has
/// focus and never changes language — so resolving against it is right only by
/// coincidence and wrong the moment the user's editor is on another layout. The
/// foreground thread's `HKL` is therefore cached by [`super::foreground`] on the
/// window-switch notification, for the same reason the executable name is: it is
/// a cross-thread query, and this runs in the hook callback.
///
/// # `ToUnicodeEx` mutates the dead-key buffer, and calling it twice does not undo that
///
/// This is the trap, and it is worth stating precisely, because the folklore
/// version of it is wrong.
///
/// `ToUnicodeEx` is not a query. Called on a dead key it **sets** the layout's
/// pending-composition buffer and returns a negative count. Called again with
/// the same arguments it **consumes** what it just set. So a pair of identical
/// calls leaves the buffer empty — which is net-zero only for the case where
/// this keystroke is itself a dead key. That case is handled by exactly that
/// pair.
///
/// The case a blind pair does **not** handle is a dead key already pending when
/// the next key arrives: the first call consumes that pending state in order to
/// say what the combination produces, and no second call can put it back. The
/// remedy is to re-arm it afterwards, which needs the vk/scan of the key that
/// set it — so that is remembered here and replayed below.
///
/// Whether the buffer this touches is shared with the foreground application's
/// own translation or is private to the layout handle is **not established**,
/// and it decides whether the residual risk is "GlowKey computes a wrong
/// character" or "GlowKey breaks dead keys system-wide". Phase 6 measures it on
/// a US-International layout. Until then this is the careful version of a call
/// whose semantics are known to be sharp.
fn unicode_char(vk: u16, scan_code: u32, mods: &Modifiers, altgr: bool) -> Option<char> {
    // A shortcut produces no text, and asking the layout about one wastes a call
    // on the keystroke path. AltGr is explicitly not one of these: `mods` no
    // longer reports it, and the layout below is exactly what resolves it.
    if mods.control || mods.command {
        return None;
    }

    let layout = super::foreground::keyboard_layout();
    let mut state = [0u8; 256];
    // Held modifiers go in the high bit.
    if mods.shift {
        state[VK_SHIFT as usize] = 0x80;
    }
    if mods.option {
        state[VK_MENU as usize] = 0x80;
    }
    // Caps Lock is a *toggle* and goes in the low bit. Without it every letter
    // comes back lowercase while Caps Lock is on, which loses the capital on
    // every Vietnamese word typed that way, and auto-capitalize with it.
    if is_toggled(VK_CAPITAL) {
        state[VK_CAPITAL as usize] = 0x01;
    }
    // AltGr is encoded in a keyboard layout as Ctrl+Alt, so the state array has
    // to say so even though the ladder is deliberately not told. This is the
    // other half of withholding it above: hidden from the policy, present for
    // the layout.
    if altgr {
        state[VK_CONTROL as usize] = 0x80;
        state[VK_MENU as usize] = 0x80;
    }

    let mut buf = [0u16; 8];
    let written = translate(vk, scan_code, &state, layout, &mut buf);

    if written < 0 {
        // This key IS a dead key. The call above armed the buffer; an identical
        // call consumes it, leaving the state as we found it. Remember the key so
        // a later keystroke can put it back.
        let mut scratch = [0u16; 8];
        translate(vk, scan_code, &state, layout, &mut scratch);
        set_pending_dead_key(Some((vk, scan_code)));
        // A dead key inserts nothing yet, which is the answer the ladder needs.
        return None;
    }

    // A non-negative count means the buffer is empty now — either it always was,
    // or this keystroke just consumed a pending composition to produce `buf`. If
    // something was pending, the application has not seen it yet, so re-arm it.
    if let Some((dead_vk, dead_scan)) = take_pending_dead_key() {
        let mut scratch = [0u16; 8];
        translate(dead_vk, dead_scan, &state, layout, &mut scratch);
    }

    if written == 0 {
        // No character at all — a modifier, a function key.
        return None;
    }

    // More than one unit comes back for a ligature layout, and for a base key
    // pressed with a dead key pending, which returns the accent *and* the letter.
    // The letter is the last one and it is the one the ladder needs: taking the
    // first would hand it the accent, which is not `is_ascii_alphabetic`, so the
    // ladder would read a word boundary and commit the word mid-composition.
    // Deliberately last, not first.
    String::from_utf16(&buf[..written as usize])
        .ok()
        .and_then(|s| s.chars().last())
}

/// One `ToUnicodeEx` call. Wraps the `unsafe` so the callers above read as the
/// state machine they are.
fn translate(vk: u16, scan_code: u32, state: &[u8; 256], layout: HKL, buf: &mut [u16; 8]) -> i32 {
    // SAFETY: `state` is the required 256 bytes, `buf` is written up to its
    // stated length, and `layout` is a handle owned by the system.
    unsafe {
        ToUnicodeEx(
            u32::from(vk),
            scan_code,
            state.as_ptr(),
            buf.as_mut_ptr(),
            buf.len() as i32,
            0,
            layout,
        )
    }
}

thread_local! {
    /// The dead key waiting for a base character, if any.
    ///
    /// Thread-local because it is only ever touched from the hook callback, and
    /// a lock there is exactly what `decisions/0008` forbids.
    static PENDING_DEAD_KEY: std::cell::Cell<Option<(u16, u32)>> =
        const { std::cell::Cell::new(None) };
}

fn set_pending_dead_key(key: Option<(u16, u32)>) {
    PENDING_DEAD_KEY.with(|cell| cell.set(key));
}

fn take_pending_dead_key() -> Option<(u16, u32)> {
    PENDING_DEAD_KEY.with(std::cell::Cell::take)
}

/// Renders the held modifiers compactly for the log ("Ctrl+Shift", "-").
///
/// Without this a logged `q` cannot be told apart from Ctrl+Q, which is the
/// difference between a plain keystroke and a quit. ASCII rather than the macOS
/// side's ⌘⌃⌥⇧, because this log is read in a console.
pub fn modifier_names(mods: &Modifiers) -> String {
    let mut names = String::new();
    if mods.command {
        names.push_str("Win+");
    }
    if mods.control {
        names.push_str("Ctrl+");
    }
    if mods.option {
        names.push_str("Alt+");
    }
    if mods.shift {
        names.push_str("Shift+");
    }
    if names.is_empty() {
        names.push('-');
    } else {
        names.pop(); // the trailing '+'
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One case per mapped key. The ladder branches on these, so a wrong answer
    /// here is a wrong decision — a Backspace read as `Key::Other` would take the
    /// word-character path and append instead of deleting.
    #[test]
    fn every_mapped_key_is_recognized() {
        assert_eq!(key_for(VK_BACK), Key::Backspace);
        assert_eq!(key_for(VK_DELETE), Key::ForwardDelete);
        assert_eq!(key_for(VK_ESCAPE), Key::Escape);
        assert_eq!(key_for(VK_SPACE), Key::Space);
        assert_eq!(key_for(VK_RETURN), Key::Return);
        assert_eq!(key_for(VK_TAB), Key::Tab);
    }

    /// Every one of these moves the caret with no text change, so every one must
    /// reach the ladder as the same class and flush. Listed individually rather
    /// than looped: the point is that none is missing.
    #[test]
    fn all_caret_moves_are_one_class() {
        for vk in [
            VK_LEFT, VK_RIGHT, VK_UP, VK_DOWN, VK_HOME, VK_END, VK_PRIOR, VK_NEXT,
        ] {
            assert_eq!(key_for(vk), Key::CaretMove, "vk {vk:#x} must flush");
        }
    }

    #[test]
    fn letters_map_across_the_whole_range() {
        assert_eq!(key_for(0x41), Key::Letter('a'));
        assert_eq!(key_for(0x5A), Key::Letter('z'));
        // The hotkey letters the ladder names itself: W corrects, E toggles the
        // app, Z is a toggle preset.
        assert_eq!(key_for(0x57), Key::Letter('w'));
        assert_eq!(key_for(0x45), Key::Letter('e'));
    }

    #[test]
    fn the_letter_range_has_no_neighbours() {
        // 0x40 is '@' and 0x5B is VK_LWIN. Widening the range by one in either
        // direction would turn the Windows key into a letter.
        assert_eq!(key_for(0x40), Key::Other);
        assert_eq!(key_for(0x5B), Key::Other);
    }

    #[test]
    fn digits_are_not_letters() {
        // VNI needs digits to reach the engine as characters, which they do via
        // `ch` — but they are not `Key::Letter`, and treating them as such would
        // make them hotkey-matchable by character.
        for vk in 0x30..=0x39u16 {
            assert_eq!(key_for(vk), Key::Other);
        }
    }

    #[test]
    fn modifier_names_read_as_a_shortcut() {
        let none = Modifiers::default();
        assert_eq!(modifier_names(&none), "-");
        let ctrl_shift = Modifiers {
            control: true,
            shift: true,
            ..Modifiers::default()
        };
        assert_eq!(modifier_names(&ctrl_shift), "Ctrl+Shift");
    }

    /// The pending dead key is stored and taken exactly once.
    ///
    /// Taking it twice would re-arm a composition the user already spent, putting
    /// an accent on the wrong letter — the failure the re-arm exists to prevent,
    /// in reverse.
    #[test]
    fn a_pending_dead_key_is_taken_once() {
        set_pending_dead_key(Some((0xC0, 41)));
        assert_eq!(take_pending_dead_key(), Some((0xC0, 41)));
        assert_eq!(take_pending_dead_key(), None, "taken once, not twice");
    }

    /// An AltGr keystroke must reach the engine as a character.
    ///
    /// The ladder's shortcut filter is `control || option || command`, so
    /// withholding only the synthetic Control is not enough — the Right Alt on
    /// its own still fires it, and the composition is destroyed on every AltGr
    /// key. Both halves have to be withheld, which is what
    /// `modifiers()` does and what this pins.
    #[test]
    fn altgr_reaches_the_engine_rather_than_flushing_the_word() {
        // What `modifiers()` produces for AltGr: neither half reported.
        let altgr = Modifiers {
            control: false,
            option: false,
            ..Modifiers::default()
        };
        assert!(
            !altgr.is_shortcut(),
            "AltGr must reach the engine as a character, not flush the word"
        );

        // The two near-misses, spelled out because each was a real candidate fix
        // and each still flushes.
        let only_control_withheld = Modifiers {
            control: false,
            option: true,
            ..Modifiers::default()
        };
        assert!(
            only_control_withheld.is_shortcut(),
            "withholding only the synthetic Control is not enough"
        );
        let only_alt_withheld = Modifiers {
            control: true,
            option: false,
            ..Modifiers::default()
        };
        assert!(only_alt_withheld.is_shortcut());

        // A deliberate Ctrl+Alt — with no Right Alt held — still is a shortcut.
        let ctrl_alt = Modifiers {
            control: true,
            option: true,
            ..Modifiers::default()
        };
        assert!(ctrl_alt.is_shortcut());
    }
}
