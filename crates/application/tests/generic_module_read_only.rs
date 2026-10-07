//! The recognized read-only generic module (`2C7C:0125`), end to end through the application
//! boundary.
//!
//! Scope chosen by the user: **read-only recognition and diagnostics only**. A generic module must
//! be identified, carry its own profile through the snapshots, and pass every read-only gate — and
//! it must be refused, with a reason the UI can show, by every surface that writes to the module or
//! to Windows. The DJI 一代 module (`2CA3:4006`) keeps every right it had.

use std::sync::Arc;
use std::time::SystemTime;

use dji4g_application::{
    ActionReadinessKey, ActionRequest, AtControlAvailability, AtObservation, BackendEvent,
    CheckMask, CheckResult, Controller, DiagnosticCheckId, DiagnosticCheckState,
    FakeActionExecutor, FakeClock, InventoryObservation, PrepareError, ReducerState,
    RefreshCycleId, UiCommand, reduce_state,
};
use dji4g_at_protocol::ValidatedToolLine;
use dji4g_domain::{
    ActionKind, AttachState, Availability, CellularSnapshot, DJI_GEN1, DeviceEpoch, DevicePresence,
    DeviceProfile, LimitedReason, QUECTEL_GENERIC, RegistrationState, SimState,
    StableDeviceIdentity,
};

const GENERIC_INSTANCE: &str = r"USB\VID_2C7C&PID_0125\INSTANCE";

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10_000)
}

fn identity(profile: DeviceProfile) -> StableDeviceIdentity {
    StableDeviceIdentity {
        container_id: "{container}".to_owned(),
        device_instance_id: format!(
            "USB\\VID_{:04X}&PID_{:04X}\\INSTANCE",
            profile.vid, profile.pid
        ),
        vid: profile.vid,
        pid: profile.pid,
    }
}

fn controller_for(profile: DeviceProfile, at: SystemTime) -> Controller {
    Controller::new(
        ReducerState::test_ready_for(profile, at),
        Arc::new(FakeActionExecutor::new()),
        Arc::new(FakeClock::new(at)),
    )
}

/// A minimal read-only cellular reading: the fields the AT path fills when the module answers.
fn cellular(firmware: &str) -> CellularSnapshot {
    CellularSnapshot {
        sim: SimState::Ready,
        registration: RegistrationState::RegisteredHome,
        attached: AttachState::Attached,
        carrier: Some("EXAMPLE".to_owned()),
        radio_access_technology: Some("LTE".to_owned()),
        signal_rssi_dbm: Some(-70),
        apn: Some("internet".to_owned()),
        pdp_address: Some("10.0.0.2".to_owned()),
        pdp_state: Some("active".to_owned()),
        firmware: Some(firmware.to_owned()),
        serving_cell: None,
        sim_identity: None,
        numbers: None,
        temperature_celsius: None,
        temperature_status: dji4g_domain::FeatureStatus::NotProbed,
    }
}

// --- recognition ------------------------------------------------------------------------------

/// The generic module is identified as itself: it is present, it is supported, and neither the
/// snapshot nor the readiness evidence calls it a DJI module or an unsupported device.
#[test]
fn generic_module_is_recognized_with_its_own_profile() {
    let at = now();
    let state = ReducerState::test_ready_for(QUECTEL_GENERIC, at);
    let snapshot = state.snapshot();

    let device = snapshot.app.device.as_ref().expect("device snapshot");
    assert_eq!(device.identity.device_instance_id, GENERIC_INSTANCE);
    assert_eq!(device.identity.profile(), Some(QUECTEL_GENERIC));
    assert!(device.identity.is_supported());
    assert!(!device.identity.allows_controlled_actions());

    // The capability container and the availability verdict are both derived from the presence's
    // profile, so neither can come from a hard-coded DJI profile while the generic one is present.
    let with_inventory = reduce_state(
        &ReducerState::new(at),
        BackendEvent::InventoryFinished {
            cycle: RefreshCycleId(1),
            epoch: DeviceEpoch(1),
            result: CheckResult::Passed {
                value: InventoryObservation {
                    epoch: DeviceEpoch(1),
                    presence: DevicePresence::Supported(QUECTEL_GENERIC),
                    identity: Some(identity(QUECTEL_GENERIC)),
                    problem_code: None,
                    at_port: Some("COM41".to_owned()),
                    adapter_id: Some("{adapter}".to_owned()),
                },
                observed_at: at,
            },
        },
        at,
    );
    assert_eq!(
        with_inventory
            .snapshot()
            .feature_status
            .as_ref()
            .map(|capability| capability.profile()),
        Some(QUECTEL_GENERIC)
    );
    assert_eq!(
        snapshot.app.availability,
        Availability::Limited(LimitedReason::ReadOnlyModule)
    );
}

/// The read path really is open: a fresh generic module produces the same read-only evidence a DJI
/// module does (adapter binding, AT verdict, cellular snapshot, diagnostics).
#[test]
fn generic_module_read_only_evidence_flows_normally() {
    let at = now();
    let epoch = DeviceEpoch(1);
    let state = ReducerState::new(at);
    let state = reduce_state(
        &state,
        BackendEvent::InventoryFinished {
            cycle: RefreshCycleId(1),
            epoch,
            result: CheckResult::Passed {
                value: InventoryObservation {
                    epoch,
                    presence: DevicePresence::Supported(QUECTEL_GENERIC),
                    identity: Some(identity(QUECTEL_GENERIC)),
                    problem_code: None,
                    at_port: Some("COM41".to_owned()),
                    adapter_id: Some("{adapter}".to_owned()),
                },
                observed_at: at,
            },
        },
        at,
    );
    let state = reduce_state(
        &state,
        BackendEvent::AtFinished {
            cycle: RefreshCycleId(1),
            epoch,
            result: CheckResult::Passed {
                value: AtObservation {
                    availability: AtControlAvailability::Available,
                    cellular: Some(cellular("EC200A")),
                },
                observed_at: at,
            },
        },
        at,
    );
    let snapshot = state.snapshot();

    assert_eq!(
        snapshot
            .app
            .device
            .as_ref()
            .map(|device| device.at_port.as_deref()),
        Some(Some("COM41")),
        "the AT interface found by the inventory survives into the snapshot"
    );
    assert_eq!(
        snapshot
            .app
            .cellular
            .as_ref()
            .and_then(|c| c.firmware.as_deref()),
        Some("EC200A"),
        "the AT read-only path accepted the generic module"
    );
    assert_eq!(
        snapshot.diagnostics.get(DiagnosticCheckId::UsbDevice).state,
        DiagnosticCheckState::Passed
    );
    assert_eq!(
        snapshot.diagnostics.get(DiagnosticCheckId::AtControl).state,
        DiagnosticCheckState::Passed
    );

    // The verdict is honest and read-only: the module is inspected and never reported as
    // unsupported, and it can never reach the fully-`Available` master enable.  With only the
    // inventory and AT checks done the data path is genuinely unproven, so the pre-existing
    // incomplete-evidence verdict is the one returned.
    assert_eq!(
        snapshot.app.availability,
        Availability::Limited(LimitedReason::IncompleteEvidence)
    );
    assert!(!matches!(
        snapshot.app.availability,
        Availability::Available | Availability::UnsupportedDevice
    ));
    assert_eq!(snapshot.app.freshness, dji4g_domain::Freshness::Fresh);
    assert_eq!(
        snapshot
            .feature_status
            .as_ref()
            .map(|capability| capability.profile()),
        Some(QUECTEL_GENERIC),
        "the capability container is scoped to the matched profile"
    );
}

// --- refusals ---------------------------------------------------------------------------------

/// Every modeled action (they all write to the module or to Windows) is refused for the generic
/// module with an explicit stable reason, and the same reason reaches the UI as a localized code.
#[test]
fn generic_module_refuses_every_controlled_action_with_a_reason() {
    let at = now();
    let snapshot = ReducerState::test_ready_for(QUECTEL_GENERIC, at).snapshot();

    assert_eq!(snapshot.action_readiness.len(), 8);
    for entry in &snapshot.action_readiness {
        let code = entry
            .ready
            .as_ref()
            .expect_err("every controlled action must be refused");
        assert_eq!(code.category, dji4g_domain::ErrorCode::Unsupported);
        assert_eq!(code.stable.as_str(), "app:read_only_module");
    }
    assert!(
        snapshot
            .action_readiness
            .iter()
            .any(|entry| entry.key == ActionReadinessKey::RestartModule),
        "the readiness list still covers the repair surface"
    );
}

/// Prepare is refused at the same gate, and the reason the user sees is the localization catalog's
/// read-only sentence rather than a generic failure.
#[test]
fn generic_module_cannot_prepare_a_controlled_repair() {
    let at = now();
    let mut controller = controller_for(QUECTEL_GENERIC, at);

    for action in [
        ActionKind::RenewDhcp,
        ActionKind::RestartAdapter,
        ActionKind::ReenumerateDevice,
        ActionKind::RestartModule,
    ] {
        let outcome = controller.handle_command(UiCommand::PrepareAction {
            request: ActionRequest::from(action.clone()),
        });
        assert!(outcome.is_err(), "{action:?} must not be accepted");
        assert!(matches!(
            controller.prepare_action(action),
            Err(PrepareError::Prerequisite(_))
        ));
    }
    let feedback = controller
        .snapshot()
        .feedback
        .as_ref()
        .map(|feedback| feedback.code.stable.as_str().to_owned());
    assert_eq!(feedback.as_deref(), Some("app:read_only_module"));
    assert!(controller.snapshot().prepared_action.is_none());
}

/// SMS send, SMS delete and storage switching all write to the module: each is refused with the
/// same visible reason, and the send rejection releases a pending compose draft exactly like the
/// busy rejection does.
#[test]
fn generic_module_refuses_sms_send_delete_and_storage_switching() {
    let at = now();
    let mut controller = controller_for(QUECTEL_GENERIC, at);

    assert!(
        controller
            .handle_command(UiCommand::SmsSend {
                recipient: "+8613800138000".to_owned(),
                body: "hello".to_owned(),
            })
            .is_err()
    );
    let snapshot = controller.snapshot();
    assert_eq!(snapshot.command_state.sms_send_rejected_seq, 1);
    assert!(snapshot.sms_send.is_none());
    assert_eq!(
        snapshot.feedback.as_ref().unwrap().code.stable.as_str(),
        "app:read_only_module"
    );

    assert!(
        controller
            .handle_command(UiCommand::SmsReadStorage {
                storage: dji4g_domain::SmsStorageId("ME".into()),
            })
            .is_err()
    );
    assert_eq!(
        controller
            .snapshot()
            .feedback
            .as_ref()
            .unwrap()
            .code
            .stable
            .as_str(),
        "app:read_only_module"
    );

    assert!(
        controller
            .handle_command(UiCommand::SmsDelete {
                fragments: Vec::new(),
            })
            .is_err()
    );
    assert_eq!(
        controller
            .snapshot()
            .feedback
            .as_ref()
            .unwrap()
            .code
            .stable
            .as_str(),
        "app:read_only_module"
    );
    assert!(controller.snapshot().sms_delete.is_none());
}

/// The expert AT console writes arbitrary text to the module, so it is refused too — the refusal
/// also lands in the tool state so the console shows why nothing ran. The *read-only* tool reads
/// stay available, because that is exactly the inspection this module is recognized for.
#[test]
fn generic_module_refuses_the_expert_at_console_but_keeps_read_only_tool_reads() {
    let at = now();
    let mut controller = controller_for(QUECTEL_GENERIC, at);
    let line = ValidatedToolLine::parse("AT+VENDOR=1").expect("valid line");

    assert!(
        controller
            .handle_command(UiCommand::PrepareExpertTool { line })
            .is_err()
    );
    let snapshot = controller.snapshot();
    assert!(snapshot.device_tools.pending_expert.is_none());
    assert_eq!(
        snapshot.feedback.as_ref().unwrap().code.stable.as_str(),
        "app:read_only_module"
    );

    // Read-only tool reads remain the module's inspection surface.
    controller
        .handle_command(UiCommand::RunToolRead {
            id: dji4g_at_protocol::ToolReadId::ALL[0],
        })
        .expect("a read-only tool read must stay available");
    assert!(controller.snapshot().device_tools.task.is_some());
}

// --- the DJI module is unchanged ---------------------------------------------------------------

/// The same evidence set, for the DJI module, keeps every right it had: `Available`, every action
/// ready, no read-only refusal anywhere.
#[test]
fn dji_module_keeps_its_full_behaviour() {
    let at = now();
    let snapshot = ReducerState::test_ready(at).snapshot();

    assert_eq!(snapshot.app.availability, Availability::Available);
    assert!(
        snapshot
            .action_readiness
            .iter()
            .all(|entry| entry.ready.is_ok())
    );
    assert_eq!(
        snapshot
            .app
            .device
            .as_ref()
            .and_then(|device| device.identity.profile()),
        Some(DJI_GEN1)
    );

    // And the write path still opens: a controlled repair prepares normally.
    let mut controller = controller_for(DJI_GEN1, at);
    let id = controller
        .prepare_action(ActionKind::RestartModule)
        .expect("the DJI module keeps its repairs");
    assert!(
        controller
            .snapshot()
            .prepared_action
            .is_some_and(|action| action.id == id)
    );

    let mut sms = controller_for(DJI_GEN1, at);
    sms.handle_command(UiCommand::SmsReadStorage {
        storage: dji4g_domain::SmsStorageId("ME".into()),
    })
    .expect("storage switching stays available for the DJI module");
    assert_eq!(sms.snapshot().command_state.sms_send_rejected_seq, 0);
}

/// An unidentified port keeps the pre-existing behaviour: the read-only refusal is keyed to a
/// module the panel positively identified, so it can never widen into a blanket refusal.
#[test]
fn an_unidentified_port_is_not_treated_as_a_read_only_module() {
    let at = now();
    let mut controller = Controller::new(
        ReducerState::new(at),
        Arc::new(FakeActionExecutor::new()),
        Arc::new(FakeClock::new(at)),
    );

    // No device at all: the refusal is the ordinary "no target" one, not the read-only reason.
    assert!(matches!(
        controller.prepare_action(ActionKind::RestartModule),
        Err(PrepareError::Prerequisite(_))
    ));
    let code = controller
        .snapshot()
        .feedback
        .as_ref()
        .map(|feedback| feedback.code.stable.as_str().to_owned());
    assert_ne!(code.as_deref(), Some("app:read_only_module"));

    assert!(
        controller
            .handle_command(UiCommand::SmsReadStorage {
                storage: dji4g_domain::SmsStorageId("ME".into()),
            })
            .is_ok()
    );
    assert_eq!(
        controller
            .snapshot()
            .feedback
            .as_ref()
            .map(|f| f.code.stable.as_str()),
        None
    );
}

/// The empty-evidence guard: a refresh started for a generic module still schedules read-only
/// checks, so the diagnostics page keeps working.
#[test]
fn generic_module_schedules_read_only_diagnostics() {
    let at = now();
    let state = ReducerState::test_ready_for(QUECTEL_GENERIC, at);
    let state = reduce_state(
        &state,
        BackendEvent::RefreshStarted {
            cycle: RefreshCycleId(9),
            epoch: DeviceEpoch(1),
            scheduled: CheckMask::only(DiagnosticCheckId::UsbDevice),
        },
        at,
    );
    let snapshot = state.snapshot();
    assert!(matches!(
        snapshot.diagnostics.get(DiagnosticCheckId::UsbDevice).state,
        DiagnosticCheckState::Running { .. }
    ));
    assert!(
        snapshot
            .app
            .cellular
            .as_ref()
            .is_none_or(|cellular| cellular.firmware.is_none())
    );
}

/// The presence profile is what both the capability scope and the availability verdict follow, so
/// the generic module can never be reported as an unsupported device nor as fully available, and the
/// DJI fixture is unchanged.
#[test]
fn presence_profile_decides_capability_scope_and_availability() {
    let at = now();

    let generic = ReducerState::test_ready_for(QUECTEL_GENERIC, at).snapshot();
    assert_eq!(
        generic
            .app
            .device
            .as_ref()
            .and_then(|device| device.identity.profile()),
        Some(QUECTEL_GENERIC)
    );
    assert_eq!(
        generic.app.availability,
        Availability::Limited(LimitedReason::ReadOnlyModule)
    );
    assert_ne!(
        generic.app.availability,
        Availability::UnsupportedDevice,
        "the generic module is supported, not unsupported"
    );
    assert_ne!(generic.app.availability, Availability::Available);

    let dji = ReducerState::test_ready(at).snapshot();
    assert_eq!(
        dji.app
            .device
            .as_ref()
            .and_then(|device| device.identity.profile()),
        Some(DJI_GEN1)
    );
    assert_eq!(dji.app.availability, Availability::Available);

    // Both fixtures carry the same shape of presence evidence; only the matched profile differs.
    assert_ne!(
        DevicePresence::Supported(QUECTEL_GENERIC),
        DevicePresence::Supported(DJI_GEN1)
    );
    assert_ne!(
        DevicePresence::Supported(QUECTEL_GENERIC),
        DevicePresence::Unsupported {
            vid: 0x2C7C,
            pid: 0x0125
        }
    );
}

#[test]
fn generic_sms_refresh_and_read_are_rejected_without_queueing() {
    for command in [UiCommand::SmsRefresh, UiCommand::SmsRead { index: 1 }] {
        let mut controller = controller_for(QUECTEL_GENERIC, now());
        assert!(controller.handle_command(command).is_err());
        assert!(controller.take_sms_requests().is_empty());
        let snapshot = controller.snapshot();
        assert!(!snapshot.sms_refresh_pending);
        assert_eq!(
            snapshot.feedback.unwrap().code.stable.as_str(),
            "app:read_only_module"
        );
    }
}

#[test]
fn dji_sms_refresh_and_read_still_queue_normally() {
    for command in [UiCommand::SmsRefresh, UiCommand::SmsRead { index: 1 }] {
        let mut controller = controller_for(DJI_GEN1, now());
        assert!(controller.handle_command(command).is_ok());
        assert_eq!(controller.take_sms_requests().len(), 1);
    }
}
