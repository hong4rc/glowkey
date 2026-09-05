//! The one place eframe runs, for the life of the process.
//!
//! winit permits one event loop per process and offers no reset, so the old
//! shape — `run_native` per Settings open, on the hook's thread — worked exactly
//! once: the second open returned `RecreationAttempt`, and About had to be a
//! message box because no second toolkit window could ever exist. A message box
//! is modal, plays a sound, and its nested loop never lets the main loop turn,
//! so the hotkey's deferred indicator refresh sat in the queue while About was
//! open and the toggle looked dead. Three defects, one cause.
//!
//! Here the event loop runs once, on its own thread (`with_any_thread`), and
//! never ends. The root viewport is a **shim**: one point square, undecorated,
//! parked off-screen, no taskbar entry, never focused. It exists to carry the
//! loop and to host the real windows, which are *deferred viewports*: Settings
//! and About are open exactly while the root keeps asking for them each frame,
//! and reopen by asking again. That is the reopen the old shape could not do.
//!
//! **Why the root is not simply hidden.** On this egui (0.29.1) on Windows, a
//! viewport made invisible stops receiving the redraw events eframe needs to
//! process anything at all, including the command to become visible again
//! (egui issues #3655, #5229). A hidden root would therefore never drain its
//! command queue and no window could ever open. Off-screen and visible, it does.
//!
//! Threading rules, because there are now two message loops in the process:
//! the hook, the tray and `hook::with_session` stay on the main thread; this
//! thread never calls into them. It gets a `Settings` snapshot to edit and hands
//! the edited value back through `shell::deliver_settings_result`, which posts a
//! message to the main thread and lets it do the merge, the save and the session
//! rebuild — on the thread that owns them.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui;
use glowkey_engine::Settings;

use super::about_ui;
use super::settings_ui::{self, SettingsApp};
use crate::settings_spec::ListId;

/// Something the main thread asks the UI thread to do.
#[derive(Debug)]
pub enum UiCommand {
    /// Open Settings on this snapshot of the session, or bring it to front.
    OpenSettings(Settings),
    /// Open About, or bring it to front.
    OpenAbout,
}

static SENDER: OnceLock<Sender<UiCommand>> = OnceLock::new();
/// The context, once eframe has created it, so another thread can wake the
/// loop. `request_repaint` is the one call documented safe from any thread.
static CONTEXT: OnceLock<egui::Context> = OnceLock::new();

const SETTINGS_VIEWPORT: &str = "glowkey_settings";
const ABOUT_VIEWPORT: &str = "glowkey_about";

/// Spawns the UI thread. Called once at startup, after the session exists.
///
/// If the thread cannot be spawned, [`open_settings`] and [`open_about`] log
/// and do nothing; GlowKey keeps typing.
pub fn start() {
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("glowkey-ui".into())
        .spawn(move || run(rx))
        .is_ok();
    if spawned {
        let _ = SENDER.set(tx);
    } else {
        crate::log::log("UI could not start the UI thread — Settings and About are unavailable");
    }
}

/// Asks for Settings on `snapshot`.
pub fn open_settings(snapshot: Settings) {
    send(UiCommand::OpenSettings(snapshot));
}

/// Asks for About.
pub fn open_about() {
    send(UiCommand::OpenAbout);
}

fn send(command: UiCommand) {
    let Some(tx) = SENDER.get() else {
        crate::log::log("UI no UI thread; command dropped");
        return;
    };
    if tx.send(command).is_err() {
        crate::log::log("UI the UI thread is gone; command dropped");
        return;
    }
    // Wake the loop. Before the first frame the context does not exist yet;
    // the command waits in the channel and the first frame drains it.
    //
    // The root by name: `request_repaint()` targets whichever viewport is on
    // the context's stack *at this instant*, and from another thread that can
    // be Settings or About mid-frame. Only the root drains the channel; waking
    // a child would leave the command sitting there.
    if let Some(ctx) = CONTEXT.get() {
        ctx.request_repaint_of(egui::ViewportId::ROOT);
    }
}

/// The thread body: one `run_native`, forever.
fn run(rx: Receiver<UiCommand>) {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("GlowKey")
            .with_inner_size([1.0, 1.0])
            .with_position([-32000.0, -32000.0])
            .with_decorations(false)
            .with_resizable(false)
            .with_taskbar(false)
            .with_active(false),
        event_loop_builder: Some(Box::new(|builder| {
            use winit::platform::windows::EventLoopBuilderExtWindows;
            builder.with_any_thread(true);
        })),
        ..Default::default()
    };
    let result = eframe::run_native(
        "GlowKey",
        native_options,
        Box::new(move |cc| {
            settings_ui::install_system_font(&cc.egui_ctx);
            settings_ui::apply_style(&cc.egui_ctx);
            let _ = CONTEXT.set(cc.egui_ctx.clone());
            Ok(Box::new(UiHost::new(rx)))
        }),
    );
    if let Err(err) = result {
        crate::log::log(&format!("UI the UI thread's event loop ended: {err}"));
    }
}

/// The root app: drains commands and keeps the open windows alive.
struct UiHost {
    rx: Receiver<UiCommand>,
    /// The settings window while it is open. Behind a mutex because the
    /// deferred viewport's closure runs from the integration, not from here.
    settings: Option<Arc<Mutex<SettingsApp>>>,
    about_open: Arc<Mutex<bool>>,
    /// The viewports asked for in the last frame. Headless egui embeds child
    /// viewports, so their ids never reach the output; the tests read this.
    #[cfg(test)]
    asked_for: Vec<egui::ViewportId>,
}

impl UiHost {
    fn new(rx: Receiver<UiCommand>) -> Self {
        Self {
            rx,
            settings: None,
            about_open: Arc::new(Mutex::new(false)),
            #[cfg(test)]
            asked_for: Vec::new(),
        }
    }

    /// Takes every waiting command.
    fn drain(&mut self, ctx: &egui::Context) {
        while let Ok(command) = self.rx.try_recv() {
            match command {
                UiCommand::OpenSettings(snapshot) => {
                    if self.settings.is_some() {
                        ctx.send_viewport_cmd_to(settings_id(), egui::ViewportCommand::Focus);
                    } else {
                        self.settings = Some(Arc::new(Mutex::new(SettingsApp::new(snapshot))));
                    }
                }
                UiCommand::OpenAbout => {
                    if *lock(&self.about_open) {
                        ctx.send_viewport_cmd_to(about_id(), egui::ViewportCommand::Focus);
                    } else {
                        *lock(&self.about_open) = true;
                    }
                }
            }
        }
    }

    /// One frame of the root: drain, then ask for each open window.
    fn frame(&mut self, ctx: &egui::Context) {
        // The shim never closes; the process ends when the main loop does.
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        // A settings window whose user closed it has decided its result; hand
        // that to the main thread and stop asking for the viewport. Before the
        // commands, so an OpenSettings that arrived in the same instant makes a
        // fresh window rather than focusing the one about to go.
        if let Some(app) = &self.settings {
            let done = {
                let mut app = lock(app);
                app.take_result().map(|outcome| (app.baseline(), outcome))
            };
            if let Some((baseline, outcome)) = done {
                super::shell::deliver_settings_result(baseline, outcome);
                self.settings = None;
            }
        }

        self.drain(ctx);
        #[cfg(test)]
        self.asked_for.clear();

        if let Some(app) = &self.settings {
            let app = Arc::clone(app);
            #[cfg(test)]
            self.asked_for.push(settings_id());
            ctx.show_viewport_deferred(
                settings_id(),
                settings_ui::viewport_builder(),
                move |ctx, _class| {
                    lock(&app).draw(ctx);
                    if ctx.input(|i| i.viewport().close_requested()) {
                        // The root decides what happens next; make sure it runs.
                        ctx.request_repaint_of(egui::ViewportId::ROOT);
                    }
                },
            );
        }

        // The three list editors, each its own window while Settings has it
        // open. They close with Settings: once the app is dropped above, they
        // are no longer asked for.
        if let Some(app) = &self.settings {
            for list in ListId::ALL {
                if !lock(app).list_open(list) {
                    continue;
                }
                let app = Arc::clone(app);
                #[cfg(test)]
                self.asked_for.push(settings_ui::list_viewport_id(list));
                ctx.show_viewport_deferred(
                    settings_ui::list_viewport_id(list),
                    settings_ui::list_viewport_builder(list),
                    move |ctx, _class| {
                        let mut app = lock(&app);
                        app.draw_list(list, ctx);
                        if ctx.input(|i| i.viewport().close_requested()) {
                            app.set_list_open(list, false);
                            drop(app);
                            ctx.request_repaint_of(egui::ViewportId::ROOT);
                        }
                    },
                );
            }
        }

        if *lock(&self.about_open) {
            let open = Arc::clone(&self.about_open);
            #[cfg(test)]
            self.asked_for.push(about_id());
            ctx.show_viewport_deferred(
                about_id(),
                about_ui::viewport_builder(),
                move |ctx, _class| {
                    about_ui::draw(ctx);
                    if ctx.input(|i| i.viewport().close_requested()) {
                        *lock(&open) = false;
                        ctx.request_repaint_of(egui::ViewportId::ROOT);
                    }
                },
            );
        }
    }
}

impl eframe::App for UiHost {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame(ctx);
    }

    /// The colour behind every viewport. eframe's default is a hardcoded
    /// near-black that ignores the theme; this follows it.
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.window_fill().to_normalized_gamma_f32()
    }
}

fn settings_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of(SETTINGS_VIEWPORT)
}

fn about_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of(ABOUT_VIEWPORT)
}

/// A lock that survives a poisoned mutex: the UI is the last thing that should
/// take the process down over a panic in a paint.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host_with(commands: Vec<UiCommand>) -> UiHost {
        let (tx, rx) = mpsc::channel();
        for c in commands {
            tx.send(c).unwrap();
        }
        UiHost::new(rx)
    }

    /// A command opens its window; a second one focuses rather than doubles it.
    #[test]
    fn open_commands_create_one_window_each() {
        let ctx = egui::Context::default();
        let mut host = host_with(vec![
            UiCommand::OpenSettings(Settings::default()),
            UiCommand::OpenSettings(Settings::default()),
            UiCommand::OpenAbout,
            UiCommand::OpenAbout,
        ]);
        let _ = ctx.run(egui::RawInput::default(), |ctx| host.frame(ctx));
        assert!(host.settings.is_some());
        assert!(*lock(&host.about_open));
    }

    /// Once the settings window has decided its result, the root drops it, so
    /// the viewport is no longer asked for and the next open starts fresh.
    #[test]
    fn a_decided_settings_window_is_released() {
        let ctx = egui::Context::default();
        let mut host = host_with(vec![UiCommand::OpenSettings(Settings::default())]);
        let _ = ctx.run(egui::RawInput::default(), |ctx| host.frame(ctx));
        lock(host.settings.as_ref().unwrap()).finalize();
        let _ = ctx.run(egui::RawInput::default(), |ctx| host.frame(ctx));
        assert!(host.settings.is_none());
        // The result went to the main thread's slot.
        assert!(super::super::shell::take_pending_settings_result().is_some());
    }

    /// After the window has decided, another OpenSettings makes a new window
    /// on the new snapshot — the reopen the old shape could not do. Even when
    /// the command lands in the same frame as the decision.
    #[test]
    fn settings_reopens_fresh_after_a_decision() {
        let ctx = egui::Context::default();
        let (tx, rx) = mpsc::channel();
        let mut host = UiHost::new(rx);
        tx.send(UiCommand::OpenSettings(Settings::default()))
            .unwrap();
        let _ = ctx.run(egui::RawInput::default(), |ctx| host.frame(ctx));
        let first = Arc::clone(host.settings.as_ref().unwrap());
        lock(&first).finalize();

        let second_snapshot = Settings {
            auto_capitalize: true,
            ..Settings::default()
        };
        tx.send(UiCommand::OpenSettings(second_snapshot.clone()))
            .unwrap();
        let _ = ctx.run(egui::RawInput::default(), |ctx| host.frame(ctx));
        let second = host.settings.as_ref().expect("a new window");
        assert!(!Arc::ptr_eq(&first, second), "the old window was reused");
        assert!(lock(second).baseline().auto_capitalize);
        let _ = super::super::shell::take_pending_settings_result();
    }

    /// A list the settings window opened is asked for by the root as its own
    /// viewport, and released when the flag clears.
    #[test]
    fn an_open_list_gets_its_own_viewport() {
        let ctx = egui::Context::default();
        let mut host = host_with(vec![UiCommand::OpenSettings(Settings::default())]);
        let _ = ctx.run(egui::RawInput::default(), |ctx| host.frame(ctx));
        lock(host.settings.as_ref().unwrap()).set_list_open(ListId::Macros, true);
        let _ = ctx.run(egui::RawInput::default(), |ctx| host.frame(ctx));
        assert!(host.asked_for.contains(&settings_id()));
        assert!(
            host.asked_for
                .contains(&settings_ui::list_viewport_id(ListId::Macros)),
            "{:?}",
            host.asked_for
        );
        lock(host.settings.as_ref().unwrap()).set_list_open(ListId::Macros, false);
        let _ = ctx.run(egui::RawInput::default(), |ctx| host.frame(ctx));
        assert!(!host
            .asked_for
            .contains(&settings_ui::list_viewport_id(ListId::Macros)));
    }

    /// The root shim refuses to close: it carries the event loop.
    #[test]
    fn the_root_cancels_its_own_close() {
        let ctx = egui::Context::default();
        let mut host = host_with(Vec::new());
        let mut input = egui::RawInput::default();
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .events
            .push(egui::ViewportEvent::Close);
        let output = ctx.run(input, |ctx| host.frame(ctx));
        let root = &output.viewport_output[&egui::ViewportId::ROOT];
        assert!(root
            .commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::CancelClose)));
    }
}
