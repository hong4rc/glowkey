//! Everything that writes to the outside world.
//!
//! The tap is a pure decision (`decide`) followed by an impure emit, and this is
//! the impure half: the synthesized backspaces and text that actually mutate the
//! user's document, the circuit breaker that caps a runaway, and the Chromium
//! omnibox guard that runs immediately before an edit lands.
//!
//! Every mutation in GlowKey goes through here, from the one tagged event source,
//! in one ordered `CGEventPost` queue. That is the invariant the whole
//! full-suppression design rests on (see the module docs on `super`): a backspace
//! can never overtake the character it deletes.

use std::ptr::NonNull;
use std::sync::atomic::Ordering;
use std::time::Instant;

use glowkey_engine::KeyResponse;
use objc2_app_kit::NSWorkspace;
use objc2_core_graphics::{CGEvent, CGEventFlags, CGEventSource, CGEventTapLocation};

use super::keys::{KEY_CODE_DELETE, KEY_CODE_FORWARD_DELETE};
use super::{debug_enabled, TapState, DISABLED, GLOWKEY_TAG, RUNAWAY_LIMIT, RUNAWAY_WINDOW};

/// Chromium-family browsers, matched by bundle-id prefix. Their omnibox keeps an
/// inline-autocomplete **trailing selection** after each keystroke, which a
/// synthetic Backspace deletes instead of a character (`hoongf`→`hoồng`). The
/// omnibox guard (see [`TapState::emit_edit`]) applies only in these apps.
const CHROMIUM_BUNDLE_PREFIXES: &[&str] = &[
    "com.google.Chrome",
    "com.microsoft.edgemac",
    "org.chromium.Chromium",
    "com.brave.Browser",
    "com.vivaldi.Vivaldi",
    "com.operasoftware.Opera",
    "company.thebrowser.Browser", // Arc
];

/// Whether `bundle_id` is a Chromium-family browser (see [`CHROMIUM_BUNDLE_PREFIXES`]).
pub(super) fn is_chromium_browser(bundle_id: &str) -> bool {
    CHROMIUM_BUNDLE_PREFIXES
        .iter()
        .any(|prefix| bundle_id.starts_with(prefix))
}

impl TapState {
    /// Records an emit and returns false if the rate indicates a runaway; latches
    /// [`DISABLED`] on a trip so a loop is capped rather than sustained. Human
    /// typing never approaches the limit.
    fn circuit_ok(&self) -> bool {
        if DISABLED.load(Ordering::Relaxed) {
            return false;
        }
        let now = Instant::now();
        let mut times = self.recent_emits.borrow_mut();
        while times
            .front()
            .is_some_and(|t| now.duration_since(*t) > RUNAWAY_WINDOW)
        {
            times.pop_front();
        }
        times.push_back(now);
        if times.len() > RUNAWAY_LIMIT {
            DISABLED.store(true, Ordering::Relaxed);
            crate::log::log("RUNAWAY circuit breaker latched — input disabled until reset");
            eprintln!("GlowKey: runaway detected — transformation disabled. Restart to re-enable.");
            return false;
        }
        true
    }
    /// Emits one edit through the session-posting path, honoring the circuit breaker
    /// and debug logging.
    pub(super) fn emit_edit(&self, response: &KeyResponse) {
        if !self.circuit_ok() {
            return;
        }
        // Timed and logged on the next line as `EMIT took=…µs`.
        //
        // The span is the emit alone, not the whole keystroke, because the emit is
        // the only part that can cost milliseconds: the Chromium omnibox guard
        // below makes an accessibility round-trip capped at 50 ms (§6.1). Timing
        // the whole of `handle_key_down` instead would fold in `save_settings`'s
        // file write and `hud::flash`'s first-call window creation, which happen
        // on hotkey and per-app-toggle keys — so a slow number would sometimes
        // mean "wrote a settings file", and anyone reading the log for a typing
        // latency report would be sent at the wrong subsystem. The engine's own
        // cost is not a suspect and is pinned separately at a couple of
        // microseconds by `crates/glowkey-engine/tests/latency.rs`.
        let started = Instant::now();
        // Chromium omnibox guard: the omnibox's inline autocomplete keeps a
        // trailing selection, which the first synthetic Backspace would delete
        // instead of a character (`hoongf`→`hoồng`). When an edit with backspaces
        // is about to land in a Chromium browser AND the focused element really
        // has a selection (one cheap AX check), clear the selection first with a
        // forward-delete. In a normal field the selection is empty, so nothing is
        // posted and nothing can regress; forward-delete is also a no-op at the
        // end of the text, GlowKey's normal caret position.
        if response.backspaces > 0 {
            let chromium = self
                .last_bundle_id
                .borrow()
                .as_deref()
                .is_some_and(is_chromium_browser);
            if chromium && crate::ax::focused_text_field_has_selection() {
                crate::log::log("OMNIBOX trailing selection detected — clearing with ⌦");
                post_key(&self.source, KEY_CODE_FORWARD_DELETE as u16, true);
                post_key(&self.source, KEY_CODE_FORWARD_DELETE as u16, false);
            }
        }
        if debug_enabled() {
            eprintln!(
                "GlowKey emit: backspaces={} insert={:?}",
                response.backspaces, response.insert
            );
        }
        emit(&self.source, response);
        crate::log::log(&format!("EMIT took={}µs", started.elapsed().as_micros()));
    }
}

/// Emits the engine's edit — `backspaces` deletions then the inserted text — at the
/// session level. Session posting goes through the normal input path, which the OS
/// routes to the focused element correctly even for multi-process apps (Chrome's
/// text field lives in a renderer process, not the main one). GlowKey's own events
/// are tagged on their source and skipped by the tap, so they do not feed back.
pub(super) fn emit(source: &CGEventSource, response: &KeyResponse) {
    for _ in 0..response.backspaces {
        post_key(source, KEY_CODE_DELETE as u16, true);
        post_key(source, KEY_CODE_DELETE as u16, false);
    }
    if !response.insert.is_empty() {
        post_string(source, &response.insert);
    }
}

/// Posts a synthetic keystroke at the session level, from GlowKey's tagged source.
pub(super) fn post_key(source: &CGEventSource, keycode: u16, key_down: bool) {
    post_key_with_flags(source, keycode, CGEventFlags(0), key_down);
}

/// Posts a synthetic keystroke carrying explicit modifier flags. Replaying a
/// boundary key needs the flags the user actually held, or ⇧1 comes back as `1`
/// instead of `!`.
pub(super) fn post_key_with_flags(
    source: &CGEventSource,
    keycode: u16,
    flags: CGEventFlags,
    key_down: bool,
) {
    if let Some(event) = CGEvent::new_keyboard_event(Some(source), keycode, key_down) {
        CGEvent::set_flags(Some(&event), flags);
        CGEvent::post(CGEventTapLocation::SessionEventTap, Some(&event));
    }
}

/// Posts a synthetic key event carrying a Unicode string, from GlowKey's source.
pub(super) fn post_string(source: &CGEventSource, text: &str) {
    let utf16: Vec<u16> = text.encode_utf16().collect();
    // Key-down carries the string; a matching key-up keeps the event pair balanced.
    for key_down in [true, false] {
        let Some(event) = CGEvent::new_keyboard_event(Some(source), 0, key_down) else {
            return;
        };
        if key_down {
            unsafe {
                CGEvent::keyboard_set_unicode_string(
                    Some(&event),
                    utf16.len() as u64,
                    utf16.as_ptr(),
                );
            }
        }
        CGEvent::post(CGEventTapLocation::SessionEventTap, Some(&event));
    }
}

/// GlowKey's own bundle identifier (None when running unbundled, e.g. tests).
pub(super) fn own_bundle_id() -> Option<String> {
    use std::sync::OnceLock;
    static OWN: OnceLock<Option<String>> = OnceLock::new();
    OWN.get_or_init(|| {
        objc2_foundation::NSBundle::mainBundle()
            .bundleIdentifier()
            .map(|s| s.to_string())
    })
    .clone()
}

/// Bundle identifier of the frontmost application, for the ignore list.
pub(super) fn frontmost_bundle_id() -> Option<String> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;
    app.bundleIdentifier().map(|s| s.to_string())
}

/// True when the event was synthesized by GlowKey — its source carries our tag.
/// Reading the source from the event is the documented way to recognize our own
/// output and avoid a feedback loop.
pub(super) fn is_own_event(event: NonNull<CGEvent>) -> bool {
    let Some(source) = CGEvent::new_source_from_event(Some(unsafe { event.as_ref() })) else {
        return false;
    };
    CGEventSource::user_data(Some(&source)) == GLOWKEY_TAG
}
