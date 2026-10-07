//! The panel's single source of truth for colour, size, spacing and typography.
//!
//! Everything the eye sees is decided here and nowhere else. Pages never invent a colour, a font
//! size or a control height: they call a [`scale`] getter or a shared control in
//! [`super::components`]. That is what keeps every button on every page exactly the same size.
//!
//! Light gray surfaces and soft blue selections keep the hierarchy quiet. Primary actions have
//! their own stronger blue; success, error and chart series retain their semantic colors.

use eframe::egui::{self, Color32, FontId, TextStyle};

/// One catalog string in the language this page was rendered with.
fn t(language: crate::localization::Language, key: crate::localization::TextKey) -> String {
    crate::localization::LocalizedText::new(language, key).text
}

/// The active ramp, as one process-wide switch at runtime.
///
/// Under `cfg(test)` it is a thread-local instead: the whole suite shares one process, and a test
/// that pins a theme would otherwise rewrite the ramp under a test running beside it. Nothing about
/// the shipped build changes — the tests just stop racing each other over it.
mod dark_flag {
    #[cfg(not(test))]
    use std::sync::atomic::{AtomicBool, Ordering};

    #[cfg(test)]
    use std::cell::Cell;

    #[cfg(not(test))]
    static DARK: AtomicBool = AtomicBool::new(false);

    #[cfg(test)]
    thread_local! {
        static DARK: Cell<bool> = const { Cell::new(false) };
    }

    pub(super) fn set(dark: bool) {
        #[cfg(not(test))]
        DARK.store(dark, Ordering::Relaxed);
        #[cfg(test)]
        DARK.set(dark);
    }

    pub(super) fn get() -> bool {
        #[cfg(not(test))]
        return DARK.load(Ordering::Relaxed);
        #[cfg(test)]
        return DARK.get();
    }
}

#[derive(Clone, Copy)]
struct Palette {
    surface: Color32,
    surface_alt: Color32,
    surface_page: Color32,
    surface_sunken: Color32,
    surface_raised: Color32,
    ink: Color32,
    secondary: Color32,
    faint: Color32,
    disabled_ink: Color32,
    line: Color32,
    border: Color32,
    selected: Color32,
    on_selected: Color32,
    selected_hover: Color32,
    selected_pressed: Color32,
    accent: Color32,
    on_accent: Color32,
    accent_hover: Color32,
    accent_pressed: Color32,
    hover: Color32,
    pressed: Color32,
    disabled_fill: Color32,
    success: Color32,
    danger: Color32,
    success_fill: Color32,
    danger_fill: Color32,
}

const fn rgb(value: u32) -> Color32 {
    Color32::from_rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            surface: rgb(0x253140),
            surface_alt: rgb(0x202A37),
            surface_page: rgb(0x1B2430),
            surface_sunken: rgb(0x2C3A4C),
            surface_raised: rgb(0x34465A),
            ink: rgb(0xE5ECF5),
            secondary: rgb(0xB8C6D8),
            faint: rgb(0xA2B2C7),
            disabled_ink: rgb(0x7D90A8),
            line: rgb(0x384B61),
            border: rgb(0x526780),
            selected: rgb(0x2B4666),
            on_selected: rgb(0xD5E6FA),
            selected_hover: rgb(0x314E71),
            selected_pressed: rgb(0x37577D),
            accent: rgb(0x78A9DE),
            on_accent: rgb(0x142438),
            accent_hover: rgb(0x8DB9E8),
            accent_pressed: rgb(0xA1C7F1),
            hover: rgb(0x2D3E52),
            pressed: rgb(0x354B64),
            disabled_fill: rgb(0x253140),
            success: rgb(0x5FC48A),
            danger: rgb(0xF27B72),
            success_fill: rgb(0x1B2F25),
            danger_fill: rgb(0x331F1E),
        }
    } else {
        Palette {
            surface: rgb(0xFFFFFF),
            surface_alt: rgb(0xF0F3F7),
            surface_page: rgb(0xF4F6F9),
            surface_sunken: rgb(0xF0F3F7),
            surface_raised: rgb(0xE8EDF4),
            ink: rgb(0x243247),
            secondary: rgb(0x526174),
            faint: rgb(0x64748B),
            disabled_ink: rgb(0x8997A8),
            line: rgb(0xDFE5ED),
            border: rgb(0xCAD3DF),
            selected: rgb(0xE5EEFA),
            on_selected: rgb(0x295B94),
            selected_hover: rgb(0xD8E6F8),
            selected_pressed: rgb(0xCADDF4),
            accent: rgb(0x3A6FA8),
            on_accent: rgb(0xFFFFFF),
            accent_hover: rgb(0x326297),
            accent_pressed: rgb(0x2B5685),
            hover: rgb(0xE8EEF6),
            pressed: rgb(0xDBE5F1),
            disabled_fill: rgb(0xE9EDF3),
            success: rgb(0x1D6F42),
            danger: rgb(0xB3261E),
            success_fill: rgb(0xEDF7F1),
            danger_fill: rgb(0xFCEFEE),
        }
    }
}

/// Colour, typography and geometry tokens.
///
/// Control sizes and the fixed layout arrays stay `const`; colours and font sizes are functions
/// because the active theme decides their value at runtime.
pub(crate) mod scale {
    use eframe::egui::Color32;

    pub(crate) fn set_dark(dark: bool) {
        super::dark_flag::set(dark);
    }

    #[must_use]
    pub(crate) fn dark() -> bool {
        super::dark_flag::get()
    }

    pub(crate) fn surface() -> Color32 {
        super::palette(dark()).surface
    }
    pub(crate) fn surface_alt() -> Color32 {
        super::palette(dark()).surface_alt
    }
    pub(crate) fn surface_page() -> Color32 {
        super::palette(dark()).surface_page
    }
    pub(crate) fn surface_sunken() -> Color32 {
        super::palette(dark()).surface_sunken
    }
    pub(crate) fn surface_raised() -> Color32 {
        super::palette(dark()).surface_raised
    }
    pub(crate) fn ink() -> Color32 {
        super::palette(dark()).ink
    }
    pub(crate) fn secondary() -> Color32 {
        super::palette(dark()).secondary
    }
    pub(crate) fn faint() -> Color32 {
        super::palette(dark()).faint
    }
    pub(crate) fn disabled_ink() -> Color32 {
        super::palette(dark()).disabled_ink
    }
    pub(crate) fn line() -> Color32 {
        super::palette(dark()).line
    }
    pub(crate) fn border() -> Color32 {
        super::palette(dark()).border
    }
    pub(crate) fn selected() -> Color32 {
        super::palette(dark()).selected
    }
    pub(crate) fn on_selected() -> Color32 {
        super::palette(dark()).on_selected
    }
    pub(crate) fn selected_hover() -> Color32 {
        super::palette(dark()).selected_hover
    }
    pub(crate) fn selected_pressed() -> Color32 {
        super::palette(dark()).selected_pressed
    }
    pub(crate) fn accent() -> Color32 {
        super::palette(dark()).accent
    }
    pub(crate) fn on_accent() -> Color32 {
        super::palette(dark()).on_accent
    }
    pub(crate) fn accent_hover() -> Color32 {
        super::palette(dark()).accent_hover
    }
    pub(crate) fn accent_pressed() -> Color32 {
        super::palette(dark()).accent_pressed
    }
    pub(crate) fn hover() -> Color32 {
        super::palette(dark()).hover
    }
    pub(crate) fn pressed() -> Color32 {
        super::palette(dark()).pressed
    }
    pub(crate) fn disabled_fill() -> Color32 {
        super::palette(dark()).disabled_fill
    }
    pub(crate) fn success() -> Color32 {
        super::palette(dark()).success
    }
    pub(crate) fn danger() -> Color32 {
        super::palette(dark()).danger
    }
    pub(crate) fn success_fill() -> Color32 {
        super::palette(dark()).success_fill
    }
    pub(crate) fn danger_fill() -> Color32 {
        super::palette(dark()).danger_fill
    }

    pub(crate) const DOWNLOAD: Color32 = Color32::from_rgb(0x0b, 0x57, 0xd0);
    pub(crate) const UPLOAD: Color32 = Color32::from_rgb(0xb2, 0x67, 0x00);

    // -------------------------------------------------------------- typography
    /// Page title. Exactly one per page.
    pub(crate) const TITLE: f32 = 18.0;
    /// Card and section titles.
    pub(crate) const HEADING: f32 = 15.0;
    /// Body text, labels and measured values.
    pub(crate) const BODY: f32 = 14.0;
    /// Metadata, hints, timestamps. Still readable prose, not a caption tier: much of the panel's
    /// explanatory text lives here, so it may not drop to a size that has to be squinted at.
    pub(crate) const META: f32 = 12.0;
    /// The two large rate readouts and their axis notes.
    pub(crate) const RATE_NUMBER: f32 = 22.0;
    pub(crate) const RATE_AUX: f32 = 11.0;

    // --------------------------------------------------------------- geometry
    /// Every button, tab, input and combo box in the panel is exactly this tall.
    pub(crate) const CONTROL_H: f32 = 28.0;
    /// Sidebar menu rows: a comfortable click target rather than a dense list row, because this
    /// menu is the panel's primary navigation.
    pub(crate) const NAV_H: f32 = 40.0;
    /// Sidebar width, and one step narrower below 1000pt. Wide enough for a full Chinese status
    /// sentence to wrap onto two lines instead of being cut off.
    pub(crate) const SIDEBAR_W: f32 = 196.0;
    pub(crate) const SIDEBAR_W_NARROW: f32 = 172.0;
    /// Content column cap.
    pub(crate) const CONTENT_MAX_W: f32 = 1200.0;
    /// Smallest button width so single-word actions do not collapse.
    pub(crate) const BUTTON_MIN_W: f32 = 64.0;
    /// Minimum width of one segment in a segmented control.
    pub(crate) const SEGMENT_MIN_W: f32 = 72.0;
    /// Body text column in the overview's key/value pairs.
    pub(crate) const LABEL_COLUMN: f32 = 76.0;

    // ------------------------------------------------------------------ radii
    /// Buttons, tabs, inputs, badges.
    pub(crate) const RADIUS_CONTROL: f32 = 6.0;
    /// Cards, banners, menus, the content sheet.
    pub(crate) const RADIUS_CONTAINER: f32 = 8.0;

    // ---------------------------------------------------------------- spacing
    // One 2pt grid. Nothing in the panel may introduce a spacing value that is not on this list,
    // which is what keeps the rhythm identical from page to page.
    //
    //   inner padding, inline gaps ......... 8
    //   row padding, label/value gaps ...... 10
    //   card and section padding ........... 12
    //   block-to-block separation .......... 16
    /// Padding inside a button, left and right.
    pub(crate) const BUTTON_PAD_X: f32 = 12.0;
    /// Padding inside a card.
    pub(crate) const CARD_PAD: f32 = 12.0;
    /// Padding inside a bordered block or a status surface.
    pub(crate) const BLOCK_PAD: f32 = 10.0;
    /// Gap between the label column and the value in an info grid.
    pub(crate) const COLUMN_GAP: f32 = 10.0;
    /// Vertical gap between two rows of the same list.
    pub(crate) const ROW_GAP: f32 = 4.0;
    /// Horizontal and vertical gap between inline controls (buttons in a row, a switch and text).
    pub(crate) const CONTROL_GAP: [f32; 2] = [8.0, 6.0];
    /// Vertical gap between two stacked controls, and between the lines of a card's prose. Wide
    /// enough that a paragraph-heavy card does not read as a wall of text.
    pub(crate) const SECTION_GAP: f32 = 10.0;
    /// Vertical gap between two cards.
    pub(crate) const BLOCK_GAP: f32 = 12.0;
    /// Separation between the page title and the first card.
    pub(crate) const PAGE_GAP: f32 = 16.0;
    /// Legacy alias kept so the shared section frame reads in layout terms.
    pub(crate) const SECTION_MARGIN: [f32; 2] = [CARD_PAD, CARD_PAD];
}

/// Apply the currently stored theme choice. This is what every call site uses, so a page never
/// has to thread the preference through its own arguments.
pub(crate) fn apply_style(ctx: &egui::Context) {
    style_root(ctx, stored_theme(ctx));
}

/// Read the stored theme choice, falling back to following the system.
#[must_use]
pub(crate) fn stored_theme(ctx: &egui::Context) -> egui::ThemePreference {
    ctx.data_mut(|data| {
        data.get_temp::<egui::ThemePreference>(egui::Id::new(THEME_CHOICE_KEY))
            .unwrap_or(egui::ThemePreference::System)
    })
}

/// Store a new theme choice. The next [`apply_style`] applies it.
pub(crate) fn store_theme(ctx: &egui::Context, preference: egui::ThemePreference) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new(THEME_CHOICE_KEY), preference));
}

pub(crate) fn theme_preference(theme: dji4g_application::ThemeCode) -> egui::ThemePreference {
    match theme {
        dji4g_application::ThemeCode::System => egui::ThemePreference::System,
        dji4g_application::ThemeCode::Light => egui::ThemePreference::Light,
        dji4g_application::ThemeCode::Dark => egui::ThemePreference::Dark,
    }
}

pub(crate) fn theme_code(theme: egui::ThemePreference) -> dji4g_application::ThemeCode {
    match theme {
        egui::ThemePreference::System => dji4g_application::ThemeCode::System,
        egui::ThemePreference::Light => dji4g_application::ThemeCode::Light,
        egui::ThemePreference::Dark => dji4g_application::ThemeCode::Dark,
    }
}

pub(crate) fn apply_settings_theme(ctx: &egui::Context, theme: dji4g_application::ThemeCode) {
    let preference = theme_preference(theme);
    #[cfg(debug_assertions)]
    let preference = ctx
        .data(|data| data.get_temp::<egui::ThemePreference>(egui::Id::new("review-theme-override")))
        .unwrap_or(preference);
    store_theme(ctx, preference);
    apply_style(ctx);
}

const THEME_CHOICE_KEY: &str = "panel-theme-choice";

/// The theme choices offered in the settings page, in display order.
pub(crate) const THEME_CHOICES: [egui::ThemePreference; 3] = [
    egui::ThemePreference::System,
    egui::ThemePreference::Light,
    egui::ThemePreference::Dark,
];

/// Label for a theme choice.
#[must_use]
pub(crate) fn theme_label(
    preference: egui::ThemePreference,
    language: crate::localization::Language,
) -> String {
    match preference {
        egui::ThemePreference::System => t(language, crate::localization::TextKey::ThemeSystem),
        egui::ThemePreference::Light => t(language, crate::localization::TextKey::ThemeLight),
        egui::ThemePreference::Dark => t(language, crate::localization::TextKey::ThemeDark),
    }
}

/// Style the context for a theme, then keep both theme slots consistent.
///
/// `preference` chooses the active ramp: `System` follows the operating system, and `Light` or
/// `Dark` pin it. Both slots receive a complete style, so an OS theme change cannot restore egui's
/// stock fonts or unrelated accent colors.
pub(crate) fn style_root(ctx: &egui::Context, preference: egui::ThemePreference) {
    ctx.set_theme(preference);
    let active_dark = match preference {
        egui::ThemePreference::Dark => true,
        egui::ThemePreference::Light => false,
        egui::ThemePreference::System => ctx.style().visuals.dark_mode,
    };
    scale::set_dark(active_dark);
    // Both slots get a complete style so an OS theme change can never restore stock egui.
    ctx.set_style_of(egui::Theme::Light, themed_style(false));
    ctx.set_style_of(egui::Theme::Dark, themed_style(true));
}

/// Build a complete style for one ramp. Used for both slots so neither can fall back to stock egui.
fn themed_style(dark: bool) -> egui::Style {
    // Start from egui's own defaults, then replace every visible decision with a token.
    let mut style = egui::Style {
        visuals: if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        },
        ..Default::default()
    };
    apply_visuals(&mut style.visuals, dark);
    style.animation_time = 0.1;
    style.spacing.item_spacing = egui::vec2(scale::CONTROL_GAP[0], scale::SECTION_GAP);
    style.spacing.button_padding = egui::vec2(scale::BUTTON_PAD_X, 0.0);
    style.spacing.window_margin = egui::Margin::same(scale::BLOCK_PAD);
    style.spacing.interact_size = egui::vec2(scale::CONTROL_H, scale::CONTROL_H);
    style.spacing.menu_margin = egui::Margin::same(6.0);
    style.spacing.combo_width = 96.0;
    // Solid scrollbars with a real gutter. egui's default bar floats *over* the content it
    // scrolls, which covered the right edge of drop-down lists and menu rows; a solid bar takes
    // its own space instead, and the small outer margin keeps it just clear of the content.
    let mut scroll = egui::style::ScrollStyle::solid();
    scroll.bar_width = 12.0;
    scroll.bar_outer_margin = 2.0;
    style.spacing.scroll = scroll;
    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(scale::TITLE)),
        (TextStyle::Body, FontId::proportional(scale::BODY)),
        (TextStyle::Button, FontId::proportional(scale::BODY)),
        (TextStyle::Monospace, FontId::monospace(scale::BODY)),
        (TextStyle::Small, FontId::proportional(scale::META)),
    ]
    .into();
    style
}

/// Drive every widget colour from the ramp, so no widget can keep a stock accent.
fn apply_visuals(visuals: &mut egui::Visuals, dark: bool) {
    let colors = palette(dark);
    let surface = colors.surface;
    let alt = colors.surface_alt;
    let sunken = colors.surface_sunken;
    let raised = colors.surface_raised;
    let ink = colors.ink;
    let secondary = colors.secondary;
    let line = colors.line;
    let border = colors.border;
    let hover = colors.hover;
    let pressed = colors.pressed;
    visuals.dark_mode = dark;
    visuals.panel_fill = alt;
    visuals.window_fill = surface;
    visuals.faint_bg_color = sunken;
    visuals.extreme_bg_color = sunken;
    visuals.code_bg_color = sunken;
    visuals.selection.bg_fill = colors.selected;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, colors.on_selected);
    visuals.hyperlink_color = colors.accent;
    visuals.window_rounding = egui::Rounding::same(scale::RADIUS_CONTAINER);
    // A hairline keeps the sheet separated from the menu bar without a shadow.
    visuals.window_stroke = egui::Stroke::new(1.0_f32, line);
    visuals.window_shadow = egui::epaint::Shadow::NONE;
    visuals.popup_shadow = egui::epaint::Shadow::NONE;
    visuals.menu_rounding = egui::Rounding::same(scale::RADIUS_CONTAINER);

    let rounding = egui::Rounding::same(scale::RADIUS_CONTROL);
    visuals.widgets.noninteractive.bg_fill = surface;
    visuals.widgets.noninteractive.weak_bg_fill = surface;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, line);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, secondary);
    visuals.widgets.noninteractive.rounding = rounding;

    visuals.widgets.inactive.bg_fill = raised;
    visuals.widgets.inactive.weak_bg_fill = sunken;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, border);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, ink);
    visuals.widgets.inactive.rounding = rounding;

    visuals.widgets.hovered.bg_fill = hover;
    visuals.widgets.hovered.weak_bg_fill = hover;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, border);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, ink);
    visuals.widgets.hovered.rounding = rounding;

    visuals.widgets.active.bg_fill = pressed;
    visuals.widgets.active.weak_bg_fill = pressed;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, colors.accent);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, ink);
    visuals.widgets.active.rounding = rounding;

    // egui derives the disabled look by dimming the inactive visuals, so the muted ink and the
    // receded fill above are what a disabled control actually renders as.
}

/// The one primary action on a page. Fixed height and radius so it matches every other button.
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
            if enabled {
                let fill = scale::accent();
                widgets.noninteractive.weak_bg_fill = fill;
                widgets.inactive.weak_bg_fill = fill;
                // Hover and press use the action palette with its own contrasting label.
                widgets.hovered.weak_bg_fill = scale::accent_hover();
                widgets.active.weak_bg_fill = scale::accent_pressed();
            } else {
                let fill = scale::disabled_fill();
                widgets.noninteractive.weak_bg_fill = fill;
                widgets.inactive.weak_bg_fill = fill;
                widgets.hovered.weak_bg_fill = fill;
                widgets.active.weak_bg_fill = fill;
            }
            super::components::centered_button(
                ui,
                egui::RichText::new(self.label)
                    .size(scale::BODY)
                    .color(if enabled {
                        scale::on_accent()
                    } else {
                        scale::disabled_ink()
                    }),
                egui::Button::new("")
                    .stroke(egui::Stroke::NONE)
                    .rounding(egui::Rounding::same(scale::RADIUS_CONTROL))
                    .min_size(egui::vec2(scale::BUTTON_MIN_W, scale::CONTROL_H)),
                egui::vec2(scale::BUTTON_MIN_W, scale::CONTROL_H),
            )
        })
        .inner
    }
}

#[cfg(test)]
mod restyle_regressions {
    use super::*;
    fn luminance(c: Color32) -> f64 {
        let linear = |v: u8| {
            let v = f64::from(v) / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
    }
    fn contrast(a: Color32, b: Color32) -> f64 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
    #[test]
    fn both_theme_slots_use_complete_palette_roles() {
        let ctx = egui::Context::default();
        for preference in [
            egui::ThemePreference::System,
            egui::ThemePreference::Light,
            egui::ThemePreference::Dark,
        ] {
            style_root(&ctx, preference);
            for (theme, dark) in [(egui::Theme::Light, false), (egui::Theme::Dark, true)] {
                let style = ctx.style_of(theme);
                let p = palette(dark);
                assert_eq!(style.spacing.interact_size.y, scale::CONTROL_H);
                assert_eq!(style.visuals.selection.bg_fill, p.selected);
                assert_eq!(style.visuals.selection.stroke.color, p.on_selected);
                assert_eq!(style.visuals.hyperlink_color, p.accent);
                assert_eq!(style.visuals.window_fill, p.surface);
                scale::set_dark(dark);
                assert_eq!(scale::ink(), style.visuals.widgets.inactive.fg_stroke.color);
                assert_ne!(p.accent, p.selected);
            }
        }
    }
    #[test]
    fn text_and_interaction_states_remain_readable() {
        for dark in [false, true] {
            let p = palette(dark);
            for background in [p.surface, p.surface_alt, p.surface_page] {
                for text in [p.ink, p.secondary] {
                    assert!(
                        contrast(text, background) >= 4.5,
                        "{dark}: {text:?} / {background:?}"
                    );
                }
            }
            assert!(contrast(p.faint, p.surface) >= 4.5);
            for bg in [p.accent, p.accent_hover, p.accent_pressed] {
                assert!(contrast(p.on_accent, bg) >= 4.5);
            }
            for bg in [p.selected, p.selected_hover, p.selected_pressed] {
                assert!(contrast(p.on_selected, bg) >= 4.5);
            }
            // egui retains glyph colors when selecting text.
            assert!(contrast(p.ink, p.selected) >= 4.5);
        }
    }
    #[test]
    fn surfaces_have_distinct_roles() {
        for dark in [false, true] {
            let p = palette(dark);
            assert_ne!(p.surface, p.surface_page);
            assert_ne!(p.surface, p.surface_alt);
            assert_ne!(p.surface_sunken, p.surface_raised);
            assert!(contrast(p.ink, p.surface) > 10.0);
        }
    }
    #[test]
    fn the_type_scale_stays_at_four_steps() {
        assert_eq!(
            [scale::META, scale::BODY, scale::HEADING, scale::TITLE],
            [12.0, 14.0, 15.0, 18.0]
        );
    }
    #[test]
    fn a_stored_theme_choice_survives_a_restyle() {
        let ctx = egui::Context::default();
        store_theme(&ctx, egui::ThemePreference::Dark);
        apply_style(&ctx);
        assert!(scale::dark());
    }
}
