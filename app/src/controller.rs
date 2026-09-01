//! The macOS InputMethodKit shell: a Rust subclass of `IMKInputController` that
//! feeds keystrokes to [`glowkey_engine`] and applies the resulting edits to the
//! host application's text.
//!
//! This is the platform half. It owns no Vietnamese logic — every language
//! decision lives in the engine crate. Its jobs are: receive key events, ask the
//! engine what edit to make, and render that edit via the client's text protocol.
//!
//! NOTE: InputMethodKit can only be exercised by installing the built `.app` into
//! `~/Library/Input Methods/` and enabling it in System Settings — it cannot be
//! unit-tested. The engine carries the tested logic; this layer is verified by
//! building, installing, and typing. See `docs/checkpoint.md`.

use std::cell::RefCell;

use glowkey_engine::{ExclusionList, PlacementStyle, Session};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, AnyThread, DeclaredClass};
use objc2_foundation::{NSObject, NSString};
use objc2_input_method_kit::{IMKInputController, IMKServer};

/// A syllable-length output buffer is generous; NFC Vietnamese syllables are
/// short. Documented here to keep the constant near its rationale.
#[allow(dead_code)]
const MAX_SYLLABLE_UTF16: usize = 32;

/// The connection name must exactly match `InputMethodConnectionName` in
/// `Info.plist`, or the server fails to register (silently).
const CONNECTION_NAME: &str = "io.glowkey.inputmethod.GlowKey_Connection";

/// Per-controller state. One controller is created per text input session.
pub struct ControllerState {
    session: RefCell<Session>,
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
        fn handle_event(&self, event: Option<&AnyObject>, client: Option<&AnyObject>) -> bool {
            self.handle_event_impl(event, client)
        }

        /// Flush any in-progress word when composition is committed by the system.
        #[unsafe(method(commitComposition:))]
        fn commit_composition(&self, _sender: Option<&AnyObject>) {
            self.ivars().session.borrow_mut().flush();
        }

        /// Reset engine state when this input session becomes active.
        #[unsafe(method(activateServer:))]
        fn activate_server(&self, _sender: Option<&AnyObject>) {
            self.refresh_frontmost_app();
        }
    }
);

impl GlowKeyController {
    /// The keystroke path. Currently a scaffold: decodes the event and asks the
    /// engine for an edit. Rendering the edit to the client (insertText /
    /// setMarkedText) is wired in the next build once the client protocol surface
    /// is pinned; see docs/checkpoint.md.
    fn handle_event_impl(&self, _event: Option<&AnyObject>, _client: Option<&AnyObject>) -> bool {
        // Not yet consuming events — return false so the host is unaffected while
        // the rendering layer is completed. This keeps an installed build inert
        // and safe rather than eating keystrokes.
        false
    }

    /// Update the engine with the frontmost application's bundle identifier, so the
    /// ignore list applies. Wired against NSWorkspace in the next build.
    fn refresh_frontmost_app(&self) {
        // Placeholder: bundle-id resolution lands with the rendering layer.
    }
}

impl ControllerState {
    fn new() -> Self {
        Self {
            session: RefCell::new(Session::new(
                PlacementStyle::New,
                ExclusionList::with_defaults(),
            )),
        }
    }
}

/// Launches the input method server and runs the app's event loop.
pub fn run() {
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

// Compile-time guard that the class is a proper declared objc2 class.
const _: () = {
    fn assert_declared<T: DeclaredClass>() {}
    let _ = assert_declared::<GlowKeyController>;
};
