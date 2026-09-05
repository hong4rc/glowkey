//! `handle` against a recording platform: every `Decision` variant reaches the
//! port through the calls a shell has to implement, in the order it has to
//! implement them.

use glowkey_input::{
    handle, hotkey, Ctx, Decision, HotkeyPreset, KeyEvent, Modifiers, Notice, Platform,
};
use glowkey_session::{
    AppId, ExclusionDefaults, ExclusionList, ExclusionToggle, InputMode, PlacementStyle, Session,
};

const TERMINAL: &str = "example.terminal";
const EDITOR: &str = "example.editor";

/// What a shell would have done, written down instead.
#[derive(Debug, Default, PartialEq, Eq)]
struct Recorded {
    injected: Vec<(usize, String)>,
    replays: usize,
    saves: usize,
    indicator: usize,
    notices: Vec<String>,
}

/// The recording shell. `app` is what it claims is in front.
#[derive(Debug, Default)]
struct Recorder {
    app: Option<AppId>,
    log: Recorded,
}

impl Platform for Recorder {
    fn inject(&mut self, backspaces: usize, text: &str) {
        self.log.injected.push((backspaces, text.to_string()));
    }
    fn replay_key(&mut self) {
        self.log.replays += 1;
    }
    fn app_in_front(&mut self) -> Option<AppId> {
        self.app.clone()
    }
    fn request_save(&mut self) {
        self.log.saves += 1;
    }
    fn request_indicator(&mut self) {
        self.log.indicator += 1;
    }
    fn notify(&mut self, notice: Notice<'_>) {
        self.log.notices.push(match notice {
            Notice::Decided { decision, .. } => format!("decided {decision}"),
            Notice::ModeToggled(mode) => format!("mode {mode:?}"),
            Notice::PersonalWordsChanged => "personal words".into(),
            Notice::Corrected { was, becomes } => format!("corrected {was}->{becomes}"),
            Notice::AppToggled { app, outcome } => format!("app {app} {outcome:?}"),
            Notice::NoAppInFront => "no app".into(),
            _ => "other".into(),
        });
    }
}

fn ctx() -> Ctx {
    Ctx {
        toggle_hotkey: hotkey::resolve(HotkeyPreset::default(), None),
    }
}

fn session_in(app: &str) -> Session {
    let defaults = ExclusionDefaults::new([TERMINAL, EDITOR], [TERMINAL]);
    let mut session = Session::new(
        PlacementStyle::default(),
        ExclusionList::with_defaults(defaults),
    );
    session.set_frontmost_app(app);
    session
}

fn ctrl_shift(letter: char) -> KeyEvent {
    KeyEvent::character(letter).with_mods(Modifiers {
        control: true,
        shift: true,
        option: false,
        command: false,
    })
}

#[test]
fn a_letter_is_suppressed_and_injected() {
    let mut session = session_in("example.textedit");
    let mut shell = Recorder::default();
    let decision = handle(&mut session, &KeyEvent::character('a'), &ctx(), &mut shell);
    assert!(matches!(decision, Decision::Emit(_)));
    assert!(decision.suppresses());
    assert_eq!(shell.log.injected, vec![(0, "a".to_string())]);
    assert_eq!(shell.log.replays, 0);
    assert_eq!(shell.log.notices, vec!["decided Emit bs=0 ins=\"a\""]);
}

#[test]
fn a_passthrough_touches_nothing_but_the_log() {
    // An excluded application: every key passes through.
    let mut session = session_in(EDITOR);
    let mut shell = Recorder::default();
    let decision = handle(&mut session, &KeyEvent::character('a'), &ctx(), &mut shell);
    assert!(matches!(decision, Decision::Passthrough));
    assert!(!decision.suppresses());
    assert_eq!(shell.log.notices, vec!["decided Passthrough"]);
    assert_eq!(
        shell.log,
        Recorded {
            notices: shell.log.notices.clone(),
            ..Recorded::default()
        }
    );
}

#[test]
fn the_mode_hotkey_is_consumed_and_announced_and_repaints_the_indicator() {
    let mut session = session_in("example.textedit");
    let mut shell = Recorder::default();
    let decision = handle(
        &mut session,
        &KeyEvent::key(glowkey_input::Key::Space).with_mods(Modifiers {
            control: true,
            shift: true,
            option: false,
            command: false,
        }),
        &ctx(),
        &mut shell,
    );
    assert!(matches!(decision, Decision::Consume));
    assert_eq!(session.mode(), InputMode::English);
    assert_eq!(shell.log.notices, vec!["decided Consume", "mode English"]);
    assert_eq!(shell.log.indicator, 1);
    assert_eq!(shell.log.saves, 0);
    assert!(shell.log.injected.is_empty());
}

#[test]
fn the_app_toggle_asks_the_shell_which_app_and_saves_a_permanent_change() {
    let mut session = session_in("example.textedit");
    let mut shell = Recorder {
        app: Some(AppId::from("example.textedit")),
        ..Recorder::default()
    };
    let decision = handle(&mut session, &ctrl_shift('E'), &ctx(), &mut shell);
    assert!(matches!(decision, Decision::ToggleApp));
    assert!(session.exclusions().is_excluded("example.textedit"));
    assert_eq!(
        shell.log.notices,
        vec!["decided ToggleApp", "app example.textedit Excluded"]
    );
    assert_eq!((shell.log.saves, shell.log.indicator), (1, 1));
}

#[test]
fn a_terminal_re_enabled_by_hotkey_is_not_saved() {
    let mut session = session_in(TERMINAL);
    let mut shell = Recorder {
        app: Some(AppId::from(TERMINAL)),
        ..Recorder::default()
    };
    handle(&mut session, &ctrl_shift('E'), &ctx(), &mut shell);
    assert!(shell.log.notices.contains(&format!(
        "app {TERMINAL} {:?}",
        ExclusionToggle::EnabledSessionOnly
    )));
    assert_eq!(
        shell.log.saves, 0,
        "a session-only change must not reach the file"
    );
    assert_eq!(shell.log.indicator, 1);
}

#[test]
fn the_app_toggle_with_no_app_known_changes_nothing() {
    let mut session = session_in("example.textedit");
    let mut shell = Recorder::default();
    let decision = handle(&mut session, &ctrl_shift('E'), &ctx(), &mut shell);
    assert!(
        decision.suppresses(),
        "the key is GlowKey's even when it does nothing"
    );
    assert_eq!(shell.log.notices, vec!["decided ToggleApp", "no app"]);
    assert_eq!((shell.log.saves, shell.log.indicator), (0, 0));
    assert!(!session.exclusions().is_excluded("example.textedit"));
}

#[test]
fn an_auto_fix_restore_injects_then_replays_the_boundary_key() {
    let mut session = session_in("example.textedit");
    let mut shell = Recorder::default();
    for ch in "work".chars() {
        handle(&mut session, &KeyEvent::character(ch), &ctx(), &mut shell);
    }
    shell.log = Recorded::default();
    let decision = handle(&mut session, &KeyEvent::character(' '), &ctx(), &mut shell);
    assert!(matches!(decision, Decision::EmitThenReplayKey(_)));
    assert_eq!(shell.log.injected.len(), 1, "one edit: the restore");
    assert_eq!(shell.log.injected[0].1, "work");
    assert_eq!(
        shell.log.replays, 1,
        "the space is typed again after the edit"
    );
}

#[test]
fn the_correction_hotkey_reports_the_swap_and_asks_for_a_save() {
    let mut session = session_in("example.textedit");
    let mut shell = Recorder::default();
    // `was` renders as valid Vietnamese (`ứa`) and so is not auto-fixed; the
    // correction hotkey swaps it back and remembers the word.
    for ch in "was ".chars() {
        handle(&mut session, &KeyEvent::character(ch), &ctx(), &mut shell);
    }
    shell.log = Recorded::default();
    let decision = handle(&mut session, &ctrl_shift('W'), &ctx(), &mut shell);
    assert!(matches!(decision, Decision::Emit(_)), "got {decision:?}");
    assert!(
        shell
            .log
            .notices
            .iter()
            .any(|n| n.starts_with("corrected ")),
        "{:?}",
        shell.log.notices
    );
    assert!(shell.log.notices.contains(&"personal words".to_string()));
    assert_eq!(shell.log.saves, 1);
    // The notices come before the injection, so a log reads cause before effect.
    assert_eq!(shell.log.injected.len(), 1);
}

#[test]
fn the_decided_notice_carries_the_session_before_the_change() {
    struct Watcher(Vec<bool>);
    impl Platform for Watcher {
        fn inject(&mut self, _: usize, _: &str) {}
        fn replay_key(&mut self) {}
        fn app_in_front(&mut self) -> Option<AppId> {
            Some(AppId::from("example.textedit"))
        }
        fn request_save(&mut self) {}
        fn request_indicator(&mut self) {}
        fn notify(&mut self, notice: Notice<'_>) {
            if let Notice::Decided { session, .. } = notice {
                self.0.push(session.is_active());
            }
        }
    }
    let mut session = session_in("example.textedit");
    let mut shell = Watcher(Vec::new());
    handle(&mut session, &ctrl_shift('E'), &ctx(), &mut shell);
    // Active at decision time; excluded afterwards.
    assert_eq!(shell.0, vec![true]);
    assert!(!session.is_active());
}
