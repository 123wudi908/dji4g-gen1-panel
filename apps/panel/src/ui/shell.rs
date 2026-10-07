//! Navigation drawer for the panel's pages.
//!
//! The sidebar owns one thing: the page list. The availability verdict used to be pinned at its
//! foot as a chip; that duplicated the overview's banner, so it now lives only on the page that
//! owns it.
use super::{icons, scale};
use crate::app::{NAV_ITEMS, Page};
use crate::localization::{Language, LocalizedText, TextKey};
use eframe::egui::{self, Color32, RichText, Ui};

pub(crate) fn navigation(ui: &mut Ui, current: &mut Page, language: Language) {
    ui.spacing_mut().item_spacing.y = 2.0;
    for (page, key) in NAV_ITEMS.into_iter().filter(|(p, _)| *p != Page::Settings) {
        nav_item(ui, current, page, LocalizedText::new(language, key).text);
    }
    ui.add_space(6.0);
    nav_item(
        ui,
        current,
        Page::Settings,
        LocalizedText::new(language, TextKey::NavSettings).text,
    );
}

fn glyph(page: Page) -> &'static str {
    match page {
        Page::Overview => "\u{e871}",
        Page::Sms => "\u{e159}",
        Page::DeviceTools => "\u{e429}",
        Page::Diagnostics => "\u{e640}",
        Page::Repairs => "\u{f8cd}",
        // Signal bars: the page is the module's radio side, and this is the one glyph in the bundled
        // subset that says so without repeating a neighbour's icon.
        Page::Wireless => "\u{e202}",
        Page::Settings => "\u{e8b8}",
    }
}

fn nav_item(ui: &mut Ui, current: &mut Page, page: Page, label: String) {
    let selected = *current == page;
    // The current page shares the soft blue selection treatment of segmented controls.
    let ink = if selected {
        scale::on_selected()
    } else {
        scale::secondary()
    };
    let response = ui
        .scope(|ui| {
            let widgets = &mut ui.visuals_mut().widgets;
            widgets.inactive.weak_bg_fill = if selected {
                scale::selected()
            } else {
                Color32::TRANSPARENT
            };
            // Keep selected navigation states within their blue selection palette.
            widgets.hovered.weak_bg_fill = if selected {
                scale::selected_hover()
            } else {
                scale::hover()
            };
            widgets.active.weak_bg_fill = if selected {
                scale::selected_pressed()
            } else {
                scale::pressed()
            };
            ui.add_sized(
                [ui.available_width(), scale::NAV_H],
                egui::Button::new("")
                    .stroke(egui::Stroke::NONE)
                    .rounding(egui::Rounding::same(scale::RADIUS_CONTROL)),
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
    let icon = icons::text(ui.ctx(), glyph(page), scale::HEADING + 3.0).color(ink);
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
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &label,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(scale::HEADING),
            color: ink,
            ..Default::default()
        },
    );
    let text = ui.fonts(|f| f.layout_job(job));
    ui.painter().galley(
        egui::pos2(
            response.rect.left() + 40.0,
            response.rect.center().y - text.size().y / 2.0,
        ),
        text,
        ink,
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.shrink(1.0),
            egui::Rounding::same(scale::RADIUS_CONTROL),
            egui::Stroke::new(2.0_f32, scale::accent()),
        );
    }
    if response.clicked() {
        *current = page;
    }
}

pub(crate) fn sidebar_width(width: f32) -> f32 {
    if width < 1000.0 {
        scale::SIDEBAR_W_NARROW
    } else {
        scale::SIDEBAR_W
    }
}

pub(crate) fn brand(ui: &mut Ui, language: Language) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.label(
            RichText::new("DJI 4G")
                .size(scale::HEADING)
                .strong()
                .color(scale::ink()),
        );
        ui.label(
            RichText::new(LocalizedText::new(language, TextKey::NavGroupModule).text)
                .size(scale::META)
                .color(scale::faint()),
        );
    });
}
