//! What the tray icon says, and the rule that it must not lie.
//!
//! `docs/decisions/0007` exists because a menu bar claiming `VI` over a dead tap
//! is a defect rather than a limitation: the user has no other way to find out,
//! and "it stopped working and told me nothing" is the worst failure an input
//! method has.
//!
//! Windows has **two ways to be silently dead that macOS does not**:
//!
//! - the hook removed by `LowLevelHooksTimeout`, which happens with no event, no
//!   error and no second chance;
//! - UIPI refusing our injection because the window in front is elevated.
//!
//! Both produce exactly the same user experience — typing does nothing — and
//! they need completely different remedies, so the indicator distinguishes them
//! in its text even though they share the `⚠` glyph.
//!
//! The state is computed here, as a pure function of four inputs, so the rule can
//! be tested without a tray, a hook, or a window.

use glowkey_engine::InputMode;

use crate::strings::t;

use super::elevation::Reach;

/// What the tray shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indicator {
    /// Vietnamese, working. `VI`.
    Vietnamese,
    /// Vietnamese is on as a mode, but the application in front is on the ignore
    /// list. Dimmed `VI`.
    ///
    /// A separate state from [`Indicator::English`] deliberately. Both mean "your
    /// keys are not being transformed", and collapsing them is what made the
    /// macOS glyph say `EN` when the user had done nothing of the sort — the
    /// ignore list being the feature this application exists for.
    ExcludedApp,
    /// Vietnamese is switched off. `EN`.
    English,
    /// GlowKey is not working, and the user cannot tell by looking at their
    /// document. `⚠`.
    Broken(Breakage),
}

/// Why GlowKey is not working. The two causes need different remedies, so they
/// stay distinct all the way to the menu text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Breakage {
    /// The keyboard hook is not installed. Either it never installed, or Windows
    /// removed it for being slow.
    ///
    /// **This is a GlowKey bug**, not a limitation, and the menu says so and
    /// offers to reinstall.
    HookGone,
    /// The window in front is at a higher integrity level, so injected input is
    /// discarded by UIPI.
    ///
    /// **This is permanent and correct**, and the menu names the window rather
    /// than apologising. GlowKey does not request elevation — see
    /// `docs/decisions/0009`.
    ElevatedWindow,
}

/// The state to display, from everything that can affect it.
///
/// Ordered by severity, and the order is the rule: a broken GlowKey reports
/// broken even while Vietnamese is switched off, because "off" invites the user
/// to switch it on and discover nothing happens. Within the two breakages the
/// dead hook wins, since it affects every window rather than the one in front.
#[must_use]
pub fn state(hook_installed: bool, reach: Reach, mode: InputMode, app_excluded: bool) -> Indicator {
    if !hook_installed {
        return Indicator::Broken(Breakage::HookGone);
    }
    if reach == Reach::BlockedByElevation {
        return Indicator::Broken(Breakage::ElevatedWindow);
    }
    // `Reach::Unknown` deliberately does NOT report broken. It is the ordinary
    // answer for a window that closed while we were looking at it, and treating
    // it as a failure would make the tray flicker `⚠` during normal use — an
    // indicator that cries wolf is a different way of not being believed.
    if mode == InputMode::English {
        return Indicator::English;
    }
    if app_excluded {
        return Indicator::ExcludedApp;
    }
    Indicator::Vietnamese
}

impl Indicator {
    /// The glyph drawn in the tray.
    #[must_use]
    pub fn glyph(self) -> &'static str {
        match self {
            Indicator::Vietnamese | Indicator::ExcludedApp => "VI",
            Indicator::English => "EN",
            Indicator::Broken(_) => "!",
        }
    }

    /// Whether the glyph is drawn dimmed.
    ///
    /// The only difference between `VI` and excluded-`VI`: same letters, less
    /// ink. It reads as "on, but not here", which is what it means.
    #[must_use]
    pub fn dimmed(self) -> bool {
        self == Indicator::ExcludedApp
    }

    /// The tooltip and menu line. This is where the two breakages separate.
    ///
    /// `app` is the executable in front, used only to name the offending window
    /// in the elevated case — the user needs to know *which* window, because the
    /// answer ("this one is elevated") does not generalise.
    #[must_use]
    pub fn describe(self, app: Option<&str>) -> String {
        match self {
            Indicator::Vietnamese => t("GlowKey — Vietnamese", "GlowKey — tiếng Việt").to_string(),
            Indicator::ExcludedApp => match app {
                Some(app) => t(
                    "GlowKey — off in {} (ignore list)",
                    "GlowKey — tắt trong {} (danh sách bỏ qua)",
                )
                .replace("{}", app),
                None => t(
                    "GlowKey — off in this app (ignore list)",
                    "GlowKey — tắt trong ứng dụng này (danh sách bỏ qua)",
                )
                .to_string(),
            },
            Indicator::English => t("GlowKey — English", "GlowKey — tiếng Anh").to_string(),
            Indicator::Broken(Breakage::HookGone) => {
                // Named as a fault of ours, because it is one. Windows removes a
                // hook whose callback is too slow without saying anything, so the
                // user's only signal is this line.
                t(
                    "GlowKey — NOT RUNNING: the keyboard hook is gone. Click to reinstall it.",
                    "GlowKey — KHÔNG CHẠY: bộ bắt phím đã mất. Bấm để cài lại.",
                )
                .to_string()
            }
            Indicator::Broken(Breakage::ElevatedWindow) => match app {
                // Named as a limitation, because it is one, and stated without
                // apology: an input method that asked for administrator rights to
                // fix this would be a worse thing than the limitation.
                Some(app) => t(
                    "GlowKey — cannot type into {}: it runs elevated, and Windows blocks \
                     input from ordinary programs into elevated windows.",
                    "GlowKey — không gõ được vào {}: ứng dụng chạy với quyền quản trị, và \
                     Windows chặn nhập liệu từ chương trình thường vào cửa sổ đó.",
                )
                .replace("{}", app),
                None => t(
                    "GlowKey — cannot type into this window: it runs elevated.",
                    "GlowKey — không gõ được vào cửa sổ này: nó chạy với quyền quản trị.",
                )
                .to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule `decisions/0007` is about: a dead hook is reported, whatever else
    /// is true. Every one of these would previously have shown a confident,
    /// wrong glyph.
    #[test]
    fn a_dead_hook_reports_broken_whatever_else_is_true() {
        for mode in [InputMode::Vietnamese, InputMode::English] {
            for excluded in [true, false] {
                assert_eq!(
                    state(false, Reach::Ok, mode, excluded),
                    Indicator::Broken(Breakage::HookGone),
                    "a dead hook must not be hidden by mode={mode:?} excluded={excluded}"
                );
            }
        }
    }

    /// English mode with a dead hook must still say broken. This is the case that
    /// looks most reasonable to collapse and is the most harmful: `EN` invites
    /// the user to switch Vietnamese on and find that nothing happens.
    #[test]
    fn english_does_not_mask_a_dead_hook() {
        assert_eq!(
            state(false, Reach::Ok, InputMode::English, false),
            Indicator::Broken(Breakage::HookGone)
        );
    }

    #[test]
    fn an_elevated_window_reports_broken() {
        assert_eq!(
            state(
                true,
                Reach::BlockedByElevation,
                InputMode::Vietnamese,
                false
            ),
            Indicator::Broken(Breakage::ElevatedWindow)
        );
    }

    /// A dead hook outranks an elevated window: the hook affects every window,
    /// the elevation only this one, and the remedies are different.
    #[test]
    fn the_dead_hook_outranks_the_elevated_window() {
        assert_eq!(
            state(
                false,
                Reach::BlockedByElevation,
                InputMode::Vietnamese,
                false
            ),
            Indicator::Broken(Breakage::HookGone)
        );
    }

    /// `Unknown` is not a failure. A window that closed while we were examining
    /// it is ordinary, and reporting `⚠` for it would make the tray flicker
    /// during normal use.
    #[test]
    fn an_unexaminable_window_is_not_reported_as_broken() {
        assert_eq!(
            state(true, Reach::Unknown, InputMode::Vietnamese, false),
            Indicator::Vietnamese
        );
    }

    /// Excluded and English are different states with the same practical effect,
    /// and must stay different. Collapsing them is the macOS bug this repo's UI
    /// pass fixed.
    #[test]
    fn excluded_is_not_english() {
        let excluded = state(true, Reach::Ok, InputMode::Vietnamese, true);
        let english = state(true, Reach::Ok, InputMode::English, false);
        assert_eq!(excluded, Indicator::ExcludedApp);
        assert_eq!(english, Indicator::English);
        assert_ne!(excluded, english);
        // Same letters, less ink — that is the whole visual difference.
        assert_eq!(excluded.glyph(), "VI");
        assert!(excluded.dimmed());
        assert!(!state(true, Reach::Ok, InputMode::Vietnamese, false).dimmed());
    }

    /// The two breakages must not produce the same sentence: their remedies are
    /// "click to reinstall" and "nothing, this is permanent".
    #[test]
    fn the_two_breakages_say_different_things() {
        let hook = Indicator::Broken(Breakage::HookGone).describe(Some("notepad.exe"));
        let uipi = Indicator::Broken(Breakage::ElevatedWindow).describe(Some("taskmgr.exe"));
        assert_ne!(hook, uipi);
        assert!(uipi.contains("taskmgr.exe"), "name the offending window");
        assert!(
            hook.contains("reinstall"),
            "a dead hook has a remedy and must offer it"
        );
    }

    /// The working states never claim a problem, which is the other half of not
    /// lying.
    #[test]
    fn a_working_glowkey_reports_no_problem() {
        for indicator in [
            Indicator::Vietnamese,
            Indicator::ExcludedApp,
            Indicator::English,
        ] {
            let described = indicator.describe(Some("notepad.exe"));
            assert!(!described.contains("NOT RUNNING"));
            assert!(!described.contains("cannot type"));
        }
    }
}
