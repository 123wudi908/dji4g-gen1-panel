//! A small, evidence-limited guide for computer/agent network conflicts.

use std::time::SystemTime;

use dji4g_application::{ControllerSnapshot, HostNetworkPhase, UiCommand};
use dji4g_domain::{
    Availability, HostNetworkFinding, HostProxyMode, IpFamily, host_observation_is_fresh,
};
use eframe::egui::{self, RichText, Ui};

use crate::{app::UiCommandSink, localization::Language};

use super::{StatusTone, detail_text, section_frame, section_heading, wrapped_label};

/// One catalog string in the language this page was rendered with.
fn t(language: crate::localization::Language, key: crate::localization::TextKey) -> String {
    crate::localization::LocalizedText::new(language, key).text
}

fn busy(phase: HostNetworkPhase) -> bool {
    matches!(
        phase,
        HostNetworkPhase::Checking
            | HostNetworkPhase::Preparing
            | HostNetworkPhase::Applying
            | HostNetworkPhase::Restoring
    )
}

#[must_use]
pub fn brief(
    snapshot: &ControllerSnapshot,
    now: SystemTime,
    language: Language,
) -> (StatusTone, String) {
    let host = &snapshot.host_network;
    if busy(host.phase) {
        return (
            StatusTone::Progress,
            match host.phase {
                HostNetworkPhase::Preparing => {
                    t(language, crate::localization::TextKey::HostPreparingPlan)
                }
                HostNetworkPhase::Applying => {
                    t(language, crate::localization::TextKey::HostBackingUp)
                }
                HostNetworkPhase::Restoring => {
                    t(language, crate::localization::TextKey::HostRestoring)
                }
                _ => t(language, crate::localization::TextKey::HostChecking),
            },
        );
    }
    if host.phase == HostNetworkPhase::AwaitingRestart {
        return (
            StatusTone::Caution,
            t(language, crate::localization::TextKey::HostConfigChanged),
        );
    }
    if let Some(code) = &host.error_code {
        return (
            StatusTone::Caution,
            format!(
                "{} ({code})",
                t(language, crate::localization::TextKey::HostNotFinished)
            ),
        );
    }
    let Some(observation) = &host.observation else {
        return (
            StatusTone::Neutral,
            t(language, crate::localization::TextKey::HostNotChecked),
        );
    };
    if !host_observation_is_fresh(observation.observed_at, now) {
        return (
            StatusTone::Caution,
            t(language, crate::localization::TextKey::HostStale),
        );
    }
    if let Some(binding) = &observation.binding
        && binding.source == dji4g_domain::ProxyBindingSource::ConfigurationOnly
    {
        return (
            StatusTone::Caution,
            crate::localization::format_positional(
                language,
                crate::localization::TextKey::HostConfiguredAdapterRef,
                &[&binding.interface_alias],
            ),
        );
    }
    match host.finding {
        Some(HostNetworkFinding::MissingBoundInterface) => {
            let module = if snapshot.app.availability == Availability::Available {
                t(language, crate::localization::TextKey::HostModulePassed)
            } else {
                String::new()
            };
            (
                StatusTone::Negative,
                format!(
                    "{module}{}",
                    t(language, crate::localization::TextKey::HostMissingAdapter)
                ),
            )
        }
        Some(HostNetworkFinding::BoundInterfaceDown) => (
            StatusTone::Caution,
            t(language, crate::localization::TextKey::HostAdapterDown),
        ),
        Some(HostNetworkFinding::BindingAmbiguous | HostNetworkFinding::EvidenceIncomplete) => (
            StatusTone::Caution,
            t(language, crate::localization::TextKey::HostOutletUnknown),
        ),
        _ => {
            let tun = snapshot.app.network.as_ref().is_some_and(|network| {
                matches!(
                    network.system_default_route,
                    dji4g_domain::DefaultRouteOwner::VpnOrTun
                )
            });
            if tun {
                (
                    StatusTone::Neutral,
                    t(language, crate::localization::TextKey::HostProxyInUse),
                )
            } else {
                (
                    StatusTone::Neutral,
                    t(language, crate::localization::TextKey::HostNoKnownProblem),
                )
            }
        }
    }
}

fn send(ui: &mut Ui, language: Language, sink: &dyn UiCommandSink, command: UiCommand) {
    let id = egui::Id::new("host-network-send-error");
    match sink.try_send(command) {
        Ok(()) => ui.data_mut(|data| data.remove::<String>(id)),
        Err(_) => ui.data_mut(|data| {
            data.insert_temp(
                id,
                t(
                    language,
                    crate::localization::TextKey::HostCommandNotSubmitted,
                )
                .to_owned(),
            )
        }),
    }
}

pub(crate) fn render(
    ui: &mut Ui,
    snapshot: &ControllerSnapshot,
    now: SystemTime,
    language: Language,
    sink: &dyn UiCommandSink,
) {
    let host = &snapshot.host_network;
    let busy = busy(host.phase);
    section_frame(ui, |ui| {
        ui.heading(t(language, crate::localization::TextKey::HostHeading));
        let (tone, message) = brief(snapshot, now, language);
        wrapped_label(
            ui,
            RichText::new(format!("{} {message}", tone.marker())).color(tone.color()),
        );
        if snapshot.app.device.is_none() {
            wrapped_label(
                ui,
                t(language, crate::localization::TextKey::HostNoModuleHint),
            );
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !busy,
                    egui::Button::new(t(language, crate::localization::TextKey::HostCheckAgain)),
                )
                .clicked()
            {
                send(ui, language, sink, UiCommand::InspectHostNetwork);
            }
            if host.finding == Some(HostNetworkFinding::MissingBoundInterface)
                && host
                    .observation
                    .as_ref()
                    .is_some_and(|value| host_observation_is_fresh(value.observed_at, now))
                && host
                    .observation
                    .as_ref()
                    .and_then(|value| value.binding.as_ref())
                    .is_some_and(|binding| binding.repairable)
                && host.phase == HostNetworkPhase::Ready
                && let Some(finding_id) = host.finding_id
                && ui
                    .button(t(language, crate::localization::TextKey::HostReviewRepair))
                    .clicked()
            {
                send(
                    ui,
                    language,
                    sink,
                    UiCommand::PrepareProxyRepair { finding_id },
                );
            }
        });
        if let Some(error) =
            ui.data(|data| data.get_temp::<String>(egui::Id::new("host-network-send-error")))
        {
            wrapped_label(ui, RichText::new(error).color(StatusTone::Negative.color()));
        }
        if host.preview.is_none()
            && host.result.is_none()
            && let Some(observation) = &host.observation
        {
            ui.separator();
            let mode = match observation.system_proxy {
                HostProxyMode::Disabled => t(language, crate::localization::TextKey::HostProxyOff),
                HostProxyMode::Manual => t(language, crate::localization::TextKey::HostProxyManual),
                HostProxyMode::AutoConfig => {
                    t(language, crate::localization::TextKey::HostProxyAutoScript)
                }
                HostProxyMode::AutoDetect => {
                    t(language, crate::localization::TextKey::HostProxyAutoDetect)
                }
                HostProxyMode::Mixed => t(language, crate::localization::TextKey::HostProxyMixed),
                HostProxyMode::Unknown => {
                    t(language, crate::localization::TextKey::HostProxyUnknown)
                }
            };
            wrapped_label(
                ui,
                format!(
                    "{}：{mode}",
                    t(language, crate::localization::TextKey::HostWindowsProxy)
                ),
            );
            for family in [IpFamily::V4, IpFamily::V6] {
                let aliases = observation
                    .default_routes
                    .iter()
                    .filter(|route| route.family == family)
                    .filter_map(|route| {
                        observation
                            .adapters
                            .iter()
                            .find(|adapter| adapter.luid == route.luid)
                    })
                    .map(|adapter| adapter.alias.as_str())
                    .collect::<Vec<_>>();
                if !aliases.is_empty() {
                    wrapped_label(
                        ui,
                        format!(
                            "{}：{}",
                            if family == IpFamily::V4 {
                                "IPv4"
                            } else {
                                "IPv6"
                            },
                            aliases.join("、")
                        ),
                    );
                }
            }
            if let Some(binding) = &observation.binding {
                wrapped_label(
                    ui,
                    format!(
                        "{}：{}",
                        if binding.source == dji4g_domain::ProxyBindingSource::ConfigurationOnly {
                            t(
                                language,
                                crate::localization::TextKey::HostConfiguredAdapter,
                            )
                        } else {
                            t(
                                language,
                                crate::localization::TextKey::HostProxyBoundAdapter,
                            )
                        },
                        binding.interface_alias
                    ),
                );
                wrapped_label(
                    ui,
                    format!(
                        "Clash Verge Rev {}",
                        binding
                            .version
                            .as_deref()
                            .map(str::to_owned)
                            .unwrap_or_else(|| t(
                                language,
                                crate::localization::TextKey::HostVersionUnknown
                            ))
                    ),
                );
                if !binding.repairable {
                    wrapped_label(
                        ui,
                        t(
                            language,
                            crate::localization::TextKey::HostUnsupportedConfig,
                        ),
                    );
                }
            }
            if let Some(code) = &observation.proxy_error_code {
                wrapped_label(
                    ui,
                    format!(
                        "{} ({code})",
                        t(language, crate::localization::TextKey::HostConfigUnreadable)
                    ),
                );
            }
        }
        if let Some(preview) = &host.preview {
            ui.separator();
            wrapped_label(ui, "Clash Verge Rev v2.5.5");
            wrapped_label(
                ui,
                format!(
                    "{}“{}”{}",
                    t(
                        language,
                        crate::localization::TextKey::HostRemoveOutletPrefix
                    ),
                    preview.interface_alias,
                    t(
                        language,
                        crate::localization::TextKey::HostRemoveOutletSuffix
                    )
                ),
            );
            let expired = now > preview.expires_at;
            if expired {
                wrapped_label(
                    ui,
                    RichText::new(t(language, crate::localization::TextKey::HostPlanExpired))
                        .color(StatusTone::Caution.color()),
                );
            }
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        !busy,
                        egui::Button::new(t(
                            language,
                            crate::localization::TextKey::HostKeepOriginal,
                        )),
                    )
                    .on_disabled_hover_text(t(
                        language,
                        crate::localization::TextKey::HostWaitCurrentOperation,
                    ))
                    .clicked()
                {
                    send(
                        ui,
                        language,
                        sink,
                        UiCommand::CancelProxyRepair {
                            plan_id: preview.plan_id,
                        },
                    );
                }
                if ui
                    .add_enabled(
                        !expired && !busy,
                        egui::Button::new(t(
                            language,
                            crate::localization::TextKey::HostBackupAndRepair,
                        )),
                    )
                    .clicked()
                {
                    send(
                        ui,
                        language,
                        sink,
                        UiCommand::ConfirmProxyRepair {
                            plan_id: preview.plan_id,
                        },
                    );
                }
            });
        }
        if let Some(result) = &host.result {
            wrapped_label(
                ui,
                t(
                    language,
                    crate::localization::TextKey::HostRestartedCheckAgain,
                ),
            );
            if ui
                .add_enabled(
                    !busy,
                    egui::Button::new(t(
                        language,
                        crate::localization::TextKey::HostRestoreOriginal,
                    )),
                )
                .on_disabled_hover_text(t(
                    language,
                    crate::localization::TextKey::HostWaitCurrentOperation,
                ))
                .clicked()
            {
                send(
                    ui,
                    language,
                    sink,
                    UiCommand::RestoreProxyRepair {
                        backup_id: result.backup_id,
                    },
                );
            }
        }
        // The troubleshooting steps used to hide behind a disclosure triangle inside this card;
        // they are now a flat always-visible block, kept at the quiet tier so the card stays short.
        ui.label(section_heading(t(
            language,
            crate::localization::TextKey::HostStepsHeading,
        )));
        wrapped_label(
            ui,
            detail_text(t(language, crate::localization::TextKey::HostStepsBody)),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_describes_the_operation_in_each_phase() {
        let mut snapshot =
            dji4g_application::Controller::for_test(SystemTime::UNIX_EPOCH).snapshot();
        for (phase, message) in [
            (HostNetworkPhase::Checking, "正在检查"),
            (HostNetworkPhase::Preparing, "正在准备"),
            (HostNetworkPhase::Applying, "正在备份并修复"),
            (HostNetworkPhase::Restoring, "正在恢复"),
        ] {
            snapshot.host_network.phase = phase;
            let (tone, text) = brief(&snapshot, SystemTime::UNIX_EPOCH, Language::ZhCn);
            assert_eq!(tone, StatusTone::Progress);
            assert!(text.starts_with(message), "{phase:?}: {text}");
        }
    }

    #[test]
    fn restore_click_is_blocked_while_an_operation_is_running() {
        struct Sink(std::sync::Mutex<Vec<UiCommand>>);
        impl UiCommandSink for Sink {
            fn try_send(&self, command: UiCommand) -> Result<(), dji4g_application::UiSendError> {
                self.0.lock().unwrap().push(command);
                Ok(())
            }
        }
        for phase in [
            HostNetworkPhase::AwaitingRestart,
            HostNetworkPhase::Checking,
            HostNetworkPhase::Preparing,
            HostNetworkPhase::Applying,
            HostNetworkPhase::Restoring,
        ] {
            let ctx = egui::Context::default();
            super::super::apply_style(&ctx);
            let mut snapshot =
                dji4g_application::Controller::for_test(SystemTime::UNIX_EPOCH).snapshot();
            snapshot.host_network.phase = phase;
            snapshot.host_network.result = Some(dji4g_application::ProxyRepairResult {
                backup_id: 7,
                changed: true,
            });
            let sink = Sink(std::sync::Mutex::new(vec![]));
            let mut point = None;
            for tick in 0..4 {
                let events = if tick >= 2 {
                    let pos = point.expect("restore button should be visible");
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: tick == 2,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]
                } else {
                    vec![]
                };
                let output = ctx.run(
                    egui::RawInput {
                        events,
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(900.0, 600.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            render(ui, &snapshot, SystemTime::UNIX_EPOCH, Language::EnUs, &sink);
                        });
                    },
                );
                if tick < 2 {
                    point = output.shapes.iter().find_map(|shape| {
                        if let egui::Shape::Text(text) = &shape.shape
                            && text.galley.text() == "Restore original configuration"
                        {
                            Some(text.pos + text.galley.size() * 0.5)
                        } else {
                            None
                        }
                    });
                }
            }
            let commands = sink.0.lock().unwrap();
            let expected = usize::from(phase == HostNetworkPhase::AwaitingRestart);
            assert_eq!(commands.len(), expected, "{phase:?}");
            if let Some(command) = commands.first() {
                assert!(matches!(
                    command,
                    UiCommand::RestoreProxyRepair { backup_id: 7 }
                ));
            }
        }
    }
    #[test]
    fn disk_binding_is_presented_as_unverified_runtime() {
        let mut snapshot =
            dji4g_application::Controller::for_test(SystemTime::UNIX_EPOCH).snapshot();
        snapshot.host_network.observation = Some(dji4g_domain::HostNetworkObservation {
            adapters: vec![],
            default_routes: vec![],
            system_proxy: HostProxyMode::Manual,
            binding: Some(dji4g_domain::ProxyBinding {
                source: dji4g_domain::ProxyBindingSource::ConfigurationOnly,
                client: dji4g_domain::ProxyClient::ClashVergeRev,
                version: None,
                interface_alias: "test-port".into(),
                repairable: false,
            }),
            proxy_inspection_complete: false,
            proxy_error_code: None,
            observed_at: SystemTime::UNIX_EPOCH,
        });
        let (_, text) = brief(&snapshot, SystemTime::UNIX_EPOCH, Language::ZhCn);
        assert!(text.contains("磁盘配置"));
        assert!(text.contains("尚未核实当前运行态"));
        assert!(!text.contains("已不存在"));
    }
}
