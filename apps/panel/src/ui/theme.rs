//! Shared light Material palette and logical-pixel scale.
use eframe::egui::{self, Color32, FontId, TextStyle};
pub(crate) mod scale {
    use eframe::egui::Color32;

    pub(crate) const INK: Color32 = Color32::from_rgb(0x1f, 0x1f, 0x1f);
    pub(crate) const SECONDARY: Color32 = Color32::from_rgb(0x44, 0x47, 0x46);
    pub(crate) const LINE: Color32 = Color32::from_rgb(0xda, 0xdc, 0xe0);
    pub(crate) const DOWNLOAD: Color32 = Color32::from_rgb(0x0b, 0x57, 0xd0);
    pub(crate) const UPLOAD: Color32 = Color32::from_rgb(0xb2, 0x67, 0x00);
    /// Measured module temperature.  A hue of its own: the rate chart's blue and amber already mean
    /// download and upload, and the status tones own green/amber/red, so a violet line can never be
    /// read as a throughput series or as a verdict.
    pub(crate) const TEMPERATURE: Color32 = Color32::from_rgb(0x6b, 0x4f, 0xb5);
    pub(crate) const WARNING: Color32 = Color32::from_rgb(0x91, 0x61, 0x00);
    pub(crate) const AXIS_LABEL: Color32 = Color32::from_rgb(0x62, 0x67, 0x6c);
    pub(crate) const GRID: Color32 = Color32::from_rgb(0xeb, 0xeb, 0xeb);
    pub(crate) const AXIS: Color32 = Color32::from_rgb(0xd8, 0xd8, 0xd8);

    pub(crate) const PAGE: f32 = 22.0;
    pub(crate) const SECTION: f32 = 16.0;
    pub(crate) const LABEL: f32 = 14.0;
    pub(crate) const BODY: f32 = 14.0;
    pub(crate) const BUTTON: f32 = 14.0;
    pub(crate) const META: f32 = 12.0;
    pub(crate) const RATE_NUMBER: f32 = 28.0;
    pub(crate) const RATE_AUX: f32 = 13.0;

    pub(crate) const MUTED: Color32 = SECONDARY;
    pub(crate) const FAINT: Color32 = AXIS_LABEL;
    pub(crate) const DETAIL: Color32 = Color32::from_rgb(0x55, 0x5f, 0x6b);

    pub(crate) const SECTION_MARGIN: [f32; 2] = [12.0, 12.0];
    pub(crate) const SECTION_GAP: f32 = 12.0;
    pub(crate) const COLUMN_GAP: f32 = 12.0;
    pub(crate) const ROW_GAP: f32 = 8.0;
    pub(crate) const CONTROL_GAP: [f32; 2] = [8.0, 6.0];

    pub(crate) const LABEL_COLUMN: f32 = 96.0;
}

pub(crate) fn style_root(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Light);
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = Color32::from_rgb(0xf6, 0xf8, 0xfc);
    visuals.window_fill = Color32::WHITE;
    visuals.faint_bg_color = Color32::from_rgb(0xf1, 0xf3, 0xf4);
    visuals.extreme_bg_color = Color32::from_rgb(0xf8, 0xfa, 0xfd);
    visuals.widgets.noninteractive.bg_fill = Color32::WHITE;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, scale::INK);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(0xf1, 0xf3, 0xf4);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xe8, 0xf0, 0xfe);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(0xd3, 0xe3, 0xfd);
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
    ] {
        widget.rounding = egui::Rounding::same(8.0);
        widget.bg_stroke = egui::Stroke::new(1.0_f32, Color32::from_rgb(116, 119, 117));
        widget.fg_stroke = egui::Stroke::new(1.0_f32, scale::INK);
    }
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, scale::DOWNLOAD);
    visuals.selection.bg_fill = scale::DOWNLOAD;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, Color32::WHITE);
    visuals.hyperlink_color = scale::DOWNLOAD;
    visuals.window_rounding = egui::Rounding::same(16.0);
    visuals.window_stroke = egui::Stroke::NONE;

    let mut style = (*ctx.style()).clone();
    style.visuals = visuals;
    style.animation_time = 0.1;
    style.spacing.item_spacing = egui::vec2(8.0, scale::SECTION_GAP);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.window_margin = egui::Margin::same(16.0);
    style.spacing.interact_size = egui::vec2(32.0, 32.0);

    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(scale::PAGE)),
        (TextStyle::Body, FontId::proportional(scale::BODY)),
        (TextStyle::Button, FontId::proportional(scale::BUTTON)),
        (TextStyle::Monospace, FontId::monospace(scale::BODY)),
        (TextStyle::Small, FontId::proportional(scale::META)),
    ]
    .into();
    // Keep both style slots consistent: system preference events must not restore tiny defaults.
    ctx.set_style_of(egui::Theme::Light, style.clone());
    ctx.set_style_of(egui::Theme::Dark, style);
}

/// Consistent, clearly visible primary action across page and confirmation surfaces.
pub(crate) fn primary_button(label: impl Into<String>) -> impl egui::Widget + 'static {
    PrimaryButton {
        label: label.into(),
    }
}

struct PrimaryButton {
    label: String,
}

impl egui::Widget for PrimaryButton {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let enabled = ui.is_enabled();
        ui.scope(|ui| {
            let widgets = &mut ui.visuals_mut().widgets;
            let disabled_fill = Color32::from_rgb(232, 234, 237);
            widgets.noninteractive.weak_bg_fill = disabled_fill;
            widgets.inactive.weak_bg_fill = if enabled {
                scale::DOWNLOAD
            } else {
                disabled_fill
            };
            widgets.hovered.weak_bg_fill = if enabled {
                Color32::from_rgb(23, 100, 220)
            } else {
                disabled_fill
            };
            widgets.active.weak_bg_fill = if enabled {
                Color32::from_rgb(9, 75, 180)
            } else {
                disabled_fill
            };
            ui.add(
                egui::Button::new(
                    egui::RichText::new(self.label)
                        .color(if enabled {
                            Color32::WHITE
                        } else {
                            scale::AXIS_LABEL
                        })
                        .size(scale::BUTTON),
                )
                .stroke(egui::Stroke::NONE)
                .rounding(18.0)
                .min_size(egui::vec2(88.0, 36.0)),
            )
        })
        .inner
    }
}

#[cfg(test)]
mod material_regressions {
    use super::*;

    #[test]
    fn light_theme_keeps_controls_sized_after_system_theme_changes() {
        let ctx = egui::Context::default();
        ctx.set_theme(egui::Theme::Dark);
        style_root(&ctx);
        ctx.set_theme(egui::Theme::Light);
        assert!(ctx.style().spacing.interact_size.y >= 32.0);
        assert_eq!(ctx.style().visuals.selection.bg_fill, scale::DOWNLOAD);
    }
}
