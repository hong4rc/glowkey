//! The macOS InputMethodKit shell: a Rust subclass of `IMKInputController` that
//! feeds keystrokes to [`glowkey_engine`] and renders the result as marked
//! (composing) text in the host application.
//!
//! This is the platform half. It owns no Vietnamese logic — every language
//! decision lives in the engine crate. Its jobs are: decode key events, advance
//! the engine, and show the composing word via the client's text protocol.
//!
//! ## Rendering model — marked text
//!
//! The word being typed is shown as marked (underlined) composing text via
//! `setMarkedText:selectionRange:replacementRange:`. The host owns and redraws
//! that region, so this works in every application that supports Chinese/Japanese/
//! Korean input — a far larger and more reliable set than apps that honor an
//! arbitrary `replacementRange` into already-committed text. At a word boundary the
//! composing text is committed with `insertText:` and the boundary key passes
//! through to the host.
//!
//! NOTE: InputMethodKit can only be exercised by installing the built `.app` into
//! `~/Library/Input Methods/` and enabling it in System Settings — it cannot be
//! unit-tested. The engine carries the tested logic; this layer is verified by
//! building, installing, and typing. See `docs/checkpoint.md`.

use std::cell::RefCell;

use glowkey_engine::{ExclusionList, PlacementStyle, Session};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, AnyThread, ClassType, DeclaredClass};
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
use objc2_foundation::{NSObject, NSRange, NSString};
use objc2_input_method_kit::{IMKInputController, IMKServer};

/// The connection name must exactly match `InputMethodConnectionName` in
/// `Info.plist`, or the server fails to register (silently).
const CONNECTION_NAME: &str = "io.glowkey.inputmethod.GlowKey_Connection";

/// `NSNotFound` as an `NSUInteger` — `NSIntegerMax`. Passed as a range location to
/// mean "the current marked text" for insert/replace operations.
const NS_NOT_FOUND: usize = usize::MAX >> 1;

/// macOS virtual key code for Delete/Backspace.
const KEY_CODE_DELETE: u16 = 51;

/// True when a shortcut modifier is held — Command, Control, or Option. Such
/// events are shortcuts, not text, and must reach the host untouched. Shift is
/// deliberately excluded: it produces uppercase letters.
fn is_shortcut(mods: NSEventModifierFlags) -> bool {
    let shortcut = NSEventModifierFlags::Command.0
        | NSEventModifierFlags::Control.0
        | NSEventModifierFlags::Option.0;
    mods.0 & shortcut != 0
}

/// Per-controller state. One controller is created per text input session.
pub struct ControllerState {
    session: RefCell<Session>,
    /// Last bundle id seen from the client, so the engine is reset only when the
    /// frontmost application actually changes.
    last_bundle_id: RefCell<Option<String>>,
}

define_class!(
    // A Rust-defined Objective-C class subclassing IMKInputController. macOS
    // instantiates it by the name given in Info.plist's
    // `InputMethodServerControllerClass`.
    #[unsafe(super(IMKInputController))]
    #[name = "GlowKeyController"]
    #[ivars = ControllerState]
    pub struct GlowKeyController;

    impl GlowKeyController {
        /// IMK constructs the controller through this initializer. We set the Rust
        /// ivars here so every session starts with a fresh engine and the default
        /// ignore list; without this the ivars would be uninitialized and any
        /// access would be undefined behavior.
        #[unsafe(method_id(initWithServer:delegate:client:))]
        fn init_with_server(
            this: objc2::rc::Allocated<Self>,
            server: Option<&IMKServer>,
            delegate: Option<&AnyObject>,
            client: Option<&AnyObject>,
        ) -> Option<Retained<Self>> {
            let this = this.set_ivars(ControllerState::new());
            unsafe {
                msg_send![super(this), initWithServer: server, delegate: delegate, client: client]
            }
        }

        /// Called by IMK for each event routed to this input method. Returning
        /// `true` consumes the event; `false` lets the host handle it.
        #[unsafe(method(handleEvent:client:))]
        fn handle_event(&self, event: Option<&NSEvent>, client: Option<&AnyObject>) -> bool {
            self.handle_event_impl(event, client)
        }

        /// Flush any in-progress word when composition is committed by the system.
        #[unsafe(method(commitComposition:))]
        fn commit_composition(&self, _sender: Option<&AnyObject>) {
            self.flush_session();
        }

        /// Flush when this input session becomes active (focus moved here), so no
        /// stale word from a previous field bleeds in.
        #[unsafe(method(activateServer:))]
        fn activate_server(&self, _sender: Option<&AnyObject>) {
            self.flush_session();
        }

        /// Flush when this input session is deactivated (focus left), so the diff
        /// baseline never survives a focus change.
        #[unsafe(method(deactivateServer:))]
        fn deactivate_server(&self, _sender: Option<&AnyObject>) {
            self.flush_session();
        }
    }
);

impl GlowKeyController {
    /// The keystroke path: decode the event, advance the engine, and render.
    fn handle_event_impl(&self, event: Option<&NSEvent>, client: Option<&AnyObject>) -> bool {
        let (Some(event), Some(client)) = (event, client) else {
            return false;
        };

        // Only key-down events carry typed text.
        if event.r#type() != NSEventType::KeyDown {
            return false;
        }

        // Shortcuts (⌘/⌃/⌥ chords) must reach the host untouched — never let ⌘S
        // arrive at the engine as tone key `s`.
        if is_shortcut(event.modifierFlags()) {
            return false;
        }

        let key = decode_key(event);

        // A failed borrow means IMK re-entered mid-edit; skip rather than panic.
        let Ok(mut session) = self.ivars().session.try_borrow_mut() else {
            return false;
        };

        // Keep the ignore list honest: read the client's bundle id and, if the
        // application changed, tell the session (which flushes and re-evaluates
        // exclusion). Using the client's own id avoids any focus-timing race.
        if let Some(bundle_id) = client_bundle_id(client) {
            let mut last = self.ivars().last_bundle_id.borrow_mut();
            if last.as_deref() != Some(bundle_id.as_str()) {
                session.set_frontmost_app(bundle_id.clone());
                *last = Some(bundle_id);
            }
        }

        if !session.is_active() {
            return false;
        }

        match key {
            DecodedKey::Backspace => {
                if !session.is_composing() {
                    return false; // nothing composing — let the host delete
                }
                session.backspace();
                let word = session.current_word().to_string();
                self.render_marked(client, &word);
                true
            }
            DecodedKey::Letter(ch) => {
                session.process_key(ch);
                let word = session.current_word().to_string();
                self.render_marked(client, &word);
                true
            }
            DecodedKey::Boundary => {
                if session.is_composing() {
                    let word = session.commit_word();
                    self.commit_text(client, &word);
                }
                // Let the host insert the boundary character itself.
                false
            }
            DecodedKey::Ignore => false,
        }
    }

    /// Shows `text` as marked (composing) text, caret at the end. An empty string
    /// clears the marked text.
    fn render_marked(&self, client: &AnyObject, text: &str) {
        let ns = NSString::from_str(text);
        let caret = text.encode_utf16().count();
        let selection = NSRange::new(caret, 0);
        let replacement = NSRange::new(NS_NOT_FOUND, 0);
        unsafe {
            let _: () = msg_send![
                client,
                setMarkedText: &*ns,
                selectionRange: selection,
                replacementRange: replacement,
            ];
        }
    }

    /// Commits `text` as ordinary inserted text, replacing any marked text.
    fn commit_text(&self, client: &AnyObject, text: &str) {
        let ns = NSString::from_str(text);
        let replacement = NSRange::new(NS_NOT_FOUND, 0);
        unsafe {
            let _: () = msg_send![client, insertText: &*ns, replacementRange: replacement];
        }
    }

    /// Flushes the session's in-progress word. Uses `try_borrow_mut` because IMK
    /// can call lifecycle methods re-entrantly (an insert can trigger a commit);
    /// a failed borrow means a flush is already underway, which is fine to skip.
    fn flush_session(&self) {
        if let Ok(mut session) = self.ivars().session.try_borrow_mut() {
            session.flush();
        }
    }
}

/// A decoded key event, reduced to what the engine cares about.
enum DecodedKey {
    /// An ASCII letter that can extend a Vietnamese syllable.
    Letter(char),
    /// The Delete/Backspace key.
    Backspace,
    /// A printable non-letter that ends a word (space, digit, punctuation).
    Boundary,
    /// Anything the engine should not see (function keys, arrows, etc.).
    Ignore,
}

/// Decodes an `NSEvent` key-down into a [`DecodedKey`].
fn decode_key(event: &NSEvent) -> DecodedKey {
    if event.keyCode() == KEY_CODE_DELETE {
        return DecodedKey::Backspace;
    }
    let Some(chars) = event.characters() else {
        return DecodedKey::Ignore;
    };
    let s = chars.to_string();
    let Some(ch) = s.chars().next() else {
        return DecodedKey::Ignore;
    };
    if ch.is_ascii_alphabetic() {
        DecodedKey::Letter(ch)
    } else if ch.is_control() {
        DecodedKey::Ignore
    } else {
        DecodedKey::Boundary
    }
}

/// Reads the client's application bundle identifier via `IMKTextInput`.
fn client_bundle_id(client: &AnyObject) -> Option<String> {
    let ns: Option<Retained<NSString>> = unsafe { msg_send![client, bundleIdentifier] };
    ns.map(|s| s.to_string())
}

impl ControllerState {
    fn new() -> Self {
        Self {
            session: RefCell::new(Session::new(
                PlacementStyle::New,
                ExclusionList::with_defaults(),
            )),
            last_bundle_id: RefCell::new(None),
        }
    }
}

/// Launches the input method server and runs the app's event loop.
pub fn run() {
    // Force the Objective-C runtime to register GlowKeyController. objc2 registers
    // a define_class! class lazily on the first `class()` call; if nothing ever
    // references it, IMK resolves `InputMethodServerControllerClass` to nil and no
    // controller is ever created — a silent, total failure. This must happen
    // before the server starts.
    let _ = GlowKeyController::class();

    // Register the IMK server under the Info.plist connection name.
    let name = NSString::from_str(CONNECTION_NAME);
    let bundle_id = NSString::from_str("io.glowkey.inputmethod.GlowKey");
    let _server: Retained<IMKServer> = unsafe {
        let alloc = IMKServer::alloc();
        msg_send![alloc, initWithName: &*name, bundleIdentifier: &*bundle_id]
    };

    // Run the main run loop so IMK can dispatch to the controller, which it
    // instantiates on demand with fresh per-session state.
    unsafe {
        let app: Retained<NSObject> = msg_send![objc2::class!(NSApplication), sharedApplication];
        let _: () = msg_send![&*app, run];
    }
}

// Compile-time guard that the class is a proper declared objc2 class. Runtime
// registration is forced in `run()` (see the `class()` call there).
const _: () = {
    fn assert_declared<T: DeclaredClass>() {}
    let _ = assert_declared::<GlowKeyController>;
};

#[cfg(test)]
mod registration_tests {
    use super::GlowKeyController;
    use objc2::ClassType;
    use objc2_foundation::NSString;

    #[test]
    fn controller_class_registers_with_objc_runtime() {
        // Before the fix, NSClassFromString("GlowKeyController") was nil because
        // objc2 registers lazily and run() never referenced the class.
        let _ = GlowKeyController::class();
        let name = NSString::from_str("GlowKeyController");
        let cls: *const std::ffi::c_void =
            unsafe { objc2::ffi::objc_getClass(name.UTF8String() as *const _) as *const _ };
        assert!(
            !cls.is_null(),
            "GlowKeyController must be registered with the Obj-C runtime"
        );
    }
}
