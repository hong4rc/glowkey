//! The Settings window itself: four tabs, built once on first open.
//!
//! The tabs are not written here. They are `settings_spec::TABS`, the same data
//! the Windows window is built from, and this is the AppKit renderer of it: for
//! each row, make the native control the row calls for, set its state from the
//! session, wire its action, and add it to the tab's stack. What the rows *are*
//! — order, wording in both languages, which setting each binds to, which row
//! depends on which — is decided once, in the spec, so the two windows cannot
//! drift apart the way they had.
//!
//! Everything AppKit-specific stays here: `NSSegmentedControl` for a choice,
//! `NSButton` checkboxes, the hotkey recorder's status line, wrapping widths,
//! the accessibility help each control carries.

use objc2::rc::Retained;
use objc2::runtime::Sel;
use objc2::{msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSButton, NSColor, NSControlStateValueOff, NSControlStateValueOn, NSFont,
    NSSegmentSwitchTracking, NSSegmentedControl, NSStackView, NSTabView, NSTabViewItem,
    NSTextField, NSUserInterfaceLayoutOrientation, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSArray, NSPoint, NSRect, NSSize, NSString};

use super::widgets::LABEL_COLUMN_WIDTH;
use super::PrefsController;
use crate::settings_spec::{
    expand_shortcuts, hotkey_display, shortcut_display, Control, ListId, Row, TabSpec, Text,
    Toggle, HOTKEY_PRESETS, MANAGE, TABS, WINDOW_TITLE,
};

/// The window's content size, in points. The same on every platform.
const WINDOW_SIZE: (f64, f64) = (460.0, 540.0);
/// Inset of a tab's stack from the pane edge (`widgets::tab_stack`).
const PANE_INSET: f64 = 18.0;
/// How far a dependent row sits under its parent: a checkbox glyph plus its gap.
const DEPENDENT_INDENT: f64 = 20.0;
/// Larger gap before a section header.
const SECTION_GAP: f64 = 22.0;
/// The label on the recorder segment. macOS only — Windows has no recorder —
/// so it is a detail of this renderer rather than of the spec.
const CUSTOM_HOTKEY: Text = Text::new("Custom…", "Tùy chọn…");

impl PrefsController {
    /// Constructs the window and its static controls once.
    pub(super) fn build_window(&self, mtm: MainThreadMarker) {
        let content = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(WINDOW_SIZE.0, WINDOW_SIZE.1),
        );
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable;
        let window: Retained<NSWindow> = unsafe {
            let alloc = NSWindow::alloc(mtm);
            msg_send![
                alloc,
                initWithContentRect: content,
                styleMask: style,
                backing: NSBackingStoreType::Buffered,
                defer: false,
            ]
        };
        window.setTitle(&NSString::from_str(WINDOW_TITLE.get()));
        unsafe { window.setReleasedWhenClosed(false) };

        // Rebuilt from scratch: a language change tears the window down and
        // builds it again, and stale references to the old controls would
        // otherwise pile up here.
        self.ivars().dependents.borrow_mut().clear();
        self.ivars().list_counts.borrow_mut().clear();

        // Wide enough for the longest label in this language, so none is
        // truncated; never narrower than the macOS form's usual column.
        let label_width = self.label_column_width(mtm);

        let tabs = NSTabView::new(mtm);
        for tab in &TABS {
            let view = self.build_tab(tab, label_width, mtm);
            let item = NSTabViewItem::new();
            item.setLabel(&NSString::from_str(tab.title.get()));
            item.setView(Some(&view));
            tabs.addTabViewItem(&item);
        }
        window.setContentView(Some(&tabs));
        *self.ivars().window.borrow_mut() = Some(window);

        self.refresh_hotkey_ui();
        self.refresh_dependents();
        self.refresh_list_counts();
    }

    /// One tab: a vertical stack of section headers and rows.
    fn build_tab(
        &self,
        tab: &TabSpec,
        label_width: f64,
        mtm: MainThreadMarker,
    ) -> Retained<NSStackView> {
        let stack = self.tab_stack(mtm);
        let mut last: Option<Retained<NSView>> = None;
        for section in tab.sections {
            if let Some(previous) = &last {
                unsafe {
                    let _: () =
                        msg_send![&stack, setCustomSpacing: SECTION_GAP, afterView: &**previous];
                }
            }
            stack.addArrangedSubview(&self.section_header(section.title.get(), mtm));
            for row in section.rows {
                last = Some(self.add_row(&stack, row, label_width, mtm));
            }
        }
        stack
    }

    /// Adds one row (and its caption) to `stack`; returns the last view added,
    /// so the caller can put a section gap after it.
    fn add_row(
        &self,
        stack: &NSStackView,
        row: &Row,
        label_width: f64,
        mtm: MainThreadMarker,
    ) -> Retained<NSView> {
        let label = row.label.map(|l| l.get()).unwrap_or("");
        let caption = row
            .caption
            .map(|c| expand_shortcuts(c.get(), |s| shortcut_display(s).to_string()));
        let indent = if row.enabled_when.is_some() {
            DEPENDENT_INDENT
        } else {
            0.0
        };

        // The view that goes in the stack, and the control that carries the
        // caption as its accessibility help (a screen reader hears what a
        // sighted user reads under the control, not a bare title).
        let (view, control, caption_inset): (Retained<NSView>, Option<Retained<NSView>>, f64) =
            match row.control {
                Control::Language(options) => {
                    let current = self.state().language();
                    let selected = options.iter().position(|(_, v)| *v == current);
                    let seg = self.segmented(
                        options.iter().map(|(text, _)| text.get()),
                        selected,
                        sel!(languageChanged:),
                        mtm,
                    );
                    let row_view = self.form_row(label, &seg, label_width, mtm);
                    (
                        stack_view(row_view),
                        Some(control_view(seg)),
                        label_width + 8.0,
                    )
                }
                Control::InputMethod(options) => {
                    let current = self.state().input_method();
                    let selected = options.iter().position(|(_, v)| *v == current);
                    let seg = self.segmented(
                        options.iter().map(|(text, _)| text.get()),
                        selected,
                        sel!(inputMethodChanged:),
                        mtm,
                    );
                    let row_view = self.form_row(label, &seg, label_width, mtm);
                    (
                        stack_view(row_view),
                        Some(control_view(seg)),
                        label_width + 8.0,
                    )
                }
                Control::ToneMarks(options) => {
                    let current = self.state().style();
                    let selected = options.iter().position(|(_, v)| *v == current);
                    let seg = self.segmented(
                        options.iter().map(|(text, _)| text.get()),
                        selected,
                        sel!(toneChanged:),
                        mtm,
                    );
                    let row_view = self.form_row(label, &seg, label_width, mtm);
                    (
                        stack_view(row_view),
                        Some(control_view(seg)),
                        label_width + 8.0,
                    )
                }
                Control::Checkbox(toggle) => {
                    let checkbox: Retained<NSButton> = unsafe {
                        NSButton::checkboxWithTitle_target_action(
                            &NSString::from_str(label),
                            Some(self.as_ref()),
                            Some(toggle_selector(toggle)),
                            mtm,
                        )
                    };
                    checkbox.setState(if self.toggle_value(toggle) {
                        NSControlStateValueOn
                    } else {
                        NSControlStateValueOff
                    });
                    if let Some(parent) = row.enabled_when {
                        self.ivars()
                            .dependents
                            .borrow_mut()
                            .push((parent, checkbox.clone()));
                    }
                    let view = if indent > 0.0 {
                        stack_view(self.indented(&checkbox, indent, mtm))
                    } else {
                        control_view(checkbox.clone())
                    };
                    (view, Some(control_view(checkbox)), DEPENDENT_INDENT)
                }
                Control::ToggleHotkey => {
                    // Presets plus "Custom…", which arms the recorder (the tap
                    // captures the next ⌃/⌥ combo; Esc, a click, or an app switch
                    // cancel). The selected segment is set by `refresh_hotkey_ui`.
                    let labels = HOTKEY_PRESETS
                        .iter()
                        .map(|p| hotkey_display(*p))
                        .chain(std::iter::once(CUSTOM_HOTKEY.get().to_string()));
                    let seg = self.segmented(labels, None, sel!(hotkeyChanged:), mtm);
                    let row_view = self.form_row(label, &seg, label_width, mtm);
                    // This row is two views. The picker goes in here; the status
                    // line under it — "Current: ⌃⇧Space", or the recording
                    // prompt while "Custom…" is armed — is handed back as this
                    // row's view so the common path adds it in order.
                    stack.addArrangedSubview(&row_view);
                    let status = self.caption("", mtm);
                    let status_row = self.caption_row(&status, label_width + 8.0, mtm);
                    *self.ivars().hotkey_seg.borrow_mut() = Some(seg);
                    *self.ivars().hotkey_label.borrow_mut() = Some(status);
                    (stack_view(status_row), None, 0.0)
                }
                Control::Shortcut(shortcut) => {
                    let value = self.make_label(shortcut_display(shortcut), mtm);
                    let row_view = self.form_row(label, &value, label_width, mtm);
                    // The value label carries the help: the row is opaque
                    // without its caption.
                    (
                        stack_view(row_view),
                        Some(control_view(value)),
                        label_width + 8.0,
                    )
                }
                Control::List(list) => {
                    let count = self.make_label("", mtm);
                    count.setTextColor(Some(&NSColor::secondaryLabelColor()));
                    let button: Retained<NSButton> = unsafe {
                        NSButton::buttonWithTitle_target_action(
                            &NSString::from_str(MANAGE.get()),
                            Some(self.as_ref()),
                            Some(list_selector(list)),
                            mtm,
                        )
                    };
                    let cluster = NSStackView::new(mtm);
                    cluster.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
                    cluster.setSpacing(8.0);
                    cluster.addArrangedSubview(&count);
                    cluster.addArrangedSubview(&button);
                    self.ivars().list_counts.borrow_mut().push((list, count));
                    let row_view = self.form_row(label, &cluster, label_width, mtm);
                    (
                        stack_view(row_view),
                        Some(control_view(button)),
                        label_width + 8.0,
                    )
                }
            };

        stack.addArrangedSubview(&view);

        let Some(caption) = caption else {
            return view;
        };
        if let Some(control) = &control {
            let help = NSString::from_str(&caption);
            // `msg_send!` rather than the typed `NSAccessibility` method: that
            // protocol sits behind the `NSAccessibilityProtocols` feature, which
            // the crate does not enable for one call.
            // SAFETY: `control` is a live NSView, which conforms to
            // NSAccessibility; the selector takes one NSString.
            unsafe {
                let _: () = msg_send![&**control, setAccessibilityHelp: &*help];
            }
        }
        let text = self.wrapping_caption(&caption, caption_inset + indent, mtm);
        let caption_view = self.caption_row(&text, caption_inset + indent, mtm);
        stack.addArrangedSubview(&caption_view);
        stack_view(caption_view)
    }

    /// A `SelectOne` segmented control with these labels and this action.
    fn segmented(
        &self,
        labels: impl Iterator<Item = impl AsRef<str>>,
        selected: Option<usize>,
        action: Sel,
        mtm: MainThreadMarker,
    ) -> Retained<NSSegmentedControl> {
        let labels: Vec<Retained<NSString>> =
            labels.map(|l| NSString::from_str(l.as_ref())).collect();
        let labels = NSArray::from_retained_slice(&labels);
        let seg: Retained<NSSegmentedControl> = unsafe {
            NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(
                &labels,
                NSSegmentSwitchTracking::SelectOne,
                Some(self.as_ref()),
                Some(action),
                mtm,
            )
        };
        if let Some(index) = selected {
            seg.setSelectedSegment(index as isize);
        }
        seg
    }

    /// The label column: the widest label in the window, in this language, and
    /// never under `LABEL_COLUMN_WIDTH`. Measured with the same label view the
    /// rows use, so the answer is the one AppKit will lay out.
    fn label_column_width(&self, mtm: MainThreadMarker) -> f64 {
        TABS.iter()
            .flat_map(|tab| tab.sections.iter())
            .flat_map(|section| section.rows.iter())
            .filter(|row| !matches!(row.control, Control::Checkbox(_)))
            .filter_map(|row| row.label)
            .map(|label| {
                self.make_label(label.get(), mtm)
                    .intrinsicContentSize()
                    .width
                    + 4.0
            })
            .fold(LABEL_COLUMN_WIDTH, f64::max)
    }

    /// A section title: bold, small, secondary — the shape macOS System
    /// Settings gives a group heading.
    fn section_header(&self, title: &str, mtm: MainThreadMarker) -> Retained<NSTextField> {
        let label = self.make_label(title, mtm);
        label.setFont(Some(&NSFont::boldSystemFontOfSize(11.0)));
        label.setTextColor(Some(&NSColor::secondaryLabelColor()));
        label
    }

    /// A caption that wraps at the pane width rather than carrying `\n`.
    ///
    /// The spec's strings have no hard breaks — a break chosen for one toolkit's
    /// width is wrong in the other — so the wrapping is decided here, from the
    /// window width and the inset the caption sits at.
    fn wrapping_caption(
        &self,
        text: &str,
        inset: f64,
        mtm: MainThreadMarker,
    ) -> Retained<NSTextField> {
        let label = NSTextField::wrappingLabelWithString(&NSString::from_str(text), mtm);
        label.setSelectable(false);
        label.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        label.setTextColor(Some(&NSColor::secondaryLabelColor()));
        // `preferredMaxLayoutWidth` alone is a hint for the intrinsic height,
        // not a limit: pin the width too, so the height AppKit computes is for
        // the width it actually lays out at.
        let width = WINDOW_SIZE.0 - 2.0 * PANE_INSET - inset;
        label.setPreferredMaxLayoutWidth(width);
        label
            .widthAnchor()
            .constraintEqualToConstant(width)
            .setActive(true);
        label
    }

    /// `view` pushed right by `inset` points, so it reads as sitting under the
    /// row above.
    fn caption_row(
        &self,
        view: &NSView,
        inset: f64,
        mtm: MainThreadMarker,
    ) -> Retained<NSStackView> {
        self.indented(view, inset, mtm)
    }

    fn indented(&self, view: &NSView, inset: f64, mtm: MainThreadMarker) -> Retained<NSStackView> {
        let row = NSStackView::new(mtm);
        row.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
        row.setSpacing(0.0);
        let spacer = self.make_label("", mtm);
        spacer
            .widthAnchor()
            .constraintEqualToConstant(inset)
            .setActive(true);
        row.addArrangedSubview(&spacer);
        row.addArrangedSubview(view);
        row
    }

    /// The current value of a toggle, from the session or — for the login item
    /// — from the operating system.
    fn toggle_value(&self, toggle: Toggle) -> bool {
        let state = self.state();
        match toggle {
            Toggle::LaunchAtLogin => crate::login_item::is_enabled(),
            Toggle::OpenSettingsAtLaunch => state.open_settings_at_launch(),
            Toggle::QuickTelex => state.quick_telex(),
            Toggle::TelexBrackets => state.telex_brackets(),
            Toggle::AutoFix => state.auto_fix(),
            Toggle::StrictSpellCheck => state.strict_spell_check(),
            Toggle::AutoCapitalize => state.auto_capitalize(),
            Toggle::RestoreEnglishWords => state.restore_english_words(),
            Toggle::AlwaysMacro => state.always_macro(),
        }
    }

    /// Enables or disables every dependent checkbox from its parent's current
    /// value. Called after the window is built and whenever a parent changes.
    pub(super) fn refresh_dependents(&self) {
        for (parent, checkbox) in self.ivars().dependents.borrow().iter() {
            checkbox.setEnabled(self.toggle_value(*parent));
        }
    }

    /// Rewrites the count beside each "Manage…" button from the session.
    /// Called after the window is built and whenever a list window changes its
    /// list, so the number never lags what the window would show.
    pub(super) fn refresh_list_counts(&self) {
        let state = self.state();
        for (list, label) in self.ivars().list_counts.borrow().iter() {
            let count = match list {
                ListId::ExcludedApps => state.exclusion_ids().len(),
                ListId::Macros => state.macros().len(),
                ListId::PersonalWords => state.word_overrides().len(),
            };
            label.setStringValue(&NSString::from_str(&count.to_string()));
        }
    }
}

/// The action a toggle's checkbox fires. Each handler lives in `prefs/mod.rs`.
fn toggle_selector(toggle: Toggle) -> Sel {
    match toggle {
        Toggle::LaunchAtLogin => sel!(launchAtLoginChanged:),
        Toggle::OpenSettingsAtLaunch => sel!(openAtLaunchChanged:),
        Toggle::QuickTelex => sel!(quickTelexChanged:),
        Toggle::TelexBrackets => sel!(telexBracketsChanged:),
        Toggle::AutoFix => sel!(autoFixChanged:),
        Toggle::StrictSpellCheck => sel!(strictSpellCheckChanged:),
        Toggle::AutoCapitalize => sel!(autoCapitalizeChanged:),
        Toggle::RestoreEnglishWords => sel!(englishRestoreChanged:),
        Toggle::AlwaysMacro => sel!(alwaysMacroChanged:),
    }
}

/// The action that opens a list's window.
fn list_selector(list: ListId) -> Sel {
    match list {
        ListId::ExcludedApps => sel!(manageExcludedApps:),
        ListId::Macros => sel!(manageMacros:),
        ListId::PersonalWords => sel!(managePersonalWords:),
    }
}

/// A control (`NSButton`, `NSSegmentedControl`, `NSTextField`) as the `NSView`
/// two levels up its class chain.
fn control_view<T>(control: Retained<T>) -> Retained<NSView>
where
    T: objc2::ClassType<Super = objc2_app_kit::NSControl> + objc2::Message + 'static,
{
    control.into_super().into_super()
}

/// A stack view as the `NSView` one level up.
fn stack_view(stack: Retained<NSStackView>) -> Retained<NSView> {
    stack.into_super()
}
