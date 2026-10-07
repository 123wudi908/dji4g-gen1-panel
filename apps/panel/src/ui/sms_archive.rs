//! A read-only personal history surface. It never constructs live module commands.
use super::modal;
use crate::sms_archive::ArchiveService;
use eframe::egui::{self, Ui};
use std::time::{SystemTime, UNIX_EPOCH};

/// One catalog string in the language this page was rendered with.
fn t(language: crate::localization::Language, key: crate::localization::TextKey) -> String {
    crate::localization::LocalizedText::new(language, key).text
}

#[derive(Default)]
pub(crate) struct ArchiveUi {
    search: String,
    days: u64,
    confirm: Option<Confirmation>,
    pub error: Option<String>,
    pub export_path: Option<std::path::PathBuf>,
}

#[derive(Clone, Copy)]
enum Confirmation {
    Clear,
    Export,
}
pub(crate) enum ArchiveAction {
    SetEnabled(bool),
    Clear,
    Export,
}

pub(crate) fn render(
    ui: &mut Ui,
    language: crate::localization::Language,
    archive: Option<&ArchiveService>,
    state: &mut ArchiveUi,
) -> Option<ArchiveAction> {
    ui.heading(t(language, crate::localization::TextKey::ArchiveHeading));
    ui.label(t(language, crate::localization::TextKey::ArchiveIntro));
    ui.small(t(language, crate::localization::TextKey::ArchiveRetention));
    let Some(archive) = archive else {
        ui.label(t(
            language,
            crate::localization::TextKey::ArchiveUnavailable,
        ));
        return None;
    };
    let mut action = None;
    let mut enabled = archive.enabled();
    if ui
        .checkbox(
            &mut enabled,
            t(language, crate::localization::TextKey::ArchiveToggle),
        )
        .changed()
    {
        action = Some(ArchiveAction::SetEnabled(enabled));
    }
    // The service reports stable codes; the panel turns them into prose in the current language.
    let status = crate::localization::stable_code_text(archive.status()).map_or_else(
        || archive.status().to_owned(),
        |key| crate::localization::LocalizedText::new(language, key).text,
    );
    ui.label(status);
    if let Some(error) = &state.error {
        ui.colored_label(super::StatusTone::Negative.color(), error);
    }
    if let Some(path) = &state.export_path {
        let target = path.display().to_string();
        ui.label(crate::localization::format_positional(
            language,
            crate::localization::TextKey::ArchiveExportTarget,
            &[&target],
        ));
    }
    ui.horizontal_wrapped(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut state.search)
                .hint_text(t(language, crate::localization::TextKey::ArchiveSearchHint))
                .desired_width(200.0),
        );
        super::components::segmented_control(
            ui,
            egui::Id::new("archive-date-filter"),
            &mut state.days,
            &[
                super::components::TabItem::new(
                    0,
                    t(language, crate::localization::TextKey::ArchiveFilterAll),
                ),
                super::components::TabItem::new(
                    7,
                    t(language, crate::localization::TextKey::ArchiveFilterWeek),
                ),
                super::components::TabItem::new(
                    30,
                    t(language, crate::localization::TextKey::ArchiveFilterMonth),
                ),
            ],
        );
        if ui
            .add_enabled(
                !archive.busy() && !archive.rows().is_empty(),
                egui::Button::new(t(language, crate::localization::TextKey::ArchiveExportTxt)),
            )
            .clicked()
        {
            state.confirm = Some(Confirmation::Export);
        }
        if ui
            .add_enabled(
                !archive.busy(),
                egui::Button::new(t(language, crate::localization::TextKey::ArchiveClear)),
            )
            .clicked()
        {
            state.confirm = Some(Confirmation::Clear);
        }
    });
    ui.separator();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let query = state.search.trim().to_lowercase();
    let rows = archive
        .rows()
        .iter()
        .rev()
        .filter(|row| {
            (state.days == 0 || now.saturating_sub(row.captured_unix_secs()) <= state.days * 86400)
                && (query.is_empty()
                    || row.sender().to_lowercase().contains(&query)
                    || row.body().to_lowercase().contains(&query))
        })
        .collect::<Vec<_>>();
    let shown = rows.len().to_string();
    let saved = archive.rows().len().to_string();
    ui.label(crate::localization::format_positional(
        language,
        crate::localization::TextKey::ArchiveCount,
        &[&shown, &saved],
    ));
    if rows.is_empty() {
        let empty = if archive.rows().is_empty() {
            t(language, crate::localization::TextKey::ArchiveEmpty)
        } else {
            t(language, crate::localization::TextKey::ArchiveNoMatch)
        };
        ui.label(empty);
    }
    egui::ScrollArea::vertical()
        .id_salt("archive-rows")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for row in &rows {
                // A stored message is a flat, always-open card: the sender/timestamp line is the
                // card's heading and the body is readable without a disclosure click.  The body and
                // source note keep the quiet tiers so an always-visible list stays scannable.
                super::section_frame(ui, |ui| {
                    let timestamp =
                        row.reported_timestamp()
                            .map(str::to_owned)
                            .unwrap_or_else(|| {
                                t(language, crate::localization::TextKey::ArchiveNoTimestamp)
                            });
                    let incomplete = if row.incomplete() {
                        t(language, crate::localization::TextKey::ArchiveIncompleteTag)
                    } else {
                        String::new()
                    };
                    ui.label(super::section_heading(format!(
                        "{}    {timestamp}{incomplete}",
                        row.sender(),
                    )));
                    super::wrapped_label(ui, super::detail_text(row.body()));
                    let group = row.context_label();
                    ui.label(super::meta_text(crate::localization::format_positional(
                        language,
                        crate::localization::TextKey::ArchiveSourceGroup,
                        &[&group],
                    )));
                    if ui
                        .button(t(language, crate::localization::TextKey::ArchiveCopyBody))
                        .clicked()
                    {
                        ui.ctx().copy_text(row.body().to_owned());
                    }
                });
            }
        });
    if let Some(confirm) = state.confirm {
        let (title, description, confirm_label, confirmed_action) = match confirm {
            Confirmation::Clear => (
                t(language, crate::localization::TextKey::ArchiveClear),
                t(
                    language,
                    crate::localization::TextKey::ArchiveClearDescription,
                ),
                t(language, crate::localization::TextKey::ArchiveClearConfirm),
                ArchiveAction::Clear,
            ),
            Confirmation::Export => (
                t(language, crate::localization::TextKey::ArchiveExportTitle),
                t(
                    language,
                    crate::localization::TextKey::ArchiveExportDescription,
                ),
                t(language, crate::localization::TextKey::ArchiveExportConfirm),
                ArchiveAction::Export,
            ),
        };
        let confirm_action = match confirm {
            Confirmation::Clear => modal::DialogAction::destructive(&confirm_label),
            Confirmation::Export => modal::DialogAction::primary(&confirm_label),
        };
        let cancel_label = t(language, crate::localization::TextKey::ArchiveCancel);
        let outcome = modal::show(
            ui.ctx(),
            &modal::Dialog::new("sms-archive-confirmation", &title).description(&description),
            |_ui| {},
            &[
                confirm_action.enabled(!archive.busy()),
                modal::DialogAction::cancel(&cancel_label),
            ],
        );
        if outcome.dismissed() {
            state.confirm = None;
        }
        if outcome.chosen() == Some(0) {
            action = Some(confirmed_action);
        }
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every stored message is a flat, always-open card, so the sender line and the body are
    /// painted on the first frame with no click.  Filtering narrows the list; it can no longer
    /// change whether a listed message is open, because there is no disclosure state left.
    #[test]
    fn history_rows_show_sender_and_body_without_a_disclosure_click() {
        let archive = ArchiveService::review_fixture(true);
        let first = archive.rows()[0].sender().to_owned();
        let second = archive.rows()[1].sender().to_owned();
        let body = archive.rows()[0].body().to_owned();
        let context = egui::Context::default();
        context.style_mut(|style| style.animation_time = 0.0);
        let mut state = ArchiveUi::default();
        let render_frame = |state: &mut ArchiveUi| {
            context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 900.0),
                    )),
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        assert!(
                            render(
                                ui,
                                crate::localization::Language::ZhCn,
                                Some(&archive),
                                state,
                            )
                            .is_none()
                        );
                    });
                },
            )
        };
        let texts = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let shown = texts(&render_frame(&mut state));
        for sender in [&first, &second] {
            assert!(
                shown.iter().any(|text| text.starts_with(sender.as_str())),
                "row heading for {sender} should be visible"
            );
        }
        assert!(
            shown.iter().filter(|text| **text == body).count() >= 3,
            "every stored body should be visible without a click"
        );
        state.search = first.clone();
        let filtered = texts(&render_frame(&mut state));
        assert!(filtered.iter().any(|text| text.starts_with(&first)));
        assert!(
            !filtered.iter().any(|text| text.starts_with(&second)),
            "filtering still narrows the list"
        );
        assert!(
            filtered.contains(&body),
            "a listed message keeps showing its body"
        );
    }
}
