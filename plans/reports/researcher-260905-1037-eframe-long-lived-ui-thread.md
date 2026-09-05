# eframe 0.29 / egui 0.29 / winit 0.30: long-lived Settings+About on a background thread (Windows)

Scope: pinned eframe 0.29.1 / egui 0.29.1 / winit 0.30.13, no upgrade. Report only.

## Recommended architecture (concise)

1. Spawn one dedicated background OS thread at process start (`std::thread::Builder::new().name("glowkey-ui")`), never the hook/tray thread. Call `eframe::run_native` exactly once on it, with `NativeOptions.event_loop_builder` set to call `EventLoopBuilderExtWindows::with_any_thread(true)`. This call **blocks that thread for the process's life** — it is not "run once per open", it is "run once per process, forever."
2. Root viewport: `ViewportBuilder::default().with_visible(false)`. **Never send `ViewportCommand::Visible(true)` to it** — on Windows 11 + eframe 0.29.1 this is a known broken path (see Q2/pitfalls). Root's `close_requested` is always answered with `ViewportCommand::CancelClose` — the root never actually closes while the process runs; it exists only as the carrier for the event loop and to host child viewports.
3. Settings and About are each a separate **deferred viewport** (`ctx.show_viewport_deferred`), opened/closed by whether the root's `update()` calls `show_viewport_deferred(id, builder, closure)` that frame — not by toggling `Visible`. Stop calling it to close; call it again with the same `ViewportId` to reopen. State (e.g. `SettingsApp`, `AboutApp` equivalents) lives in the root `App` struct, owned across opens/closes — or is reconstructed fresh each reopen, your choice; both are legal since the viewport is only "alive" while you keep calling it.
4. Commands from the hook/tray thread (open Settings, open About, quit) go through `std::sync::mpsc::Sender` into the root app; after sending, call `egui::Context::request_repaint()` (or `request_repaint_after(Duration::ZERO)`) to wake the otherwise-idle event loop. `Context` is `Send + Sync` and `request_repaint()` is documented safe to call from any thread.
5. Idle cost: with root hidden and no viewport open, eframe/egui is purely reactive — no render loop runs — repaint happens only on `request_repaint()` or real OS input events. CPU cost at rest is ~0.
6. Quit: do not try to gracefully unwind the event loop through `ViewportCommand::Close` on root (that's the path you're deliberately blocking with `CancelClose`). Simplest correct option consistent with this being a single background-utility process: tray "Exit" performs its existing cleanup (unhook `WH_KEYBOARD_LL`, `PostQuitMessage`, etc.) on the message-loop thread, then calls `std::process::exit(0)` — OS reclaims the UI thread and its window(s) too. This is a design choice, flag it to the user before implementing since it changes the existing single-`run_native`-call structure in `settings_ui.rs`.

This directly replaces the current design (`app/src/platform/windows/settings_ui.rs`), which opens/blocks/tears-down a whole `run_native` per Settings open on the **hook's own thread**, and explicitly documents (in its own doc comment) that a second `run_native` call in-process fails with `RecreationAttempt` — the exact constraint this research resolves.

## Q1 — event loop on non-main thread

`NativeOptions::event_loop_builder: Option<eframe::EventLoopBuilderHook>` where `EventLoopBuilderHook = Box<dyn FnOnce(&mut winit::event_loop::EventLoopBuilder<UserEvent>)>` (docs.rs, eframe 0.29.1). Inside it call `use winit::platform::windows::EventLoopBuilderExtWindows; builder.with_any_thread(true);`.

Pitfalls:
- `run_native` **blocks the calling thread** until the whole event loop exits (not per-window) — this has been true since egui's 0.24/0.25 multi-viewport rewrite; there is no "run and return when this window closes" mode anymore (see below, Q2). Call it from a thread you're prepared to give up forever, i.e. a dedicated one, not the hook/message-loop thread.
- `with_any_thread(true)` docs (winit `EventLoopBuilderExtWindows`) warn: windows created on that thread are destroyed when the thread terminates, and using a window after its owning thread has ended is unspecified. Not an issue here since the UI thread lives for the process's life.
- Windows GL contexts (WGL, which is what eframe's `glow` backend uses) are thread-affine: don't touch the `glow::Context` / GL objects from any thread other than the one that created them. Since everything (event loop, egui, glow rendering) stays on the one dedicated UI thread, this is automatically satisfied — just don't reach into eframe's GL state from the hook thread.
- Two independent Win32 message loops in one process (main thread's tray+hook loop, UI thread's winit loop) is legitimate; nothing about `WH_KEYBOARD_LL` or the tray's window class requires being on the same thread as the eframe window.

Sources: [EventLoopBuilderExtWindows (winit docs.rs)](https://docs.rs/winit/latest/x86_64-pc-windows-msvc/winit/platform/windows/trait.EventLoopBuilderExtWindows.html), [Running egui app outside main thread — egui discussion #1489](https://github.com/emilk/egui/discussions/1489), [How To Run Winit On A Non-Main Thread](https://yutani.rbind.io/post/winit-and-r/).

## Q2 — hidden root viewport, Close semantics, preventing exit

- `ViewportCommand::Close` on the **root** viewport: if not intercepted, the whole app shuts down after that frame.
- To keep root alive: `if ctx.input(|i| i.viewport().close_requested()) { ctx.send_viewport_cmd(ViewportCommand::CancelClose); }`.
- `ViewportCommand::Visible(bool)` exists and is the replacement for the old `frame.set_visible()`.
- **Critical known bug (eframe 0.29.1 on Windows 11, exactly your pinned version):** once a viewport (including root) is set `Visible(false)`, it stops receiving the repaint/redraw events eframe needs to *process* further viewport commands — so a later `ViewportCommand::Visible(true)` sent to that same viewport is silently ignored; the window never comes back. This is filed as [egui issue #3655 — `send_viewport_cmd` does not work for invisible viewports](https://github.com/emilk/egui/issues/3655) and [egui issue #5229 — Can't set window Visible(true) again](https://github.com/emilk/egui/issues/5229), fixed later via PR #7905 (post-dates 0.29.1; not available without an upgrade you're not doing). **Implication: do not implement Settings show/hide by toggling the root's (or any single persistent viewport's) `Visible` command on this version.** Set `with_visible(false)` once at creation and never touch it again; use deferred viewports (Q3) for anything the user needs to actually see and reopen.
- `run_and_return` (the old eframe field that let `run_native` return once the window closed while the process continued) does **not** exist in this API generation — it was removed when multi-viewport landed (~0.24/0.25). `run_native` now always blocks for the process's life; do not look for it in 0.29's `NativeOptions`.
- `App::on_exit` still exists and fires once when the event loop actually terminates (i.e., only if you let root's close go through) — irrelevant to the "stay alive" design.

Sources: [ViewportCommand (egui docs.rs)](https://docs.rs/egui/latest/egui/viewport/enum.ViewportCommand.html), [Hiding the main window / on_close handling — egui discussion #3885](https://github.com/emilk/egui/discussions/3885), [issue #3655](https://github.com/emilk/egui/issues/3655), [issue #5229](https://github.com/emilk/egui/issues/5229), [eframe CHANGELOG](https://github.com/emilk/egui/blob/main/crates/eframe/CHANGELOG.md).

## Q3 — deferred vs immediate viewports for Settings + About

- `Context::show_viewport_deferred(id, builder, closure)`: closure is `'static` and stored, called by the integration possibly at a different point than the call site, repainted **independently** of the parent — cheapest, but the closure/captured state must be communicated in (Arc/Mutex or channels) since it doesn't share the calling stack frame the way an immediate viewport's closure does. Best for Settings, which is long-lived and stateful.
- `Context::show_viewport_immediate(id, builder, closure)`: runs the closure right there, simpler to reason about, but couples repaint of parent and child (either needing repaint forces both to repaint) — fine for the small About window.
- Recommendation: **deferred for both** is simplest and consistent (one mental model, one pattern in the codebase); immediate for About is also fine given how small/rare it is. Given KISS/DRY, use the same deferred pattern for both rather than mixing.
- Each viewport must be called with `show_viewport_*` **every frame it should stay open**; stop calling it (flip a bool you check before calling) to close, and calling it again later with the *same* `ViewportId` reopens it — this is the documented mechanism for close+reopen, not a create/destroy API.
- Per-viewport close is observed the same way as root: inside that viewport's closure, `ctx.input(|i| i.viewport().close_requested())` (this reads the *current* viewport's state when called from within its own closure) — set your local `open` flag false there, do not call `CancelClose` unless you want to veto that particular window's close (e.g. "unsaved changes" prompt — not needed here).
- Each viewport gets its own `ViewportBuilder` (`with_title`, `with_inner_size`, `with_icon`, etc.) — same API surface as your existing `NativeOptions.viewport` builder in `settings_ui.rs`, just attached to a `ViewportId` instead of to `NativeOptions`.

Sources: [egui::viewport module docs.rs](https://docs.rs/egui/latest/egui/viewport/index.html), [multiple_viewports example (emilk/egui)](https://github.com/emilk/egui/blob/main/examples/multiple_viewports/src/main.rs), [Context docs.rs](https://docs.rs/egui/latest/egui/struct.Context.html).

## Q4 — waking the loop from another thread

- `egui::Context` is `Send + Sync` in this version; `Context::request_repaint()` is safe and documented to work from any thread — when called off the UI thread, eframe's registered repaint callback wakes the winit event loop (`EventLoopProxy`-based under the hood).
- Pattern: keep one `mpsc::Sender<UiCommand>` (e.g. `OpenSettings`, `OpenAbout`, `Quit`) cloned into the hook/tray thread; keep the `Receiver` polled inside root's `update()` (non-blocking `try_recv` loop). After `sender.send(...)`, call `ctx.request_repaint()` using a `Context` clone captured when the app was constructed (or via `CreationContext::egui_ctx` handed out at `run_native` construction time) so the tray thread can wake the idle UI thread without holding any lock the UI thread needs.
- Do **not** hand the whole `Context` around expecting arbitrary calls to be safe from other threads — only `request_repaint`/texture allocation are the documented cross-thread-safe operations; UI construction (anything using `ctx` inside `update`) must stay on the UI thread. (Older issues about "already mutably borrowed" panics are historical/pre-multi-viewport; current docs.rs `Context` states repaint calls are the safe cross-thread surface.)

Sources: [Context docs.rs](https://docs.rs/egui/latest/egui/struct.Context.html), [Handling actions in another thread — egui discussion #484](https://github.com/emilk/egui/discussions/484), [Consider making Context !Send+!Sync — issue #1399](https://github.com/emilk/egui/issues/1399) (historical debate, resolved as Send+Sync in current releases).

## Q5 — idle cost

eframe/egui only repaints on demand (input event, `request_repaint`/`request_repaint_after`) — it is not a fixed-framerate render loop by default. With root hidden (`with_visible(false)`, never toggled) and zero viewports currently being called via `show_viewport_deferred`/`_immediate`, there is no window to draw and no scheduled repaint, so glow issues no draw calls and the UI thread parks on the winit event loop waiting for a wakeup. Cost at rest is effectively the cost of one idle, hidden win32 window (message pump only) — negligible, and no different in kind from what the current per-open `run_native` design already pays only while a window is open; the difference here is the same near-zero floor is sustained across the process's life rather than the window being fully destroyed between opens.

## Q6 — known egui 0.29 issues with deferred viewports on Windows

Directly relevant found in this research: the Visible(false)→Visible(true) deadlock (#3655, #5229, Q2) — the main one that would bite this design if implemented naively. Also found, not directly load-bearing for this task but worth knowing: [issue #4945](https://github.com/emilk/egui/issues/4945) — `request_repaint()`-driven animation doesn't work correctly while a deferred viewport is open (repaint scheduling interaction between parent/child); and [issue #4758](https://github.com/emilk/egui/issues/4758) — text selection glitches across multiple deferred viewports when an unfocused one needs repaint. Neither blocks Settings/About (no continuous animation, no cross-viewport text selection expected), but note them if focus-follow or animated widgets are added later.

`ViewportBuilder::with_taskbar(bool)` exists (controls whether that viewport gets its own taskbar entry) — I did not find a citable GitHub issue specific to "wrong taskbar icon per deferred viewport" within the 6-query budget for this report; treat per-viewport taskbar behavior as **untested for this exact combination** and verify empirically once Settings+About are both deferred viewports (this is the one item in this report not independently corroborated by 3 sources — flagged under Limitations).

## Trade-off summary

| Approach | Reopen support | Idle cost | Windows-0.29-safe | Complexity |
|---|---|---|---|---|
| Current: `run_native` per open, on hook thread | No (2nd call = `RecreationAttempt`) | Zero when closed | Yes, but fails on 2nd open | Low, but broken for repeat use |
| Toggle root `Visible(true/false)` | Yes in theory | Zero when hidden | **No** — broken on Win11+0.29.1 (#3655/#5229) | Low but non-functional |
| One background thread, one `run_native`, hidden root forever, Settings/About as deferred viewports | Yes, arbitrarily many times | Zero at rest | Yes | Moderate (channel plumbing, one-time thread/lifetime redesign) |

Recommended: the third row. It's the only option that satisfies all four stated requirements on this exact version pin.

## Adoption risk / maturity

eframe/egui multi-viewport (the feature this whole design leans on) shipped ~0.24–0.25 (late 2023) and is still evolving — the Visible-toggle bug found here was open against 0.29.1 and only fixed in a later, unpinned release. egui's viewport API has had several small breaking changes release-to-release historically (`on_close_event` → `close_requested`+`CancelClose`, `frame.close()` → `send_viewport_cmd`), so staying pinned at 0.29 means staying on the current, exact behavior described above — do not assume any egui example/discussion dated to a version outside 0.28–0.30 applies verbatim.

## Limitations / unresolved

- `with_taskbar` / per-deferred-viewport taskbar and focus behavior on Windows 11 for this exact version: not independently verified against a citable source within the search budget (6 queries used: 4 WebSearch, 2 WebFetch). Recommend a quick manual check once implemented (open Settings, open About, alt-tab, check taskbar) rather than trusting this report on that specific point.
- Did not verify whether `EventLoopBuilderExtWindows::with_any_thread` interacts with the existing `Win32_UI_Accessibility` `SetWinEventHook` foreground-change hook already running on the main thread (`docs/decisions/0008`) — these are independent OS mechanisms (hook vs. window message pump) and should not conflict, but wasn't empirically tested here.
- Did not investigate an alternative "separate process" design (mentioned as a discarded option in the current `settings_ui.rs` doc comment) — out of scope; the background-thread approach was the one the research question asked to evaluate.

Status: DONE
Summary: Root cause of the requirement is real (winit: one event loop per process); the correct 0.29-safe design is one dedicated UI thread running `run_native` once for the process's life with `with_any_thread(true)`, a permanently-hidden root viewport (never toggle its Visible command — broken on this exact version/OS per #3655/#5229), and Settings/About each as independently open/closable deferred viewports driven by an mpsc channel + `request_repaint()` from the hook/tray thread.
Concerns/Blockers: Implementing this is a real architectural change to `app/src/platform/windows/settings_ui.rs` (new persistent thread, new quit path away from letting root actually close) — flag to the user as a design decision before implementing, not just a bugfix.
