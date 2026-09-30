use dji4g_application::{
    ActionReadinessKey, BackendEvent, CheckResult, DeviceEpoch, FailureCode, ReducerState,
    RefreshCycleId, StableCode, reduce_state,
};
use dji4g_domain::{ActionKind, ErrorCode, HotspotStatus};
use dji4g_panel::{
    localization::{Language, TextKey, failure_text},
    ui::repairs_vm,
};
use std::time::SystemTime;

const NOW: SystemTime = SystemTime::UNIX_EPOCH;

fn failure(stable: &'static str) -> FailureCode {
    FailureCode::new(
        ErrorCode::Unsupported,
        StableCode::try_from_static(stable).unwrap(),
    )
}

#[test]
fn dhcp_disabled_feedback_is_precise_in_both_requested_languages() {
    let code = failure("repair:dhcp_disabled");
    let chinese = failure_text(&code, Language::ZhCn);
    assert_eq!(chinese.key, TextKey::RepairDhcpDisabled);
    assert_eq!(chinese.text, "模块网卡未启用 IPv4 DHCP，无法续租。");
    let english = failure_text(&code, Language::EnUs);
    assert_eq!(english.key, TextKey::RepairDhcpDisabled);
    assert_eq!(
        english.text,
        "IPv4 DHCP is disabled on the module adapter; its lease cannot be renewed."
    );
}

#[test]
fn failed_hotspot_uses_shared_readiness_and_unsupported_has_visible_reason() {
    for (result, expected_enabled) in [
        (
            CheckResult::Failed {
                code: failure("hotspot:operation_failed"),
                observed_at: NOW,
            },
            true,
        ),
        (
            CheckResult::Unavailable {
                code: failure("hotspot:unsupported"),
                observed_at: NOW,
            },
            false,
        ),
    ] {
        let state = reduce_state(
            &ReducerState::test_ready(NOW),
            BackendEvent::HotspotFinished {
                cycle: RefreshCycleId(1),
                epoch: DeviceEpoch(1),
                result,
            },
            NOW,
        );
        let snapshot = state.snapshot();
        assert!(matches!(
            snapshot.app.hotspot,
            HotspotStatus::Failed { .. } | HotspotStatus::Unsupported(_)
        ));
        let ready = snapshot
            .action_readiness
            .iter()
            .find(|entry| entry.key == ActionReadinessKey::ToggleHotspot)
            .unwrap()
            .ready
            .is_ok();
        assert_eq!(ready, expected_enabled);
        let vm = repairs_vm(&snapshot, NOW, Language::ZhCn);
        let toggle = vm
            .actions
            .iter()
            .find(|action| matches!(action.action, ActionKind::ToggleHotspot { .. }))
            .unwrap();
        assert_eq!(toggle.enabled, ready);
        if !toggle.enabled {
            assert!(!toggle.disabled_reason.as_ref().unwrap().text.is_empty());
        }
    }
}
