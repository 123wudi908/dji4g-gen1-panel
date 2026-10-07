use dji4g_at_protocol::{Apn, PdpContextId, VerifiedUsbNetProfile};
use dji4g_domain::{
    AfterStateHash, DeviceEpoch, DnsProfile, ErrorCode, OperationOutcome, RollbackOutcome,
};
use dji4g_windows_platform::{
    AdapterProof, DispatchResult, FakeRepairBackend, HotspotProof, RepairAction, RepairBackend,
    RepairError, RepairObservation, TargetProof, WindowsRepairExecutor,
};
use std::net::{IpAddr, Ipv4Addr};

fn apn(value: &str) -> Apn {
    Apn::try_from(value).expect("fixture APN")
}

fn ready_backend() -> FakeRepairBackend {
    FakeRepairBackend::ready(RepairObservation::fixture(
        DeviceEpoch(4),
        11,
        TargetProof::dji_gen1([0x11; 32]),
        AdapterProof::fixture([0x22; 32]),
    ))
}

#[test]
fn refresh_dhcp_revalidates_and_applies_with_one_native_mutation() {
    let executor = WindowsRepairExecutor::new(ready_backend());
    let plan = executor
        .prepare(RepairAction::RefreshDhcp)
        .expect("prepare");

    let result = executor.execute(&plan);

    assert!(matches!(
        result.outcome(),
        OperationOutcome::Applied {
            after_state_hash: AfterStateHash(_)
        }
    ));
    assert_eq!(executor.backend().mutation_count(), 1);
}

/// The recognized read-only generic module can never be the target of a controlled repair: no
/// plan may be prepared for it and no native write may happen.
#[test]
fn recognized_read_only_module_can_never_prepare_or_execute_a_repair() {
    let generic = TargetProof::for_profile(dji4g_domain::QUECTEL_GENERIC, [0x77; 32]);
    assert!(!generic.is_supported());
    let backend = FakeRepairBackend::ready(RepairObservation::fixture(
        DeviceEpoch(4),
        11,
        generic,
        AdapterProof::fixture([0x22; 32]),
    ));
    let executor = WindowsRepairExecutor::new(backend);

    for action in [
        RepairAction::RefreshDhcp,
        RepairAction::RestartAdapter,
        RepairAction::ReenumerateDevice,
        RepairAction::RestartModule,
        RepairAction::SetUsbNetProfile {
            profile: VerifiedUsbNetProfile::DjiNdis,
        },
    ] {
        assert!(
            matches!(executor.prepare(action), Err(RepairError::Unsupported)),
            "a read-only module must not accept a repair plan"
        );
    }
    assert_eq!(executor.backend().mutation_count(), 0);
}

#[test]
fn stale_before_state_fails_closed_without_dispatch() {
    let mut backend = ready_backend();
    let mut executor = WindowsRepairExecutor::new(backend.clone());
    let plan = executor
        .prepare(RepairAction::RestartAdapter)
        .expect("prepare");

    // The before-state hash covers only action-relevant fields; an unrelated observation change
    // (here a bumped revision) must NOT fail a wanted repair...
    backend.mutate_observation(|observation| observation.revision += 1);
    executor.replace_backend(backend);
    let unrelated = executor.execute(&plan);
    assert!(
        matches!(unrelated.outcome(), OperationOutcome::Applied { .. }),
        "unrelated state churn must not fail an action-relevant revalidation"
    );
    assert_eq!(executor.backend().mutation_count(), 1);

    // ...while a change to the adapter the action restarts must fail closed.
    let mut changed = ready_backend();
    let mut executor = WindowsRepairExecutor::new(changed.clone());
    let plan = executor
        .prepare(RepairAction::RestartAdapter)
        .expect("prepare");
    changed.mutate_observation(|observation| observation.adapter_up = false);
    executor.replace_backend(changed);
    let result = executor.execute(&plan);

    assert!(matches!(
        result.outcome(),
        OperationOutcome::Failed {
            code: ErrorCode::EvidenceExpired,
            ..
        }
    ));
    assert_eq!(executor.backend().mutation_count(), 0);
}

#[test]
fn timeout_or_removal_is_outcome_unknown_and_never_retried() {
    let mut backend = ready_backend();
    backend.set_dispatch_result(DispatchResult::Unknown(ErrorCode::Timeout));
    let executor = WindowsRepairExecutor::new(backend);
    let plan = executor
        .prepare(RepairAction::RestartModule)
        .expect("prepare");

    let result = executor.execute(&plan);

    assert!(matches!(
        result.outcome(),
        OperationOutcome::OutcomeUnknown {
            code: ErrorCode::Timeout
        }
    ));
    assert_eq!(executor.backend().mutation_count(), 1);
}

#[test]
fn typed_writes_and_apn_inactive_guard_are_enforced() {
    let mut backend = ready_backend();
    backend.set_contexts(&[(1, "IP", "old.example", false)]);
    let executor = WindowsRepairExecutor::new(backend);
    let cid = PdpContextId::try_from(1).unwrap();
    let plan = executor
        .prepare(RepairAction::SetApn {
            cid,
            apn: apn("new.example"),
        })
        .expect("inactive existing CID");
    let result = executor.execute(&plan);
    assert!(matches!(result.outcome(), OperationOutcome::Applied { .. }));
    assert_eq!(executor.backend().last_typed_write(), Some("SetApn"));

    let mut active_backend = ready_backend();
    active_backend.set_contexts(&[(1, "IP", "old.example", true)]);
    let active_executor = WindowsRepairExecutor::new(active_backend);
    assert!(matches!(
        active_executor.prepare(RepairAction::SetApn {
            cid,
            apn: apn("new.example")
        }),
        Err(RepairError::PdpContextActive)
    ));
}

#[test]
fn apn_write_rejects_ipv6_and_dual_stack_contexts_before_dispatch() {
    for pdp_type in ["IPV6", "IPV4V6"] {
        let mut backend = ready_backend();
        backend.set_contexts(&[(1, pdp_type, "old.example", false)]);
        let executor = WindowsRepairExecutor::new(backend);
        let cid = PdpContextId::try_from(1).unwrap();
        assert!(matches!(
            executor.prepare(RepairAction::SetApn {
                cid,
                apn: apn("new.example")
            }),
            Err(RepairError::PdpContextIncomplete)
        ));
        assert_eq!(executor.backend().mutation_count(), 0);
    }
}

#[test]
fn usb_profile_is_closed_to_zero_or_one_and_readback_mismatch_fails() {
    let mut backend = ready_backend();
    backend.set_usb_profile(Some(VerifiedUsbNetProfile::DjiNdis));
    backend.set_after_usb_profile(Some(VerifiedUsbNetProfile::DjiNdis));
    let executor = WindowsRepairExecutor::new(backend);
    let plan = executor
        .prepare(RepairAction::SetUsbNetProfile {
            profile: VerifiedUsbNetProfile::Ecm,
        })
        .expect("prepare");
    let result = executor.execute(&plan);
    assert!(matches!(result.outcome(), OperationOutcome::Failed { .. }));
    assert_eq!(executor.backend().mutation_count(), 1);
}

#[test]
fn uac_cancel_is_failed_with_zero_writes() {
    let mut backend = ready_backend();
    backend.set_dispatch_result(DispatchResult::Rejected(ErrorCode::OperationCancelled));
    let executor = WindowsRepairExecutor::new(backend);
    let plan = executor
        .prepare(RepairAction::RestartAdapter)
        .expect("prepare");

    let result = executor.execute(&plan);

    assert!(matches!(
        result.outcome(),
        OperationOutcome::Failed {
            code: ErrorCode::OperationCancelled,
            ..
        }
    ));
    assert_eq!(executor.backend().mutation_count(), 0);
}

#[test]
fn target_drift_is_rejected_before_any_action_boundary_call() {
    let mut backend = ready_backend();
    let mut executor = WindowsRepairExecutor::new(backend.clone());
    let plan = executor
        .prepare(RepairAction::RefreshDhcp)
        .expect("prepare");
    backend.mutate_observation(|observation| {
        observation.target = TargetProof::dji_gen1([0x99; 32]);
    });
    executor.replace_backend(backend);

    let result = executor.execute(&plan);

    assert!(matches!(result.outcome(), OperationOutcome::Failed { .. }));
    assert_eq!(executor.backend().mutation_count(), 0);
}

#[test]
fn after_scan_identity_drift_never_reports_applied() {
    let mut backend = ready_backend();
    let executor = WindowsRepairExecutor::new(backend.clone());
    let plan = executor
        .prepare(RepairAction::RefreshDhcp)
        .expect("prepare");
    let mut after = backend.observe().expect("before observation");
    after.target = TargetProof::dji_gen1([0x9a; 32]);
    backend.set_after_observation(after);

    let result = executor.execute(&plan);

    assert!(matches!(
        result.outcome(),
        OperationOutcome::OutcomeUnknown {
            code: ErrorCode::DeviceIdentityChanged
        }
    ));
    assert_eq!(executor.backend().mutation_count(), 1);
}

#[test]
fn modeled_dns_change_requires_exact_readback_and_preserves_old_state_in_before_hash() {
    let mut backend = ready_backend();
    let requested = DnsProfile::Static {
        servers: vec![IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))],
    };
    let executor = WindowsRepairExecutor::new(backend.clone());
    let plan = executor
        .prepare(RepairAction::ApplyDnsProfile {
            profile: requested.clone(),
        })
        .expect("automatic DNS state is a valid before state");
    let applied = executor.execute(&plan);
    assert!(matches!(
        applied.outcome(),
        OperationOutcome::Applied { .. }
    ));
    assert_eq!(executor.backend().mutation_count(), 1);

    backend = ready_backend();
    backend.set_after_dns_profile(Some(DnsProfile::Automatic));
    let mismatch_executor = WindowsRepairExecutor::new(backend);
    let plan = mismatch_executor
        .prepare(RepairAction::ApplyDnsProfile { profile: requested })
        .expect("prepare");
    let result = mismatch_executor.execute(&plan);
    assert!(matches!(result.outcome(), OperationOutcome::Failed { .. }));
    assert_eq!(mismatch_executor.backend().mutation_count(), 1);
}

#[test]
fn hotspot_toggle_requires_fresh_exact_source_profile_and_capability_proof() {
    let mut unsupported = ready_backend();
    unsupported.set_hotspot(None, false);
    let executor = WindowsRepairExecutor::new(unsupported);
    assert!(matches!(
        executor.prepare(RepairAction::ToggleHotspot { enabled: true }),
        Err(RepairError::HotspotUnavailable)
    ));
    assert_eq!(executor.backend().mutation_count(), 0);

    let mut backend = ready_backend();
    backend.set_hotspot(Some(HotspotProof::fixture([0x55; 32], [0x66; 32])), false);
    let executor = WindowsRepairExecutor::new(backend);
    let plan = executor
        .prepare(RepairAction::ToggleHotspot { enabled: true })
        .expect("fresh supported hotspot");
    let result = executor.execute(&plan);
    assert!(matches!(result.outcome(), OperationOutcome::Applied { .. }));
    assert_eq!(executor.backend().last_typed_write(), Some("ToggleHotspot"));
}

#[test]
fn physical_reenumeration_requires_one_exact_root_and_after_scan_proof() {
    let mut backend = ready_backend();
    backend.mutate_observation(|observation| {
        observation.reenumerated = false;
    });
    let executor = WindowsRepairExecutor::new(backend);
    let plan = executor
        .prepare(RepairAction::ReenumerateDevice)
        .expect("exact root is present");
    let result = executor.execute(&plan);
    assert!(matches!(result.outcome(), OperationOutcome::Applied { .. }));

    let mut ambiguous = ready_backend();
    ambiguous.mutate_observation(|observation| observation.target_count = 2);
    let executor = WindowsRepairExecutor::new(ambiguous);
    assert!(matches!(
        executor.prepare(RepairAction::ReenumerateDevice),
        Err(RepairError::TargetAmbiguous)
    ));
    assert_eq!(executor.backend().mutation_count(), 0);
}

#[test]
fn adapter_partial_failure_keeps_best_effort_rollback_evidence() {
    let mut backend = ready_backend();
    backend.set_dispatch_result(DispatchResult::Failed {
        code: ErrorCode::PermissionDenied,
        rollback: RollbackOutcome::Failed {
            code: ErrorCode::RollbackFailed,
        },
    });
    let executor = WindowsRepairExecutor::new(backend);
    let plan = executor
        .prepare(RepairAction::RestartAdapter)
        .expect("prepare");
    let result = executor.execute(&plan);
    assert!(matches!(
        result.outcome(),
        OperationOutcome::Failed {
            code: ErrorCode::PermissionDenied,
            rollback: RollbackOutcome::Failed {
                code: ErrorCode::RollbackFailed
            }
        }
    ));
}

#[test]
fn uncertain_lease_and_restart_calls_are_not_proved_by_a_surviving_device() {
    for action in [
        RepairAction::RefreshDhcp,
        RepairAction::RestartAdapter,
        RepairAction::ReenumerateDevice,
        RepairAction::RestartModule,
    ] {
        let mut backend = ready_backend();
        backend.set_dispatch_result(DispatchResult::Unknown(ErrorCode::Timeout));
        // Native scanning can still find the same up adapter and USB root after a timeout.
        // That existence proof carries no evidence that the requested operation completed.
        let unchanged = backend.observe().unwrap();
        backend.set_after_observation(unchanged);
        let executor = WindowsRepairExecutor::new(backend);
        let plan = executor.prepare(action.clone()).expect("ready target");

        assert_eq!(
            executor.execute(&plan).outcome().clone(),
            OperationOutcome::OutcomeUnknown {
                code: ErrorCode::Timeout
            },
            "{action:?} requires a definite dispatch result"
        );
        assert_eq!(executor.backend().mutation_count(), 1);
        assert!(matches!(
            executor.execute(&plan).outcome(),
            OperationOutcome::Failed { .. }
        ));
        assert_eq!(
            executor.backend().mutation_count(),
            1,
            "never retry uncertainty"
        );
    }
}

#[test]
fn exact_dns_readback_can_resolve_an_uncertain_write_but_old_static_dns_cannot() {
    let old = DnsProfile::Static {
        servers: vec![
            "223.5.5.5".parse().unwrap(),
            "119.29.29.29".parse().unwrap(),
        ],
    };
    for (after, expected_applied) in [(DnsProfile::Automatic, true), (old.clone(), false)] {
        let mut backend = ready_backend();
        backend.set_dns_profile(Some(old.clone()));
        backend.set_dispatch_result(DispatchResult::Unknown(ErrorCode::Timeout));
        backend.set_after_dns_profile(Some(after));
        let executor = WindowsRepairExecutor::new(backend);
        let plan = executor
            .prepare(RepairAction::ApplyDnsProfile {
                profile: DnsProfile::Automatic,
            })
            .unwrap();
        let outcome = executor.execute(&plan).outcome().clone();
        if expected_applied {
            assert!(matches!(outcome, OperationOutcome::Applied { .. }));
        } else {
            assert_eq!(
                outcome,
                OperationOutcome::OutcomeUnknown {
                    code: ErrorCode::Timeout
                }
            );
        }
        assert_eq!(executor.backend().mutation_count(), 1);
    }
}

#[test]
fn at_repairs_and_physical_reenumeration_do_not_require_a_working_network_adapter() {
    for action in [
        RepairAction::SetUsbNetProfile {
            profile: VerifiedUsbNetProfile::Ecm,
        },
        RepairAction::SetApn {
            cid: PdpContextId::try_from(1).unwrap(),
            apn: apn("new.example"),
        },
        RepairAction::RestartModule,
        RepairAction::ReenumerateDevice,
    ] {
        let mut backend = ready_backend();
        backend.set_contexts(&[(1, "IP", "old.example", false)]);
        backend.mutate_observation(|observation| {
            observation.adapter = AdapterProof::fixture([0; 32]);
            observation.adapter_up = false;
        });
        backend.set_dns_profile(None);
        backend.set_hotspot(None, false);
        let executor = WindowsRepairExecutor::new(backend);
        let plan = executor
            .prepare(action.clone())
            .expect("USB root and usable AT suffice");
        assert!(
            matches!(
                executor.execute(&plan).outcome(),
                OperationOutcome::Applied { .. }
            ),
            "{action:?} must not depend on the missing network interface"
        );
        assert_eq!(executor.backend().mutation_count(), 1);
    }

    for action in [
        RepairAction::RefreshDhcp,
        RepairAction::RestartAdapter,
        RepairAction::ApplyDnsProfile {
            profile: DnsProfile::Static {
                servers: vec!["1.1.1.1".parse().unwrap()],
            },
        },
        RepairAction::ToggleHotspot { enabled: true },
    ] {
        let mut backend = ready_backend();
        backend
            .mutate_observation(|observation| observation.adapter = AdapterProof::fixture([0; 32]));
        let executor = WindowsRepairExecutor::new(backend);
        assert!(matches!(
            executor.prepare(action),
            Err(RepairError::TargetNotFound)
        ));
        assert_eq!(executor.backend().mutation_count(), 0);
    }
}

#[test]
fn every_adapter_repair_binds_the_same_adapter_before_and_after_dispatch() {
    let actions = [
        RepairAction::RefreshDhcp,
        RepairAction::RestartAdapter,
        RepairAction::ApplyDnsProfile {
            profile: DnsProfile::Static {
                servers: vec!["1.1.1.1".parse().unwrap()],
            },
        },
        RepairAction::ToggleHotspot { enabled: true },
    ];
    for action in actions {
        let mut backend = ready_backend();
        let executor = WindowsRepairExecutor::new(backend.clone());
        let plan = executor.prepare(action.clone()).unwrap();
        backend.mutate_observation(|observation| {
            observation.adapter = AdapterProof::fixture([0x99; 32])
        });
        assert!(matches!(
            executor.execute(&plan).outcome(),
            OperationOutcome::Failed {
                code: ErrorCode::EvidenceExpired,
                ..
            }
        ));
        assert_eq!(
            executor.backend().mutation_count(),
            0,
            "adapter drift before {action:?}"
        );

        let mut backend = ready_backend();
        let mut after = backend.observe().unwrap();
        after.adapter = AdapterProof::fixture([0x99; 32]);
        // Model a matching requested state on the replacement interface; it still must not
        // certify a write intended for the original module adapter.
        backend.set_after_observation(after);
        if let RepairAction::ApplyDnsProfile { profile } = &action {
            backend.set_after_dns_profile(Some(profile.clone()));
        }
        if matches!(action, RepairAction::ToggleHotspot { .. }) {
            backend.set_after_hotspot(Some(HotspotProof::fixture([0x33; 32], [0x44; 32])), true);
        }
        let executor = WindowsRepairExecutor::new(backend);
        let plan = executor.prepare(action.clone()).unwrap();
        assert_eq!(
            executor.execute(&plan).outcome().clone(),
            OperationOutcome::OutcomeUnknown {
                code: ErrorCode::DeviceIdentityChanged
            },
            "adapter drift after {action:?}"
        );
        assert_eq!(executor.backend().mutation_count(), 1);
    }
}

#[test]
fn apn_readback_must_preserve_the_written_ip_type_not_just_the_cid_and_apn() {
    for pdp_type in ["IPV6", "IPV4V6"] {
        for dispatch in [
            DispatchResult::Applied,
            DispatchResult::Unknown(ErrorCode::Timeout),
        ] {
            let mut backend = ready_backend();
            backend.set_contexts(&[(1, "IP", "old.example", false)]);
            backend.set_dispatch_result(dispatch);
            let mut after = backend.observe().unwrap();
            after.set_contexts(&[(1, pdp_type, "new.example", false)]);
            backend.set_after_observation(after);
            let executor = WindowsRepairExecutor::new(backend);
            let plan = executor
                .prepare(RepairAction::SetApn {
                    cid: PdpContextId::try_from(1).unwrap(),
                    apn: apn("new.example"),
                })
                .unwrap();

            let result = executor.execute(&plan);
            match dispatch {
                DispatchResult::Applied => assert!(matches!(
                    result.outcome(),
                    OperationOutcome::Failed {
                        code: ErrorCode::VerificationFailed,
                        ..
                    }
                )),
                DispatchResult::Unknown(_) => assert!(matches!(
                    result.outcome(),
                    OperationOutcome::OutcomeUnknown {
                        code: ErrorCode::Timeout
                    }
                )),
                _ => unreachable!(),
            }
            assert_eq!(executor.backend().mutation_count(), 1);
        }
    }
}

#[test]
fn apn_verification_requires_an_exact_inactive_ip_readback_and_is_never_retried() {
    for active in [false, true] {
        for dispatch in [
            DispatchResult::Applied,
            DispatchResult::Unknown(ErrorCode::Timeout),
        ] {
            let mut backend = ready_backend();
            backend.set_contexts(&[(1, "IP", "old.example", false)]);
            backend.set_dispatch_result(dispatch);
            let mut after = backend.observe().unwrap();
            after.set_contexts(&[(1, "IP", "new.example", active)]);
            backend.set_after_observation(after);
            let executor = WindowsRepairExecutor::new(backend);
            let plan = executor
                .prepare(RepairAction::SetApn {
                    cid: PdpContextId::try_from(1).unwrap(),
                    apn: apn("new.example"),
                })
                .unwrap();

            let result = executor.execute(&plan);
            if active && matches!(dispatch, DispatchResult::Unknown(_)) {
                assert!(matches!(
                    result.outcome(),
                    OperationOutcome::OutcomeUnknown {
                        code: ErrorCode::Timeout
                    }
                ));
            } else if active {
                assert!(matches!(
                    result.outcome(),
                    OperationOutcome::Failed {
                        code: ErrorCode::VerificationFailed,
                        ..
                    }
                ));
            } else {
                assert!(matches!(result.outcome(), OperationOutcome::Applied { .. }));
            }
            assert_eq!(executor.backend().mutation_count(), 1);
            assert!(matches!(
                executor.execute(&plan).outcome(),
                OperationOutcome::Failed { .. }
            ));
            assert_eq!(executor.backend().mutation_count(), 1);
        }
    }
}
