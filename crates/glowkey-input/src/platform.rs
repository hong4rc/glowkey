//! The port a shell implements, and the one call it makes per key.
//!
//! [`decide`] answers *what* to do with a key; carrying the answer out is the
//! operating system's job, and until now each shell wrote that part on its own.
//! [`Platform`] names it. A shell implements the handful of things every shell
//! has to be able to do, calls [`handle`] once per key-down, and gets back the
//! [`Decision`] so it can tell the OS whether to suppress the original key.
//!
//! The trait is deliberately small. Every method is something both shipping
//! shells already did; nothing here is speculative. What a shell shows the user
//! about an event (a HUD, a tray tooltip, a log line) is not policy, so it goes
//! through one loosely typed channel, [`Platform::notify`], with a default that
//! does nothing.

use std::fmt;

use glowkey_session::{AppId, ExclusionToggle, InputMode, Session};

use crate::decision::{Decision, Effects};
use crate::event::KeyEvent;
use crate::ladder::{decide, Ctx};

/// What a shell must be able to do for the policy.
///
/// Every method is called from inside the platform's key callback, while the
/// original keystroke is being dispatched. None of them may block on anything
/// outside the process, and none may touch the disk: a save is *requested*
/// here and performed by the shell after the callback has returned
/// (`docs/decisions/0008`). Both shipping shells set a flag and wake their loop.
pub trait Platform {
    /// Replace `backspaces` UTF-16 code units before the caret with `text`,
    /// from GlowKey's own event source so the edit lands in order.
    fn inject(&mut self, backspaces: usize, text: &str);

    /// Type the key that started this call again, from GlowKey's own source,
    /// so it lands *after* an edit just injected instead of racing it. The
    /// shell knows which key that was; the policy does not need to.
    fn replay_key(&mut self);

    /// The application in front, for the per-application toggle. A shell that
    /// can afford a fresh query answers it now (macOS); one that cannot answers
    /// from its cache (Windows). `None` when nothing is known yet.
    fn app_in_front(&mut self) -> Option<AppId>;

    /// Something changed that has to survive a quit. Set a flag; never write
    /// here.
    fn request_save(&mut self);

    /// The indicator (menu bar, tray) no longer reflects the state.
    fn request_indicator(&mut self);

    /// Something the user might be shown or the log should record. The default
    /// ignores it; a shell handles the notices it has a surface for.
    fn notify(&mut self, notice: Notice<'_>) {
        let _ = notice;
    }
}

/// Something that happened while handling a key, for the shell to show or log.
///
/// Not `Debug`: it borrows the session, which is not.
#[non_exhaustive]
pub enum Notice<'a> {
    /// The policy decided. Sent before anything is carried out, with the
    /// session as it stood at that moment, so a log line can record the state
    /// that led to the decision rather than the state after it.
    Decided {
        /// The key that was handled.
        event: &'a KeyEvent,
        /// What the policy decided.
        decision: &'a Decision,
        /// The session before the decision was carried out.
        session: &'a Session,
    },
    /// The VN/EN mode was toggled to this.
    ModeToggled(InputMode),
    /// The personal-words list changed, so any open editor should reload.
    PersonalWordsChanged,
    /// A word was corrected by the correction hotkey.
    Corrected {
        /// What was on screen.
        was: &'a str,
        /// What replaced it.
        becomes: &'a str,
    },
    /// The application in front was toggled in the ignore list.
    AppToggled {
        /// The application.
        app: &'a AppId,
        /// What the toggle did.
        outcome: ExclusionToggle,
    },
    /// The toggle hotkey was pressed before any application was known. The key
    /// is consumed anyway (it is GlowKey's), but nothing changed.
    NoAppInFront,
}

/// Handles one key-down event end to end: decides, then carries the decision
/// and its effects out through `platform`. Returns the decision so the shell can
/// tell the OS whether to suppress the original key (see
/// [`Decision::suppresses`]).
///
/// The order is fixed and is part of the contract: the [`Notice::Decided`]
/// first, so a log reads cause before consequence and the line is on disk
/// before an emit path that might panic; then the [`Effects`] in field order;
/// then the decision itself.
pub fn handle<P: Platform + ?Sized>(
    session: &mut Session,
    event: &KeyEvent,
    ctx: &Ctx,
    platform: &mut P,
) -> Decision {
    let mut effects = Effects::default();
    let decision = decide(session, event, ctx, &mut effects);
    platform.notify(Notice::Decided {
        event,
        decision: &decision,
        session,
    });

    if let Some(mode) = effects.mode_toggled {
        platform.notify(Notice::ModeToggled(mode));
    }
    if effects.personal_words_changed {
        platform.notify(Notice::PersonalWordsChanged);
    }
    if let Some((was, becomes)) = &effects.corrected {
        platform.notify(Notice::Corrected { was, becomes });
    }
    if effects.refresh_glyph {
        platform.request_indicator();
    }
    if effects.save_settings {
        platform.request_save();
    }

    match &decision {
        Decision::Passthrough | Decision::Consume => {}
        Decision::ToggleApp => match platform.app_in_front() {
            Some(app) => {
                let outcome = session.toggle_app_exclusion(app.as_str());
                platform.notify(Notice::AppToggled { app: &app, outcome });
                // A session-only suspension changes nothing persisted: by design
                // the saved list still excludes the terminal.
                if outcome != ExclusionToggle::EnabledSessionOnly {
                    platform.request_save();
                }
                // The ladder returns `ToggleApp` without asking for a repaint; it
                // cannot know the platform has an indicator.
                platform.request_indicator();
            }
            None => platform.notify(Notice::NoAppInFront),
        },
        Decision::Emit(response) => {
            platform.inject(response.backspaces, &response.insert);
        }
        Decision::EmitThenReplayKey(response) => {
            platform.inject(response.backspaces, &response.insert);
            // Replayed from GlowKey's own queue rather than passed through.
            // Letting the original through loses the race: it is the event being
            // dispatched right now, so the host applies it *before* the
            // backspaces just queued, and the edit eats the boundary key instead
            // of the word it meant to replace (`ddc`␣ → `đddc`).
            platform.replay_key();
        }
    }
    decision
}

impl Decision {
    /// Whether the original key must be suppressed. Everything but a
    /// passthrough is: the edit, the replay, or nothing at all is GlowKey's to
    /// type instead.
    #[must_use]
    pub fn suppresses(&self) -> bool {
        !matches!(self, Self::Passthrough)
    }
}

/// The decision as both shells write it to their logs.
impl fmt::Display for Decision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Passthrough => f.write_str("Passthrough"),
            Self::Consume => f.write_str("Consume"),
            Self::ToggleApp => f.write_str("ToggleApp"),
            Self::Emit(r) => write!(f, "Emit bs={} ins={:?}", r.backspaces, r.insert),
            Self::EmitThenReplayKey(r) => {
                write!(
                    f,
                    "EmitThenReplayKey bs={} ins={:?}",
                    r.backspaces, r.insert
                )
            }
        }
    }
}
