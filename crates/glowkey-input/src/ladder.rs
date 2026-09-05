//! The decision ladder: what to do with one key-down event.
//!
//! **The ordering is the specification.** The steps below are not in a
//! convenient order, they are in the order they were argued into. Each one is a
//! bug that reached a user and was fixed by moving a question ahead of another
//! question:
//!
//! 1. the hotkeys, **before** the shortcut filter — a flush destroys the memory
//!    ⌃⇧W needs, and the filter flushes;
//! 2. the shortcut filter, which flushes and passes through;
//! 3. the Backspace ladder, five cases, exhaustive with no catch-all arm;
//! 4. caret moves flush;
//! 5. word character versus boundary, with **every** letter suppressed and
//!    re-emitted;
//! 6. the boundary key replayed after a restore, never passed through natively.
//!
//! Moving any of them is a behaviour change even when the diff looks like tidying.
//!
//! The one thing that is *not* here is hotkey recording. Recording produces a
//! value shaped by the platform — the virtual key code it reported for the key the
//! user pressed — so the platform runs [`crate::hotkey::capture`] itself, ahead of
//! this function, exactly where the recording branch used to sit.

use glowkey_session::{BackspaceOutcome, BoundaryBackspace, InputMethod, Session};

use crate::decision::{Decision, Effects};
use crate::event::{Key, KeyEvent};
use crate::hotkey::{self, Hotkey};

/// What the platform must supply but the policy must not fetch for itself.
///
/// Data, never a trait object: the point of this crate is that the policy can be
/// called from a test with no operating system under it, and a callback is an
/// operating system waiting to happen. Anything the ladder needs that is not a
/// plain value is a sign the boundary is in the wrong place — push it back to the
/// platform.
///
/// It holds one field. The frontmost application, which looked like it belonged
/// here, turned out not to: the session already knows it, and asking twice is how
/// the two answers start to disagree.
#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    /// The VN/EN toggle hotkey, already resolved for this platform.
    pub toggle_hotkey: Hotkey,
}

/// Decides what to do with one key-down event: pass it through, suppress it, or
/// suppress it and emit an edit.
///
/// Pure with respect to the outside world — no event synthesis, no window-server
/// query, no disk. Anything the platform has to *do* is reported through
/// `effects`, which the caller is expected to clear first and to carry out in
/// field order the moment this returns.
pub fn decide(
    session: &mut Session,
    event: &KeyEvent,
    ctx: &Ctx,
    effects: &mut Effects,
) -> Decision {
    // ── 1. Hotkeys, ahead of the shortcut filter ────────────────────────────

    // VN/EN toggle hotkey (user-configurable preset): flip mode and consume the
    // key. Checked before the shortcut filter, since it is one.
    if ctx.toggle_hotkey.matches(event) {
        let mode = session.toggle_mode();
        effects.mode_toggled = Some(mode);
        // The persistent menu-bar glyph too: the toggle happened here rather than
        // via the menu, so nothing else is going to repaint it.
        effects.refresh_glyph = true;
        return Decision::Consume;
    }

    // Per-app toggle hotkey (⌃⇧E): enable/disable Vietnamese for the current app
    // in one keystroke, without opening the menu.
    if hotkey::is_app_toggle(event) {
        return Decision::ToggleApp;
    }

    // ⌃⇧W: correct the word just typed and remember the decision. Checked here,
    // with the other ⌃⇧ hotkeys, because the shortcut filter below would
    // otherwise flush and pass it through — and a flush is exactly what destroys
    // the memory this needs.
    if hotkey::is_correction(event) {
        // Never in an excluded app: excluded means hands off, and this edit
        // rewrites text that is already on screen.
        if !session.is_active() {
            return Decision::Passthrough;
        }
        let described = session.correctable_word();
        return match session.correct_last_word() {
            Some(edit) => {
                // The decision is in memory only until the platform writes it;
                // saying so here rather than saving is what keeps this function
                // free of disk side effects, and therefore drivable by a test
                // against the user's real settings without touching the file.
                effects.save_settings = true;
                effects.personal_words_changed = true;
                effects.corrected = described;
                Decision::Emit(edit)
            }
            // Nothing to correct: no word remembered, or the caret has moved
            // since. Consumed either way, which is a real trade-off rather than a
            // free choice: a Control-modified key inserts no text, so passing it
            // through would not put a stray `W` in the document — it would hand
            // ⌃⇧W to the focused app. Swallowing it everywhere GlowKey is active
            // is the price of the hotkey being fixed rather than configurable.
            None => Decision::Consume,
        };
    }

    // ── 2. The shortcut filter ──────────────────────────────────────────────

    if event.mods.is_shortcut() {
        // A shortcut may move the caret or change the selection (⌘A select-all,
        // ⌘V paste, ⌘←). Flush so a later edit is not computed against a stale
        // baseline, then let it through.
        session.flush();
        return Decision::Passthrough;
    }

    // Normally an inactive session means hands off entirely. The exception is
    // UniKey's always-macro: Vietnamese is off, but a shortcut should still
    // expand, which needs the keys to reach the engine.
    if !session.is_active() && !session.macros_active() {
        return Decision::Passthrough;
    }

    // ── 3. The Backspace ladder ─────────────────────────────────────────────

    if event.key == Key::Backspace {
        // Usually the host performs the delete and we only re-sync the engine to
        // whatever the screen will then show — but not always: undoing a
        // spell-check escape suppresses the key and rewrites the word instead.
        // Five cases, in order:
        //   - deleting the boundary right after a committed word re-composes it
        //     so the next keys keep editing it (hồng␣⌫z → hông);
        //   - deleting a boundary with no word in front of it (the ␣ of
        //     `hồng, `) removes nothing the engine composed, and the word is one
        //     more Backspace away;
        //   - mid-word, shrink the composition by one visible character and stay
        //     composed, so the next key is still a Telex key rather than a
        //     literal (hoongf⌫z → hôn, not hồnz);
        //   - undoing a spell-check escape rewrites the word in one edit and
        //     swallows the keystroke (`hoongfa`⌫ → `hồng`);
        //   - if the engine cannot stay in step, flush and stop composing.
        //
        // Exhaustive here for the same reason as the match below: the two in-step
        // answers differ from "nothing remembered" only in that the caller must
        // not flush, and a `bool` that hid that difference is what made
        // `hoongf, ⌫⌫z` produce `hồngz`.
        match session.recompose_after_boundary_backspace() {
            BoundaryBackspace::Reopened | BoundaryBackspace::BoundaryRemoved => {
                return Decision::Passthrough;
            }
            BoundaryBackspace::NotApplicable => {}
        }
        // Exhaustive on purpose — no catch-all arm. A future outcome falling
        // through as a plain delete is the failure this path is most exposed to,
        // and the compiler is the only thing that reliably stops it.
        return match session.backspace_visible_char() {
            // The escape lifted: the word transforms again. Suppress the
            // keystroke and emit the whole repair ourselves — the user's delete is
            // accounted for inside the edit. Letting the host delete and posting
            // this afterwards would mix a native keystroke with a synthesized
            // edit, which is exactly the race the full-suppression model exists to
            // remove.
            BackspaceOutcome::Repair(edit) => Decision::Emit(edit),
            BackspaceOutcome::InStep => Decision::Passthrough,
            BackspaceOutcome::Flush => {
                session.flush();
                Decision::Passthrough
            }
        };
    }

    // ── 4. Caret moves flush ────────────────────────────────────────────────

    if event.key == Key::CaretMove {
        // Arrow / Home / End / Page keys move the caret without our knowledge, so
        // the engine's diff baseline (and any re-composition memory) is now stale.
        // Flush and let the key through — same contract as a mouse click.
        session.flush();
        return Decision::Passthrough;
    }

    // ── 5. Word character versus boundary ───────────────────────────────────

    // A word-extending character is a letter always, plus a digit in VNI (where
    // digits carry tone/diacritic marks — `viet65` → việt). Everything else is a
    // word boundary.
    let is_word_char = |ch: char| {
        ch.is_ascii_alphabetic()
            || (ch.is_ascii_digit() && session.input_method() == InputMethod::Vni)
            // With the bracket shortcuts on these are vowel keys, so they must
            // reach the engine instead of committing the word. Off (the default)
            // they stay ordinary punctuation and `[` types a bracket.
            || (session.telex_brackets()
                && session.input_method() == InputMethod::Telex
                && matches!(ch, '[' | ']' | '{' | '}'))
    };
    match event.ch {
        Some(ch) if is_word_char(ch) => {
            let response = session.process_key(ch);
            if !response.handled {
                return Decision::Passthrough;
            }
            // Suppress the key and synthesize the edit — for EVERY letter,
            // including a plain append (`{backspaces:0, insert:ch}`). This is the
            // crux of correctness: mixing native passthrough with synthesized
            // edits races, because a natively-typed character and a synthesized
            // backspace posted a moment later reach the document out of order (the
            // app→renderer path in multiprocess apps like Chrome is asynchronous).
            // The symptom is the first transform after a letter landing wrong:
            // `aa` → `aâ`, `hoongf` → `hoồng`.
            //
            // With every letter suppressed and re-emitted from one tagged source,
            // all document mutations flow through a single ordered queue, so a
            // backspace can never overtake the character it deletes. This is how
            // EVKey/OpenKey drive the document.
            Decision::Emit(response)
        }
        // ── 6. The boundary key, replayed after a restore ───────────────────
        //
        // A word boundary (space, punctuation, Telex digit): commit the word. If
        // auto-fix restores an invalid result to its raw keys, emit that edit and
        // replay the boundary key after it; otherwise the word is already on
        // screen and the boundary key just passes through.
        Some(ch) => {
            let restore = session.commit();
            // Sentence-ending punctuation primes the next word for capitalization.
            session.note_boundary(ch);
            match restore {
                Some(restore) => Decision::EmitThenReplayKey(restore),
                None => Decision::Passthrough,
            }
        }
        None => Decision::Passthrough,
    }
}
