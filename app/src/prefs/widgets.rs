//! The small view helpers every pane is built from.
//!
//! Nothing here knows what a setting is: a labelled row, a caption, a stack with
//! the right insets, a scroll view around a list. They are together because they
//! are the vocabulary the four tabs and the three list windows all speak.

use objc2::msg_send;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSColor, NSFont, NSScrollView, NSStackView, NSTextAlignment, NSTextField,
    NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_foundation::{MainThreadMarker, NSEdgeInsets, NSString};

use super::PrefsController;

pub(super) const LABEL_COLUMN_WIDTH: f64 = 92.0;

impl PrefsController {
    /// One tab's content stack: vertical, leading-aligned, inset from the pane.
    pub(super) fn tab_stack(&self, mtm: MainThreadMarker) -> Retained<NSStackView> {
        let stack = NSStackView::new(mtm);
        stack.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
        stack.setSpacing(6.0);
        stack.setEdgeInsets(NSEdgeInsets {
            top: 18.0,
            left: 18.0,
            bottom: 18.0,
            right: 18.0,
        });
        // Leading-align arranged subviews (NSLayoutAttribute::Leading == 5).
        unsafe {
            let _: () = msg_send![&stack, setAlignment: 5isize];
        }
        stack
    }

    /// A plain primary-color label.
    pub(super) fn make_label(&self, text: &str, mtm: MainThreadMarker) -> Retained<NSTextField> {
        NSTextField::labelWithString(&NSString::from_str(text), mtm)
    }

    /// A smaller secondary-color caption for explanatory text.
    pub(super) fn caption(&self, text: &str, mtm: MainThreadMarker) -> Retained<NSTextField> {
        let label = self.make_label(text, mtm);
        label.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        label.setTextColor(Some(&NSColor::secondaryLabelColor()));
        label
    }

    /// One aligned form row: a fixed-width, right-aligned label followed by its
    /// control — the two-column macOS settings form. The fixed label width lines the
    /// controls up across rows.
    pub(super) fn form_row(
        &self,
        title: &str,
        control: &NSView,
        mtm: MainThreadMarker,
    ) -> Retained<NSStackView> {
        let row = NSStackView::new(mtm);
        row.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
        row.setSpacing(8.0);

        let label = self.make_label(title, mtm);
        label.setAlignment(NSTextAlignment::Right);
        let width = label
            .widthAnchor()
            .constraintEqualToConstant(LABEL_COLUMN_WIDTH);
        width.setActive(true);

        row.addArrangedSubview(&label);
        row.addArrangedSubview(control);
        row
    }

    /// Puts a growing list inside a scroll view, so content past the bottom of the
    /// window can still be reached.
    ///
    /// The three list windows — excluded apps, macros, personal words — each put a
    /// bare `NSStackView` straight into a fixed-size window. Rows past the bottom
    /// edge were simply not reachable: the fourteen shipped exclusions overflow a
    /// 380pt window on a clean install, so the app's headline feature was cut off
    /// out of the box, and an import that reported "214 macros" showed about
    /// thirteen.
    ///
    /// The document view is pinned to the clip view's **width** and left free in
    /// height — that pairing is what makes a stack view scroll vertically instead
    /// of collapsing to nothing or scrolling in both directions.
    pub(super) fn scrollable(
        &self,
        list: &NSStackView,
        min_height: f64,
        mtm: MainThreadMarker,
    ) -> Retained<NSScrollView> {
        let scroll = NSScrollView::new(mtm);
        scroll.setHasVerticalScroller(true);
        scroll.setHasHorizontalScroller(false);
        // The window's own background shows through, so the list does not read as
        // a sunken well the way a table would. These are lists of rows, not data
        // to be edited in place.
        scroll.setDrawsBackground(false);
        scroll.setDocumentView(Some(list));
        list.setTranslatesAutoresizingMaskIntoConstraints(false);

        let clip = scroll.contentView();
        list.leadingAnchor()
            .constraintEqualToAnchor(&clip.leadingAnchor())
            .setActive(true);
        list.trailingAnchor()
            .constraintEqualToAnchor(&clip.trailingAnchor())
            .setActive(true);
        list.topAnchor()
            .constraintEqualToAnchor(&clip.topAnchor())
            .setActive(true);
        // Height deliberately unconstrained: the stack grows with its rows and the
        // scroll view pages through whatever exceeds the window.
        scroll
            .heightAnchor()
            .constraintGreaterThanOrEqualToConstant(min_height)
            .setActive(true);
        scroll
    }
}
