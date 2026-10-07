//! First-connection checks reuse measured diagnostics, never infer driver absence from no IP.
use dji4g_application::{DiagnosticCheckSnapshot, DiagnosticCheckState};
use dji4g_domain::Freshness;
use std::time::SystemTime;

/// One catalog string in the language this page was rendered with.
fn t(language: crate::localization::Language, key: crate::localization::TextKey) -> String {
    crate::localization::LocalizedText::new(language, key).text
}

pub(crate) fn installation_busy(snapshot: &dji4g_application::ControllerSnapshot) -> bool {
    snapshot.serial_work_busy
        || snapshot
            .sms_send
            .as_ref()
            .is_some_and(|send| send.phase != dji4g_application::SmsSendPhase::Finished)
        || matches!(
            snapshot.operation.as_ref().map(|op| &op.state),
            Some(dji4g_application::OperationState::Running { .. })
        )
        || snapshot.prepared_action.as_ref().is_some_and(|action| {
            matches!(
                action.state,
                dji4g_application::PreparedActionState::AwaitingConfirmation
            )
        })
}

/// Whether the bundled driver installation may be offered for the module identified right now.
///
/// The bundled package is scoped to the write-capable profile (`USB\VID_2CA3&PID_4006…`), so a
/// recognized read-only module is never offered it — with its own visible reason — and an
/// unidentified port keeps the previous behaviour, where the installer itself decides.
pub(crate) fn controller_may_install_driver(
    snapshot: &dji4g_application::ControllerSnapshot,
) -> bool {
    snapshot
        .app
        .device
        .as_ref()
        .and_then(|device| device.identity.profile())
        .is_none_or(dji4g_domain::DeviceProfile::allows_controlled_actions)
}

fn current_state(check: &DiagnosticCheckSnapshot, now: SystemTime) -> DiagnosticCheckState {
    if check.freshness(now) == Freshness::Stale {
        DiagnosticCheckState::Expired
    } else {
        check.state.clone()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GuideState {
    Detecting,
    Expired,
    NotRun,
    ProbeDisabled,
    Failed,
    Passed,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NextStepVm {
    pub state: GuideState,
    pub text: String,
    pub destination: Option<crate::app::Page>,
}

pub(crate) fn next_step_vm(
    snapshot: &dji4g_application::ControllerSnapshot,
    now: SystemTime,
    language: crate::localization::Language,
) -> NextStepVm {
    use crate::app::Page;
    use dji4g_application::DiagnosticCheckId as Id;
    for id in [
        Id::UsbDevice,
        Id::AtControl,
        Id::WindowsAdapter,
        Id::Cellular,
        Id::BoundPublic,
        Id::BoundDns,
    ] {
        if matches!(id, Id::BoundPublic | Id::BoundDns) && !snapshot.settings.active_probe {
            return NextStepVm {
                state: GuideState::ProbeDisabled,
                text: t(language, crate::localization::TextKey::GuideProbeOff),
                destination: Some(Page::Settings),
            };
        }
        let state = snapshot
            .diagnostics
            .iter()
            .find(|check| check.id == id)
            .map(|check| current_state(check, now));
        match state {
            Some(DiagnosticCheckState::Passed) => continue,
            Some(DiagnosticCheckState::Running { .. }) => {
                return NextStepVm {
                    state: GuideState::Detecting,
                    text: t(language, crate::localization::TextKey::GuideCollecting),
                    destination: None,
                };
            }
            Some(DiagnosticCheckState::Expired) => {
                return NextStepVm {
                    state: GuideState::Expired,
                    text: t(language, crate::localization::TextKey::GuideEvidenceStale),
                    destination: Some(Page::Diagnostics),
                };
            }
            Some(DiagnosticCheckState::Unexecuted {
                reason: dji4g_application::UnexecutedReason::DisabledBySetting,
            }) => {
                return NextStepVm {
                    state: GuideState::ProbeDisabled,
                    text: t(language, crate::localization::TextKey::GuideCheckDisabled),
                    destination: Some(Page::Settings),
                };
            }
            Some(DiagnosticCheckState::Unexecuted { .. }) | None => {
                return NextStepVm {
                    state: GuideState::NotRun,
                    text: t(language, crate::localization::TextKey::GuideCheckNotRun),
                    destination: Some(Page::Diagnostics),
                };
            }
            Some(
                DiagnosticCheckState::Failed { .. } | DiagnosticCheckState::Unavailable { .. },
            ) => {
                let text = match id {
                    Id::UsbDevice => t(language, crate::localization::TextKey::GuideUsbFailed),
                    Id::AtControl | Id::WindowsAdapter => {
                        t(language, crate::localization::TextKey::GuideAdapterFailed)
                    }
                    Id::Cellular => t(language, crate::localization::TextKey::GuideCellularFailed),
                    _ => t(
                        language,
                        crate::localization::TextKey::GuideBoundProbeFailed,
                    ),
                };
                return NextStepVm {
                    state: GuideState::Failed,
                    text,
                    destination: Some(Page::Diagnostics),
                };
            }
        }
    }
    NextStepVm {
        state: GuideState::Passed,
        text: t(language, crate::localization::TextKey::GuidePassed),
        destination: None,
    }
}

pub(crate) fn next_step(
    snapshot: &dji4g_application::ControllerSnapshot,
    now: SystemTime,
    language: crate::localization::Language,
) -> String {
    next_step_vm(snapshot, now, language).text
}

pub(crate) fn render_guide(
    ui: &mut eframe::egui::Ui,
    snapshot: &dji4g_application::ControllerSnapshot,
    now: SystemTime,
    language: crate::localization::Language,
) {
    use super::{diagnostic_state_vm, section_frame, section_heading, wrapped_label};
    use dji4g_application::DiagnosticCheckId as Id;
    section_frame(ui, |ui| {
        ui.label(section_heading(t(
            language,
            crate::localization::TextKey::GuideStartHeading,
        )));
        wrapped_label(ui, t(language, crate::localization::TextKey::GuideSteps));
        let mut passed_checks = Vec::new();
        for (id, title) in [
            (
                Id::UsbDevice,
                t(language, crate::localization::TextKey::CheckUsbDetection),
            ),
            (
                Id::WindowsAdapter,
                t(
                    language,
                    crate::localization::TextKey::CheckAdapterInterface,
                ),
            ),
            (
                Id::AtControl,
                t(language, crate::localization::TextKey::CheckAtSerial),
            ),
            (
                Id::Cellular,
                t(language, crate::localization::TextKey::CheckSimCellular),
            ),
            (
                Id::BoundPublic,
                t(language, crate::localization::TextKey::CheckBoundPublic),
            ),
            (
                Id::BoundDns,
                t(language, crate::localization::TextKey::CheckBoundDns),
            ),
        ] {
            if let Some(check) = snapshot.diagnostics.iter().find(|check| check.id == id) {
                let state = current_state(check, now);
                if state == DiagnosticCheckState::Passed {
                    passed_checks.push(title);
                    continue;
                }
                let vm = diagnostic_state_vm(&state, language);
                ui.horizontal_wrapped(|ui| {
                    ui.label(title);
                    ui.colored_label(vm.tone.color(), vm.label.text);
                    if let Some(detail) = vm.detail {
                        ui.label(detail.text);
                    }
                });
            }
        }
        if !passed_checks.is_empty() {
            // Always visible: the panel has no disclosure triangles any more, so the passed checks
            // stay on the card as one quiet wrapped line instead of hiding behind a header.
            //
            // The mark is `StatusTone::Positive`'s own, not a "✓": the bundled CJK face has no
            // U+2713, so a tick would render as an empty box. These four marks are the panel's
            // whole status vocabulary and every one of them exists in the font.
            let passed_mark = super::StatusTone::Positive.marker();
            wrapped_label(
                ui,
                super::meta_text(
                    crate::localization::format_text_in(
                        language,
                        crate::localization::TextKey::GuidePassedCount,
                        &crate::localization::TextArgs {
                            count: Some(passed_checks.len()),
                            detail: Some(
                                passed_checks
                                    .iter()
                                    .map(|title| format!("{passed_mark} {title}"))
                                    .collect::<Vec<_>>()
                                    .join("  "),
                            ),
                            ..Default::default()
                        },
                    )
                    .text,
                ),
            );
        }
        ui.separator();
        wrapped_label(ui, next_step(snapshot, now, language));
        wrapped_label(
            ui,
            t(language, crate::localization::TextKey::GuideStuckHint),
        );
    });
}

pub(crate) fn render(
    ui: &mut eframe::egui::Ui,
    snapshot: &dji4g_application::ControllerSnapshot,
    now: SystemTime,
    language: crate::localization::Language,
) -> bool {
    let mut install_requested = false;
    use super::{diagnostic_state_vm, meta_text, section_frame, section_heading, wrapped_label};
    use dji4g_application::DiagnosticCheckId;
    section_frame(ui, |ui| {
        ui.label(section_heading(t(
            language,
            crate::localization::TextKey::FirstCheckHeading,
        )));
        wrapped_label(
            ui,
            t(language, crate::localization::TextKey::FirstCheckIntro),
        );
        for (id, title) in [
            (
                DiagnosticCheckId::UsbDevice,
                t(language, crate::localization::TextKey::FirstCheckUsb),
            ),
            (
                DiagnosticCheckId::WindowsAdapter,
                t(language, crate::localization::TextKey::FirstCheckAdapter),
            ),
            (
                DiagnosticCheckId::AtControl,
                t(language, crate::localization::TextKey::FirstCheckAt),
            ),
        ] {
            if let Some(check) = snapshot.diagnostics.iter().find(|check| check.id == id) {
                let state = current_state(check, now);
                let vm = diagnostic_state_vm(&state, language);
                ui.horizontal_wrapped(|ui| {
                    ui.label(title);
                    ui.colored_label(
                        vm.tone.color(),
                        format!("{} {}", vm.tone.marker(), vm.label.text),
                    );
                });
                if let Some(detail) = vm.detail {
                    wrapped_label(ui, meta_text(detail.text));
                }
            }
        }
        // Always open: the panel has no disclosure triangles, so the installation section is a card
        // section. Its content is unchanged — only the triangle and the collapsed default are gone.
        ui.label(section_heading(t(
            language,
            crate::localization::TextKey::DriverInstallHeading,
        )));
        {
            let bundled = super::onboarding::bundled_driver_available();
            if !controller_may_install_driver(snapshot) {
                // The bundled package only ever binds `USB\VID_2CA3&PID_4006…`, so offering the
                // installer for a recognized read-only module would promise something it cannot
                // do. Say why instead of showing a dead button.
                wrapped_label(
                    ui,
                    t(language, crate::localization::TextKey::ReadOnlyModuleReason),
                );
            } else if bundled {
                wrapped_label(
                    ui,
                    t(language, crate::localization::TextKey::DriverBundledNote),
                );
                wrapped_label(
                    ui,
                    meta_text(t(
                        language,
                        crate::localization::TextKey::DriverElevationNote,
                    )),
                );
                install_requested = ui
                    .button(t(
                        language,
                        crate::localization::TextKey::DriverInstallAction,
                    ))
                    .clicked();
            } else {
                wrapped_label(
                    ui,
                    t(language, crate::localization::TextKey::DriverNoneNote),
                );
            }
            super::components::link(
                ui,
                &t(
                    language,
                    crate::localization::TextKey::DriverDjiCompatibilityLink,
                ),
                "https://repair.dji.com/help/content?customId=01700008285&lang=en&paperDocType=ARTICLE&re=US&spaceId=17",
            );
            wrapped_label(
                ui,
                meta_text(t(
                    language,
                    crate::localization::TextKey::DriverSeparateNote,
                )),
            );
            super::components::link(
                ui,
                &t(language, crate::localization::TextKey::DriverDjiSupportLink),
                super::onboarding::OFFICIAL_SUPPORT_URL,
            );
            super::components::link(
                ui,
                &t(language, crate::localization::TextKey::DriverVendorLink),
                "https://forums.quectel.com/t/how-to-get-driver-tools/38963",
            );
            wrapped_label(
                ui,
                meta_text(t(
                    language,
                    crate::localization::TextKey::DriverVendorLinkNote,
                )),
            );
        }
    });
    install_requested
}

#[cfg(test)]
mod tests {
    use super::*;
    use dji4g_application::{DiagnosticCheckId, DiagnosticSet};
    use dji4g_domain::DeviceEpoch;
    use std::time::Duration;

    #[test]
    fn guidance_distinguishes_not_run_running_expired_and_passed() {
        use dji4g_application::{
            BackendEvent, CheckMask, ReducerState, RefreshCycleId, reduce_state,
        };
        let now = SystemTime::now();
        assert_eq!(
            next_step_vm(
                &ReducerState::new(now).snapshot(),
                now,
                crate::localization::Language::ZhCn
            )
            .state,
            GuideState::NotRun
        );
        let ready = ReducerState::test_ready(now);
        assert_eq!(
            next_step_vm(&ready.snapshot(), now, crate::localization::Language::ZhCn).state,
            GuideState::Passed
        );
        assert_eq!(
            next_step_vm(
                &ready.snapshot(),
                now + Duration::from_secs(3600),
                crate::localization::Language::ZhCn
            )
            .state,
            GuideState::Expired
        );
        let running = reduce_state(
            &ready,
            BackendEvent::RefreshStarted {
                cycle: RefreshCycleId(40),
                epoch: DeviceEpoch(1),
                scheduled: CheckMask::only(DiagnosticCheckId::AtControl),
            },
            now,
        );
        let guide = next_step_vm(
            &running.snapshot(),
            now,
            crate::localization::Language::ZhCn,
        );
        assert_eq!(guide.state, GuideState::Detecting);
        assert_eq!(guide.destination, None);
    }

    #[test]
    fn disabled_active_probes_direct_to_settings_not_network_repair() {
        let now = SystemTime::now();
        let mut snapshot = crate::demo::demo_snapshot(crate::demo::DemoScenario::Available, now);
        snapshot.settings.active_probe = false;
        assert!(next_step(&snapshot, now, crate::localization::Language::ZhCn).contains("设置"));
        assert!(next_step(&snapshot, now, crate::localization::Language::ZhCn).contains("关闭"));
    }

    #[test]
    fn initial_guide_does_not_diagnose_missing_driver() {
        let now = SystemTime::now();
        let snapshot = dji4g_application::ReducerState::new(now).snapshot();
        assert!(
            next_step(&snapshot, now, crate::localization::Language::ZhCn)
                .contains("未识别设备不等于缺驱动")
        );
        assert!(!installation_busy(&snapshot));
    }

    /// The bundled package only binds `USB\VID_2CA3&PID_4006…`: a recognized read-only module is
    /// never offered the installer, the DJI module keeps it, and an unidentified port keeps the
    /// previous behaviour (the installer itself decides).
    #[test]
    fn driver_installation_is_only_offered_to_write_capable_modules() {
        let now = SystemTime::now();

        let generic =
            dji4g_application::ReducerState::test_ready_for(dji4g_domain::QUECTEL_GENERIC, now)
                .snapshot();
        assert!(!controller_may_install_driver(&generic));
        assert!(generic.app.device.is_some());

        let dji = dji4g_application::ReducerState::test_ready(now).snapshot();
        assert!(controller_may_install_driver(&dji));

        let unidentified = dji4g_application::ReducerState::new(now).snapshot();
        assert!(unidentified.app.device.is_none());
        assert!(
            controller_may_install_driver(&unidentified),
            "no module identified: the installer decides, as before"
        );
    }

    #[test]
    fn expired_success_cannot_mark_setup_ready() {
        let set = DiagnosticSet::new(DeviceEpoch::default());
        let mut check = set
            .iter()
            .find(|c| c.id == DiagnosticCheckId::AtControl)
            .unwrap()
            .clone();
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        check.state = DiagnosticCheckState::Passed;
        check.expires_at = Some(now - Duration::from_secs(1));
        assert_eq!(current_state(&check, now), DiagnosticCheckState::Expired);
        check.expires_at = Some(now + Duration::from_secs(1));
        assert_eq!(current_state(&check, now), DiagnosticCheckState::Passed);
    }
}

/// Whether the installation section used to open by default.
///
/// The panel no longer collapses any section, so nothing calls this in production; it is kept
/// because its test pins the "code 28 means a driver really is missing" rule that the guidance text
/// relies on.
#[cfg(test)]
fn driver_installation_expanded(
    snapshot: &dji4g_application::ControllerSnapshot,
    now: SystemTime,
) -> bool {
    // A missing serial response is not proof of a missing driver. Code 28 is explicit PnP evidence.
    snapshot
        .app
        .device
        .as_ref()
        .is_some_and(|device| device.problem_code == Some(28))
        && snapshot.diagnostics.iter().any(|check| {
            check.id == dji4g_application::DiagnosticCheckId::UsbDevice
                && check.freshness(now) == Freshness::Fresh
        })
}
#[test]
fn interface_failure_alone_is_not_driver_installation_evidence() {
    use dji4g_application::{
        BackendEvent, CheckMask, CheckResult, DiagnosticCheckId, FailureCode, ReducerState,
        RefreshCycleId, reduce_state,
    };
    let now = SystemTime::now();
    let state = ReducerState::test_ready(now);
    let state = reduce_state(
        &state,
        BackendEvent::RefreshStarted {
            cycle: RefreshCycleId(90),
            epoch: dji4g_domain::DeviceEpoch(1),
            scheduled: CheckMask::only(DiagnosticCheckId::AtControl),
        },
        now,
    );
    let state = reduce_state(
        &state,
        BackendEvent::AtFinished {
            cycle: RefreshCycleId(90),
            epoch: dji4g_domain::DeviceEpoch(1),
            result: CheckResult::Failed {
                code: FailureCode::new(
                    dji4g_domain::ErrorCode::Timeout,
                    dji4g_application::StableCode::try_from_static("at:timeout").unwrap(),
                ),
                observed_at: now,
            },
        },
        now,
    );
    assert!(!driver_installation_expanded(&state.snapshot(), now));
}
