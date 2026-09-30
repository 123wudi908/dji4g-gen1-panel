//! Repair confirmation must describe the exact parameters that the user is authorizing.

use std::{net::IpAddr, time::SystemTime};

use dji4g_application::{
    ActionKindTag, ControlledRepairRequest, Controller, OperationState, OperationUiSnapshot,
    ReducerState,
};
use dji4g_domain::{ActionKind, AfterStateHash, DnsProfile, OperationOutcome, UsbNetworkProfile};
use dji4g_panel::{
    localization::Language,
    native_dialog::{DialogRequest, confirm_message_for_action, result_request},
    ui::prepared_action_text,
};

const NOW: SystemTime = SystemTime::UNIX_EPOCH;

fn message(action: &ActionKind) -> String {
    confirm_message_for_action(
        action,
        dji4g_application::action_requires_elevation(action),
        dji4g_application::action_disruption(action),
        dji4g_application::action_risk(action),
        false,
        Language::ZhCn,
    )
    .expect("controlled repair confirmation")
    .1
}

fn static_dns() -> ActionKind {
    ActionKind::ApplyDnsProfile {
        profile: DnsProfile::Static {
            servers: vec![
                "1.1.1.1".parse::<IpAddr>().unwrap(),
                "8.8.8.8".parse::<IpAddr>().unwrap(),
            ],
        },
    }
}

#[test]
fn static_dns_confirmation_names_the_actual_servers_and_never_says_automatic() {
    let text = message(&static_dns());
    assert!(text.contains("操作：应用静态 DNS（2 个服务器）"));
    assert!(text.contains("DNS 服务器：1.1.1.1、8.8.8.8"));
    assert!(!text.contains("恢复自动获取 DNS"));

    let automatic = message(&ActionKind::ApplyDnsProfile {
        profile: DnsProfile::Automatic,
    });
    assert!(automatic.contains("操作：恢复自动获取 DNS"));
    assert!(!automatic.contains("DNS 服务器："));
}

#[test]
fn usb_confirmation_distinguishes_ecm_from_dji_ndis() {
    let ecm = message(&ActionKind::SetVerifiedUsbNetworkProfile {
        profile: UsbNetworkProfile::Ecm,
    });
    assert!(ecm.contains("操作：切换为 ECM 网卡"));
    assert!(!ecm.contains("DJI NDIS"));
    assert!(ecm.contains("只保存 USB 网络配置"));
    assert!(ecm.contains("需手动重启模块后复检"));
    assert!(ecm.contains("当前网卡模式尚未验证"));

    let ndis = message(&ActionKind::SetVerifiedUsbNetworkProfile {
        profile: UsbNetworkProfile::DjiNdis,
    });
    assert!(ndis.contains("操作：切换为电脑网卡（DJI NDIS）"));
    assert!(!ndis.contains("ECM"));
    assert!(ndis.contains("只保存 USB 网络配置"));
}

#[test]
fn apn_confirmation_shows_the_selected_cid_and_complete_editor_value() {
    let apn = format!("{}.example", "a".repeat(64));
    let request = ControlledRepairRequest::try_apn(3, &apn).unwrap();
    let text = message(&request.clone().into_action());
    assert!(text.contains("操作：修改 PDP 上下文 3 的 APN"));
    assert!(text.contains(&format!("新 APN：{apn}\n")));
    assert!(!text.contains("{cid}"));
    // Disclosure for the explicit confirmation does not widen normal diagnostic formatting.
    assert!(!format!("{request:?}").contains(&apn));
}

#[test]
fn prepared_tags_never_invent_dns_or_usb_parameters_and_format_the_apn_cid() {
    for (action, expected) in [
        (static_dns(), "修改 DNS 配置"),
        (
            ActionKind::SetVerifiedUsbNetworkProfile {
                profile: UsbNetworkProfile::Ecm,
            },
            "切换 USB 网络配置",
        ),
        (
            ActionKind::EditApn {
                cid: 3,
                apn: "internet.example".into(),
            },
            "修改 PDP 上下文 3 的 APN",
        ),
    ] {
        let mut controller = Controller::for_test(NOW);
        controller
            .prepare_repair(ControlledRepairRequest::try_from_action(action).unwrap())
            .unwrap();
        let prepared = controller.snapshot().prepared_action.unwrap();
        assert_eq!(
            prepared_action_text(&prepared, Language::ZhCn).text,
            expected
        );
    }
}

#[test]
fn cancelling_parameterized_repairs_never_executes_and_cannot_be_confirmed_later() {
    for action in [
        static_dns(),
        ActionKind::SetVerifiedUsbNetworkProfile {
            profile: UsbNetworkProfile::Ecm,
        },
        ActionKind::EditApn {
            cid: 3,
            apn: "internet.example".into(),
        },
    ] {
        let mut controller = Controller::for_test(NOW);
        let id = controller
            .prepare_repair(ControlledRepairRequest::try_from_action(action).unwrap())
            .unwrap();
        assert_eq!(controller.executor_call_count(), 0);
        controller.cancel_action(id).unwrap();
        assert!(controller.snapshot().prepared_action.is_none());
        assert!(controller.confirm_action(id).is_err());
        assert_eq!(controller.executor_call_count(), 0);
    }

    let controller = Controller::for_test(NOW);
    assert!(ControlledRepairRequest::try_apn(3, "bad,apn").is_err());
    assert!(controller.snapshot().prepared_action.is_none());
    assert_eq!(controller.executor_call_count(), 0);
}

#[test]
fn result_dialog_does_not_guess_parameters_missing_from_the_operation_tag() {
    for action in [
        ActionKindTag::ApplyDnsProfile,
        ActionKindTag::SetVerifiedUsbNetworkProfile,
    ] {
        // This is a presentation test for verified Applied outcomes. The default fake executor
        // supplies no after-state hash, which deliberately maps to OutcomeUnknown instead.
        let mut snapshot = ReducerState::test_ready(NOW).snapshot();
        let operation_id = 7;
        snapshot.operation = Some(OperationUiSnapshot {
            operation_id,
            action,
            started_at: NOW,
            state: OperationState::Finished {
                outcome: OperationOutcome::Applied {
                    after_state_hash: AfterStateHash([1_u8; 32]),
                },
                finished_at: NOW,
            },
        });
        let DialogRequest::Result { message, .. } =
            result_request(&snapshot, operation_id, Language::ZhCn).unwrap();
        if action == ActionKindTag::SetVerifiedUsbNetworkProfile {
            assert!(message.contains("USB 网络配置已保存"));
            assert!(message.contains("需手动重启模块后复检"));
            assert!(message.contains("当前网卡模式尚未验证"));
        } else {
            assert!(message.contains("已应用"));
        }
        assert!(!message.contains("自动获取 DNS"));
        assert!(!message.contains("DJI NDIS"));
    }
}
