//! Bundled Google Material Symbols outline icons; UI text always remains independently readable.
use eframe::egui::{self, RichText};
pub(crate) const MAIL: &str = "\u{e159}";
pub(crate) fn text(ctx: &egui::Context, glyph: &str, size: f32) -> RichText {
    if !ctx.data(|data| {
        data.get_temp::<bool>(egui::Id::new("panel-icons-installed"))
            .unwrap_or(false)
    }) {
        return RichText::new("");
    }
    RichText::new(glyph).font(egui::FontId::new(
        size,
        egui::FontFamily::Name("panel-icons".into()),
    ))
}

pub(crate) const GITHUB_PROJECT_URL: &str = "https://github.com/zhu-hailin/dji4g-gen1-panel";

/// A keyboard-accessible circular link; the texture is cached once for both themes.
pub(crate) fn github_project(
    ui: &mut egui::Ui,
    language: crate::localization::Language,
) -> egui::Response {
    let texture_id = egui::Id::new("github-mark-texture");
    let cached = ui
        .ctx()
        .data(|data| data.get_temp::<egui::TextureHandle>(texture_id));
    let texture = cached.unwrap_or_else(|| {
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [64, 64],
            include_bytes!("../../assets/github/mark-github-64.rgba"),
        );
        let texture = ui
            .ctx()
            .load_texture("GitHub mark", image, egui::TextureOptions::LINEAR);
        ui.ctx()
            .data_mut(|data| data.insert_temp(texture_id, texture.clone()));
        texture
    });
    let size = super::scale::CONTROL_H;
    let response = ui.add_sized(
        [size, size],
        egui::Button::new("")
            .rounding(egui::Rounding::same(size / 2.0))
            .stroke(egui::Stroke::NONE),
    );
    let label = super::t(language, crate::localization::TextKey::GitHubProject);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), &label)
    });
    let ink = if response.hovered() || response.has_focus() {
        super::scale::accent()
    } else {
        super::scale::secondary()
    };
    ui.painter().image(
        texture.id(),
        egui::Rect::from_center_size(response.rect.center(), egui::Vec2::splat(20.0)),
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        ink,
    );
    if response.has_focus() {
        ui.painter().circle_stroke(
            response.rect.center(),
            size / 2.0 - 1.0,
            egui::Stroke::new(2.0_f32, super::scale::accent()),
        );
    }
    if response.clicked() {
        ui.ctx()
            .open_url(egui::OpenUrl::new_tab(GITHUB_PROJECT_URL));
    }
    response
        .on_hover_text(label)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[cfg(test)]
mod github_tests {
    use super::*;
    #[test]
    fn github_link_opens_the_project_only_after_activation() {
        let ctx = egui::Context::default();
        super::super::apply_style(&ctx);
        let mut rect = egui::Rect::NOTHING;
        for tick in 0..3 {
            let events = if tick == 0 {
                vec![]
            } else {
                vec![
                    egui::Event::PointerMoved(rect.center()),
                    egui::Event::PointerButton {
                        pos: rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed: tick == 1,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            };
            let output = ctx.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        rect = github_project(ui, crate::localization::Language::ZhCn).rect;
                        assert!((rect.width() - rect.height()).abs() < 0.1);
                    });
                },
            );
            if tick < 2 {
                assert!(output.platform_output.open_url.is_none());
            } else {
                assert_eq!(
                    output.platform_output.open_url.unwrap().url,
                    GITHUB_PROJECT_URL
                );
            }
        }
    }
}
