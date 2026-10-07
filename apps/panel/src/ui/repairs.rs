//! Safe, confirmation-first repair controls. This module only emits closed UiCommand values.

use std::time::SystemTime;

use dji4g_application::{
    ActionReadinessKey, ControlledRepairError, ControlledRepairRequest, ControllerSnapshot,
};
use dji4g_domain::{ActionKind, DnsProfile, HotspotStatus};
use eframe::egui::{self, RichText, Ui};
use std::net::{IpAddr, Ipv4Addr};

use super::{field_label, meta_text, scale, section_frame, section_heading, wrapped_label};
use crate::app::PanelCommandSink;
use crate::localization::{Language, LocalizedText, TextKey};

/// One catalog string in the language this page was rendered with.
fn t(language: crate::localization::Language, key: crate::localization::TextKey) -> String {
    crate::localization::LocalizedText::new(language, key).text
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepairActionVm {
    pub action: ActionKind,
    pub title: LocalizedText,
    pub enabled: bool,
    /// Honest reason a disabled action cannot be prepared, shown as the button tooltip.
    pub disabled_reason: Option<LocalizedText>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepairsVm {
    pub title: LocalizedText,
    pub notice: LocalizedText,
    pub driver_notice: LocalizedText,
    pub actions: Vec<RepairActionVm>,
}

#[must_use]
pub fn repairs_vm(snapshot: &ControllerSnapshot, now: SystemTime, language: Language) -> RepairsVm {
    let app = snapshot.app.as_ref();
    let build = |action: ActionKind, key: ActionReadinessKey, _title: TextKey| {
        let availability =
            super::action_availability::repair_action_availability(snapshot, key, now, language);
        RepairActionVm {
            title: crate::localization::action_text(&action, language),
            action,
            enabled: availability.enabled,
            disabled_reason: availability.reason,
        }
    };
    let actions = vec![
        build(
            ActionKind::RenewDhcp,
            ActionReadinessKey::RenewDhcp,
            TextKey::ActionRenewDhcp,
        ),
        build(
            ActionKind::ApplyDnsProfile {
                profile: DnsProfile::Automatic,
            },
            ActionReadinessKey::ApplyDnsProfile,
            TextKey::ActionApplyDnsAutomatic,
        ),
        build(
            ActionKind::ApplyDnsProfile {
                profile: DnsProfile::Static {
                    servers: vec![
                        IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
                        IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
                    ],
                },
            },
            ActionReadinessKey::ApplyDnsProfile,
            TextKey::ActionApplyDnsStatic,
        ),
        build(
            ActionKind::ToggleHotspot {
                enabled: !matches!(
                    app.hotspot,
                    HotspotStatus::On { .. } | HotspotStatus::Starting
                ),
            },
            ActionReadinessKey::ToggleHotspot,
            if matches!(
                app.hotspot,
                HotspotStatus::On { .. } | HotspotStatus::Starting
            ) {
                TextKey::ActionDisableHotspot
            } else {
                TextKey::ActionEnableHotspot
            },
        ),
        build(
            ActionKind::SetVerifiedUsbNetworkProfile {
                profile: dji4g_domain::UsbNetworkProfile::DjiNdis,
            },
            ActionReadinessKey::SetUsbNetworkProfile,
            TextKey::ActionSetUsbProfileDjiNdis,
        ),
        build(
            ActionKind::RestartAdapter,
            ActionReadinessKey::RestartAdapter,
            TextKey::ActionRestartAdapter,
        ),
        build(
            ActionKind::EditApn {
                cid: 1,
                apn: String::new(),
            },
            ActionReadinessKey::EditApn,
            TextKey::ActionEditApn,
        ),
        build(
            ActionKind::ReenumerateDevice,
            ActionReadinessKey::ReenumerateDevice,
            TextKey::ActionReenumerateDevice,
        ),
        build(
            ActionKind::RestartModule,
            ActionReadinessKey::RestartModule,
            TextKey::ActionRestartModule,
        ),
        build(
            ActionKind::SetVerifiedUsbNetworkProfile {
                profile: dji4g_domain::UsbNetworkProfile::Ecm,
            },
            ActionReadinessKey::SetUsbNetworkProfile,
            TextKey::ActionSetUsbProfileEcm,
        ),
    ];
    // Shared readiness owns the hotspot gate too: a previous failure may be retried, while
    // unsupported states carry the same visible reason as every other repair entry point.
    RepairsVm {
        title: LocalizedText::new(language, TextKey::RepairsTitle),
        notice: LocalizedText::new(language, TextKey::RepairsReadOnlyNotice),
        driver_notice: LocalizedText::new(language, TextKey::RepairsDriverNotIncluded),
        actions,
    }
}

pub(crate) fn render(
    ui: &mut Ui,
    snapshot: &ControllerSnapshot,
    now: SystemTime,
    language: Language,
    sink: &dyn PanelCommandSink,
) -> bool {
    let vm = repairs_vm(snapshot, now, language);
    super::components::page_heading(
        ui,
        &vm.title.text,
        &t(language, crate::localization::TextKey::RepairsIntro),
    );
    wrapped_label(
        ui,
        RichText::new(vm.notice.text.clone())
            .size(scale::RATE_AUX)
            .color(scale::secondary()),
    );
    wrapped_label(ui, meta_text(vm.driver_notice.text.clone()));
    let install_requested = super::driver_setup::render(ui, snapshot, now, language);
    // The setup checklist follows the driver card it belongs to. It used to sit on the overview,
    // where it was the only thing on a read-only page that asked the user to do something.
    super::driver_setup::render_guide(ui, snapshot, now, language);
    let (usb_actions, other_actions): (Vec<_>, Vec<_>) =
        vm.actions.into_iter().partition(|action| {
            matches!(
                action.action,
                ActionKind::SetVerifiedUsbNetworkProfile { .. }
            )
        });
    section_frame(ui, |ui| {
        ui.label(section_heading(t(
            language,
            crate::localization::TextKey::RepairsAdapterModeHeading,
        )));
        super::components::link(
            ui,
            &t(language, crate::localization::TextKey::RepairsDjiGuideLink),
            "https://dl.djicdn.com/downloads/DJI_Mavic_3/DJI_Cellular_Dongle_LTE_USB_Modem_User_Guide_v1.0.pdf",
        );
        wrapped_label(
            ui,
            t(
                language,
                crate::localization::TextKey::RepairsAdapterModeNote,
            ),
        );
        wrapped_label(
            ui,
            meta_text(t(
                language,
                crate::localization::TextKey::RepairsUsbSwitchNote,
            )),
        );
        wrapped_label(
            ui,
            meta_text(t(
                language,
                crate::localization::TextKey::RepairsUsbOnlyNote,
            )),
        );
        for action in usb_actions {
            render_action_button(ui, action, language, sink);
        }
    });
    // No nested scroll area: the shell already scrolls the page, so the grouped actions share
    // one scrollbar instead of fighting each other for height. Sections follow the domain risk
    // metadata, never a list index, so a high-risk action can never hide in the low-risk group.
    let (low_risk, interrupting): (Vec<_>, Vec<_>) = other_actions
        .into_iter()
        .partition(|action| !is_interrupting(&action.action));
    section_frame(ui, |ui| {
        ui.label(section_heading(t(
            language,
            crate::localization::TextKey::RepairsLowRiskHeading,
        )));
        ui.vertical(|ui| {
            // Even gaps in both axes so a wrapped group still reads as an aligned block.
            ui.spacing_mut().item_spacing =
                egui::vec2(scale::CONTROL_GAP[0], scale::CONTROL_GAP[1]);
            for action in low_risk {
                render_action_button(ui, action, language, sink);
            }
        });
    });
    section_frame(ui, |ui| {
        ui.label(section_heading(t(
            language,
            crate::localization::TextKey::RepairsInterruptsConnection,
        )));
        for action in &interrupting {
            if matches!(action.action, ActionKind::EditApn { .. }) {
                let _ = render_apn_editor(ui, action.clone(), language, sink);
            }
        }
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing =
                egui::vec2(scale::CONTROL_GAP[0], scale::CONTROL_GAP[1]);
            for action in interrupting {
                if matches!(action.action, ActionKind::EditApn { .. }) {
                    continue;
                }
                render_action_button(ui, action, language, sink);
            }
        });
    });
    install_requested
}

fn is_interrupting(action: &ActionKind) -> bool {
    matches!(
        dji4g_application::action_disruption(action),
        Some(
            dji4g_domain::DisruptionLevel::ConnectionInterrupting
                | dji4g_domain::DisruptionLevel::DeviceReenumeration
        )
    )
}

/// One repair button: disabled buttons carry the precise prerequisite reason as a tooltip so a
/// silent no-op is impossible.
fn render_action_button(
    ui: &mut Ui,
    action: RepairActionVm,
    language: Language,
    sink: &dyn PanelCommandSink,
) {
    let button = ui
        .horizontal_wrapped(|ui| {
            ui.vertical(|ui| {
                ui.set_max_width((ui.available_width() - 140.0).max(160.0));
                ui.label(RichText::new(&action.title.text).size(14.0).strong());
                if let Some(reason) = &action.disabled_reason {
                    wrapped_label(ui, meta_text(&reason.text));
                }
            });
            super::components::action_button(
                ui,
                &t(language, crate::localization::TextKey::RepairsViewPlan),
                super::components::ButtonKind::Outlined,
                action.enabled,
                action.disabled_reason.as_ref().map(|r| r.text.as_str()),
            )
        })
        .inner;
    ui.separator();
    if button.clicked() {
        if let Ok(request) = ControlledRepairRequest::try_from_action(action.action.clone()) {
            sink.prepare_repair_now(request);
        }
    }
}

/// Keep the editable APN/CID only in egui's transient UI memory.  It is converted to the typed
/// application request at the button edge and is never copied into a controller snapshot or an
/// audit/debug value.  A fixed CID default is intentional: the platform still proves that the
/// selected context exists, is complete, and is inactive before any write.
fn render_apn_editor(
    ui: &mut Ui,
    action: RepairActionVm,
    language: Language,
    sink: &dyn PanelCommandSink,
) -> egui::Response {
    let cid_id = egui::Id::new("repair.apn.cid");
    let apn_id = egui::Id::new("repair.apn.value");
    let mut cid = ui.data(|data| data.get_temp::<u8>(cid_id)).unwrap_or(1);
    let mut value = ui
        .data(|data| data.get_temp::<String>(apn_id))
        .unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        ui.label(field_label(t(
            language,
            crate::localization::TextKey::FieldPdpContext,
        )));
        ui.add(egui::DragValue::new(&mut cid).range(1..=16));
        ui.label(field_label(t(
            language,
            crate::localization::TextKey::FieldNewApn,
        )));
        // Height matches the neighbouring buttons so the input row does not read as shorter
        // than the controls beside it; the row wraps, so the width cannot overflow.
        ui.add_sized(
            [160.0, scale::CONTROL_H],
            egui::TextEdit::singleline(&mut value).password(true),
        );
    });
    ui.data_mut(|data| {
        data.insert_temp(cid_id, cid);
        data.insert_temp(apn_id, value.clone());
    });
    let request = ControlledRepairRequest::try_apn(cid, value.as_str());
    let disabled_reason = if !action.enabled {
        Some(
            action.disabled_reason.unwrap_or_else(|| {
                LocalizedText::new(language, TextKey::ErrorCapabilityUnavailable)
            }),
        )
    } else {
        request.as_ref().err().map(|error| {
            let key = match error {
                ControlledRepairError::InvalidPdpContextId => {
                    TextKey::ProtocolPdpContextIdOutOfRange
                }
                ControlledRepairError::InvalidApn if value.is_empty() => TextKey::ProtocolApnEmpty,
                ControlledRepairError::InvalidApn
                    if value.len() > dji4g_at_protocol::Apn::MAX_LEN =>
                {
                    TextKey::ProtocolApnTooLong
                }
                _ => TextKey::RepairApnInvalid,
            };
            LocalizedText::new(language, key)
        })
    };
    let button = ui.add_enabled(
        action.enabled && request.is_ok(),
        egui::Button::new(
            crate::localization::format_text_in(
                language,
                TextKey::ActionEditApn,
                &crate::localization::TextArgs::cid(cid),
            )
            .text,
        ),
    );
    if let Some(reason) = disabled_reason {
        button.clone().on_disabled_hover_text(reason.text.clone());
        wrapped_label(ui, meta_text(reason.text));
    }
    if button.clicked() {
        if let Ok(request) = request {
            sink.prepare_repair_now(request);
        }
    }
    button
}

#[cfg(test)]
mod tests {
    use super::*;
    use dji4g_application::{ActionRequest, UiCommand, UiSendError};
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingSink(Mutex<Vec<ControlledRepairRequest>>);

    impl PanelCommandSink for RecordingSink {
        fn try_send(&self, _command: UiCommand) -> Result<(), UiSendError> {
            panic!("the APN editor must use the typed repair boundary")
        }
        fn prepare_repair_now(&self, request: ControlledRepairRequest) {
            self.0.lock().unwrap().push(request);
        }
        fn prepare_action_now(&self, _request: ActionRequest) {
            panic!("the APN editor must not emit a historical domain action")
        }
    }

    fn apn_action() -> RepairActionVm {
        RepairActionVm {
            action: ActionKind::EditApn {
                cid: 1,
                apn: String::new(),
            },
            title: LocalizedText::new(Language::ZhCn, TextKey::ActionEditApn),
            enabled: true,
            disabled_reason: None,
        }
    }

    fn frame(
        context: &egui::Context,
        sink: &RecordingSink,
        events: Vec<egui::Event>,
        focus: bool,
    ) -> (egui::FullOutput, egui::Response) {
        let mut button = None;
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(552.0, 300.0),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    let response = render_apn_editor(ui, apn_action(), Language::ZhCn, sink);
                    if focus {
                        response.request_focus();
                    }
                    button = Some(response);
                });
            },
        );
        (output, button.unwrap())
    }

    fn contains_text(output: &egui::FullOutput, expected: &str) -> bool {
        fn has_text(shape: &egui::Shape, expected: &str) -> bool {
            match shape {
                egui::Shape::Text(text) => text.galley.job.text == expected,
                egui::Shape::Vec(shapes) => shapes.iter().any(|shape| has_text(shape, expected)),
                _ => false,
            }
        }
        output
            .shapes
            .iter()
            .any(|shape| has_text(&shape.shape, expected))
    }

    fn enter(pressed: bool) -> Vec<egui::Event> {
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]
    }

    #[test]
    fn invalid_apn_has_visible_reason_and_keyboard_cannot_prepare_it() {
        for (value, reason) in [
            (String::new(), "APN 不能为空。"),
            ("bad,apn".into(), "APN 含不允许的字符。"),
            ("中文".into(), "APN 含不允许的字符。"),
            ("a".repeat(101), "APN 不能超过 100 个 ASCII 字节。"),
        ] {
            let context = egui::Context::default();
            let sink = RecordingSink::default();
            context.data_mut(|data| data.insert_temp(egui::Id::new("repair.apn.value"), value));
            let (output, response) = frame(&context, &sink, Vec::new(), false);
            assert!(!response.enabled());
            assert!(
                contains_text(&output, reason),
                "missing visible input reason"
            );
            context.memory_mut(|memory| memory.request_focus(response.id));
            let (_, response) = frame(&context, &sink, enter(true), false);
            assert!(!response.clicked());
            let _ = frame(&context, &sink, enter(false), false);
            assert!(sink.0.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn valid_apn_keyboard_activation_preserves_cid_and_prepares_exactly_once() {
        let context = egui::Context::default();
        let sink = RecordingSink::default();
        context.data_mut(|data| {
            data.insert_temp(egui::Id::new("repair.apn.cid"), 3_u8);
            data.insert_temp(
                egui::Id::new("repair.apn.value"),
                "internet.example".to_owned(),
            );
        });
        let (_, response) = frame(&context, &sink, Vec::new(), true);
        assert!(response.enabled());
        let _ = frame(&context, &sink, enter(true), false);
        let _ = frame(&context, &sink, enter(false), false);
        let _ = frame(&context, &sink, Vec::new(), false);
        assert_eq!(
            sink.0.lock().unwrap().as_slice(),
            &[ControlledRepairRequest::try_apn(3, "internet.example").unwrap()]
        );
    }
}
