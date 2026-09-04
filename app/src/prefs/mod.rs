//! The Settings window: a single non-resizable pane for the typing preferences
//! (tone-mark style, auto-fix) and the "Excluded apps" list, per `docs/ui-design.md`.
//!
//! Like the menu bar and tap, this is objc2 AppKit and can only be verified by
//! running. All controls are native (`NSSegmentedControl`, `NSButton`,
//! `NSTextField`, `NSStackView`), so focus, keyboard navigation, and light/dark
//! parity come for free. Every change applies to the live session and is persisted
//! immediately — there is no Apply/OK button, matching macOS Settings behaviour.
//!
//! Apps can be added to the ignore list three ways: the "Add App…" picker here, the
//! menu bar's "Disable for <App>", or ⌃⇧E while in the app. This window lists what
//! is excluded, lets you add/remove entries, and adjusts the typing options.

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSAlert, NSApplication, NSControlStateValueOn, NSModalResponseOK, NSOpenPanel,
    NSSegmentedControl, NSStackView, NSTextField, NSWindow,
};
use objc2_foundation::{MainThreadMarker, NSBundle, NSString, NSURL};

use std::cell::RefCell;

use glowkey_engine::{HotkeyPreset, InputMethod, Language, PlacementStyle};

use crate::strings::t;
use crate::tap::TapState;

/// Ivars for the Settings window controller: the shared `TapState`, the window
/// (built lazily on first open), the vertical stack that holds the excluded-app
/// rows, and the ordered bundle-id list that maps a row's "Remove" button (by its
/// integer tag) back to the app it removes.
pub struct PrefsIvars {
    state: *const TapState,
    window: RefCell<Option<Retained<NSWindow>>>,
    /// Separate window holding the excluded-app list, so it stays off the main
    /// Settings pane (advanced/rare, opened via "Manage Excluded Apps…").
    excluded_window: RefCell<Option<Retained<NSWindow>>>,
    /// Windows replaced by a language change, held until the next rebuild.
    /// Releasing one inside the action of a control it owns would free the view
    /// tree under the AppKit frames still unwinding that click.
    retired_windows: RefCell<Vec<Retained<NSWindow>>>,
    list_stack: RefCell<Option<Retained<NSStackView>>>,
    apps: RefCell<Vec<String>>,
    /// Separate window for text-expansion macros (gõ tắt), with its input fields,
    /// list stack, and the ordered shortcuts (for remove-by-tag).
    macros_window: RefCell<Option<Retained<NSWindow>>>,
    macro_shortcut: RefCell<Option<Retained<NSTextField>>>,
    macro_expansion: RefCell<Option<Retained<NSTextField>>>,
    macros_list: RefCell<Option<Retained<NSStackView>>>,
    macro_count: RefCell<usize>,
    /// Separate window for per-word decisions about the English/Telex ambiguity,
    /// with its input field, list stack, and the ordered keys (for tag lookup).
    words_window: RefCell<Option<Retained<NSWindow>>>,
    word_keys: RefCell<Option<Retained<NSTextField>>>,
    words_list: RefCell<Option<Retained<NSStackView>>>,
    word_order: RefCell<Vec<String>>,
    /// Toggle-hotkey controls, kept so the recorder can reflect a captured combo:
    /// the preset segmented control and the "Current: …" label.
    hotkey_seg: RefCell<Option<Retained<NSSegmentedControl>>>,
    hotkey_label: RefCell<Option<Retained<NSTextField>>>,
}

mod excluded;
mod macros_window;
mod personal_words;
mod tabs;
mod widgets;

pub(crate) use widgets::hotkey_display;

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "GlowKeyPrefsController"]
    #[ivars = PrefsIvars]
    pub struct PrefsController;

    unsafe impl NSObjectProtocol for PrefsController {}

    impl PrefsController {
        /// Tone-mark style changed on the segmented control (0 = Modern, 1 = Classic).
        #[unsafe(method(toneChanged:))]
        fn tone_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let seg: isize = unsafe { msg_send![sender, selectedSegment] };
            let style = if seg == 0 {
                PlacementStyle::New
            } else {
                PlacementStyle::Old
            };
            self.state().set_style_and_save(style);
        }

        /// Input method changed (0 = Telex, 1 = VNI, 2 = Simple Telex).
        #[unsafe(method(inputMethodChanged:))]
        fn input_method_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let seg: isize = unsafe { msg_send![sender, selectedSegment] };
            let method = match seg {
                1 => InputMethod::Vni,
                2 => InputMethod::SimpleTelex,
                _ => InputMethod::Telex,
            };
            self.state().set_input_method_and_save(method);
        }

        /// Toggle-hotkey segment clicked (0..=3 presets; 4 = "Custom…" arms the
        /// recorder — the tap captures the next ⌃/⌥ combo, Esc cancels).
        #[unsafe(method(hotkeyChanged:))]
        fn hotkey_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let seg: isize = unsafe { msg_send![sender, selectedSegment] };
            let preset = match seg {
                0 => HotkeyPreset::CtrlShiftSpace,
                1 => HotkeyPreset::CtrlSpace,
                2 => HotkeyPreset::OptionSpace,
                3 => HotkeyPreset::CtrlShiftZ,
                _ => {
                    // "Custom…": arm the recorder. The label is the prompt; the
                    // segment snaps to the real state when recording ends.
                    if !self.state().is_recording_hotkey() {
                        self.state().begin_hotkey_recording();
                        if let Some(label) = self.ivars().hotkey_label.borrow().as_ref() {
                            label.setStringValue(&NSString::from_str(
                                t("Press a ⌃/⌥ combo… (Esc cancels)", "Bấm tổ hợp ⌃/⌥… (Esc để hủy)"),
                            ));
                        }
                    }
                    return;
                }
            };
            self.state().set_toggle_hotkey_and_save(preset);
            self.refresh_hotkey_ui();
        }

        /// Interface language segment clicked (0 = System, 1 = Vietnamese, 2 = English).
        /// Rebuilds the windows, since every label in them is now the wrong language.
        #[unsafe(method(languageChanged:))]
        fn language_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let seg: isize = unsafe { msg_send![sender, selectedSegment] };
            let language = match seg {
                1 => Language::Vietnamese,
                2 => Language::English,
                _ => Language::System,
            };
            self.state().set_language_and_save(language);
            self.rebuild_windows();
        }

        /// Quick Telex checkbox toggled.
        #[unsafe(method(quickTelexChanged:))]
        fn quick_telex_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let state: isize = unsafe { msg_send![sender, state] };
            self.state()
                .set_quick_telex_and_save(state == NSControlStateValueOn);
        }

        /// "Import…" — read a macro table and merge it into the list.
        #[unsafe(method(importMacros:))]
        fn import_macros(&self, _sender: Option<&AnyObject>) {
            macros_window::import_macros(self);
        }

        /// "Export…" — write the current macro table to a file.
        #[unsafe(method(exportMacros:))]
        fn export_macros(&self, _sender: Option<&AnyObject>) {
            macros_window::export_macros(self);
        }

        /// "Expand macros even when Vietnamese is off" checkbox toggled.
        #[unsafe(method(alwaysMacroChanged:))]
        fn always_macro_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let state: isize = unsafe { msg_send![sender, state] };
            self.state()
                .set_always_macro_and_save(state == NSControlStateValueOn);
        }

        /// Mid-word spell check checkbox toggled.
        #[unsafe(method(strictSpellCheckChanged:))]
        fn strict_spell_check_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let state: isize = unsafe { msg_send![sender, state] };
            self.state()
                .set_strict_spell_check_and_save(state == NSControlStateValueOn);
        }

        /// Telex bracket shortcuts checkbox toggled.
        #[unsafe(method(telexBracketsChanged:))]
        fn telex_brackets_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let state: isize = unsafe { msg_send![sender, state] };
            self.state()
                .set_telex_brackets_and_save(state == NSControlStateValueOn);
        }

        /// "Restore common English words" checkbox toggled.
        #[unsafe(method(englishRestoreChanged:))]
        fn english_restore_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let state: isize = unsafe { msg_send![sender, state] };
            self.state()
                .set_restore_english_words_and_save(state == NSControlStateValueOn);
        }

        /// Auto-fix checkbox toggled.
        #[unsafe(method(autoFixChanged:))]
        fn auto_fix_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let state: isize = unsafe { msg_send![sender, state] };
            self.state().set_auto_fix_and_save(state == NSControlStateValueOn);
        }

        /// Auto-capitalize checkbox toggled.
        #[unsafe(method(autoCapitalizeChanged:))]
        fn auto_capitalize_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let state: isize = unsafe { msg_send![sender, state] };
            self.state()
                .set_auto_capitalize_and_save(state == NSControlStateValueOn);
        }

        /// "Open at launch" checkbox toggled.
        #[unsafe(method(openAtLaunchChanged:))]
        fn open_at_launch_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let state: isize = unsafe { msg_send![sender, state] };
            self.state()
                .set_open_settings_at_launch_and_save(state == NSControlStateValueOn);
        }

        /// "Launch at login" checkbox toggled (mirrors the menu item).
        #[unsafe(method(launchAtLoginChanged:))]
        fn launch_at_login_changed(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let state: isize = unsafe { msg_send![sender, state] };
            crate::login_item::set_enabled(state == NSControlStateValueOn);
        }

        /// Remove-app button clicked; its tag indexes the current `apps` list.
        #[unsafe(method(removeApp:))]
        fn remove_app(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let tag: isize = unsafe { msg_send![sender, tag] };
            let bundle_id = self
                .ivars()
                .apps
                .borrow()
                .get(tag as usize)
                .cloned();
            if let Some(bundle_id) = bundle_id {
                self.state().remove_exclusion_and_save(&bundle_id);
                self.refresh_list();
            }
        }

        /// Opens the separate Excluded-Apps window (built on first use).
        #[unsafe(method(manageExcludedApps:))]
        fn manage_excluded_apps(&self, _sender: Option<&AnyObject>) {
            let mtm = MainThreadMarker::from(self);
            if self.ivars().excluded_window.borrow().is_none() {
                self.build_excluded_window(mtm);
            }
            self.refresh_list();
            if let Some(window) = self.ivars().excluded_window.borrow().as_ref() {
                window.center();
                window.makeKeyAndOrderFront(None);
            }
            NSApplication::sharedApplication(mtm).activate();
        }

        /// "Add App…" — open a file picker on /Applications and disable Vietnamese
        /// for each app chosen (resolving its bundle id).
        #[unsafe(method(addApp:))]
        fn add_app(&self, _sender: Option<&AnyObject>) {
            let mtm = MainThreadMarker::from(self);
            let panel = NSOpenPanel::openPanel(mtm);
            let apps_dir = NSURL::fileURLWithPath(&NSString::from_str("/Applications"));
            panel.setCanChooseFiles(true); // .app bundles are packages (file-like)
            panel.setCanChooseDirectories(false);
            panel.setAllowsMultipleSelection(true);
            panel.setDirectoryURL(Some(&apps_dir));
            panel.setMessage(Some(&NSString::from_str(
                t("Choose apps to disable Vietnamese in.", "Chọn ứng dụng để tắt tiếng Việt."),
            )));
            let response = panel.runModal();
            if response != NSModalResponseOK {
                return;
            }
            let urls = panel.URLs();
            for url in urls.iter() {
                let bundle_id = NSBundle::bundleWithURL(&url)
                    .and_then(|b| b.bundleIdentifier())
                    .map(|s| s.to_string());
                if let Some(bundle_id) = bundle_id {
                    self.state().add_exclusion_and_save(&bundle_id);
                }
            }
            self.refresh_list();
        }

        /// Opens the Personal Words window (built on first use).
        #[unsafe(method(managePersonalWords:))]
        fn manage_personal_words(&self, _sender: Option<&AnyObject>) {
            let mtm = MainThreadMarker::from(self);
            if self.ivars().words_window.borrow().is_none() {
                self.build_personal_words_window(mtm);
            }
            self.refresh_words();
            if let Some(window) = self.ivars().words_window.borrow().as_ref() {
                window.center();
                window.makeKeyAndOrderFront(None);
            }
            NSApplication::sharedApplication(mtm).activate();
        }

        #[unsafe(method(addWordAsTyped:))]
        fn add_word_as_typed(&self, _sender: Option<&AnyObject>) {
            self.add_word(glowkey_engine::WordPreference::Raw);
        }

        #[unsafe(method(addWordAsVietnamese:))]
        fn add_word_as_vietnamese(&self, _sender: Option<&AnyObject>) {
            self.add_word(glowkey_engine::WordPreference::Vietnamese);
        }

        #[unsafe(method(flipWord:))]
        fn flip_word(&self, sender: Option<&AnyObject>) {
            let Some(keys) = self.word_at_tag(sender) else {
                return;
            };
            // Read the current verdict rather than tracking it on the button: the
            // list is the truth, and the button only knows which row it is on.
            let flipped = match self.state().word_override(&keys) {
                Some(glowkey_engine::WordPreference::Raw) => {
                    glowkey_engine::WordPreference::Vietnamese
                }
                _ => glowkey_engine::WordPreference::Raw,
            };
            self.state().set_word_override_and_save(&keys, flipped);
            self.refresh_words();
        }

        #[unsafe(method(removeWord:))]
        fn remove_word(&self, sender: Option<&AnyObject>) {
            let Some(keys) = self.word_at_tag(sender) else {
                return;
            };
            self.state().remove_word_override_and_save(&keys);
            self.refresh_words();
        }

        /// Opens the separate Macros window (built on first use).
        #[unsafe(method(manageMacros:))]
        fn manage_macros(&self, _sender: Option<&AnyObject>) {
            let mtm = MainThreadMarker::from(self);
            if self.ivars().macros_window.borrow().is_none() {
                self.build_macros_window(mtm);
            }
            self.refresh_macros();
            if let Some(window) = self.ivars().macros_window.borrow().as_ref() {
                window.center();
                window.makeKeyAndOrderFront(None);
            }
            NSApplication::sharedApplication(mtm).activate();
        }

        /// "Add" — read the two macro fields, store the macro, clear the fields.
        #[unsafe(method(addMacro:))]
        fn add_macro(&self, _sender: Option<&AnyObject>) {
            let shortcut = self
                .ivars()
                .macro_shortcut
                .borrow()
                .as_ref()
                .map(|f| f.stringValue().to_string())
                .unwrap_or_default();
            let expansion = self
                .ivars()
                .macro_expansion
                .borrow()
                .as_ref()
                .map(|f| f.stringValue().to_string())
                .unwrap_or_default();
            // Ask before overwriting. `add_macro` replaces silently, and the
            // Import in this same window refused to overwrite and reported what it
            // skipped — one window, two opposite rules, and the destructive one
            // was the silent one.
            if self.state().has_macro(&shortcut) {
                let mtm = MainThreadMarker::from(self);
                let replace = self.ask(
                    &t("Replace “{}”?", "Thay thế “{}”?").replace("{}", shortcut.trim()),
                    t(
                        "That shortcut already has an expansion.",
                        "Chữ viết tắt đó đã có nội dung thay thế.",
                    ),
                    &[t("Replace", "Thay thế"), t("Cancel", "Huỷ")],
                    mtm,
                );
                if replace != 0 {
                    return;
                }
            }
            if self.state().add_macro_and_save(&shortcut, &expansion) {
                let empty = NSString::from_str("");
                if let Some(f) = self.ivars().macro_shortcut.borrow().as_ref() {
                    f.setStringValue(&empty);
                }
                if let Some(f) = self.ivars().macro_expansion.borrow().as_ref() {
                    f.setStringValue(&empty);
                }
                self.refresh_macros();
            }
        }

        /// Remove-macro button clicked; its tag is the macro's index.
        #[unsafe(method(removeMacro:))]
        fn remove_macro(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let tag: isize = unsafe { msg_send![sender, tag] };
            self.state().remove_macro_and_save(tag as usize);
            self.refresh_macros();
        }
    }
);

impl PrefsController {
    pub(super) fn state(&self) -> &TapState {
        // Safe: the pointer is to a leaked, program-lifetime TapState on the main
        // thread, the same one the tap and menu use.
        unsafe { &*self.ivars().state }
    }

    fn new(state: *const TapState, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(PrefsIvars {
            state,
            window: RefCell::new(None),
            excluded_window: RefCell::new(None),
            retired_windows: RefCell::new(Vec::new()),
            list_stack: RefCell::new(None),
            apps: RefCell::new(Vec::new()),
            macros_window: RefCell::new(None),
            macro_shortcut: RefCell::new(None),
            macro_expansion: RefCell::new(None),
            macros_list: RefCell::new(None),
            macro_count: RefCell::new(0),
            words_window: RefCell::new(None),
            word_keys: RefCell::new(None),
            words_list: RefCell::new(None),
            word_order: RefCell::new(Vec::new()),
            hotkey_seg: RefCell::new(None),
            hotkey_label: RefCell::new(None),
        });
        unsafe { msg_send![super(this), init] }
    }

    /// A short modal report — the only feedback an import or a failed write gets.
    /// Silence after a file operation reads as "nothing happened".
    pub(super) fn notify(&self, message: &str, detail: &str, mtm: MainThreadMarker) {
        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str(message));
        if !detail.is_empty() {
            alert.setInformativeText(&NSString::from_str(detail));
        }
        alert.runModal();
    }

    /// A modal question with up to three answers, returning which was chosen as
    /// its index in `buttons`.
    ///
    /// The first button is the default (Return) and, when there are three, the
    /// last is Cancel (Escape) — AppKit's own ordering, so the keyboard behaves
    /// the way it does everywhere else.
    pub(super) fn ask(
        &self,
        message: &str,
        detail: &str,
        buttons: &[&str],
        mtm: MainThreadMarker,
    ) -> usize {
        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str(message));
        if !detail.is_empty() {
            alert.setInformativeText(&NSString::from_str(detail));
        }
        let mut last = None;
        for title in buttons {
            last = Some(alert.addButtonWithTitle(&NSString::from_str(title)));
        }
        // Escape cancels. AppKit does this by itself only for a button whose title
        // is the literal string "Cancel", so in Vietnamese ("Huỷ") the key did
        // nothing — the caller passes Cancel last, and this makes that true in
        // both languages.
        if buttons.len() > 1 {
            if let Some(button) = last {
                button.setKeyEquivalent(&NSString::from_str("\u{1b}"));
            }
        }
        // NSAlertFirstButtonReturn == 1000, and the rest count up from there.
        let response = alert.runModal();
        (response - 1000).clamp(0, buttons.len() as isize - 1) as usize
    }

    /// Drops every built window so the next open rebuilds it in the current
    /// language, then reopens Settings. Labels are baked in at build time — the
    /// alternative, holding a reference to each one to re-set its title, would be
    /// a field per control for a preference changed approximately once.
    pub(super) fn rebuild_windows(&self) {
        let mtm = MainThreadMarker::from(self);
        // Safe now: nothing from that generation can be mid-click.
        self.ivars().retired_windows.borrow_mut().clear();

        // Take the guards in a `let` first. Temporaries in a `for` head live for
        // the whole loop body, which would hold three `RefMut`s across `close()`
        // — and `close()` runs arbitrary AppKit code (notifications, key-window
        // transfer) that could borrow them again and panic, in frames with no
        // unwind guard. Every other borrow in this file is fallible for the same
        // reason.
        // Personal Words is in here too. It was the one window this function
        // forgot, so after a language change it kept its English labels for the
        // rest of the run while every other window had switched — the sort of
        // half-translated interface that reads as a broken app rather than a
        // missed case.
        let windows = [
            self.ivars().window.borrow_mut().take(),
            self.ivars().excluded_window.borrow_mut().take(),
            self.ivars().macros_window.borrow_mut().take(),
            self.ivars().words_window.borrow_mut().take(),
        ];

        // Reopen whatever the user had open, rather than only Settings.
        let reopen_excluded = windows[1].as_ref().is_some_and(|w| w.isVisible());
        let reopen_macros = windows[2].as_ref().is_some_and(|w| w.isVisible());
        let reopen_words = windows[3].as_ref().is_some_and(|w| w.isVisible());

        // Controls from the discarded windows must not be reachable any more.
        self.ivars().list_stack.replace(None);
        self.ivars().words_list.replace(None);
        self.ivars().macros_list.replace(None);
        self.ivars().macro_shortcut.replace(None);
        self.ivars().macro_expansion.replace(None);
        self.ivars().hotkey_seg.replace(None);
        self.ivars().hotkey_label.replace(None);

        // Retire rather than release. This runs *from the action of a control
        // inside the window being torn down*, so dropping the last reference here
        // would free the view tree — sender and all — underneath the AppKit frames
        // still unwinding that click. `setReleasedWhenClosed(false)` opts out of
        // AppKit's own deferral, so the deferral has to be ours: the previous
        // generation was already released at the top of this function, during some
        // later event, when nothing of theirs is in flight.
        {
            let mut retired = self.ivars().retired_windows.borrow_mut();
            for window in windows.into_iter().flatten() {
                window.orderOut(None);
                retired.push(window);
            }
        }

        self.build_window(mtm);
        self.show_window();
        if reopen_excluded {
            self.manage_excluded_apps(sel!(manageExcludedApps:), None);
        }
        if reopen_macros {
            self.manage_macros(sel!(manageMacros:), None);
        }
        if reopen_words {
            self.manage_personal_words(sel!(managePersonalWords:), None);
        }
        // The About window is built once and cached elsewhere; it would otherwise
        // keep whichever language it was first opened in.
        crate::about_window::invalidate();
    }

    /// Builds the window on first call, then refreshes the list and brings it front.
    pub(super) fn show_window(&self) {
        let mtm = MainThreadMarker::from(self);
        if self.ivars().window.borrow().is_none() {
            self.build_window(mtm);
        }
        // The excluded-app list lives in its own window now, so nothing to refresh
        // here — the main pane holds only the everyday settings.
        if let Some(window) = self.ivars().window.borrow().as_ref() {
            window.center();
            window.makeKeyAndOrderFront(None);
        }
        NSApplication::sharedApplication(mtm).activate();
    }
    /// Syncs the toggle-hotkey controls to the live preset: selects the matching
    /// segment (or clears the selection for a custom combo) and updates the
    /// "Current: …" label. Called after a preset click and after a recording ends.
    /// Adds the word in the input field with `prefer`, then clears the field.
    /// Shared by both Add buttons — the only difference between them is the
    /// verdict, and duplicating the field-reading around that would be two
    /// places to keep the trimming rule.
    fn add_word(&self, prefer: glowkey_engine::WordPreference) {
        let Some(field) = self.ivars().word_keys.borrow().clone() else {
            return;
        };
        let keys = field.stringValue().to_string();
        let keys = keys.trim();
        if keys.is_empty() {
            return;
        }
        self.state().set_word_override_and_save(keys, prefer);
        field.setStringValue(&NSString::from_str(""));
        self.refresh_words();
    }

    pub(super) fn refresh_hotkey_ui(&self) {
        let preset = self.state().toggle_hotkey();
        if let Some(seg) = self.ivars().hotkey_seg.borrow().as_ref() {
            // Always a real segment — a SelectOne segmented control does not
            // reliably support clearing the selection with -1.
            let selected: isize = match preset {
                HotkeyPreset::CtrlShiftSpace => 0,
                HotkeyPreset::CtrlSpace => 1,
                HotkeyPreset::OptionSpace => 2,
                HotkeyPreset::CtrlShiftZ => 3,
                HotkeyPreset::Custom { .. } => 4,
            };
            seg.setSelectedSegment(selected);
        }
        if let Some(label) = self.ivars().hotkey_label.borrow().as_ref() {
            label.setStringValue(&NSString::from_str(
                &t("Current: {}", "Hiện tại: {}").replace("{}", &hotkey_display(preset)),
            ));
        }
    }

    // --- small view helpers (type hierarchy: header / label / caption) ---
}

thread_local! {
    /// The single Settings controller, created on first open and reused after
    /// (the window is hidden, not destroyed, on close). Main-thread only.
    static CONTROLLER: RefCell<Option<Retained<PrefsController>>> = const { RefCell::new(None) };
}

/// Opens (creating on first call) the Settings window. Called from the menu bar's
/// "Settings…" item.
pub fn show(state: *const TapState, mtm: MainThreadMarker) {
    CONTROLLER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let controller = slot.get_or_insert_with(|| PrefsController::new(state, mtm));
        controller.show_window();
    });
}

/// Called by the tap when a hotkey recording ends (captured or cancelled), so the
/// Settings controls reflect the new combo. A no-op before the window exists
/// (including under tests, where no controller is installed).
/// Refreshes the Personal Words list if that window is open.
///
/// Called by the tap after the correction hotkey records a decision. Without it
/// the one window whose whole job is showing what the hotkey wrote does not show
/// it until reopened.
pub fn personal_words_changed() {
    CONTROLLER.with(|slot| {
        if let Some(controller) = slot.borrow().as_ref() {
            controller.refresh_words();
        }
    });
}

pub fn hotkey_recording_done() {
    CONTROLLER.with(|slot| {
        // try_borrow: this can be reached re-entrantly if AppKit pumps the run
        // loop while show() still holds the slot mutably.
        if let Ok(slot) = slot.try_borrow() {
            if let Some(controller) = slot.as_ref() {
                controller.refresh_hotkey_ui();
            }
        }
    });
}
