use super::{
    ComCandidate, FunctionRole, PnpNode, PortSelectionError, SelectedPortKind, correlate_topology,
    select_at_candidate,
};

use crate::repair::TargetProof;

fn candidate(
    port: &str,
    role: FunctionRole,
    verified_modem: bool,
    ancestry: &[&str],
) -> ComCandidate {
    ComCandidate {
        port_name: port.to_owned(),
        interface_path: format!(r"\\?\USB#fixture#{port}"),
        container_id: Some("{fixture-container}".to_owned()),
        instance_id: format!(r"USB\VID_2CA3&PID_4006\{port}"),
        hardware_ids: vec![r"USB\VID_2CA3&PID_4006&REV_0318".to_owned()],
        ancestry: ancestry.iter().map(|value| (*value).to_owned()).collect(),
        role,
        verified_modem,
        problem_code: None,
    }
}

fn root_node(instance_id: &str, container_id: Option<&str>) -> PnpNode {
    PnpNode {
        instance_id: instance_id.to_owned(),
        hardware_ids: vec![r"USB\VID_2CA3&PID_4006&REV_0318".to_owned()],
        ancestry: vec![instance_id.to_owned()],
        container_id: container_id.map(str::to_owned),
        ..PnpNode::default()
    }
}

/// The generic Quectel module: root `2C7C:0125` with the usual `MI_02` AT and `MI_04` NET
/// functions.  It is recognized for read-only inspection only, but the inventory must enumerate it
/// or nothing downstream can see it at all.
const GENERIC_ROOT: &str = r"USB\VID_2C7C&PID_0125\SERIAL";

fn generic_root_node(container_id: Option<&str>) -> PnpNode {
    PnpNode {
        instance_id: GENERIC_ROOT.to_owned(),
        hardware_ids: vec![r"USB\VID_2C7C&PID_0125&REV_0001".to_owned()],
        ancestry: vec![GENERIC_ROOT.to_owned()],
        container_id: container_id.map(str::to_owned),
        ..PnpNode::default()
    }
}

fn generic_at_node(port: &str, ancestry: &[&str]) -> PnpNode {
    PnpNode {
        instance_id: format!(r"USB\VID_2C7C&PID_0125&MI_02\{port}"),
        hardware_ids: vec![r"USB\VID_2C7C&PID_0125&MI_02".to_owned()],
        ancestry: ancestry.iter().map(|value| (*value).to_owned()).collect(),
        port_name: Some(port.to_owned()),
        com_interface_path: Some(format!(r"\\?\USB#generic#{port}")),
        role: FunctionRole::DedicatedAt,
        ..PnpNode::default()
    }
}

fn generic_net_node(container_id: Option<&str>, ancestry: &[&str]) -> PnpNode {
    PnpNode {
        instance_id: r"USB\VID_2C7C&PID_0125&MI_04\NET".to_owned(),
        hardware_ids: vec![r"USB\VID_2C7C&PID_0125&MI_04".to_owned()],
        ancestry: ancestry.iter().map(|value| (*value).to_owned()).collect(),
        container_id: container_id.map(str::to_owned),
        net_interface_path: Some(r"\\?\USB#net#{0125}".to_owned()),
        ..PnpNode::default()
    }
}

fn generic_candidate(port: &str, ancestry: &[&str]) -> ComCandidate {
    ComCandidate {
        port_name: port.to_owned(),
        interface_path: format!(r"\\?\USB#generic#{port}"),
        container_id: Some("{fixture-container}".to_owned()),
        instance_id: format!(r"USB\VID_2C7C&PID_0125&MI_02\{port}"),
        hardware_ids: vec![r"USB\VID_2C7C&PID_0125&REV_0001".to_owned()],
        ancestry: ancestry.iter().map(|value| (*value).to_owned()).collect(),
        role: FunctionRole::DedicatedAt,
        verified_modem: false,
        problem_code: None,
    }
}

#[test]
fn topology_correlates_dock_children_to_only_the_pid_4006_root() {
    let root = r"USB\VID_2CA3&PID_4006\SERIAL";
    let dock = r"USB\VID_2109&PID_2817\HUB";
    let nodes = vec![
        PnpNode {
            problem_code: Some(28),
            ..root_node(root, None)
        },
        PnpNode {
            instance_id: r"USB\VID_2CA3&PID_4006&MI_02\AT".to_owned(),
            ancestry: vec![dock.to_owned(), root.to_owned()],
            port_name: Some("COM21".to_owned()),
            com_interface_path: Some(r"\\?\USB#at".to_owned()),
            role: FunctionRole::DedicatedAt,
            ..PnpNode::default()
        },
        PnpNode {
            instance_id: r"USB\VID_2CA3&PID_4006&MI_04\NET".to_owned(),
            ancestry: vec![dock.to_owned(), root.to_owned()],
            net_interface_path: Some(r"\\?\USB#net#{12345678}".to_owned()),
            ..PnpNode::default()
        },
        PnpNode {
            instance_id: r"USB\VID_2CA3&PID_4009\UNSUPPORTED".to_owned(),
            ancestry: vec![r"USB\VID_2CA3&PID_4009\UNSUPPORTED".to_owned()],
            ..PnpNode::default()
        },
    ];

    let snapshot = correlate_topology(nodes);
    assert_eq!(snapshot.devices.len(), 1);
    assert_eq!(snapshot.devices[0].root_instance_id, root);
    assert_eq!(snapshot.devices[0].problem_code, Some(28));
    assert_eq!(snapshot.devices[0].com_candidates.len(), 1);
    assert_eq!(snapshot.devices[0].net_candidates.len(), 1);
    assert_eq!(
        snapshot.devices[0].net_candidates[0].net_cfg_instance_id(),
        None,
        "the GUID-shaped suffix of a NET interface path is not a NetCfgInstanceId"
    );
}

#[test]
fn direct_usb_selects_dedicated_at_and_never_dm_or_nmea() {
    let root = r"USB\VID_2CA3&PID_4006\SERIAL";
    let candidates = vec![
        candidate("COM3", FunctionRole::DmDiag, false, &[root]),
        candidate("COM4", FunctionRole::Nmea, false, &[root]),
        candidate("COM19", FunctionRole::DedicatedAt, false, &[root]),
    ];

    let selected = select_at_candidate(&candidates).expect("dedicated AT must be selected");
    assert_eq!(selected.port_name(), "COM19");
    assert_eq!(selected.kind(), SelectedPortKind::DedicatedAt);
}

#[test]
fn dock_ancestry_survives_com_renumbering() {
    let candidates = vec![candidate(
        "COM27",
        FunctionRole::DedicatedAt,
        false,
        &[
            r"USB\VID_2109&PID_2817\HUB",
            r"USB\VID_2CA3&PID_4006\SERIAL",
        ],
    )];

    assert_eq!(
        select_at_candidate(&candidates).unwrap().port_name(),
        "COM27"
    );
}

#[test]
fn code_28_is_reported_but_does_not_change_identity_selection() {
    let mut at = candidate(
        "COM8",
        FunctionRole::DedicatedAt,
        false,
        &[r"USB\VID_2CA3&PID_4006\SERIAL"],
    );
    at.problem_code = Some(28);

    let selected = select_at_candidate(&[at]).unwrap();
    assert_eq!(selected.problem_code(), Some(28));
}

#[test]
fn unsupported_pid_4009_is_never_selected() {
    let mut at = candidate(
        "COM9",
        FunctionRole::DedicatedAt,
        false,
        &[r"USB\VID_2CA3&PID_4009\OTHER"],
    );
    at.hardware_ids = vec![r"USB\VID_2CA3&PID_4009".to_owned()];
    at.instance_id = r"USB\VID_2CA3&PID_4009\OTHER".to_owned();

    assert_eq!(
        select_at_candidate(&[at]),
        Err(PortSelectionError::NoSafePort)
    );
}

#[test]
fn candidate_identity_cannot_replace_a_pid_4006_ancestry_proof() {
    let at = candidate(
        "COM10",
        FunctionRole::DedicatedAt,
        false,
        &[r"USB\VID_2109&PID_2817\HUB"],
    );

    assert_eq!(
        select_at_candidate(&[at]),
        Err(PortSelectionError::NoSafePort)
    );
}

#[test]
fn com_number_without_a_device_interface_path_is_not_selectable() {
    let mut at = candidate(
        "COM11",
        FunctionRole::DedicatedAt,
        false,
        &[r"USB\VID_2CA3&PID_4006\SERIAL"],
    );
    at.interface_path.clear();

    assert_eq!(
        select_at_candidate(&[at]),
        Err(PortSelectionError::NoSafePort)
    );
}

#[test]
fn verified_modem_is_only_a_fallback() {
    let modem = candidate(
        "COM12",
        FunctionRole::Modem,
        true,
        &[r"USB\VID_2CA3&PID_4006\SERIAL"],
    );

    let selected = select_at_candidate(&[modem]).unwrap();
    assert_eq!(selected.kind(), SelectedPortKind::VerifiedModem);
}

#[test]
fn unverified_modem_is_never_probed() {
    let modem = candidate(
        "COM13",
        FunctionRole::Modem,
        false,
        &[r"USB\VID_2CA3&PID_4006\SERIAL"],
    );

    assert_eq!(
        select_at_candidate(&[modem]),
        Err(PortSelectionError::NoSafePort)
    );
}

/// Generic usbser drivers frequently leave every serial function unclassified. A single
/// unclassified COM function under the proven exact-identity root is safe to trust (the AT
/// actor still verifies the protocol); two or more stay ambiguous and none is tried.
#[test]
fn single_unclassified_residue_under_the_proven_root_is_trusted() {
    let unknown = candidate(
        "COM14",
        FunctionRole::Unknown,
        false,
        &[r"USB\VID_2CA3&PID_4006\SERIAL"],
    );

    let selected = select_at_candidate(&[unknown]).unwrap();
    assert_eq!(selected.port_name(), "COM14");
    assert_eq!(selected.kind(), SelectedPortKind::DedicatedAt);
}

#[test]
fn multiple_unclassified_ports_stay_ambiguous() {
    let root = r"USB\VID_2CA3&PID_4006\SERIAL";
    let candidates = vec![
        candidate("COM13", FunctionRole::Modem, false, &[root]),
        candidate("COM14", FunctionRole::Unknown, false, &[root]),
        candidate("COM15", FunctionRole::Unknown, false, &[root]),
    ];

    assert!(matches!(
        select_at_candidate(&candidates),
        Err(PortSelectionError::AmbiguousPort { .. })
    ));
}

#[test]
fn classified_dm_and_nmea_are_excluded_leaving_a_single_residue_at() {
    let root = r"USB\VID_2CA3&PID_4006\SERIAL";
    let candidates = vec![
        candidate("COM3", FunctionRole::DmDiag, false, &[root]),
        candidate("COM4", FunctionRole::Nmea, false, &[root]),
        candidate("COM5", FunctionRole::Unknown, false, &[root]),
    ];

    let selected = select_at_candidate(&candidates).expect("the one unclassified port wins");
    assert_eq!(selected.port_name(), "COM5");
    assert_eq!(selected.kind(), SelectedPortKind::DedicatedAt);
}

#[test]
fn residue_outside_the_proven_root_is_never_trusted() {
    let unknown = candidate(
        "COM14",
        FunctionRole::Unknown,
        false,
        &[r"USB\VID_2109&PID_2817\HUB"],
    );

    assert_eq!(
        select_at_candidate(&[unknown]),
        Err(PortSelectionError::NoSafePort)
    );
}

#[test]
fn equally_valid_dedicated_ports_are_ambiguous() {
    let root = r"USB\VID_2CA3&PID_4006\SERIAL";
    let candidates = vec![
        candidate("COM15", FunctionRole::DedicatedAt, false, &[root]),
        candidate("COM16", FunctionRole::DedicatedAt, false, &[root]),
    ];

    assert_eq!(
        select_at_candidate(&candidates),
        Err(PortSelectionError::AmbiguousPort { count: 2 })
    );
}

#[test]
fn exact_usb_identity_rejects_suffixes_malformed_tokens_and_contradictions() {
    let invalid_roots = [
        r"USB\VID_2CA3&PID_40060\SERIAL",
        r"USB\VID_2CA3&PID_4006_SUFFIX\SERIAL",
        r"USB\XVID_2CA3&PID_4006\SERIAL",
        r"USB\VID_2CA3&PID_4006&PID_4009\SERIAL",
    ];

    for invalid_root in invalid_roots {
        let mut at = candidate("COM30", FunctionRole::DedicatedAt, false, &[invalid_root]);
        at.hardware_ids = vec![invalid_root.to_owned()];
        assert_eq!(
            select_at_candidate(&[at]),
            Err(PortSelectionError::NoSafePort),
            "invalid identity was accepted: {invalid_root}"
        );
    }
}

#[test]
fn child_instance_cannot_create_a_device_without_a_proven_root_node() {
    let child = PnpNode {
        instance_id: r"USB\VID_2CA3&PID_4006&MI_02\AT".to_owned(),
        hardware_ids: vec![r"USB\VID_2CA3&PID_4006&MI_02".to_owned()],
        ancestry: vec![r"USB\VID_2CA3&PID_4006&MI_02\AT".to_owned()],
        port_name: Some("COM31".to_owned()),
        com_interface_path: Some(r"\\?\USB#at".to_owned()),
        role: FunctionRole::DedicatedAt,
        ..PnpNode::default()
    };

    assert!(correlate_topology(vec![child]).devices.is_empty());
}

#[test]
fn root_requires_matching_exact_instance_and_hardware_identity() {
    let root = r"USB\VID_2CA3&PID_4006\SERIAL";
    let child = PnpNode {
        instance_id: r"USB\VID_2CA3&PID_4006&MI_02\AT".to_owned(),
        ancestry: vec![root.to_owned()],
        port_name: Some("COM32".to_owned()),
        com_interface_path: Some(r"\\?\USB#at".to_owned()),
        role: FunctionRole::DedicatedAt,
        ..PnpNode::default()
    };
    let mut contradictory_root = root_node(root, None);
    contradictory_root.hardware_ids = vec![r"USB\VID_2CA3&PID_4009".to_owned()];

    assert!(
        correlate_topology(vec![contradictory_root, child])
            .devices
            .is_empty()
    );
}

#[test]
fn matching_container_ids_allow_direct_and_dock_children() {
    let root = r"USB\VID_2CA3&PID_4006\SERIAL";
    let container = "{AABBCCDD-0000-0000-0000-000000000001}";
    let direct = PnpNode {
        instance_id: r"USB\VID_2CA3&PID_4006&MI_02\AT".to_owned(),
        ancestry: vec![root.to_owned()],
        container_id: Some(container.to_ascii_lowercase()),
        port_name: Some("COM33".to_owned()),
        com_interface_path: Some(r"\\?\USB#direct".to_owned()),
        role: FunctionRole::DedicatedAt,
        ..PnpNode::default()
    };
    let dock = PnpNode {
        instance_id: r"USB\VID_2CA3&PID_4006&MI_04\NET".to_owned(),
        ancestry: vec![r"USB\VID_2109&PID_2817\HUB".to_owned(), root.to_owned()],
        container_id: Some(container.to_owned()),
        net_interface_path: Some(r"\\?\USB#net#{12345678}".to_owned()),
        ..PnpNode::default()
    };

    let snapshot = correlate_topology(vec![root_node(root, Some(container)), direct, dock]);
    assert_eq!(snapshot.devices.len(), 1);
    assert_eq!(snapshot.devices[0].com_candidates.len(), 1);
    assert_eq!(snapshot.devices[0].net_candidates.len(), 1);
}

#[test]
fn missing_child_container_uses_proven_parent_ancestry() {
    let root = r"USB\VID_2CA3&PID_4006\SERIAL";
    let child = PnpNode {
        instance_id: r"USB\VID_2CA3&PID_4006&MI_02\AT".to_owned(),
        ancestry: vec![root.to_owned()],
        port_name: Some("COM34".to_owned()),
        com_interface_path: Some(r"\\?\USB#at".to_owned()),
        role: FunctionRole::DedicatedAt,
        ..PnpNode::default()
    };

    let snapshot = correlate_topology(vec![root_node(root, Some("{fixture-container}")), child]);
    assert_eq!(snapshot.devices[0].com_candidates.len(), 1);
}

#[test]
fn conflicting_child_container_is_excluded_case_insensitively() {
    let root = r"USB\VID_2CA3&PID_4006\SERIAL";
    let child = PnpNode {
        instance_id: r"USB\VID_2CA3&PID_4006&MI_02\AT".to_owned(),
        ancestry: vec![root.to_owned()],
        container_id: Some("{different-container}".to_owned()),
        port_name: Some("COM35".to_owned()),
        com_interface_path: Some(r"\\?\USB#at".to_owned()),
        role: FunctionRole::DedicatedAt,
        ..PnpNode::default()
    };

    let snapshot = correlate_topology(vec![root_node(root, Some("{fixture-container}")), child]);
    assert_eq!(snapshot.devices.len(), 1);
    assert!(snapshot.devices[0].com_candidates.is_empty());
}

#[test]
fn container_id_alone_never_creates_a_target_device() {
    let node = PnpNode {
        instance_id: r"USB\VID_2109&PID_2817\HUB".to_owned(),
        hardware_ids: vec![r"USB\VID_2109&PID_2817".to_owned()],
        ancestry: vec![r"USB\VID_2109&PID_2817\HUB".to_owned()],
        container_id: Some("{fixture-container}".to_owned()),
        port_name: Some("COM36".to_owned()),
        com_interface_path: Some(r"\\?\USB#not-target".to_owned()),
        role: FunctionRole::DedicatedAt,
        ..PnpNode::default()
    };

    assert!(correlate_topology(vec![node]).devices.is_empty());
}

/// The exact topology reported from the user's machine, where the panel claimed 未检测到 while the
/// module was demonstrably present and providing internet.
///
/// One live composite root (`USB\VID_2CA3&PID_4006\5&19E527CA&0&4`, Status OK) whose HardwareIds
/// are clean and exact — the `COMPOSITE`/class entries live in CompatibleIds, not HardwareIds — one
/// live RNDIS NIC on `MI_00` (Status OK), and the Baiwang AT serial interfaces on `MI_02..MI_05` in
/// an error state.  An errored serial function publishes no working COM device interface, so it
/// contributes no COM candidate at all.
///
/// This pins the property the misdiagnosis depended on: an unavailable AT port degrades *AT
/// selection only*.  It must never turn one physical module into zero devices (`NotDetected`) or
/// into several (`ambiguous_device`), and the working NIC must still be reported.
#[test]
fn live_root_with_working_nic_and_errored_at_interfaces_is_one_device_without_a_safe_at_port() {
    let root = r"USB\VID_2CA3&PID_4006\5&19E527CA&0&4";
    let container = "{module-container}";

    let mut nodes = vec![
        PnpNode {
            instance_id: root.to_owned(),
            hardware_ids: vec![
                r"USB\VID_2CA3&PID_4006&REV_0318".to_owned(),
                r"USB\VID_2CA3&PID_4006".to_owned(),
            ],
            ancestry: vec![root.to_owned()],
            container_id: Some(container.to_owned()),
            problem_code: None,
            ..PnpNode::default()
        },
        PnpNode {
            instance_id: r"USB\VID_2CA3&PID_4006&MI_00\6&22F7C4B7&0&0000".to_owned(),
            ancestry: vec![root.to_owned()],
            container_id: Some(container.to_owned()),
            net_interface_path: Some(r"\\?\USB#VID_2CA3&PID_4006&MI_00#rndis".to_owned()),
            net_cfg_instance_id: Some("{rndis-adapter-guid}".to_owned()),
            problem_code: None,
            ..PnpNode::default()
        },
    ];

    // The Baiwang serial/AT functions (MI_02..MI_05) are in Error/Unknown state: the driver failed,
    // so no COM port name and no device interface path were ever published.
    for (index, problem_code) in [(2usize, 28u32), (3, 10), (4, 28), (5, 10)] {
        nodes.push(PnpNode {
            instance_id: format!(r"USB\VID_2CA3&PID_4006&MI_0{index}\6&ERROR&0&000{index}"),
            ancestry: vec![root.to_owned()],
            container_id: Some(container.to_owned()),
            role: FunctionRole::DedicatedAt,
            problem_code: Some(problem_code),
            ..PnpNode::default()
        });
    }

    let snapshot = correlate_topology(nodes);
    assert_eq!(
        snapshot.devices.len(),
        1,
        "one physical module must stay exactly one device"
    );

    let device = &snapshot.devices[0];
    assert_eq!(device.root_instance_id, root);
    assert_eq!(
        device.container_id.as_deref(),
        Some(container),
        "the root identity must survive so later stages can re-find the same module"
    );
    assert_eq!(
        device.problem_code, None,
        "the live composite root itself is healthy"
    );
    assert_eq!(device.net_candidates.len(), 1);
    assert_eq!(
        device.net_candidates[0].net_cfg_instance_id(),
        Some("{rndis-adapter-guid}"),
        "the working RNDIS adapter must still be reported"
    );
    assert_eq!(
        device.net_candidates[0].problem_code, None,
        "the live NIC is healthy"
    );
    assert!(
        device.com_candidates.is_empty(),
        "errored serial functions publish no COM interface, matching the reported com:[]"
    );
    assert!(
        matches!(device.select_at_port(), Err(PortSelectionError::NoSafePort)),
        "AT selection must fail closed rather than guess a port"
    );
}

use super::{AtPortSelection, PortProvenance, select_at_port_verified};

#[test]
fn verified_selection_keeps_classified_tiers_first() {
    let root = r"USB\VID_2CA3&PID_4006\SERIAL";
    let candidates = vec![
        candidate("COM5", FunctionRole::Unknown, false, &[root]),
        candidate("COM6", FunctionRole::DedicatedAt, false, &[root]),
    ];

    let selection = select_at_port_verified(&candidates, &[], 2).expect("classified tier must win");
    match &selection {
        AtPortSelection::Classified(port) => {
            assert_eq!(port.port_name(), "COM6");
            assert_eq!(
                selection_provenance(&selection),
                PortProvenance::RoleClassified
            );
        }
        AtPortSelection::HandshakeCandidates(_) => {
            panic!("a classified tier must never be handed to the prober")
        }
    }
}

fn selection_provenance(selection: &AtPortSelection) -> PortProvenance {
    match selection {
        AtPortSelection::Classified(_) => PortProvenance::RoleClassified,
        AtPortSelection::HandshakeCandidates(_) => PortProvenance::HandshakeVerified,
    }
}

#[test]
fn single_residue_stays_classified_not_probed() {
    let root = r"USB\VID_2CA3&PID_4006\SERIAL";
    let candidates = vec![candidate("COM5", FunctionRole::Unknown, false, &[root])];

    let selection = select_at_port_verified(&candidates, &[], 2).expect("one residue is enough");
    let port = match &selection {
        AtPortSelection::Classified(port) => port,
        AtPortSelection::HandshakeCandidates(_) => {
            panic!("a single unclassified residue stays classified, never probed")
        }
    };
    assert_eq!(port.port_name(), "COM5");
    assert_eq!(
        selection_provenance(&selection),
        PortProvenance::RoleClassified
    );
}

#[test]
fn ambiguous_residue_is_ordered_and_bounded_for_probing() {
    let root = r"USB\VID_2CA3&PID_4006\SERIAL";
    let candidates = vec![
        candidate("COM9", FunctionRole::Unknown, false, &[root]),
        candidate("COM3", FunctionRole::Unknown, false, &[root]),
        candidate("COM5", FunctionRole::Unknown, false, &[root]),
    ];

    let selection = select_at_port_verified(&candidates, &[], 2).expect("probe list required");
    let AtPortSelection::HandshakeCandidates(probed) = selection else {
        panic!("ambiguous residue must produce probe candidates");
    };
    assert_eq!(probed.len(), 2, "the budget caps one cycle's probes");
    // Interface paths sort deterministically (`\?\USB#fixture#COM3` < `...COM5`); COM numbers
    // renumber across replugs, paths do not.
    assert_eq!(probed[0].port_name(), "COM3");
    assert_eq!(probed[1].port_name(), "COM5");
}

#[test]
fn already_failed_candidates_are_skipped_within_the_budget() {
    let root = r"USB\VID_2CA3&PID_4006\SERIAL";
    let candidates = vec![
        candidate("COM3", FunctionRole::Unknown, false, &[root]),
        candidate("COM5", FunctionRole::Unknown, false, &[root]),
        candidate("COM7", FunctionRole::Unknown, false, &[root]),
    ];
    let failed = vec![
        r"\\?\USB#fixture#COM3".to_owned(),
        r"\\?\USB#fixture#COM5".to_owned(),
    ];

    let selection = select_at_port_verified(&candidates, &failed, 2).expect("COM7 remains");
    let AtPortSelection::HandshakeCandidates(probed) = selection else {
        panic!("a surviving candidate must be probed");
    };
    assert_eq!(probed.len(), 1);
    assert_eq!(probed[0].port_name(), "COM7");
}

#[test]
fn exhausted_candidates_stay_ambiguous_and_fail_closed() {
    let root = r"USB\VID_2CA3&PID_4006\SERIAL";
    let candidates = vec![candidate("COM3", FunctionRole::Unknown, false, &[root])];
    let failed = vec![r"\\?\USB#fixture#COM3".to_owned()];

    assert!(matches!(
        select_at_port_verified(&candidates, &failed, 2),
        Err(PortSelectionError::AmbiguousPort { count: 1 })
    ));
}

#[test]
fn candidates_outside_the_proven_root_are_never_probed() {
    let hub = r"USB\VID_2109&PID_2817\HUB";
    let candidates = vec![candidate("COM14", FunctionRole::Unknown, false, &[hub])];

    assert!(matches!(
        select_at_port_verified(&candidates, &[], 2),
        Err(PortSelectionError::AmbiguousPort { count: 0 })
    ));
}

#[test]
fn two_dedicated_at_ports_become_handshake_candidates() {
    // Real hardware exposes two "AT Port"-named functions; the handshake disambiguates instead
    // of the selection refusing them outright.
    let root = r"USB\VID_2CA3&PID_4006\SERIAL";
    let candidates = vec![
        candidate("COM5", FunctionRole::DedicatedAt, false, &[root]),
        candidate("COM3", FunctionRole::DedicatedAt, false, &[root]),
    ];

    let selection =
        select_at_port_verified(&candidates, &[], 2).expect("classified candidates must probe");
    let AtPortSelection::HandshakeCandidates(probed) = selection else {
        panic!("two classified AT ports must be disambiguated by the handshake");
    };
    assert_eq!(probed.len(), 2);
    assert_eq!(probed[0].port_name(), "COM3");
    assert_eq!(probed[1].port_name(), "COM5");
}

#[test]
fn one_dedicated_port_is_still_trusted_without_probing() {
    let root = r"USB\VID_2CA3&PID_4006\SERIAL";
    let candidates = vec![
        candidate("COM3", FunctionRole::DedicatedAt, false, &[root]),
        candidate("COM4", FunctionRole::Unknown, false, &[root]),
    ];

    let selection = select_at_port_verified(&candidates, &[], 2).expect("classified tier wins");
    assert!(matches!(
        selection,
        AtPortSelection::Classified(_) if selection_provenance(&selection) == PortProvenance::RoleClassified
    ));
}

// --- read-only generic module (`2C7C:0125`) --------------------------------------------------
//
// The inventory must enumerate the generic module exactly like the DJI one — the AT and NET
// interfaces are what the read-only checks run against — while every write path stays keyed to the
// write-capable profile (see `DeviceProfile::allows_controlled_actions` and `TargetProof`).

#[test]
fn generic_module_root_with_at_and_net_interfaces_is_enumerated() {
    let nodes = vec![
        generic_root_node(Some("{fixture-container}")),
        generic_at_node("COM41", &[GENERIC_ROOT]),
        generic_net_node(Some("{fixture-container}"), &[GENERIC_ROOT]),
    ];

    let snapshot = correlate_topology(nodes);
    assert_eq!(
        snapshot.devices.len(),
        1,
        "the generic module must be a target device"
    );
    let device = &snapshot.devices[0];
    assert_eq!(device.root_instance_id(), GENERIC_ROOT);
    assert_eq!(
        device.profile(),
        Some(dji4g_domain::QUECTEL_GENERIC),
        "the inventory must report the profile it proved, not the DJI one"
    );
    assert_eq!(device.com_candidates().len(), 1);
    assert_eq!(device.net_candidates().len(), 1);
    assert_eq!(
        device.select_at_port().unwrap().port_name(),
        "COM41",
        "the MI_02 AT function must be discoverable for the read-only checks"
    );
}

#[test]
fn generic_module_identity_is_not_write_eligible() {
    let profile = dji4g_domain::QUECTEL_GENERIC;
    assert!(!profile.allows_controlled_actions());
    assert!(dji4g_domain::DeviceProfile::DJI_GEN1.allows_controlled_actions());
    assert_eq!(
        dji4g_domain::DeviceProfile::from_vid_pid(0x2C7C, 0x0125),
        Some(profile)
    );

    // The repair executor's target proof is minted from the same profile: the generic module can
    // never satisfy it.
    assert!(!TargetProof::for_profile(profile, [0x11; 32]).is_supported());
    assert!(TargetProof::for_profile(dji4g_domain::DJI_GEN1, [0x11; 32]).is_supported());
}

#[test]
fn generic_module_at_selection_survives_dock_ancestry_and_rejects_a_hub_only_ancestry() {
    let dock = r"USB\VID_2109&PID_2817\HUB";
    let docked = generic_candidate("COM42", &[dock, GENERIC_ROOT]);
    assert_eq!(select_at_candidate(&[docked]).unwrap().port_name(), "COM42");

    let hub_only = generic_candidate("COM43", &[dock]);
    assert_eq!(
        select_at_candidate(&[hub_only]),
        Err(PortSelectionError::NoSafePort)
    );
}

#[test]
fn a_generic_root_with_dji_hardware_ids_is_not_a_proven_root() {
    let mut contradictory = generic_root_node(None);
    contradictory.hardware_ids = vec![r"USB\VID_2CA3&PID_4006&REV_0318".to_owned()];

    assert!(
        correlate_topology(vec![contradictory]).devices.is_empty(),
        "a root must still prove one consistent profile in both its instance id and hardware ids"
    );
}

#[test]
fn unlisted_quectel_pid_is_still_rejected() {
    let root = r"USB\VID_2C7C&PID_0126\SERIAL";
    let mut at = generic_candidate("COM44", &[root]);
    at.instance_id = r"USB\VID_2C7C&PID_0126\OTHER".to_owned();
    at.hardware_ids = vec![r"USB\VID_2C7C&PID_0126".to_owned()];

    assert_eq!(
        select_at_candidate(&[at]),
        Err(PortSelectionError::NoSafePort)
    );
}

#[test]
fn generic_sms_entry_points_refuse_before_any_serial_open() {
    use dji4g_domain::{
        DeviceEpoch, SmsDeleteControl, SmsFragmentKey, SmsReadControl, SmsStorageId,
        SmsTransactionControl,
    };
    use std::time::Duration;
    let inventory = correlate_topology(vec![generic_root_node(None)]);
    let device = &inventory.devices()[0];
    let epoch = DeviceEpoch(1);
    assert_eq!(
        crate::sms::require_sms_device(device).unwrap_err().code,
        "sms:read_only_module"
    );
    assert_eq!(
        crate::sms_set_pdu_mode(device, epoch).unwrap_err().code,
        "sms:read_only_module"
    );
    assert_eq!(
        crate::sms_read_verified(device, epoch, None, 1)
            .unwrap_err()
            .code,
        "sms:read_only_module"
    );
    assert_eq!(
        crate::sms_delete(device, epoch, 1).unwrap_err().code,
        "sms:read_only_module"
    );
    assert_eq!(
        crate::sms_send(device, epoch, "invalid", "test")
            .unwrap_err()
            .code,
        "sms:read_only_module"
    );
    assert_eq!(
        crate::sms_list_controlled(
            device,
            epoch,
            None,
            None,
            SmsReadControl::new(Duration::from_secs(1))
        )
        .unwrap_err()
        .code,
        "sms:read_only_module"
    );
    let send = crate::sms_send_controlled(
        device,
        epoch,
        None,
        "invalid",
        "test",
        SmsTransactionControl::new(Duration::from_secs(1)),
    );
    assert_eq!(send.failure.unwrap().code, "sms:read_only_module");
    let fragment = SmsFragmentKey {
        device_epoch: 1,
        sim_epoch: 0,
        storage: SmsStorageId("SM".into()),
        index: 1,
        payload_fingerprint: [0; 32],
    };
    let receipt = crate::sms_delete_checked(
        device,
        epoch,
        None,
        &fragment,
        SmsDeleteControl::new(Duration::from_secs(1)),
    );
    assert_eq!(receipt.code.as_deref(), Some("sms:read_only_module"));
    let mut unknown = device.clone();
    unknown.root_instance_id = r"USB\VID_1234&PID_5678\UNKNOWN".into();
    assert_eq!(
        crate::sms::require_sms_device(&unknown).unwrap_err().code,
        "sms:unsupported"
    );
    let dji = correlate_topology(vec![root_node(r"USB\VID_2CA3&PID_4006\SERIAL", None)]);
    crate::sms::require_sms_device(&dji.devices()[0]).unwrap();
}
