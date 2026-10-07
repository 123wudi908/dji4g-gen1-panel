//! Material-inspired controls shared by every page. Business commands remain at call sites.
use super::scale;
use eframe::egui::{self, Color32, Response, RichText, Stroke, Ui};

/// Paint visible glyphs at the button center, independent of the surrounding Grid/layout.
/// The empty stock button keeps pointer/keyboard behavior, disabled state and focus semantics.
/// `button` supplies the frame and minimum size; its label must be empty.
pub(crate) fn centered_button(
    ui: &mut Ui,
    text: impl Into<egui::WidgetText>,
    button: egui::Button<'_>,
    minimum: egui::Vec2,
) -> Response {
    let galley = text.into().into_galley(
        ui,
        Some(egui::TextWrapMode::Wrap),
        (ui.available_width() - 2.0 * ui.spacing().button_padding.x).max(1.0),
        egui::TextStyle::Button,
    );
    let size = (galley.size() + 2.0 * ui.spacing().button_padding).max(minimum);
    let response = ui.add(button.min_size(size));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), galley.text())
    });
    if ui.is_rect_visible(response.rect) {
        let bounds = if galley.mesh_bounds.is_positive() {
            galley.mesh_bounds
        } else {
            galley.rect
        };
        let position = response.rect.center() - bounds.center().to_vec2();
        ui.painter().galley(
            position,
            galley,
            ui.style().interact(&response).text_color(),
        );
    }
    response
}

pub(crate) fn plain_button(ui: &mut Ui, text: impl Into<egui::WidgetText>) -> Response {
    centered_button(
        ui,
        text,
        egui::Button::new(""),
        egui::vec2(scale::BUTTON_MIN_W, scale::CONTROL_H),
    )
}

#[derive(Clone, Copy)]
pub(crate) enum ButtonKind {
    Filled,
    Tonal,
    Outlined,
    Text,
    Destructive,
}

pub(crate) fn action_button(
    ui: &mut Ui,
    label: &str,
    kind: ButtonKind,
    enabled: bool,
    disabled_reason: Option<&str>,
) -> Response {
    let enabled = enabled && ui.is_enabled();
    // Every button in the panel resolves to one of these fills, and every one of them is exactly
    // `CONTROL_H` tall. That is what keeps a page from growing a row of mismatched buttons.
    let (fill, ink, stroke) = match kind {
        ButtonKind::Filled => (scale::accent(), scale::on_accent(), Stroke::NONE),
        ButtonKind::Tonal => (scale::surface_raised(), scale::ink(), Stroke::NONE),
        ButtonKind::Outlined => (
            Color32::TRANSPARENT,
            scale::ink(),
            Stroke::new(1.0_f32, scale::border()),
        ),
        ButtonKind::Text => (Color32::TRANSPARENT, scale::secondary(), Stroke::NONE),
        // The one place red is allowed outside a status message: a destructive action.
        ButtonKind::Destructive => (
            Color32::TRANSPARENT,
            scale::danger(),
            Stroke::new(1.0_f32, scale::danger()),
        ),
    };
    let (fill, ink, stroke) = if enabled {
        (fill, ink, stroke)
    } else {
        (
            if matches!(kind, ButtonKind::Filled | ButtonKind::Tonal) {
                scale::disabled_fill()
            } else {
                Color32::TRANSPARENT
            },
            scale::disabled_ink(),
            if stroke == Stroke::NONE {
                Stroke::NONE
            } else {
                Stroke::new(1.0_f32, scale::line())
            },
        )
    };
    let (hover_fill, pressed_fill) = if enabled {
        match kind {
            // A filled button carries `accent()` and its action ink, so its states step within
            // that fill; the neutral greys would put the label on a light background on hover.
            ButtonKind::Filled => (scale::accent_hover(), scale::accent_pressed()),
            ButtonKind::Tonal | ButtonKind::Outlined | ButtonKind::Text => {
                (scale::hover(), scale::pressed())
            }
            ButtonKind::Destructive => (scale::danger_fill(), scale::danger_fill()),
        }
    } else {
        (fill, fill)
    };
    let response = ui
        .scope(|ui| {
            let widgets = &mut ui.visuals_mut().widgets;
            widgets.noninteractive.weak_bg_fill = fill;
            widgets.inactive.weak_bg_fill = fill;
            widgets.hovered.weak_bg_fill = hover_fill;
            widgets.active.weak_bg_fill = pressed_fill;
            ui.add_enabled_ui(enabled, |ui| {
                centered_button(
                    ui,
                    RichText::new(label).size(scale::BODY).color(ink),
                    egui::Button::new("")
                        .stroke(stroke)
                        .rounding(egui::Rounding::same(scale::RADIUS_CONTROL))
                        .min_size(egui::vec2(scale::BUTTON_MIN_W, scale::CONTROL_H)),
                    egui::vec2(scale::BUTTON_MIN_W, scale::CONTROL_H),
                )
            })
            .inner
        })
        .inner;
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.shrink(1.0),
            egui::Rounding::same(scale::RADIUS_CONTROL),
            Stroke::new(2.0_f32, scale::accent()),
        );
    }
    match disabled_reason.filter(|_| !enabled) {
        Some(reason) => response.on_disabled_hover_text(reason),
        None => response,
    }
}

pub(crate) struct TabItem<T> {
    pub value: T,
    pub label: String,
}
impl<T> TabItem<T> {
    pub(crate) fn new(value: T, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
        }
    }
}

pub(crate) fn page_tabs<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: egui::Id,
    selected: &mut T,
    items: &[TabItem<T>],
) -> bool {
    selection(ui, id, selected, items, false)
}
pub(crate) fn segmented_control<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: egui::Id,
    selected: &mut T,
    items: &[TabItem<T>],
) -> bool {
    selection(ui, id, selected, items, true)
}
fn selection<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: egui::Id,
    selected: &mut T,
    items: &[TabItem<T>],
    segmented: bool,
) -> bool {
    let mut changed = false;
    ui.push_id(id, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = if segmented { 0.0 } else { 10.0 };
            for (index, item) in items.iter().enumerate() {
                ui.push_id(index, |ui| {
                    let active = *selected == item.value;
                    // A segmented control has one border and a soft blue selected segment.
                    let fill = if active && segmented {
                        scale::selected()
                    } else {
                        Color32::TRANSPARENT
                    };
                    let label_text = RichText::new(&item.label).size(scale::HEADING).color(
                        if active && segmented {
                            scale::on_selected()
                        } else if active {
                            scale::ink()
                        } else {
                            scale::secondary()
                        },
                    );
                    let label_text = if active {
                        label_text.strong()
                    } else {
                        label_text
                    };
                    let button = egui::Button::new(label_text)
                        .stroke(if segmented {
                            Stroke::new(1.0_f32, scale::border())
                        } else {
                            Stroke::NONE
                        })
                        .rounding(if segmented {
                            egui::Rounding {
                                nw: if index == 0 {
                                    scale::RADIUS_CONTROL
                                } else {
                                    0.0
                                },
                                sw: if index == 0 {
                                    scale::RADIUS_CONTROL
                                } else {
                                    0.0
                                },
                                ne: if index + 1 == items.len() {
                                    scale::RADIUS_CONTROL
                                } else {
                                    0.0
                                },
                                se: if index + 1 == items.len() {
                                    scale::RADIUS_CONTROL
                                } else {
                                    0.0
                                },
                            }
                        } else {
                            egui::Rounding::same(scale::RADIUS_CONTROL)
                        })
                        .min_size(egui::vec2(scale::SEGMENT_MIN_W, scale::CONTROL_H));
                    let response = ui
                        .scope(|ui| {
                            let widgets = &mut ui.visuals_mut().widgets;
                            widgets.noninteractive.weak_bg_fill = fill;
                            widgets.inactive.weak_bg_fill = fill;
                            // Selected segments keep their own hover and pressed colors.
                            widgets.hovered.weak_bg_fill = if active && segmented {
                                scale::selected_hover()
                            } else {
                                scale::hover()
                            };
                            widgets.active.weak_bg_fill = if active && segmented {
                                scale::selected_pressed()
                            } else {
                                scale::pressed()
                            };
                            ui.add(button)
                        })
                        .inner;
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::SelectableLabel,
                            ui.is_enabled(),
                            active,
                            &item.label,
                        )
                    });
                    // An underline tab marks the current page; a segmented block marks itself.
                    if active && !segmented {
                        ui.painter().line_segment(
                            [
                                response.rect.left_bottom() + egui::vec2(8.0, -1.0),
                                response.rect.right_bottom() + egui::vec2(-8.0, -1.0),
                            ],
                            Stroke::new(2.0_f32, scale::accent()),
                        );
                    }
                    if response.has_focus() {
                        ui.painter().rect_stroke(
                            response.rect.shrink(1.0),
                            egui::Rounding::same(scale::RADIUS_CONTROL),
                            Stroke::new(2.0_f32, scale::accent()),
                        );
                    }
                    if response.clicked() && !active {
                        *selected = item.value;
                        changed = true;
                    }
                });
            }
        });
    });
    changed
}

/// A content-measured footer: the explanation cannot be covered by a fixed-height action row.
pub(crate) fn entry_footer(
    ui: &mut Ui,
    label: &str,
    language: crate::localization::Language,
) -> Response {
    let explanation = |ui: &mut Ui| {
        ui.label(
            RichText::new(
                crate::localization::LocalizedText::new(
                    language,
                    crate::localization::TextKey::EntrySkipHint,
                )
                .text,
            )
            .color(scale::secondary()),
        );
        ui.label(
            RichText::new(
                crate::localization::LocalizedText::new(
                    language,
                    crate::localization::TextKey::EntryHiddenHint,
                )
                .text,
            )
            .size(scale::META)
            .color(scale::secondary()),
        );
    };
    if ui.available_width() >= 520.0 {
        let width = ui.available_width();
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2((width - 156.0).max(240.0), 36.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_min_width((width - 156.0).max(240.0));
                    explanation(ui);
                },
            );
            ui.add(super::theme::primary_button(label))
        })
        .inner
    } else {
        explanation(ui);
        ui.add(super::theme::primary_button(label))
    }
}

/// Compact desktop switch with a 44x32 interaction target and native egui keyboard semantics.
pub(crate) fn switch(ui: &mut Ui, value: &mut bool, label: &str, enabled: bool) -> Response {
    let enabled = enabled && ui.is_enabled();
    let mut response = ui.add_enabled(
        enabled,
        egui::Button::new("")
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::NONE)
            .min_size(egui::vec2(40.0, scale::CONTROL_H)),
    );
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, enabled, *value, label)
    });
    let rect = egui::Rect::from_center_size(response.rect.center(), egui::vec2(36.0, 20.0));
    let track = if !enabled {
        scale::disabled_fill()
    } else if *value {
        // An enabled switch uses the primary action color.
        scale::accent()
    } else {
        scale::surface_raised()
    };
    ui.painter().rect(
        rect,
        10.0,
        track,
        if *value {
            Stroke::NONE
        } else {
            Stroke::new(
                1.0_f32,
                scale::border().gamma_multiply(if enabled { 1.0 } else { 0.4 }),
            )
        },
    );
    let position = ui
        .ctx()
        .animate_bool(response.id.with("switch-position"), *value);
    let x = egui::lerp((rect.left() + 10.0)..=(rect.right() - 10.0), position);
    ui.painter().circle_filled(
        egui::pos2(x, rect.center().y),
        7.0,
        if !enabled {
            scale::faint().gamma_multiply(0.5)
        } else if *value {
            scale::on_accent()
        } else {
            scale::secondary()
        },
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.shrink(1.0),
            egui::Rounding::same(scale::RADIUS_CONTROL),
            Stroke::new(2.0_f32, scale::accent()),
        );
    }
    response.on_hover_text(label)
}

/// The single page title: one 15pt line, optionally followed by a quiet description.
pub(crate) fn page_heading(ui: &mut Ui, title: &str, description: &str) {
    ui.label(
        RichText::new(title)
            .size(scale::TITLE)
            .strong()
            .color(scale::ink()),
    );
    if !description.is_empty() {
        ui.label(
            RichText::new(description)
                .size(scale::META)
                .color(scale::faint()),
        );
    }
    ui.add_space(2.0);
}

/// A restrained indicator that keeps loading rows aligned with body text.
pub(crate) fn loading_spinner(ui: &mut Ui) -> Response {
    ui.add(
        egui::Spinner::new()
            .size(scale::BODY)
            .color(scale::secondary()),
    )
}

/// A quiet underlined link with the same accent used by focus and tab indicators.
pub(crate) fn link(ui: &mut Ui, label: &str, url: &str) -> Response {
    ui.add(egui::Hyperlink::from_label_and_url(
        RichText::new(label)
            .size(scale::META)
            .underline()
            .color(scale::accent()),
        url,
    ))
}

/// A compact status line. Green marks correct/available, red marks wrong/failed, and the
/// in-between tones stay on the plain sunken surface so they never compete with a real verdict.
/// The verdict and its reason share one wrapped line, which keeps the banner to a single row.
pub(crate) fn status_banner(ui: &mut Ui, title: &str, detail: &str, tone: super::StatusTone) {
    let fill = tone.fill().unwrap_or_else(scale::surface_sunken);
    egui::Frame::none()
        .fill(fill)
        .rounding(egui::Rounding::same(scale::RADIUS_CONTROL))
        .inner_margin(egui::Margin::symmetric(scale::BLOCK_PAD, 6.0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                let glyph = match tone {
                    super::StatusTone::Positive => "\u{f0be}",
                    super::StatusTone::Negative => "\u{f8b6}",
                    _ => "\u{e88e}",
                };
                ui.label(super::icons::text(ui.ctx(), glyph, scale::BODY).color(tone.color()));
                ui.label(
                    RichText::new(title)
                        .size(scale::BODY)
                        .strong()
                        .color(tone.color()),
                );
                ui.label(
                    RichText::new(detail)
                        .size(scale::META)
                        .color(scale::secondary()),
                );
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn button_glyphs_are_centered_in_top_aligned_rows_and_vertical_cells() {
        for label in ["复制地址", "顯示", "Copy address"] {
            for horizontal in [false, true] {
                let ctx = egui::Context::default();
                super::super::apply_style(&ctx);
                crate::font::install_chinese_font(&ctx).expect("Windows CJK font");
                let mut button_rect = egui::Rect::NOTHING;
                let output = ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let layout = if horizontal {
                            egui::Layout::left_to_right(egui::Align::Min)
                        } else {
                            egui::Layout::top_down(egui::Align::Min)
                        };
                        ui.with_layout(layout, |ui| {
                            button_rect = centered_button(
                                ui,
                                label,
                                egui::Button::new("").min_size(egui::vec2(110.0, scale::CONTROL_H)),
                                egui::vec2(110.0, scale::CONTROL_H),
                            )
                            .rect;
                        });
                    });
                });
                let text = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == label => Some(text),
                        _ => None,
                    })
                    .expect("visible button text");
                let glyph_rect = text.galley.mesh_bounds.translate(text.pos.to_vec2());
                let delta = glyph_rect.center() - button_rect.center();
                assert!(
                    delta.x.abs() <= 0.6 && delta.y.abs() <= 0.6,
                    "{label} horizontal={horizontal}: glyph offset {delta:?}"
                );
                assert!(button_rect.height() <= scale::CONTROL_H + 1.0);
            }
        }
    }

    #[test]
    fn tabs_switch_only_after_a_click_and_disabled_actions_do_not_fire() {
        let ctx = egui::Context::default();
        super::super::apply_style(&ctx);
        let mut selected = false;
        let mut fired = 0;
        let mut transitions = 0;
        // Drive the click at the tab's own rectangle instead of a hardcoded point: the control
        // height and the label widths are tokens, so a fixed coordinate silently stops hitting the
        // widget the moment the design changes.
        let mut tab_rect = egui::Rect::NOTHING;
        for tick in 0..4 {
            let events = if tick == 2 {
                let centre = tab_rect.center();
                vec![
                    egui::Event::PointerMoved(centre),
                    egui::Event::PointerButton {
                        pos: centre,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            } else if tick == 3 {
                let centre = tab_rect.center();
                vec![
                    egui::Event::PointerMoved(centre),
                    egui::Event::PointerButton {
                        pos: centre,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            } else {
                Vec::new()
            };
            let _ = ctx.run(
                egui::RawInput {
                    events,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(500.0, 300.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let before = selected;
                        if page_tabs(
                            ui,
                            egui::Id::new("click-tabs"),
                            &mut selected,
                            &[
                                TabItem::new(false, "连接概况"),
                                TabItem::new(true, "无线观测"),
                            ],
                        ) {
                            transitions += 1;
                        }
                        let _ = before;
                        // Remember where the first tab landed so the next pass can click it.
                        tab_rect = ui
                            .ctx()
                            .read_response(egui::Id::new("click-tabs").with(0))
                            .map_or(tab_rect, |response| response.rect);
                        if action_button(
                            ui,
                            "不可执行",
                            ButtonKind::Filled,
                            false,
                            Some("通信忙碌"),
                        )
                        .clicked()
                        {
                            fired += 1;
                        }
                    });
                },
            );
            if tick < 2 {
                assert!(!selected);
            }
        }
        assert!(selected);
        assert_eq!(transitions, 1);
        assert_eq!(fired, 0);
    }

    #[test]
    fn keyboard_activation_emits_one_click_and_keeps_focus() {
        for primary in [false, true] {
            let ctx = egui::Context::default();
            super::super::apply_style(&ctx);
            let mut id = None;
            let mut clicks = 0;
            for tick in 0..3 {
                if let Some(id) = id {
                    ctx.memory_mut(|m| m.request_focus(id));
                }
                let events = if tick == 0 {
                    vec![]
                } else {
                    vec![egui::Event::Key {
                        key: egui::Key::Enter,
                        physical_key: None,
                        pressed: tick == 1,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }]
                };
                let _ = ctx.run(
                    egui::RawInput {
                        events,
                        focused: true,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let response = if primary {
                                ui.add(super::super::theme::primary_button("检查"))
                            } else {
                                action_button(ui, "检查", ButtonKind::Filled, true, None)
                            };
                            id = Some(response.id);
                            if response.clicked() {
                                clicks += 1;
                            }
                            if tick > 0 {
                                assert!(response.has_focus());
                            }
                        });
                    },
                );
            }
            assert_eq!(clicks, 1);
        }
    }

    #[test]
    fn disabled_primary_does_not_activate_from_keyboard() {
        let ctx = egui::Context::default();
        super::super::apply_style(&ctx);
        let mut id = None;
        for tick in 0..3 {
            if let Some(id) = id {
                ctx.memory_mut(|memory| memory.request_focus(id));
            }
            let events = if tick == 0 {
                Vec::new()
            } else {
                vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: tick == 1,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }]
            };
            let _ = ctx.run(
                egui::RawInput {
                    events,
                    focused: true,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let response =
                            ui.add_enabled(false, super::super::theme::primary_button("检查"));
                        id = Some(response.id);
                        assert!(!response.enabled());
                        assert!(!response.clicked());
                    });
                },
            );
        }
    }

    #[test]
    fn footer_and_navigation_fit_small_content_widths() {
        for width in [260.0, 480.0, 800.0] {
            let ctx = egui::Context::default();
            super::super::apply_style(&ctx);
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 400.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let response =
                            entry_footer(ui, "进入面板", crate::localization::Language::ZhCn);
                        assert!(
                            response.rect.height() >= scale::CONTROL_H,
                            "the entry action must be a full-height control"
                        );
                        assert!(ui.clip_rect().contains_rect(response.rect));
                        let mut choice = false;
                        assert!(!page_tabs(
                            ui,
                            egui::Id::new("test"),
                            &mut choice,
                            &[
                                TabItem::new(false, "连接概况"),
                                TabItem::new(true, "无线观测")
                            ]
                        ));
                        assert!(!choice);
                    });
                },
            );
        }
    }
}
