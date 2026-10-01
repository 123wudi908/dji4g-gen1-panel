//! A small, evidence-limited guide for computer/agent network conflicts.

use std::time::SystemTime;

use dji4g_application::{ControllerSnapshot, HostNetworkPhase, UiCommand};
use dji4g_domain::{
    Availability, HostNetworkFinding, HostProxyMode, IpFamily, host_observation_is_fresh,
};
use eframe::egui::{self, RichText, Ui};

use crate::{app::UiCommandSink, localization::Language};

use super::{StatusTone, section_frame, wrapped_label};

fn copy(language: Language, zh: &'static str, en: &'static str) -> &'static str {
    match language {
        Language::ZhCn => zh,
        Language::EnUs => en,
    }
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
                    copy(language, "正在准备修复方案…", "Preparing the repair plan…")
                }
                HostNetworkPhase::Applying => copy(
                    language,
                    "正在备份并修复代理配置…",
                    "Backing up and repairing proxy configuration…",
                ),
                HostNetworkPhase::Restoring => copy(
                    language,
                    "正在恢复原配置…",
                    "Restoring the original configuration…",
                ),
                _ => copy(
                    language,
                    "正在检查电脑网络与代理设置…",
                    "Checking computer network and proxy settings…",
                ),
            }
            .into(),
        );
    }
    if host.phase == HostNetworkPhase::AwaitingRestart {
        return (
            StatusTone::Caution,
            copy(
                language,
                "代理配置已修改；重启代理后请重新检查。",
                "Proxy configuration changed. Restart the client, then check again.",
            )
            .into(),
        );
    }
    if let Some(code) = &host.error_code {
        return (
            StatusTone::Caution,
            format!(
                "{} ({code})",
                copy(
                    language,
                    "本次电脑网络检查或修复未完成",
                    "Computer network check or repair did not finish"
                )
            ),
        );
    }
    let Some(observation) = &host.observation else {
        return (
            StatusTone::Neutral,
            copy(
                language,
                "电脑网络与代理尚未检查。",
                "Computer network and proxy have not been checked.",
            )
            .into(),
        );
    };
    if !host_observation_is_fresh(observation.observed_at, now) {
        return (
            StatusTone::Caution,
            copy(
                language,
                "电脑网络信息已过期，请重新检查。",
                "Computer network information is stale. Check again.",
            )
            .into(),
        );
    }
    if let Some(binding) = &observation.binding
        && binding.source == dji4g_domain::ProxyBindingSource::ConfigurationOnly
    {
        return (
            StatusTone::Caution,
            match language {
                Language::ZhCn => format!(
                    "磁盘配置引用了网卡“{}”，尚未核实当前运行态。",
                    binding.interface_alias
                ),
                Language::EnUs => format!(
                    "Disk configuration references adapter '{}'; the current runtime is unverified.",
                    binding.interface_alias
                ),
            },
        );
    }
    match host.finding {
        Some(HostNetworkFinding::MissingBoundInterface) => {
            let module = if snapshot.app.availability == Availability::Available {
                copy(language, "模块连接正常；", "The module connection passed; ")
            } else {
                ""
            };
            (
                StatusTone::Negative,
                format!(
                    "{module}{}",
                    copy(
                        language,
                        "代理软件指定的出口网卡已不存在。",
                        "the proxy client references a missing network adapter."
                    )
                ),
            )
        }
        Some(HostNetworkFinding::BoundInterfaceDown) => (
            StatusTone::Caution,
            copy(
                language,
                "代理指定的网卡仍在电脑上，但当前未连接或已禁用。",
                "The proxy-bound adapter exists but is down or disabled.",
            )
            .into(),
        ),
        Some(HostNetworkFinding::BindingAmbiguous | HostNetworkFinding::EvidenceIncomplete) => (
            StatusTone::Caution,
            copy(
                language,
                "暂时无法确定代理出口问题，请查看处理步骤。",
                "The proxy outlet could not be determined; see guidance.",
            )
            .into(),
        ),
        _ => {
            let tun = snapshot.app.network.as_ref().is_some_and(|network| {
                matches!(
                    network.system_default_route,
                    dji4g_domain::DefaultRouteOwner::VpnOrTun
                )
            });
            if tun {
                (StatusTone::Neutral, copy(language, "电脑正在通过代理或 VPN 接口联网；模块通路单独检查。", "A proxy or VPN interface is in use; the module path is checked separately.").into())
            } else {
                (
                    StatusTone::Neutral,
                    copy(
                        language,
                        "未发现已知的固定出口网卡问题。",
                        "No known fixed-outlet adapter problem was found.",
                    )
                    .into(),
                )
            }
        }
    }
}

pub(crate) fn render_brief(
    ui: &mut Ui,
    snapshot: &ControllerSnapshot,
    now: SystemTime,
    language: Language,
) -> bool {
    let (tone, message) = brief(snapshot, now, language);
    let mut open = false;
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(tone.color(), format!("{} {message}", tone.marker()));
        if ui
            .button(copy(language, "电脑网络详情", "Computer network details"))
            .clicked()
        {
            open = true;
        }
    });
    open
}

fn send(ui: &mut Ui, sink: &dyn UiCommandSink, command: UiCommand) {
    let id = egui::Id::new("host-network-send-error");
    match sink.try_send(command) {
        Ok(()) => ui.data_mut(|data| data.remove::<String>(id)),
        Err(_) => ui.data_mut(|data| data.insert_temp(id, "命令未提交，请稍后重试。".to_owned())),
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
        ui.heading(copy(
            language,
            "电脑网络与代理",
            "Computer network and proxy",
        ));
        let (tone, message) = brief(snapshot, now, language);
        wrapped_label(
            ui,
            RichText::new(format!("{} {message}", tone.marker())).color(tone.color()),
        );
        if snapshot.app.device.is_none() {
            wrapped_label(
                ui,
                copy(
                    language,
                    "未连接模块，仍可检查电脑网络与代理设置。",
                    "No module is connected. Computer and proxy checks remain available.",
                ),
            );
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !busy,
                    egui::Button::new(copy(language, "重新检查", "Check again")),
                )
                .clicked()
            {
                send(ui, sink, UiCommand::InspectHostNetwork);
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
                    .button(copy(language, "查看修复方案", "Review repair"))
                    .clicked()
            {
                send(ui, sink, UiCommand::PrepareProxyRepair { finding_id });
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
                HostProxyMode::Disabled => copy(language, "未开启", "Off"),
                HostProxyMode::Manual => copy(language, "手动代理", "Manual"),
                HostProxyMode::AutoConfig => copy(
                    language,
                    "自动配置脚本；未执行脚本",
                    "Automatic script; script not executed",
                ),
                HostProxyMode::AutoDetect => {
                    copy(language, "自动检测；尚未验证", "Auto-detect; not verified")
                }
                HostProxyMode::Mixed => copy(
                    language,
                    "多种代理设置；需进一步确认",
                    "Multiple proxy settings; needs review",
                ),
                HostProxyMode::Unknown => copy(language, "未获取", "Unavailable"),
            };
            wrapped_label(
                ui,
                format!(
                    "{}：{mode}",
                    copy(language, "Windows 系统代理", "Windows system proxy")
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
                            copy(
                                language,
                                "磁盘配置引用网卡（运行态未核实）",
                                "Configured adapter (runtime unverified)",
                            )
                        } else {
                            copy(language, "代理指定网卡", "Proxy-bound adapter")
                        },
                        binding.interface_alias
                    ),
                );
                wrapped_label(
                    ui,
                    format!(
                        "Clash Verge Rev {}",
                        binding.version.as_deref().unwrap_or("版本未确认")
                    ),
                );
                if !binding.repairable {
                    wrapped_label(
                        ui,
                        copy(
                            language,
                            "此配置或版本暂不支持自动修改，请在代理软件中检查出站接口设置。",
                            "This configuration cannot be changed automatically. Review the outbound interface in the proxy client.",
                        ),
                    );
                }
            }
            if let Some(code) = &observation.proxy_error_code {
                wrapped_label(
                    ui,
                    format!(
                        "{} ({code})",
                        copy(
                            language,
                            "代理配置无法可靠读取",
                            "Proxy configuration could not be read reliably"
                        )
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
                    copy(
                        language,
                        "将取消代理软件对",
                        "Remove the proxy client's fixed outlet for "
                    ),
                    preview.interface_alias,
                    copy(
                        language,
                        "的固定出口设置。之后可能使用 Wi-Fi、有线网络或其他可用出口。将先备份原配置。请先完整退出 Clash Verge Rev 及相关核心。",
                        ". It may then use Wi-Fi, Ethernet or another available outlet. The original will be backed up. Exit Clash Verge Rev and its core first."
                    )
                ),
            );
            let expired = now > preview.expires_at;
            if expired {
                wrapped_label(
                    ui,
                    RichText::new(copy(
                        language,
                        "修复方案已过期，请重新检查后再查看方案。",
                        "This repair plan expired. Check again before preparing a new plan.",
                    ))
                    .color(StatusTone::Caution.color()),
                );
            }
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        !busy,
                        egui::Button::new(copy(language, "保留原设置", "Keep original settings")),
                    )
                    .on_disabled_hover_text(copy(
                        language,
                        "正在处理，请等待本次操作结束",
                        "Wait for the current operation to finish",
                    ))
                    .clicked()
                {
                    send(
                        ui,
                        sink,
                        UiCommand::CancelProxyRepair {
                            plan_id: preview.plan_id,
                        },
                    );
                }
                if ui
                    .add_enabled(
                        !expired && !busy,
                        egui::Button::new(copy(language, "备份并修复", "Back up and repair")),
                    )
                    .clicked()
                {
                    send(
                        ui,
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
                copy(
                    language,
                    "配置已修改。重启代理后点击“重新检查”；配置改动不等于互联网已经恢复。",
                    "Configuration changed. Restart the proxy and check again; this does not prove Internet access.",
                ),
            );
            if ui
                .add_enabled(
                    !busy,
                    egui::Button::new(copy(
                        language,
                        "恢复原配置",
                        "Restore original configuration",
                    )),
                )
                .on_disabled_hover_text(copy(
                    language,
                    "正在处理，请等待本次操作结束",
                    "Wait for the current operation to finish",
                ))
                .clicked()
            {
                send(
                    ui,
                    sink,
                    UiCommand::RestoreProxyRepair {
                        backup_id: result.backup_id,
                    },
                );
            }
        }
        egui::CollapsingHeader::new(copy(language, "查看处理步骤", "Troubleshooting steps")).show(ui, |ui| {
            wrapped_label(ui, copy(language, "先检查模块与 SIM；若模块公网检查通过，但代理仍报找不到网卡，请在代理软件中检查“出站接口”是否指向已移除或改名的网卡。多网卡用户可能有意固定出口，修改前确认用途。配置来源不明确时，请在代理软件中手动调整。", "Check the module and SIM. If the module path passes but the proxy reports a missing interface, review its outbound-interface setting. A fixed outlet may be intentional on computers with multiple adapters. Use the proxy client to edit ambiguous configurations."));
        });
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
            super::super::style_root(&ctx);
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
