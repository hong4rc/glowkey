//! The About window: the shape of the macOS one (`app/src/about_window.rs`).
//!
//! Icon, name, version with the commit, one line of description, the credit
//! line, and the one Windows-specific note (elevated windows). No button: it is
//! a window, closed from its title bar or with Esc, and it never blocks
//! anything — unlike the message box it replaces, which was modal, played the
//! system sound, and held the main loop so the hotkey's indicator refresh never
//! ran while it was up.

use std::sync::OnceLock;

use eframe::egui;

use crate::strings::t;

/// The window, before it opens.
pub fn viewport_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(t("About GlowKey", "Giới thiệu GlowKey"))
        .with_inner_size([360.0, 300.0])
        .with_resizable(false)
        .with_maximize_button(false)
        .with_minimize_button(false)
        .with_icon(super::settings_ui::window_icon())
}

/// `0.1.0 (44a38fa)` — the version with the commit `build.rs` stamped, when
/// there is one. The version alone names a dozen builds; the commit is the part
/// a bug report needs.
pub fn build_string() -> String {
    match option_env!("GLOWKEY_COMMIT") {
        Some(commit) if !commit.is_empty() => {
            format!("{} ({commit})", env!("CARGO_PKG_VERSION"))
        }
        _ => env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// One frame of the window.
pub fn draw(ctx: &egui::Context) {
    super::settings_ui::apply_theme(ctx);
    let typing = ctx.memory(|m| m.focused().is_some());
    if !typing && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(ctx.style().visuals.window_fill)
                .inner_margin(egui::Margin::symmetric(24.0, 18.0)),
        )
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                if let Some(icon) = icon_texture(ctx) {
                    ui.add(egui::Image::new(&icon).fit_to_exact_size(egui::vec2(64.0, 64.0)));
                    ui.add_space(6.0);
                }
                ui.label(egui::RichText::new("GlowKey").strong().size(22.0));
                ui.add_space(2.0);
                // The one string a user is ever asked to quote back: selectable,
                // and a Copy beside it so retyping a commit hash is never asked.
                let version = t("Version {}", "Phiên bản {}").replace("{}", &build_string());
                ui.add(
                    egui::Label::new(egui::RichText::new(version).small().color(secondary(ui)))
                        .selectable(true),
                );
                if ui.small_button(t("Copy", "Chép")).clicked() {
                    ctx.copy_text(build_string());
                }
                ui.add_space(10.0);
                ui.label(t(
                    "Vietnamese input for Windows.",
                    "Bộ gõ tiếng Việt cho Windows.",
                ));
                ui.label(
                    egui::RichText::new(t(
                        "A UniKey-style input method, written entirely in Rust.",
                        "Bộ gõ kiểu UniKey, viết hoàn toàn bằng Rust.",
                    ))
                    .small()
                    .color(secondary(ui)),
                );
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(t(
                        "GlowKey cannot type into windows that run as administrator; \
                         Windows blocks input from ordinary programs into them.",
                        "GlowKey không gõ được vào cửa sổ chạy với quyền quản trị; \
                         Windows chặn nhập liệu từ chương trình thường vào những cửa sổ đó.",
                    ))
                    .small()
                    .color(secondary(ui)),
                );
            });
        });
}

fn secondary(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_gray(170)
    } else {
        egui::Color32::from_gray(90)
    }
}

/// The icon as a texture, uploaded once. Decoding the PNG per frame would be
/// waste; there is one context for the life of the process.
fn icon_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    static TEXTURE: OnceLock<Option<egui::TextureHandle>> = OnceLock::new();
    TEXTURE
        .get_or_init(|| {
            let icon = super::settings_ui::window_icon();
            if icon.rgba.is_empty() {
                return None;
            }
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [icon.width as usize, icon.height as usize],
                &icon.rgba,
            );
            Some(ctx.load_texture("glowkey-about-icon", image, egui::TextureOptions::LINEAR))
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_build_string_names_the_crate_version() {
        assert!(build_string().starts_with(env!("CARGO_PKG_VERSION")));
    }

    /// The window renders headlessly, and Esc asks it to close.
    #[test]
    fn the_window_draws_and_escape_closes_it() {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), draw);

        let mut input = egui::RawInput::default();
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let output = ctx.run(input, draw);
        let root = &output.viewport_output[&egui::ViewportId::ROOT];
        assert!(root
            .commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Close)));
    }
}
