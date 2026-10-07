//! One explicit, module-bound check. Host routing is explanatory only.
use super::StatusTone;
use crate::{app::PanelCommandSink, localization::Language};
use dji4g_application::{
    ControllerSnapshot, DefaultRouteDto, ModuleNetworkCheckPhase as Phase,
    ModuleNetworkCheckSnapshot, NetworkRepairKind, UiCommand,
};
use dji4g_domain::{ModuleNetworkVerdict as Verdict, NetworkEvidenceState as Evidence};
use eframe::egui::{self, RichText};
use std::time::SystemTime;

/// One catalog string in the language this page was rendered with.
fn t(language: crate::localization::Language, key: crate::localization::TextKey) -> String {
    crate::localization::LocalizedText::new(language, key).text
}

#[must_use]
pub fn conclusion(check: &ModuleNetworkCheckSnapshot, language: Language) -> (StatusTone, String) {
    match check.phase {
        Phase::Queued => (
            StatusTone::Progress,
            t(language, crate::localization::TextKey::MNCQueued),
        ),
        Phase::Running => (
            StatusTone::Progress,
            t(language, crate::localization::TextKey::MNCChecking),
        ),
        Phase::Stale => (
            StatusTone::Neutral,
            t(language, crate::localization::TextKey::MNCStale),
        ),
        Phase::Finished => match check.verdict {
            Verdict::Usable => (
                StatusTone::Positive,
                t(language, crate::localization::TextKey::MNCAvailable),
            ),
            Verdict::DeviceMissing => (
                StatusTone::Caution,
                t(language, crate::localization::TextKey::MNCNoDevice),
            ),
            Verdict::AdapterIssue => (
                StatusTone::Caution,
                t(language, crate::localization::TextKey::MNCAdapterFailed),
            ),
            Verdict::AddressRouteIssue => (
                StatusTone::Caution,
                t(language, crate::localization::TextKey::MNCAdapterNoAddress),
            ),
            Verdict::LinkDown => (
                StatusTone::Caution,
                t(language, crate::localization::TextKey::MNCAdapterLinkDown),
            ),
            Verdict::BoundRouteIssue => (
                StatusTone::Caution,
                t(language, crate::localization::TextKey::MNCBoundRouteFailed),
            ),
            Verdict::PublicProbeFailed => (
                StatusTone::Caution,
                t(language, crate::localization::TextKey::MNCBoundPublicFailed),
            ),
            Verdict::DnsIssue => (
                StatusTone::Caution,
                t(language, crate::localization::TextKey::MNCBoundDnsFailed),
            ),
            Verdict::Inconclusive => (
                StatusTone::Neutral,
                t(language, crate::localization::TextKey::MNCInconclusive),
            ),
        },
    }
}
fn evidence_label(state: Evidence, language: Language) -> String {
    match state {
        Evidence::NotRun => t(
            language,
            crate::localization::TextKey::MNCEvidenceUnexecuted,
        ),
        Evidence::Running => t(language, crate::localization::TextKey::MNCEvidenceRunning),
        Evidence::Passed => t(language, crate::localization::TextKey::MNCEvidencePassed),
        Evidence::Failed => t(language, crate::localization::TextKey::MNCEvidenceFailed),
        Evidence::Unavailable => t(
            language,
            crate::localization::TextKey::MNCEvidenceUnavailable,
        ),
        Evidence::Stale => t(language, crate::localization::TextKey::MNCEvidenceExpired),
    }
}
fn repair_label(kind: NetworkRepairKind, language: Language) -> String {
    match kind {
        NetworkRepairKind::RenewDhcp => t(language, crate::localization::TextKey::MNCDhcpLease),
        NetworkRepairKind::RestartAdapter => {
            t(language, crate::localization::TextKey::MNCRestartAdapter)
        }
        NetworkRepairKind::AutomaticDns => {
            t(language, crate::localization::TextKey::MNCAutomaticDns)
        }
    }
}

/// Returns true when the user asks to open the existing driver/repair guidance.
pub fn render(
    ui: &mut egui::Ui,
    snapshot: &ControllerSnapshot,
    now: SystemTime,
    language: Language,
    sink: &dyn PanelCommandSink,
) -> bool {
    render_impl(ui, snapshot, now, language, sink)
}

fn render_impl(
    ui: &mut egui::Ui,
    snapshot: &ControllerSnapshot,
    now: SystemTime,
    language: Language,
    sink: &dyn PanelCommandSink,
) -> bool {
    let mut guidance = false;
    let confirm_id = egui::Id::new("module-network-probe-consent");
    let error_id = egui::Id::new("module-network-submit-error");
    let mut consent = ui
        .ctx()
        .data_mut(|d| d.get_temp::<bool>(confirm_id).unwrap_or(false));
    let mut error = ui
        .ctx()
        .data_mut(|d| d.get_temp::<bool>(error_id).unwrap_or(false));
    let active = snapshot
        .module_network_check
        .as_ref()
        .is_some_and(|c| c.active());
    let frame = egui::Frame::none()
        .fill(super::scale::surface())
        .rounding(12.0)
        .inner_margin(12.0);
    frame.show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.horizontal_wrapped(|ui| {
            ui.label(super::section_heading(t(
                language,
                crate::localization::TextKey::MNCHeading,
            )));
            let run_action = t(language, crate::localization::TextKey::MNCRunAction);
            let running_reason = t(language, crate::localization::TextKey::MNCRunningReason);
            if super::components::action_button(
                ui,
                &run_action,
                super::components::ButtonKind::Filled,
                !active,
                Some(&running_reason),
            )
            .clicked()
            {
                if snapshot.settings.active_probe {
                    error = sink
                        .try_send(UiCommand::CheckModuleNetwork {
                            allow_probe_once: false,
                        })
                        .is_err();
                } else {
                    consent = true;
                }
            }
            if active {
                super::components::loading_spinner(ui);
            }
        });
        let mut details = |ui: &mut egui::Ui| {
            if let Some(check) = snapshot.module_network_check.as_ref() {
                let (tone, text) = conclusion(check, language);
                super::wrapped_label(
                    ui,
                    RichText::new(format!("{} {text}", tone.marker())).color(tone.color()),
                );
                if let Some(outcome) = &check.operation_outcome {
                    super::wrapped_label(
                        ui,
                        crate::localization::format_positional(
                            language,
                            crate::localization::TextKey::MNCOperationResult,
                            &[&super::operation_outcome_text(outcome, language).text],
                        ),
                    );
                    ui.label(t(
                        language,
                        crate::localization::TextKey::MNCReadOnlyReverify,
                    ));
                }
                if check.phase == Phase::Running {
                    let current = if check.evidence.device != Evidence::Passed {
                        t(language, crate::localization::TextKey::MNCStepUsb)
                    } else if check.adapter.is_none() {
                        t(language, crate::localization::TextKey::MNCStepPorts)
                    } else {
                        t(language, crate::localization::TextKey::MNCStepRoute)
                    };
                    super::wrapped_label(
                        ui,
                        crate::localization::format_positional(
                            language,
                            crate::localization::TextKey::MNCStepCurrent,
                            &[&current],
                        ),
                    );
                }
                for repair in check.recommended_repairs() {
                    let gate = super::action_availability::repair_action_availability(
                        snapshot,
                        repair.readiness_key(),
                        now,
                        language,
                    );
                    let enabled = check.fresh(
                        snapshot
                            .app
                            .device
                            .as_ref()
                            .map_or(dji4g_domain::DeviceEpoch(0), |d| d.epoch),
                        now,
                    ) && gate.enabled;
                    if ui
                        .add_enabled(
                            enabled,
                            egui::Button::new(repair_label(repair, language))
                                .min_size(egui::vec2(0.0, 32.0)),
                        )
                        .clicked()
                    {
                        sink.prepare_network_repair_now(check.request_id, repair);
                    }
                    if !enabled {
                        super::wrapped_label(
                            ui,
                            super::meta_text(gate.reason.map(|r| r.text).unwrap_or_else(|| {
                                t(language, crate::localization::TextKey::MNCResultExpired)
                            })),
                        );
                    }
                }
                if check.phase == Phase::Finished
                    && matches!(check.verdict, Verdict::AdapterIssue | Verdict::Inconclusive)
                    && check.evidence.device == Evidence::Passed
                    && check.adapter.is_none()
                {
                    super::wrapped_label(
                        ui,
                        t(language, crate::localization::TextKey::MNCDeviceNoAdapter),
                    );
                    if ui
                        .add(
                            egui::Button::new(t(
                                language,
                                crate::localization::TextKey::MNCViewSteps,
                            ))
                            .min_size(egui::vec2(0.0, 32.0)),
                        )
                        .clicked()
                    {
                        guidance = true;
                    }
                }
                if check.verdict == Verdict::PublicProbeFailed {
                    super::wrapped_label(ui, t(language, crate::localization::TextKey::MNCSimHint));
                }
                if check.phase == Phase::Finished && check.evidence.public == Evidence::NotRun {
                    super::wrapped_label(
                        ui,
                        t(language, crate::localization::TextKey::MNCNoPublicProbe),
                    );
                }
                if let Some(probe) = check.probe.as_ref() {
                    for route in &probe.route_choices {
                        let description = match route.owner {
                            Some(DefaultRouteDto::TargetAdapter) => {
                                t(language, crate::localization::TextKey::MNCEgressAdapter)
                            }
                            Some(DefaultRouteDto::VpnOrTun) => {
                                t(language, crate::localization::TextKey::MNCEgressProxy)
                            }
                            Some(_) => t(language, crate::localization::TextKey::MNCEgressOther),
                            None => t(language, crate::localization::TextKey::MNCEgressUnknown),
                        };
                        super::wrapped_label(
                            ui,
                            super::meta_text(crate::localization::format_positional(
                                language,
                                crate::localization::TextKey::MNCEgressPath,
                                &[&format!("{:?}", route.family), &description],
                            )),
                        );
                    }
                }
                let evidence = |ui: &mut egui::Ui| {
                    // The evidence rows are supporting detail that used to hide behind a disclosure
                    // triangle, so they sit in the quiet evidence tier now that they are always shown.
                    for (name, state) in [
                        (
                            t(language, crate::localization::TextKey::MNCProbeUsb),
                            check.evidence.device,
                        ),
                        (
                            t(language, crate::localization::TextKey::MNCProbeAdapterRead),
                            check.evidence.adapter,
                        ),
                        (
                            t(language, crate::localization::TextKey::MNCProbeLink),
                            check.evidence.link,
                        ),
                        (
                            t(language, crate::localization::TextKey::MNCProbeAddressRoute),
                            check.evidence.address_route,
                        ),
                        (
                            t(language, crate::localization::TextKey::MNCProbeBoundRoute),
                            check.evidence.bound_route,
                        ),
                        (
                            t(language, crate::localization::TextKey::MNCProbeBoundPublic),
                            check.evidence.public,
                        ),
                        (
                            t(language, crate::localization::TextKey::MNCProbeBoundDns),
                            check.evidence.dns,
                        ),
                    ] {
                        ui.label(super::detail_text(format!(
                            "{name}：{}",
                            evidence_label(
                                if check.phase == Phase::Stale {
                                    Evidence::Stale
                                } else {
                                    state
                                },
                                language,
                            )
                        )));
                    }
                    if let Some(adapter) = check.adapter.as_ref() {
                        if let Some(details) = adapter.details.as_ref() {
                            let link = if details.link_up {
                                t(language, crate::localization::TextKey::MNCConnected)
                            } else {
                                t(language, crate::localization::TextKey::MNCDisconnected)
                            };
                            let dhcp = if details.dhcp_v4 {
                                t(language, crate::localization::TextKey::MNCEnabled)
                            } else {
                                t(language, crate::localization::TextKey::MNCDisabled)
                            };
                            ui.label(super::detail_text(crate::localization::format_positional(
                                language,
                                crate::localization::TextKey::MNCAdapterLinkLine,
                                &[&link, &dhcp],
                            )));
                        }
                        ui.label(super::detail_text(crate::localization::format_positional(
                            language,
                            crate::localization::TextKey::MNCProtocolLine,
                            &[&adapter.ipv4.to_string(), &adapter.ipv6.to_string()],
                        )));
                        if adapter.details.as_ref().is_some_and(|d| !d.dhcp_v4) {
                            super::wrapped_label(
                                ui,
                                super::detail_text(t(
                                    language,
                                    crate::localization::TextKey::MNCStaticAddressNote,
                                )),
                            );
                        }
                    }
                    for id in [
                        dji4g_application::DiagnosticCheckId::AtControl,
                        dji4g_application::DiagnosticCheckId::Cellular,
                    ] {
                        let state = &check.diagnostics.get(id).state;
                        super::wrapped_label(
                            ui,
                            super::detail_text(format!(
                                "{}：{}",
                                if id == dji4g_application::DiagnosticCheckId::AtControl {
                                    t(language, crate::localization::TextKey::MNCAtControl)
                                } else {
                                    t(language, crate::localization::TextKey::MNCSimCellular)
                                },
                                crate::localization::LocalizedText::new(
                                    language,
                                    crate::localization::diagnostic_state(state)
                                )
                                .text
                            )),
                        );
                    }
                    super::wrapped_label(
                        ui,
                        super::meta_text(t(
                            language,
                            crate::localization::TextKey::MNCBoundRouteNote,
                        )),
                    );
                };
                // The evidence list used to hide behind a disclosure triangle on the full page; every
                // block of this check is now always visible, so it carries a plain heading instead.
                ui.label(super::section_heading(t(
                    language,
                    crate::localization::TextKey::MNCEvidenceHeading,
                )));
                evidence(ui);
            } else {
                super::wrapped_label(ui, t(language, crate::localization::TextKey::MNCIntro));
            }
        };
        details(ui);
        if consent {
            super::wrapped_label(
                ui,
                t(language, crate::localization::TextKey::MNCProbeConsent),
            );
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add(
                        egui::Button::new(t(
                            language,
                            crate::localization::TextKey::MNCProbeCancel,
                        ))
                        .min_size(egui::vec2(0.0, 32.0)),
                    )
                    .clicked()
                {
                    consent = false;
                }
                if ui
                    .add(
                        egui::Button::new(t(language, crate::localization::TextKey::MNCLocalOnly))
                            .min_size(egui::vec2(0.0, 32.0)),
                    )
                    .clicked()
                {
                    error = sink
                        .try_send(UiCommand::CheckModuleNetwork {
                            allow_probe_once: false,
                        })
                        .is_err();
                    if !error {
                        consent = false;
                    }
                }
                if ui
                    .add(
                        egui::Button::new(t(language, crate::localization::TextKey::MNCAllowProbe))
                            .min_size(egui::vec2(0.0, 32.0)),
                    )
                    .clicked()
                {
                    error = sink
                        .try_send(UiCommand::CheckModuleNetwork {
                            allow_probe_once: true,
                        })
                        .is_err();
                    if !error {
                        consent = false;
                    }
                }
            });
        }
        if error {
            super::wrapped_label(
                ui,
                RichText::new(t(language, crate::localization::TextKey::MNCSubmitError))
                    .color(StatusTone::Caution.color()),
            );
        }
    });
    ui.ctx().data_mut(|d| {
        d.insert_temp(confirm_id, consent);
        d.insert_temp(error_id, error);
    });
    guidance
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queued_and_stale_never_display_saved_success_as_current() {
        let mut check = ModuleNetworkCheckSnapshot::queued(1, dji4g_domain::DeviceEpoch(1), false);
        check.verdict = Verdict::Usable;
        assert!(
            conclusion(&check, crate::localization::Language::ZhCn)
                .1
                .contains("排队")
        );
        check.phase = Phase::Stale;
        assert!(
            conclusion(&check, crate::localization::Language::ZhCn)
                .1
                .contains("过期")
        );
        check.phase = Phase::Finished;
        check.verdict = Verdict::DnsIssue;
        assert!(
            conclusion(&check, crate::localization::Language::ZhCn)
                .1
                .contains("公网可达")
        );
        assert!(
            conclusion(&check, crate::localization::Language::ZhCn)
                .1
                .contains("DNS")
        );
    }
}
