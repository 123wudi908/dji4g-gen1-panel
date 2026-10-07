//! Read-only radio observations. Only completed AT samples enter the bounded history.
use super::{
    HotspotAction, hotspot_vm, meta_text, scale, section_frame, section_heading, wrapped_label,
};
use crate::app::PanelCommandSink;
use crate::localization::{Language, LocalizedText, TextKey};
use dji4g_application::{ControllerSnapshot, DiagnosticCheckId, DiagnosticCheckState, UiCommand};
use dji4g_domain::{ActionKind, DeviceEpoch, ServingCell};
use eframe::egui::{self, RichText, Ui};
use std::{collections::VecDeque, time::SystemTime};

/// One catalog string in the language this page was rendered with.
fn t(language: crate::localization::Language, key: crate::localization::TextKey) -> String {
    crate::localization::LocalizedText::new(language, key).text
}

#[derive(Clone)]
struct Sample {
    at: SystemTime,
    cell: Option<ServingCell>,
}
struct CellChange {
    at: SystemTime,
    before: String,
    after: String,
}
#[derive(Default)]
pub(crate) struct WirelessHistory {
    context: Option<(DeviceEpoch, u64)>,
    samples: VecDeque<Sample>,
    changes: VecDeque<CellChange>,
    previous_cell: Option<String>,
}
impl WirelessHistory {
    #[cfg(debug_assertions)]
    pub(crate) fn review_fixture(&mut self) {
        let now = SystemTime::now();
        for i in 0..60u64 {
            let cell: ServingCell = serde_json::from_value(serde_json::json!({
                "state":"NOCONN","duplex":"FDD","rat":"LTE","mcc":"460","mnc":"01",
                "cell_id": if i < 40 { 123456 } else { 123457 }, "pci": if i < 40 { 123 } else { 124 },
                "earfcn":1650,"band":3,"ul_mhz":20.0,"dl_mhz":20.0,"tac":2748,
                "rsrp_dbm": -99 + (i % 12) as i16, "rsrq_db":-10,"rssi_dbm":-65,"sinr_db":15
            })).expect("static visual fixture");
            self.push(
                (DeviceEpoch(1), 0),
                now - std::time::Duration::from_secs((59 - i) * 3),
                Some(cell),
            );
        }
    }
    pub(crate) fn observe(&mut self, snapshot: &ControllerSnapshot) {
        let Some(check) = snapshot
            .diagnostics
            .iter()
            .find(|c| c.id == DiagnosticCheckId::AtControl)
        else {
            return;
        };
        let context = (check.epoch, snapshot.sim_epoch);
        if self.context != Some(context) {
            self.reset(context);
        }
        if matches!(
            check.state,
            DiagnosticCheckState::Running { .. } | DiagnosticCheckState::Unexecuted { .. }
        ) {
            return;
        }
        let Some(at) = check.finished_at else {
            return;
        };
        let cell = if check.state == DiagnosticCheckState::Passed {
            snapshot
                .app
                .cellular
                .as_ref()
                .and_then(|c| c.serving_cell.clone())
        } else {
            None
        };
        self.push(context, at, cell);
    }
    fn reset(&mut self, context: (DeviceEpoch, u64)) {
        self.context = Some(context);
        self.samples.clear();
        self.changes.clear();
        self.previous_cell = None;
    }
    fn push(&mut self, context: (DeviceEpoch, u64), at: SystemTime, cell: Option<ServingCell>) {
        if self.context != Some(context) {
            self.reset(context);
        }
        if self.samples.back().is_some_and(|s| s.at >= at) {
            return;
        }
        if let Some(cell) = cell
            .as_ref()
            .filter(|c| c.cell_id.is_some() || c.pci.is_some())
        {
            let identity = cell_identity(cell);
            if let Some(previous) = self.previous_cell.as_ref().filter(|old| **old != identity) {
                self.changes.push_back(CellChange {
                    at,
                    before: previous.clone(),
                    after: identity.clone(),
                });
                if self.changes.len() > 20 {
                    self.changes.pop_front();
                }
            }
            self.previous_cell = Some(identity);
        }
        self.samples.push_back(Sample { at, cell });
        if self.samples.len() > 120 {
            self.samples.pop_front();
        }
    }
}
fn cell_identity(cell: &ServingCell) -> String {
    format!(
        "{}-{} · Cell {} · PCI {} · EARFCN {} · B{}",
        cell.mcc.as_deref().unwrap_or("?"),
        cell.mnc.as_deref().unwrap_or("?"),
        cell.cell_id
            .map(|v| format!("{v:X}"))
            .unwrap_or_else(|| "?".into()),
        value(cell.pci),
        value(cell.earfcn),
        value(cell.band)
    )
}
fn value<T: std::fmt::Display>(value: Option<T>) -> String {
    value.map(|v| v.to_string()).unwrap_or_else(|| "—".into())
}
fn measurement(value: Option<i16>, unit: &str, language: Language) -> String {
    value
        .map(|v| format!("{v} {unit}"))
        .unwrap_or_else(|| t(language, crate::localization::TextKey::ValueNotReported))
}
fn summary(cell: &ServingCell, language: Language) -> String {
    let identity = cell_identity(cell);
    let rat = cell.rat.clone().unwrap_or_else(|| "—".into());
    let duplex = cell.duplex.clone().unwrap_or_else(|| "—".into());
    let rsrp = measurement(cell.rsrp_dbm, "dBm", language);
    let rsrq = measurement(cell.rsrq_db, "dB", language);
    let rssi = measurement(cell.rssi_dbm, "dBm", language);
    let sinr = value(cell.sinr_raw);
    let ul = value(cell.ul_mhz);
    let dl = value(cell.dl_mhz);
    let tac = cell
        .tac
        .map(|v| format!("{v:04X}"))
        .unwrap_or_else(|| "—".into());
    crate::localization::format_positional(
        language,
        crate::localization::TextKey::WirelessSummary,
        &[
            &identity, &rat, &duplex, &rsrp, &rsrq, &rssi, &sinr, &ul, &dl, &tac,
        ],
    )
}
fn metric(ui: &mut Ui, name: &str, value: String, note: &str) {
    egui::Frame::none()
        .fill(scale::surface_sunken())
        .rounding(10.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(meta_text(name));
            ui.label(RichText::new(value).size(21.0).strong().color(scale::ink()));
            ui.label(meta_text(note));
        });
}
/// The 无线 page: the radio observations, plus the one operation that belongs to the wireless side
/// of the panel — the Windows hotspot the module feeds.
pub(crate) fn render_page(
    ui: &mut Ui,
    snapshot: &ControllerSnapshot,
    history: &WirelessHistory,
    language: Language,
    sink: &dyn PanelCommandSink,
) {
    super::components::page_heading(
        ui,
        crate::localization::template(language, crate::localization::TextKey::NavWireless),
        &t(language, crate::localization::TextKey::WirelessIntro),
    );
    render(ui, snapshot, history, language);
    ui.add_space(scale::BLOCK_GAP);
    render_hotspot(ui, snapshot, language, sink);
}

/// The hotspot card. It lives here rather than on the overview because switching the hotspot on or
/// off is an operation: the overview reads, the pages act.
fn render_hotspot(
    ui: &mut Ui,
    snapshot: &ControllerSnapshot,
    language: Language,
    sink: &dyn PanelCommandSink,
) {
    let vm = hotspot_vm(snapshot.app.as_ref().hotspot, language);
    section_frame(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(section_heading(
                LocalizedText::new(language, TextKey::FieldHotspot).text,
            ));
            ui.colored_label(vm.tone.color(), &vm.status.text);
        });
        if !matches!(vm.reason.key, TextKey::ValueNotApplicable) {
            wrapped_label(ui, meta_text(vm.reason.text.clone()));
        }
        if let Some(action) = vm.action {
            // A failed hotspot offers 重试, which is a plain refresh (re-observe the hotspot state
            // and the rest of the evidence) — not a repair plan: `ActionKind::Refresh` is
            // deliberately not a confirmable action, so it must go through the refresh signal.
            if action == HotspotAction::Retry {
                if ui
                    .add_enabled(
                        !vm.busy,
                        egui::Button::new(LocalizedText::new(language, TextKey::ButtonRetry).text),
                    )
                    .clicked()
                {
                    let _ = sink.try_send(UiCommand::Refresh);
                }
                return;
            }
            let (label, command) = match action {
                HotspotAction::Enable => (
                    LocalizedText::new(language, TextKey::ActionEnableHotspot).text,
                    ActionKind::ToggleHotspot { enabled: true },
                ),
                HotspotAction::Disable => (
                    LocalizedText::new(language, TextKey::ActionDisableHotspot).text,
                    ActionKind::ToggleHotspot { enabled: false },
                ),
                HotspotAction::Retry => unreachable!("handled above"),
            };
            if ui.add_enabled(!vm.busy, egui::Button::new(label)).clicked() {
                sink.prepare_action_now(command);
            }
        } else if vm.busy {
            wrapped_label(
                ui,
                meta_text(LocalizedText::new(language, TextKey::StatusLoading).text),
            );
        }
    });
}

pub(crate) fn render(
    ui: &mut Ui,
    snapshot: &ControllerSnapshot,
    history: &WirelessHistory,
    language: Language,
) {
    let cell = history.samples.back().and_then(|s| s.cell.as_ref());
    let check = snapshot
        .diagnostics
        .iter()
        .find(|c| c.id == DiagnosticCheckId::AtControl);
    let fresh = check.is_some_and(|c| {
        c.freshness(SystemTime::now()) == dji4g_domain::Freshness::Fresh
            && c.state == DiagnosticCheckState::Passed
    });
    section_frame(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(section_heading(t(
                language,
                crate::localization::TextKey::WirelessServingCell,
            )));
            ui.label(meta_text(if fresh {
                t(language, crate::localization::TextKey::WirelessSampleFresh)
            } else {
                t(
                    language,
                    crate::localization::TextKey::WirelessSampleWaiting,
                )
            }));
            if let Some(cell) = cell {
                if ui
                    .button(t(
                        language,
                        crate::localization::TextKey::WirelessCopySummary,
                    ))
                    .clicked()
                {
                    ui.ctx().copy_text(summary(cell, language));
                }
            }
        });
        let Some(cell) = cell else {
            ui.add_space(8.0);
            wrapped_label(
                ui,
                t(language, crate::localization::TextKey::WirelessNoCell),
            );
            return;
        };
        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            let band = value(cell.band);
            for item in [
                crate::localization::format_positional(
                    language,
                    crate::localization::TextKey::WirelessBand,
                    &[&band],
                ),
                format!(
                    "{} / {}",
                    cell.rat.as_deref().unwrap_or("—"),
                    cell.duplex.as_deref().unwrap_or("—")
                ),
                format!("PCI {}", value(cell.pci)),
                format!("EARFCN {}", value(cell.earfcn)),
            ] {
                ui.label(RichText::new(item).color(scale::ink()).strong());
                ui.add_space(10.0);
            }
        });
        ui.add_space(10.0);
        let fields = [
            (
                "RSRP",
                measurement(cell.rsrp_dbm, "dBm", language),
                t(language, crate::localization::TextKey::WirelessRsrpNote),
            ),
            (
                "RSRQ",
                measurement(cell.rsrq_db, "dB", language),
                t(language, crate::localization::TextKey::WirelessRsrqNote),
            ),
            (
                "RSSI",
                measurement(cell.rssi_dbm, "dBm", language),
                t(language, crate::localization::TextKey::WirelessRssiNote),
            ),
            (
                &t(language, crate::localization::TextKey::WirelessSinrNote),
                value(cell.sinr_raw),
                t(language, crate::localization::TextKey::WirelessSinrUnit),
            ),
        ];
        let columns = if ui.available_width() >= 680.0 { 4 } else { 2 };
        for chunk in fields.chunks(columns) {
            ui.columns(columns, |cols| {
                for (i, (label, v, note)) in chunk.iter().enumerate() {
                    metric(&mut cols[i], label, v.clone(), note);
                }
            });
            ui.add_space(8.0);
        }
        // Always open: sections are cards, not accordions.
        ui.label(section_heading(t(
            language,
            crate::localization::TextKey::WirelessCellDetails,
        )));
        {
            wrapped_label(ui, cell_identity(cell));
            let ul = value(cell.ul_mhz);
            let dl = value(cell.dl_mhz);
            ui.label(crate::localization::format_positional(
                language,
                crate::localization::TextKey::WirelessBandwidthLine,
                &[&ul, &dl],
            ));
            let tac = cell
                .tac
                .map(|v| format!("{v:04X}"))
                .unwrap_or_else(|| "—".into());
            let state = cell
                .state
                .clone()
                .unwrap_or_else(|| t(language, crate::localization::TextKey::ValueNotReported));
            ui.label(crate::localization::format_positional(
                language,
                crate::localization::TextKey::WirelessTacLine,
                &[&tac, &state],
            ));
            ui.label(meta_text(t(
                language,
                crate::localization::TextKey::WirelessNoconnNote,
            )));
        }
    });
    section_frame(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(section_heading(t(
                language,
                crate::localization::TextKey::WirelessSignalHeading,
            )));
            let samples = history.samples.len().to_string();
            ui.label(meta_text(crate::localization::format_positional(
                language,
                crate::localization::TextKey::WirelessSampleCount,
                &[&samples],
            )));
        });
        signal_chart(ui, history, language);
        ui.label(meta_text(t(
            language,
            crate::localization::TextKey::WirelessSignalNote,
        )));
    });
    section_frame(ui, |ui| {
        let changes = history.changes.len().to_string();
        ui.label(section_heading(crate::localization::format_positional(
            language,
            crate::localization::TextKey::WirelessChangeHeading,
            &[&changes],
        )));
        ui.label(meta_text(t(
            language,
            crate::localization::TextKey::WirelessChangeNote,
        )));
        if history.changes.is_empty() {
            ui.label(t(language, crate::localization::TextKey::WirelessNoChange));
        }
        for change in history.changes.iter().rev() {
            ui.separator();
            let seconds = SystemTime::now()
                .duration_since(change.at)
                .unwrap_or_default()
                .as_secs()
                .to_string();
            ui.label(meta_text(crate::localization::format_positional(
                language,
                crate::localization::TextKey::WirelessSecondsAgo,
                &[&seconds],
            )));
            wrapped_label(
                ui,
                crate::localization::format_positional(
                    language,
                    crate::localization::TextKey::WirelessPreviousCell,
                    &[&change.before],
                ),
            );
            wrapped_label(
                ui,
                crate::localization::format_positional(
                    language,
                    crate::localization::TextKey::WirelessNewCell,
                    &[&change.after],
                ),
            );
        }
    });
}
fn signal_chart(ui: &mut Ui, history: &WirelessHistory, language: Language) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 170.0),
        egui::Sense::hover(),
    );
    let plot = rect.shrink2(egui::vec2(40.0, 15.0));
    let painter = ui.painter_at(rect);
    for dbm in [-140, -110, -80, -50] {
        let y = plot.bottom() - ((dbm + 140) as f32 / 96.0) * plot.height();
        painter.line_segment(
            [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
            egui::Stroke::new(1.0_f32, scale::line()),
        );
        painter.text(
            egui::pos2(plot.left() - 8.0, y),
            egui::Align2::RIGHT_CENTER,
            dbm.to_string(),
            egui::FontId::proportional(11.0),
            scale::secondary(),
        );
    }
    let mut previous = None;
    let mut count = 0;
    for (i, sample) in history.samples.iter().enumerate() {
        if let Some(rsrp) = sample.cell.as_ref().and_then(|c| c.rsrp_dbm) {
            count += 1;
            let x = plot.left()
                + i as f32 / history.samples.len().saturating_sub(1).max(1) as f32 * plot.width();
            let y =
                plot.bottom() - ((f32::from(rsrp) + 140.0) / 96.0).clamp(0.0, 1.0) * plot.height();
            let point = egui::pos2(x, y);
            if let Some(previous) = previous {
                painter.line_segment(
                    [previous, point],
                    egui::Stroke::new(2.0_f32, scale::DOWNLOAD),
                );
            }
            painter.circle_filled(point, 2.0, scale::DOWNLOAD);
            previous = Some(point);
        } else {
            previous = None;
        }
    }
    if count == 0 {
        painter.text(
            plot.center(),
            egui::Align2::CENTER_CENTER,
            t(language, crate::localization::TextKey::WirelessWaitingRsrp),
            egui::FontId::proportional(14.0),
            scale::secondary(),
        );
    }
    painter.text(
        rect.right_top(),
        egui::Align2::RIGHT_TOP,
        "dBm",
        egui::FontId::proportional(11.0),
        scale::secondary(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cell(id: u32) -> ServingCell {
        serde_json::from_value(serde_json::json!({"rat":"LTE","pci":1,"earfcn":1650,"band":3,"rsrp_dbm":-95,"rsrq_db":-10,"cell_id":id})).unwrap()
    }
    #[test]
    fn history_deduplicates_samples_breaks_gaps_and_resets_context() {
        let mut h = WirelessHistory::default();
        let ctx = (DeviceEpoch(1), 0);
        let at = SystemTime::UNIX_EPOCH;
        h.push(ctx, at, Some(cell(1)));
        h.push(ctx, at, Some(cell(2)));
        assert_eq!(h.samples.len(), 1);
        assert!(h.changes.is_empty());
        h.push(ctx, at + std::time::Duration::from_secs(1), None);
        assert!(h.samples.back().unwrap().cell.is_none());
        h.push(ctx, at + std::time::Duration::from_secs(2), Some(cell(2)));
        assert_eq!(h.changes.len(), 1);
        h.push(
            (DeviceEpoch(1), 1),
            at + std::time::Duration::from_secs(3),
            Some(cell(3)),
        );
        assert_eq!(h.samples.len(), 1);
        assert!(h.changes.is_empty());
    }
    #[test]
    fn history_and_changes_are_bounded_and_sinr_stays_raw() {
        let mut h = WirelessHistory::default();
        for i in 0..150 {
            h.push(
                (DeviceEpoch(1), 0),
                SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(i),
                Some(cell(i as u32)),
            );
        }
        assert_eq!(h.samples.len(), 120);
        assert_eq!(h.changes.len(), 20);
        let mut c = cell(1);
        c.sinr_raw = Some(15);
        assert!(summary(&c, Language::ZhCn).contains("SINR 原始值 15（单位未确认）"));
    }
}
