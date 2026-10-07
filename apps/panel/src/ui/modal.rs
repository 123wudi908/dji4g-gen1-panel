//! The panel's one modal dialog.
//!
//! A dialog is a small flat card in the middle of the window, over a dimmed page that can no longer
//! be touched. It is deliberately plain: a title, an optional explanation, whatever body the caller
//! needs, and a right-aligned row of action buttons built from the shared [`action_button`] control.
//! No shadow, no gradient, no close glyph and no accent colour — the scrim, one hairline above the
//! footer, and the panel's shared palette are the whole vocabulary.
//!
//! # Using it
//!
//! The caller owns whether the dialog exists; [`show`] draws one frame of it and reports what that
//! frame did. A dialog that did not report [`DialogResponse::dismissed`] has to be shown again next
//! frame:
//!
//! ```ignore
//! // Paths abbreviated: this module is `crate::ui::modal`.
//! let outcome = modal::show(
//!     ctx,
//!     &modal::Dialog::new("reboot-confirm", "重启模块")
//!         .description("重启期间模块会短暂离线，已连接的会话会被中断。")
//!         .tone(modal::DialogTone::Destructive),
//!     |ui| {
//!         ui.label("模块会在约 30 秒后重新可用。");
//!     },
//!     &[
//!         modal::DialogAction::destructive("立即重启"),
//!         modal::DialogAction::cancel("取消"),
//!     ],
//! );
//! if outcome.dismissed() {
//!     self.confirm_open = false;
//! }
//! if outcome.chosen() == Some(0) {
//!     self.restart();
//! }
//! ```
//!
//! Actions are passed in visual order, right to left: `actions[0]` is the right-most button, which
//! is where the panel puts the action the dialog exists for.
//!
//! # Sizing and placement
//!
//! * Placement is an [`egui::Area`] anchored at [`Align2::CENTER_CENTER`] in the
//!   [`egui::Order::Foreground`] layer. The dialog is centred on the screen, never moves and is never
//!   resizable; centring through `Area::anchor` keeps the panel's pixel-rounding rules in charge, so
//!   the card cannot drift by a fraction of a point from frame to frame.
//! * Width is [`DEFAULT_WIDTH`] (400pt) unless the caller sets [`Dialog::width`], clamped to
//!   `[MIN_WIDTH, screen width − 2 × PAD]` (260pt … screen − 32pt). The content column is pinned to
//!   that width, so no piece of content can widen or narrow the card — two dialogs with different
//!   content are the same rectangle.
//! * Height is measured from the content, so a card is exactly as tall as it needs to be, and is
//!   bounded by the screen. A body that can grow (a list, a log) brings its own [`egui::ScrollArea`]
//!   with an explicit maximum height: the dialog never grows a scrollbar of its own.
//! * The card is padded [`PAD`] (16pt) on all four sides, and its internal rhythm is explicit rather
//!   than egui's default item spacing: title → explanation 6pt, explanation → body 16pt, body →
//!   hairline 16pt, hairline → actions 12pt. Those are the only gaps a dialog has.
//! * Every action is the shared [`action_button`], so an action is exactly `CONTROL_H` (28pt) tall
//!   and at least `BUTTON_MIN_W` (64pt) wide, like every other control in the panel.
//!
//! # What the dialog does not do
//!
//! The scrim blocks the pointer, but keyboard focus is not trapped: egui 0.29 has no focus scope, so
//! `Tab` can still reach the page behind. The dialog also deliberately does not focus an action on
//! open — silently arming `Enter` on a destructive action is worse than one extra keystroke.

use eframe::egui::{
    self, Align2, Id, Key, Modifiers, Order, RichText, Sense, Stroke, Ui, Vec2, pos2,
};

use super::components::{ButtonKind, action_button};
use super::scale;

/// Padding between the card's edge and its content, on all four sides.
const PAD: f32 = scale::PAGE_GAP;

/// Default width of a dialog: wide enough for a wrapped sentence, narrow enough to still read as a
/// dialog rather than a small page.
const DEFAULT_WIDTH: f32 = 400.0;

/// Narrowest a dialog may become. The panel's window floor is 800pt wide, so this bound is only ever
/// reached through an explicit [`Dialog::width`].
const MIN_WIDTH: f32 = 260.0;

/// The wash that puts the page behind the dialog out of play.
///
/// Achromatic in both themes, and composed rather than invented: by day a 24% ink wash recedes the
/// sheet, by night the near-black page tone deepens it instead of fogging a dark window with white.
fn scrim() -> egui::Color32 {
    if scale::dark() {
        scale::surface_page().gamma_multiply(0.72)
    } else {
        scale::ink().gamma_multiply(0.24)
    }
}

/// How a dialog presents its title.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DialogTone {
    /// The default: the title is plain ink, like any other heading.
    Neutral,
    /// A dialog whose action destroys or interrupts something. Only the title changes colour, and
    /// it takes the panel's one red for "destructive" — red is never decoration here.
    Destructive,
}

/// One frame's description of a modal dialog.
///
/// Build it, hand it to [`show`] together with a body closure and the action buttons, and look at
/// the returned [`DialogResponse`]. See the module docs for the sizing rules.
pub(crate) struct Dialog<'a> {
    id: Id,
    title: &'a str,
    description: Option<&'a str>,
    width: f32,
    tone: DialogTone,
    dismissible: bool,
}

impl<'a> Dialog<'a> {
    /// A dialog identified by `id`.
    ///
    /// `id` must be stable across frames and unique among everything else on screen: it is the
    /// egui id of the dialog's area, and reusing one for a different dialog reuses its geometry.
    pub(crate) fn new(id: impl std::hash::Hash, title: &'a str) -> Self {
        Self {
            id: Id::new(id),
            title,
            description: None,
            width: DEFAULT_WIDTH,
            tone: DialogTone::Neutral,
            dismissible: true,
        }
    }

    /// The one sentence that explains the decision. Wrapped, and kept a tier below the title.
    #[must_use]
    pub(crate) fn description(mut self, description: &'a str) -> Self {
        self.description = Some(description);
        self
    }

    /// Widen (or narrow) the card. Clamped to `[MIN_WIDTH, screen − 2 × PAD]`.
    #[must_use]
    pub(crate) fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// How the title is drawn. Default: [`DialogTone::Neutral`].
    #[must_use]
    pub(crate) fn tone(mut self, tone: DialogTone) -> Self {
        self.tone = tone;
        self
    }

    /// Whether `Esc` and a click on the scrim close the dialog. Default: `true`.
    ///
    /// A dialog that must be answered — one that is mid-operation, or where the only way out is an
    /// explicit decision — turns this off. `Esc` is swallowed either way, so the page behind never
    /// sees it while a dialog is up.
    #[must_use]
    pub(crate) fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
        self
    }
}

/// One button in the dialog's footer, drawn with the shared [`action_button`] control.
#[derive(Clone, Copy)]
pub(crate) struct DialogAction<'a> {
    label: &'a str,
    kind: ButtonKind,
    enabled: bool,
    disabled_reason: Option<&'a str>,
    closes: bool,
}

impl<'a> DialogAction<'a> {
    /// An action drawn as `kind`. It closes the dialog unless [`DialogAction::keeping_open`].
    #[must_use]
    pub(crate) fn new(label: &'a str, kind: ButtonKind) -> Self {
        Self {
            label,
            kind,
            enabled: true,
            disabled_reason: None,
            closes: true,
        }
    }

    /// The action the dialog exists for: the solid button, right-most.
    #[must_use]
    pub(crate) fn primary(label: &'a str) -> Self {
        Self::new(label, ButtonKind::Filled)
    }

    /// The quiet way out, drawn as text so it never competes with the action beside it.
    #[must_use]
    pub(crate) fn cancel(label: &'a str) -> Self {
        Self::new(label, ButtonKind::Text)
    }

    /// An action that destroys or fails something: outlined, and the only action that may be red.
    #[must_use]
    pub(crate) fn destructive(label: &'a str) -> Self {
        Self::new(label, ButtonKind::Destructive)
    }

    /// Take the button out of play without removing it, so the footer keeps its shape.
    #[must_use]
    pub(crate) fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The reason a disabled button gives on hover, exactly as `action_button` takes it.
    #[must_use]
    pub(crate) fn disabled_reason(mut self, reason: &'a str) -> Self {
        self.disabled_reason = Some(reason);
        self
    }

    /// Keep the dialog on screen after this button is pressed — for an action the user may want to
    /// repeat, like a retry.
    #[must_use]
    pub(crate) fn keeping_open(mut self) -> Self {
        self.closes = false;
        self
    }
}

/// What one frame of a dialog did.
pub(crate) struct DialogResponse<R> {
    /// The dialog surface's own response. Its rect is the whole card, including its padding.
    pub(crate) response: egui::Response,
    /// Whatever the body closure returned this frame, so a dialog can hand a value back.
    pub(crate) body: R,
    chosen: Option<usize>,
    dismissed: bool,
    action_rects: Vec<egui::Rect>,
}

impl<R> DialogResponse<R> {
    /// Index into the `actions` slice of the button that was pressed this frame, if any.
    #[must_use]
    pub(crate) fn chosen(&self) -> Option<usize> {
        self.chosen
    }

    /// Whether the dialog is finished: `Esc`, a click on the scrim, or an action button that closes
    /// it. The caller still owns the state — this only says the dialog should be taken off screen.
    #[must_use]
    pub(crate) fn dismissed(&self) -> bool {
        self.dismissed
    }

    /// The footer buttons' rects, in the order they were declared. Exposed so a caller can anchor a
    /// popup under the button it belongs to, and so the one-height rule can be asserted.
    #[must_use]
    pub(crate) fn action_rects(&self) -> &[egui::Rect] {
        &self.action_rects
    }
}

/// Show `dialog` for one frame, lay out `body` inside it and draw `actions` in the footer.
///
/// See the module docs for the sizing and spacing rules. The two closures run in the order they are
/// written — the body first, then the footer — so the caller pays no borrow-checker penalty for
/// touching one piece of state from both.
#[must_use]
pub(crate) fn show<R>(
    ctx: &egui::Context,
    dialog: &Dialog<'_>,
    body: impl FnOnce(&mut Ui) -> R,
    actions: &[DialogAction<'_>],
) -> DialogResponse<R> {
    let screen = ctx.screen_rect();
    // `max` keeps the clamp well formed on a screen narrower than a dialog; the panel's window floor
    // is far above `MIN_WIDTH`, so it only ever matters to a test with a tiny screen.
    let width = dialog
        .width
        .clamp(MIN_WIDTH, (screen.width() - 2.0 * PAD).max(MIN_WIDTH));

    // The scrim is an interactable area over the whole screen: above the page and below the dialog.
    // It dims what is behind it and takes every pointer event that does not land on the dialog, so
    // the page cannot be clicked, hovered or dragged while a dialog is up.
    let scrim_clicked = egui::Area::new(dialog.id.with("scrim"))
        .order(Order::Middle)
        .fixed_pos(screen.min)
        .interactable(true)
        .show(ctx, |ui| {
            let response = ui.allocate_rect(screen, Sense::click_and_drag());
            ui.painter()
                .rect_filled(screen, egui::Rounding::ZERO, scrim());
            response.clicked()
        })
        .inner;

    // While a dialog is up, `Esc` belongs to it: consumed either way, so the page behind never acts
    // on it, but only a dismissible dialog closes.
    let escaped = ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));

    let mut chosen = None;
    let mut closes = false;
    let mut action_rects = Vec::with_capacity(actions.len());

    let area = egui::Area::new(dialog.id)
        .order(Order::Foreground)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .interactable(true)
        .show(ctx, |ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            egui::Frame::none()
                .fill(scale::surface())
                .rounding(egui::Rounding::same(scale::RADIUS_CONTAINER))
                .stroke(Stroke::new(1.0_f32, scale::line()))
                .inner_margin(egui::Margin::same(PAD))
                .show(ui, |ui| {
                    let content_width = width - 2.0 * PAD;
                    ui.set_min_width(content_width);
                    ui.set_max_width(content_width);
                    let content_left = ui.max_rect().left();
                    // Inside the card every gap is one of the tokens written out below, never egui's
                    // default item spacing: that is what lets two dialogs with different content
                    // still read as the same component.
                    ui.spacing_mut().item_spacing.y = 0.0;

                    ui.label(
                        RichText::new(dialog.title)
                            .size(scale::TITLE)
                            .strong()
                            .color(match dialog.tone {
                                DialogTone::Neutral => scale::ink(),
                                DialogTone::Destructive => scale::danger(),
                            }),
                    );
                    if let Some(description) = dialog.description {
                        // The explanation belongs to the title, so it sits at the tight inline gap.
                        ui.add_space(scale::CONTROL_GAP[1]);
                        ui.add(
                            egui::Label::new(
                                RichText::new(description)
                                    .size(scale::BODY)
                                    .color(scale::secondary()),
                            )
                            .wrap(),
                        );
                    }
                    ui.add_space(PAD);

                    // The caller's content, back on the panel's normal body rhythm.
                    let body_output = ui
                        .scope(|ui| {
                            ui.set_min_width(content_width);
                            ui.set_max_width(content_width);
                            ui.spacing_mut().item_spacing.y = scale::SECTION_GAP;
                            body(ui)
                        })
                        .inner;

                    if !actions.is_empty() {
                        ui.add_space(PAD);
                        // One hairline, drawn edge to edge. The panel separates with lines, never
                        // with decoration, and this is what keeps the decision from reading as part
                        // of the content it is deciding about.
                        let rule_y = ui.cursor().top();
                        ui.painter().line_segment(
                            [
                                pos2(content_left - PAD, rule_y),
                                pos2(content_left + content_width + PAD, rule_y),
                            ],
                            Stroke::new(1.0_f32, scale::line()),
                        );
                        ui.add_space(scale::BLOCK_GAP);
                        ui.horizontal(|ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.spacing_mut().item_spacing.x = scale::CONTROL_GAP[0];
                                    for (index, action) in actions.iter().enumerate() {
                                        let response = action_button(
                                            ui,
                                            action.label,
                                            action.kind,
                                            action.enabled,
                                            action.disabled_reason,
                                        );
                                        action_rects.push(response.rect);
                                        if response.clicked() {
                                            chosen = Some(index);
                                            closes |= action.closes;
                                        }
                                    }
                                },
                            );
                        });
                    }
                    body_output
                })
                .inner
        });

    DialogResponse {
        response: area.response,
        body: area.inner,
        chosen,
        dismissed: closes || (dialog.dismissible && (escaped || scrim_clicked)),
        action_rects,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Event, PointerButton, Pos2, Rect, vec2};

    /// A desktop-size panel window, in points.
    const SCREEN: [f32; 2] = [800.0, 600.0];

    fn screen() -> Rect {
        Rect::from_min_size(Pos2::ZERO, vec2(SCREEN[0], SCREEN[1]))
    }

    fn input(events: Vec<Event>) -> egui::RawInput {
        egui::RawInput {
            events,
            screen_rect: Some(screen()),
            ..Default::default()
        }
    }

    /// A press at `pos` (`pressed = true`), and the release that completes the click (`false`).
    fn pointer(pos: Pos2, pressed: bool) -> Vec<Event> {
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            },
        ]
    }

    fn escape() -> Vec<Event> {
        vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }]
    }

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        crate::ui::apply_style(&ctx);
        ctx
    }

    fn confirm() -> Dialog<'static> {
        Dialog::new("test-modal", "重启模块")
            .description("重启期间模块会短暂离线，已连接的会话会被中断。")
    }

    fn actions() -> [DialogAction<'static>; 2] {
        [
            DialogAction::primary("立即重启"),
            DialogAction::cancel("取消"),
        ]
    }

    /// One frame of `dialog`, returning what that frame did. The body closure returns a marker, so a
    /// test can prove the caller's content was laid out and its value handed back.
    fn frame(
        ctx: &egui::Context,
        events: Vec<Event>,
        dialog: &Dialog<'_>,
        actions: &[DialogAction<'_>],
    ) -> DialogResponse<u8> {
        let mut outcome = None;
        let _ = ctx.run(input(events), |ctx| {
            outcome = Some(show(
                ctx,
                dialog,
                |ui| {
                    ui.label("模块会在约 30 秒后重新可用。");
                    1
                },
                actions,
            ));
        });
        outcome.expect("every frame of a dialog produces a response")
    }

    #[test]
    fn a_dialog_centres_itself_and_keeps_every_action_one_control_height() {
        let ctx = context();
        let actions = actions();
        // The first frame of a fresh egui `Area` is a sizing pass: it measures the content and draws
        // nothing, so the card is only in its final place from the second frame on.
        let measured = frame(&ctx, Vec::new(), &confirm(), &actions);
        assert!(
            measured.response.rect.width() > 0.0,
            "the dialog must produce a response"
        );

        let outcome = frame(&ctx, Vec::new(), &confirm(), &actions);
        assert_eq!(
            outcome.body, 1,
            "the body closure ran and its value came back"
        );
        let rect = outcome.response.rect;
        // egui rounds an area's position to whole physical pixels, so a card of odd height can only
        // land within half a point of the exact centre; beyond one pixel would be a bug.
        let offset = rect.center() - screen().center();
        assert!(
            offset.x.abs() <= 1.0 && offset.y.abs() <= 1.0,
            "the dialog is not centred: {rect:?}"
        );
        assert!(
            (rect.width() - DEFAULT_WIDTH).abs() < 0.5,
            "unexpected dialog width: {rect:?}"
        );
        assert_eq!(
            outcome.action_rects().len(),
            2,
            "one rect per declared action"
        );
        for action in outcome.action_rects() {
            let height = action.height();
            let control = scale::CONTROL_H;
            assert!(
                (height - control).abs() < 0.01,
                "action is {height}pt tall, not {control}pt: {action:?}"
            );
            assert!(
                rect.contains_rect(*action),
                "an action escaped the card: {action:?} not in {rect:?}"
            );
        }
        // Declared primary first, so it is the right-most button of the footer.
        assert!(
            outcome.action_rects()[0].left() > outcome.action_rects()[1].left(),
            "the primary action must be declared first and drawn right-most"
        );
        assert!(
            outcome.chosen().is_none() && !outcome.dismissed(),
            "nothing was touched in this frame"
        );
    }

    #[test]
    fn the_card_width_is_clamped_to_the_screen() {
        let mut widths = [0.0_f32; 2];
        for (slot, requested) in [10.0_f32, 4000.0].into_iter().enumerate() {
            let ctx = context();
            let dialog = Dialog::new("width-modal", "宽度").width(requested);
            for _ in 0..2 {
                let _ = ctx.run(input(Vec::new()), |ctx| {
                    widths[slot] = show(ctx, &dialog, |_ui| (), &[]).response.rect.width();
                });
            }
        }
        assert!(
            (widths[0] - MIN_WIDTH).abs() < 0.5,
            "a narrow request must still get MIN_WIDTH: {widths:?}"
        );
        assert!(
            (widths[1] - (SCREEN[0] - 2.0 * PAD)).abs() < 0.5,
            "a huge request must stop one page gutter short of each screen edge: {widths:?}"
        );
    }

    #[test]
    fn the_card_is_exactly_as_tall_as_its_content() {
        let mut heights = [0.0_f32; 2];
        for (slot, lines) in [1_usize, 4].into_iter().enumerate() {
            let ctx = context();
            let dialog = Dialog::new("measuring-modal", "重启模块");
            // Two frames: the first measures the content, the second draws at the measured size.
            for _ in 0..2 {
                let _ = ctx.run(input(Vec::new()), |ctx| {
                    heights[slot] = show(
                        ctx,
                        &dialog,
                        |ui| {
                            for line in 0..lines {
                                ui.label(format!("第 {line} 行说明"));
                            }
                        },
                        &[],
                    )
                    .response
                    .rect
                    .height();
                });
            }
        }
        assert!(
            heights[1] > heights[0],
            "a taller body must produce a taller card, never a clipped one: {heights:?}"
        );
    }

    #[test]
    fn escape_dismisses_a_dismissible_dialog() {
        let ctx = context();
        let actions = actions();
        let _ = frame(&ctx, Vec::new(), &confirm(), &actions);
        let outcome = frame(&ctx, escape(), &confirm(), &actions);
        assert!(outcome.dismissed(), "Esc must dismiss a dismissible dialog");
        assert_eq!(outcome.chosen(), None, "Esc chooses no action");
    }

    #[test]
    fn a_non_dismissible_dialog_swallows_escape_and_the_scrim() {
        let ctx = context();
        let actions = actions();
        let dialog = confirm().dismissible(false);
        let _ = frame(&ctx, Vec::new(), &dialog, &actions);

        let outcome = frame(&ctx, escape(), &dialog, &actions);
        assert!(!outcome.dismissed(), "a pinned dialog must survive Esc");

        let rect = frame(&ctx, Vec::new(), &dialog, &actions).response.rect;
        let outside = Pos2::new(rect.left() - 40.0, rect.center().y);
        let _ = frame(&ctx, pointer(outside, true), &dialog, &actions);
        let outcome = frame(&ctx, pointer(outside, false), &dialog, &actions);
        assert!(
            !outcome.dismissed(),
            "a pinned dialog must survive a scrim click"
        );
    }

    #[test]
    fn a_scrim_click_dismisses_while_a_click_inside_does_not() {
        let ctx = context();
        let actions = actions();
        let _ = frame(&ctx, Vec::new(), &confirm(), &actions);
        let rect = frame(&ctx, Vec::new(), &confirm(), &actions).response.rect;

        // Inside the card but on neither button: the padding must not dismiss it.
        let inside = rect.center();
        let _ = frame(&ctx, pointer(inside, true), &confirm(), &actions);
        let outcome = frame(&ctx, pointer(inside, false), &confirm(), &actions);
        assert!(
            !outcome.dismissed(),
            "a click on the card must keep it open"
        );

        let outside = Pos2::new(rect.left() - 40.0, rect.center().y);
        let _ = frame(&ctx, pointer(outside, true), &confirm(), &actions);
        let outcome = frame(&ctx, pointer(outside, false), &confirm(), &actions);
        assert!(outcome.dismissed(), "a click on the scrim must dismiss");
        assert_eq!(outcome.chosen(), None, "the scrim chooses no action");
    }

    #[test]
    fn pressing_an_action_reports_which_button_and_dismisses() {
        let ctx = context();
        let actions = actions();
        let _ = frame(&ctx, Vec::new(), &confirm(), &actions);
        let cancel = frame(&ctx, Vec::new(), &confirm(), &actions).action_rects()[1].center();

        let _ = frame(&ctx, pointer(cancel, true), &confirm(), &actions);
        let outcome = frame(&ctx, pointer(cancel, false), &confirm(), &actions);
        assert_eq!(outcome.chosen(), Some(1), "index 1 is the cancel button");
        assert!(
            outcome.dismissed(),
            "an action closes the dialog by default"
        );
    }

    #[test]
    fn a_keeping_open_action_reports_its_choice_without_closing() {
        let ctx = context();
        let actions = [
            DialogAction::primary("重试").keeping_open(),
            DialogAction::cancel("关闭"),
        ];
        let _ = frame(&ctx, Vec::new(), &confirm(), &actions);
        let retry = frame(&ctx, Vec::new(), &confirm(), &actions).action_rects()[0].center();

        let _ = frame(&ctx, pointer(retry, true), &confirm(), &actions);
        let outcome = frame(&ctx, pointer(retry, false), &confirm(), &actions);
        assert_eq!(outcome.chosen(), Some(0));
        assert!(
            !outcome.dismissed(),
            "a `keeping_open` action must leave the dialog up"
        );
    }

    #[test]
    fn a_disabled_action_cannot_be_chosen() {
        let ctx = context();
        let actions = [DialogAction::primary("立即重启")
            .enabled(false)
            .disabled_reason("模块正在处理其他任务")];
        let _ = frame(&ctx, Vec::new(), &confirm(), &actions);
        let action = frame(&ctx, Vec::new(), &confirm(), &actions).action_rects()[0];
        let control = scale::CONTROL_H;
        assert!(
            (action.height() - control).abs() < 0.01,
            "a disabled action keeps the control height"
        );

        let pos = action.center();
        let _ = frame(&ctx, pointer(pos, true), &confirm(), &actions);
        let outcome = frame(&ctx, pointer(pos, false), &confirm(), &actions);
        assert_eq!(outcome.chosen(), None, "a disabled action must not fire");
        assert!(!outcome.dismissed());
    }

    #[test]
    fn the_scrim_blocks_the_page_behind_the_dialog() {
        /// One frame of a page with a single button, optionally behind a dialog. Returns the
        /// button's rect and whether that frame's click reached it.
        fn page_frame(
            ctx: &egui::Context,
            events: Vec<Event>,
            dialog: Option<&Dialog<'_>>,
        ) -> (Rect, bool) {
            let mut rect = Rect::NOTHING;
            let mut clicked = false;
            let _ = ctx.run(input(events), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let response = ui.add(egui::Button::new("页面按钮"));
                    rect = response.rect;
                    clicked = response.clicked();
                });
                if let Some(dialog) = dialog {
                    let _ = show(ctx, dialog, |_ui| (), &[]);
                }
            });
            (rect, clicked)
        }

        // Control: with no dialog up, the same click does reach the page.
        let ctx = context();
        let (button, _) = page_frame(&ctx, Vec::new(), None);
        let pos = button.center();
        let _ = page_frame(&ctx, pointer(pos, true), None);
        let (_, clicked) = page_frame(&ctx, pointer(pos, false), None);
        assert!(
            clicked,
            "the control frame must land the click without a dialog"
        );

        // With a dialog up, the scrim takes the click and the page never sees it.
        let ctx = context();
        let dialog = Dialog::new("blocking-modal", "重启模块");
        let (button, _) = page_frame(&ctx, Vec::new(), Some(&dialog));
        let pos = button.center();
        let _ = page_frame(&ctx, pointer(pos, true), Some(&dialog));
        let (_, clicked) = page_frame(&ctx, pointer(pos, false), Some(&dialog));
        assert!(
            !clicked,
            "a click outside the dialog must not reach the page behind it"
        );
    }
}
