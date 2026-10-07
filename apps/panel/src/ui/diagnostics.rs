//! Layered evidence diagnostics. The fixed order mirrors the reducer's nine checks.

use dji4g_application::{ControllerSnapshot, DiagnosticCheckId, UiCommand};
use dji4g_domain::{BoundDnsStatus, BoundPublicStatus, DefaultRouteOwner};
use eframe::egui::{RichText, Ui};

use super::{
    DiagnosticStateVm, DisplayValue, carrier_display_name, diagnostic_state_vm, scale,
    wrapped_label,
};
use crate::app::UiCommandSink;
use crate::localization::{Language, LocalizedText, TextKey, default_route_owner, diagnostic_id};

/// One catalog string in the language this page was rendered with.
fn t(language: crate::localization::Language, key: crate::localization::TextKey) -> String {
    crate::localization::LocalizedText::new(language, key).text
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticRowVm {
    pub id: DiagnosticCheckId,
    pub label: LocalizedText,
    pub state: DiagnosticStateVm,
    pub detail: Option<DisplayValue>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticsVm {
    pub title: LocalizedText,
    pub intro: LocalizedText,
    pub rows: Vec<DiagnosticRowVm>,
}

#[must_use]
pub fn diagnostics_vm(snapshot: &ControllerSnapshot, language: Language) -> DiagnosticsVm {
    let rows = snapshot
        .diagnostics
        .iter()
        .map(|check| DiagnosticRowVm {
            id: check.id,
            label: LocalizedText::new(language, diagnostic_id(check.id)),
            state: diagnostic_state_vm(&check.state, language),
            detail: detail_for(check.id, snapshot, language),
        })
        .collect();
    DiagnosticsVm {
        title: LocalizedText::new(language, TextKey::DiagnosticsTitle),
        intro: LocalizedText::new(language, TextKey::DiagnosticsIntro),
        rows,
    }
}

fn detail_for(
    id: DiagnosticCheckId,
    snapshot: &ControllerSnapshot,
    language: Language,
) -> Option<DisplayValue> {
    let app = snapshot.app.as_ref();
    let network = app.network.as_ref();
    let cellular = app.cellular.as_ref();
    match id {
        DiagnosticCheckId::UsbDevice => app.device.as_ref().map_or_else(
            || {
                Some(DisplayValue::new(t(
                    language,
                    crate::localization::TextKey::ValueNotAvailable,
                )))
            },
            |device| {
                let problem = device.problem_code.map_or_else(
                    || t(language, crate::localization::TextKey::DiagNoProblemCode),
                    |code| {
                        crate::localization::format_positional(
                            language,
                            crate::localization::TextKey::DiagProblemCode,
                            &[&code.to_string()],
                        )
                    },
                );
                Some(DisplayValue::new(problem))
            },
        ),
        DiagnosticCheckId::AtControl => app
            .device
            .as_ref()
            .and_then(|device| {
                device.at_port.as_ref().map(|port| {
                    DisplayValue::new(crate::localization::format_positional(
                        language,
                        crate::localization::TextKey::DiagPort,
                        &[&port.to_string()],
                    ))
                })
            })
            .or_else(|| {
                Some(DisplayValue::new(t(
                    language,
                    crate::localization::TextKey::ValueNotAvailable,
                )))
            }),
        DiagnosticCheckId::Cellular => cellular.map_or_else(
            || {
                Some(DisplayValue::new(t(
                    language,
                    crate::localization::TextKey::ValueNotAvailable,
                )))
            },
            |cellular| {
                let carrier = cellular
                    .carrier
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .map_or_else(
                        || t(language, crate::localization::TextKey::DiagCarrierNotRead),
                        str::to_owned,
                    );
                let rat = cellular
                    .radio_access_technology
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .map_or_else(
                        || t(language, crate::localization::TextKey::DiagRatNotRead),
                        str::to_owned,
                    );
                Some(DisplayValue::new(format!(
                    "{} · {rat}",
                    carrier_display_name(&carrier, language)
                )))
            },
        ),
        DiagnosticCheckId::WindowsAdapter => network.map_or_else(
            || {
                Some(DisplayValue::new(t(
                    language,
                    crate::localization::TextKey::ValueNotAvailable,
                )))
            },
            |network| {
                Some(DisplayValue::copyable(preview_values(
                    &network.addresses,
                    language,
                )))
            },
        ),
        DiagnosticCheckId::BoundRoute => network.map_or_else(
            || {
                Some(DisplayValue::new(t(
                    language,
                    crate::localization::TextKey::ValueNotAvailable,
                )))
            },
            |network| {
                let gateways = preview_values(&network.gateways, language);
                Some(DisplayValue::new(crate::localization::format_positional(
                    language,
                    crate::localization::TextKey::DiagRouteProbeNote,
                    &[&gateways],
                )))
            },
        ),
        DiagnosticCheckId::BoundPublic => network.map_or_else(
            || {
                Some(DisplayValue::new(t(
                    language,
                    crate::localization::TextKey::ValueNotAvailable,
                )))
            },
            |network| {
                Some(DisplayValue::new(bound_public_detail(
                    network.bound_public,
                    language,
                )))
            },
        ),
        DiagnosticCheckId::BoundDns => network.map_or_else(
            || {
                Some(DisplayValue::new(t(
                    language,
                    crate::localization::TextKey::ValueNotAvailable,
                )))
            },
            |network| {
                Some(DisplayValue::new(bound_dns_detail(
                    network.bound_dns,
                    language,
                )))
            },
        ),
        DiagnosticCheckId::SystemRoute => network.map_or_else(
            || {
                Some(DisplayValue::new(t(
                    language,
                    crate::localization::TextKey::ValueNotAvailable,
                )))
            },
            |network| {
                let mut text =
                    LocalizedText::new(language, default_route_owner(network.system_default_route))
                        .text;
                if matches!(network.system_default_route, DefaultRouteOwner::VpnOrTun) {
                    text.push_str(&t(
                        language,
                        crate::localization::TextKey::DiagProxyInterfaceNote,
                    ));
                }
                Some(DisplayValue::new(text))
            },
        ),
        DiagnosticCheckId::Hotspot => Some(DisplayValue::new(
            super::hotspot_vm(app.hotspot, language).status.text,
        )),
    }
}

fn preview_values(values: &[String], language: Language) -> String {
    if values.is_empty() {
        return t(language, crate::localization::TextKey::ValueNotAvailable);
    }
    let output = values
        .iter()
        .take(2)
        .cloned()
        .collect::<Vec<_>>()
        .join("、");
    if values.len() > 2 {
        let more = (values.len() - 2).to_string();
        crate::localization::format_positional(
            language,
            crate::localization::TextKey::ValuePreviewMore,
            &[&output, &more],
        )
    } else {
        output
    }
}

fn bound_public_detail(value: BoundPublicStatus, language: Language) -> String {
    match value {
        BoundPublicStatus::Succeeded => t(language, crate::localization::TextKey::CheckPassed),
        BoundPublicStatus::Incomplete => t(language, crate::localization::TextKey::DiagIncomplete),
        BoundPublicStatus::Failed { consecutive_cycles } => {
            let count = consecutive_cycles.to_string();
            crate::localization::format_positional(
                language,
                crate::localization::TextKey::DiagFailedRepeatedly,
                &[&count],
            )
        }
    }
}

fn bound_dns_detail(value: BoundDnsStatus, language: Language) -> String {
    match value {
        BoundDnsStatus::Succeeded => t(language, crate::localization::TextKey::CheckPassed),
        BoundDnsStatus::Failed => t(language, crate::localization::TextKey::DiagResolutionFailed),
        BoundDnsStatus::Incomplete => t(language, crate::localization::TextKey::DiagIncomplete),
    }
}

pub(crate) fn render(
    ui: &mut Ui,
    snapshot: &ControllerSnapshot,
    language: Language,
    sink: &dyn UiCommandSink,
) {
    let vm = diagnostics_vm(snapshot, language);
    wrapped_label(
        ui,
        RichText::new(vm.intro.text.clone())
            .size(scale::RATE_AUX)
            .color(scale::secondary()),
    );
    ui.add_space(8.0);
    // The page's own action only. 刷新 is the panel-wide action and lives with the overview, next
    // to 更多; repeating it here put two identical 刷新 buttons on one screen.
    let mut export_requested = false;
    let export_label = TextKey::ButtonExportDiagnostics.to_string(language);
    if super::components::action_button(
        ui,
        &export_label,
        super::components::ButtonKind::Outlined,
        true,
        None,
    )
    .clicked()
    {
        export_requested = true;
    }
    if export_requested {
        let _ = sink.try_send(UiCommand::ExportDiagnostics);
    }
    ui.add_space(16.0);
    super::network_assistance::render(ui, snapshot, std::time::SystemTime::now(), language, sink);
    ui.add_space(12.0);
    super::section_frame(ui, |ui| {
        // Nine always-visible checks in two columns: a full-width single column left most of the
        // card empty, and the pairs read as a scan-friendly grid at desktop width.
        let half = vm.rows.len().div_ceil(2);
        let (left, right) = vm.rows.split_at(half);
        super::two_columns(ui, |ui| render_rows(ui, left), |ui| render_rows(ui, right));
    });
}

/// One column of checks. Every check is an always-visible row: the tone-coloured line is the row's
/// heading and its evidence is never hidden behind a disclosure triangle.
fn render_rows(ui: &mut Ui, rows: &[DiagnosticRowVm]) {
    for (index, row) in rows.iter().enumerate() {
        ui.label(
            super::section_heading(format!(
                "{}    {} {}",
                row.label.text,
                row.state.tone.marker(),
                row.state.label.text
            ))
            .color(row.state.tone.color()),
        );
        if let Some(detail) = &row.state.detail {
            wrapped_label(ui, super::detail_text(&detail.text));
        }
        if let Some(detail) = &row.detail {
            wrapped_label(ui, super::detail_text(&detail.text));
        }
        if index + 1 < rows.len() {
            ui.separator();
        }
    }
}

trait LocalizedKeyText {
    fn to_string(self, language: Language) -> String;
}

impl LocalizedKeyText for TextKey {
    fn to_string(self, language: Language) -> String {
        crate::localization::LocalizedText::new(language, self).text
    }
}
