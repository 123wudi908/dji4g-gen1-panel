//! Material navigation drawer. Labels remain visible at every supported desktop width.
use super::{icons, scale};
use crate::app::{NAV_ITEMS, Page};
use crate::localization::{Language, LocalizedText};
use eframe::egui::{self, Color32, RichText, Ui};

pub(crate) fn navigation(ui: &mut Ui, current: &mut Page, language: Language) {
    ui.spacing_mut().item_spacing.y = 4.0;
    ui.add_space(8.0);
    for (page, key) in NAV_ITEMS.into_iter().filter(|(p, _)| *p != Page::Settings) {
        nav_item(ui, current, page, LocalizedText::new(language, key).text);
    }
    ui.add_space(16.0);
    nav_item(ui, current, Page::Settings, "设置".into());
}
fn glyph(page: Page) -> &'static str {
    match page {
        Page::Overview => "\u{e871}",
        Page::Sms => "\u{e159}",
        Page::DeviceTools => "\u{e429}",
        Page::Diagnostics => "\u{e640}",
        Page::Repairs => "\u{f8cd}",
        Page::Settings => "\u{e8b8}",
    }
}
fn nav_item(ui: &mut Ui, current: &mut Page, page: Page, label: String) {
    let selected = *current == page;
    let ink = if selected {
        Color32::from_rgb(4, 30, 73)
    } else {
        scale::SECONDARY
    };
    let response = ui
        .scope(|ui| {
            let widgets = &mut ui.visuals_mut().widgets;
            widgets.inactive.weak_bg_fill = if selected {
                Color32::from_rgb(211, 227, 253)
            } else {
                Color32::TRANSPARENT
            };
            widgets.hovered.weak_bg_fill = if selected {
                Color32::from_rgb(203, 221, 251)
            } else {
                Color32::from_rgb(232, 237, 244)
            };
            widgets.active.weak_bg_fill = if selected {
                Color32::from_rgb(193, 213, 248)
            } else {
                Color32::from_rgb(221, 230, 242)
            };
            ui.add_sized(
                [ui.available_width(), 44.0],
                egui::Button::new("")
                    .stroke(egui::Stroke::NONE)
                    .rounding(22.0),
            )
        })
        .inner;
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            &label,
        )
    });
    let mut job = egui::text::LayoutJob::default();
    let icon = icons::text(ui.ctx(), glyph(page), 20.0).color(ink);
    let icon_galley = egui::WidgetText::from(icon).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::FontSelection::Default,
    );
    ui.painter().galley(
        egui::pos2(
            response.rect.left() + 12.0,
            response.rect.center().y - icon_galley.size().y / 2.0,
        ),
        icon_galley,
        ink,
    );
    job.append(
        &label,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(14.0),
            color: ink,
            ..Default::default()
        },
    );
    let text = ui.fonts(|f| f.layout_job(job));
    ui.painter().galley(
        egui::pos2(
            response.rect.left() + 44.0,
            response.rect.center().y - text.size().y / 2.0,
        ),
        text,
        ink,
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.shrink(2.0),
            20.0,
            egui::Stroke::new(2.0_f32, scale::DOWNLOAD),
        );
    }
    if response.clicked() {
        *current = page;
    }
}
pub(crate) fn sidebar_width(width: f32) -> f32 {
    if width < 1000.0 { 148.0 } else { 188.0 }
}

pub(crate) fn brand(ui: &mut Ui) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.label(RichText::new("DJI 4G").size(20.0).color(scale::INK));
        ui.label(RichText::new("模块管理").size(12.0).color(scale::SECONDARY));
    });
}
