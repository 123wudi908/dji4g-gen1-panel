//! First-run guidance projects existing asynchronous diagnostic evidence; it opens no device.
use std::time::SystemTime;

use dji4g_application::{
    ControllerSnapshot, DiagnosticCheckId, DiagnosticCheckSnapshot, DiagnosticCheckState,
    UnexecutedReason,
};
use dji4g_domain::Freshness;
use eframe::egui;

/// One catalog string in the language this page was rendered with.
fn t(language: crate::localization::Language, key: crate::localization::TextKey) -> String {
    crate::localization::LocalizedText::new(language, key).text
}

pub const OFFICIAL_DRIVER_GUIDANCE_URL: &str = "https://repair.dji.com/help/content?customId=01700008285&lang=en&paperDocType=ARTICLE&re=US&spaceId=17";
pub const OFFICIAL_SUPPORT_URL: &str = "https://www.dji.com/cn/support";

#[derive(Default)]
pub(crate) struct OnboardingState {
    pub open: bool,
    pub completed: bool,
    pub driver_fixture: Option<bool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CheckState {
    Waiting,
    Running,
    Passed,
    Attention,
    Disabled,
    Expired,
}

impl CheckState {
    fn label(self, language: crate::localization::Language) -> String {
        match self {
            Self::Waiting => t(language, crate::localization::TextKey::OnboardingWaiting),
            Self::Running => t(language, crate::localization::TextKey::OnboardingChecking),
            Self::Passed => t(language, crate::localization::TextKey::CheckPassed),
            Self::Attention => t(
                language,
                crate::localization::TextKey::OnboardingNeedsReview,
            ),
            Self::Disabled => t(
                language,
                crate::localization::TextKey::OnboardingDisabledUnverified,
            ),
            Self::Expired => t(
                language,
                crate::localization::TextKey::OnboardingStaleRefresh,
            ),
        }
    }
    fn tone(self) -> super::StatusTone {
        match self {
            Self::Passed => super::StatusTone::Positive,
            Self::Attention => super::StatusTone::Caution,
            Self::Running => super::StatusTone::Progress,
            _ => super::StatusTone::Neutral,
        }
    }
}

fn check_state(
    check: Option<&DiagnosticCheckSnapshot>,
    enabled: bool,
    now: SystemTime,
) -> CheckState {
    if !enabled {
        return CheckState::Disabled;
    }
    let Some(check) = check else {
        return CheckState::Waiting;
    };
    if check.freshness(now) == Freshness::Stale {
        return CheckState::Expired;
    }
    match &check.state {
        DiagnosticCheckState::Passed => CheckState::Passed,
        DiagnosticCheckState::Failed { .. } => CheckState::Attention,
        DiagnosticCheckState::Running { .. } => CheckState::Running,
        DiagnosticCheckState::Unexecuted {
            reason: UnexecutedReason::DisabledBySetting,
        } => CheckState::Disabled,
        DiagnosticCheckState::Expired => CheckState::Expired,
        DiagnosticCheckState::Unavailable { .. } | DiagnosticCheckState::Unexecuted { .. } => {
            CheckState::Waiting
        }
    }
}

pub(crate) fn checks(
    snapshot: &ControllerSnapshot,
    language: crate::localization::Language,
    now: SystemTime,
) -> Vec<(String, CheckState)> {
    use DiagnosticCheckId as Id;
    [
        (
            Id::UsbDevice,
            t(language, crate::localization::TextKey::OnboardingCheckUsb),
        ),
        (
            Id::WindowsAdapter,
            t(
                language,
                crate::localization::TextKey::OnboardingCheckAdapter,
            ),
        ),
        (
            Id::AtControl,
            t(language, crate::localization::TextKey::OnboardingCheckAt),
        ),
        (
            Id::Cellular,
            t(
                language,
                crate::localization::TextKey::OnboardingCheckCellular,
            ),
        ),
        (
            Id::BoundPublic,
            t(
                language,
                crate::localization::TextKey::OnboardingCheckBoundPublic,
            ),
        ),
        (
            Id::BoundDns,
            t(
                language,
                crate::localization::TextKey::OnboardingCheckBoundDns,
            ),
        ),
    ]
    .into_iter()
    .map(|(id, label)| {
        let enabled =
            !matches!(id, Id::BoundPublic | Id::BoundDns) || snapshot.settings.active_probe;
        (
            label,
            check_state(Some(snapshot.diagnostics.get(id)), enabled, now),
        )
    })
    .collect()
}

pub(crate) enum OnboardingAction {
    None,
    Enter,
    Refresh,
    InspectHostNetwork,
    OpenRepairs,
    InstallBundledDriver,
    OpenWindowsUpdate,
}

pub(crate) fn bundled_driver_available() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
        .is_some_and(|dir| {
            [
                "dji4g-driver-setup.exe",
                "drivers/qcser.inf",
                "drivers/qcser.cat",
                "drivers/qcmdm.inf",
                "drivers/qcmdm.cat",
                "drivers/qcfilter.inf",
                "drivers/qcfilter.cat",
                "drivers/filter/amd64/qcusbfilter.sys",
                "drivers/serial/amd64/qcusbser.sys",
            ]
            .iter()
            .all(|file| dir.join(file).is_file())
        })
}

fn completion_copy(
    language: crate::localization::Language,
    ready: bool,
    result: Option<dji4g_windows_platform::driver_setup::DriverSetupOutcome>,
) -> (String, String, String) {
    use dji4g_windows_platform::driver_setup::DriverSetupOutcome as O;
    if let Some(outcome @ (O::RestartRequired | O::RestartRequiredAfterFailure)) = result {
        return (
            t(
                language,
                crate::localization::TextKey::OnboardingRestartStep,
            ),
            t(
                language,
                crate::localization::TextKey::OnboardingEnterAfterRestart,
            ),
            crate::localization::stable_code_display(language, outcome.code()),
        );
    }
    (
        t(
            language,
            crate::localization::TextKey::OnboardingAutoCheckStep,
        ),
        if ready {
            t(language, crate::localization::TextKey::OnboardingStartUsing)
        } else {
            t(language, crate::localization::TextKey::OnboardingEnterPanel)
        },
        if ready {
            t(language, crate::localization::TextKey::OnboardingPassedNote)
        } else {
            t(
                language,
                crate::localization::TextKey::OnboardingNotPassedNote,
            )
        },
    )
}

pub(crate) fn render(
    ctx: &egui::Context,
    snapshot: &ControllerSnapshot,
    language: crate::localization::Language,
    now: SystemTime,
    driver_fixture: Option<bool>,
    setup_result: Option<dji4g_windows_platform::driver_setup::DriverSetupOutcome>,
    sink: &dyn crate::app::PanelCommandSink,
) -> OnboardingAction {
    let rows = checks(snapshot, language, now);
    let ready = rows.iter().all(|(_, state)| *state == CheckState::Passed);
    let (check_heading, enter_label, completion_detail) =
        completion_copy(language, ready, setup_result);
    let mut action = OnboardingAction::None;
    egui::TopBottomPanel::bottom("onboarding-actions")
        .show_separator_line(false)
        .frame(
            egui::Frame::none()
                .fill(super::scale::surface_alt())
                .inner_margin(16.0),
        )
        .show(ctx, |ui| {
            if super::components::entry_footer(ui, &enter_label, language).clicked() {
                action = OnboardingAction::Enter;
            }
        });
    egui::CentralPanel::default()
        .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(24.0))
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("onboarding-scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_max_width(ui.available_width().min(960.0));
                    super::shell::brand(ui, language);
                    ui.add_space(16.0);
                    super::components::page_heading(
                        ui,
                        &t(language, crate::localization::TextKey::OnboardingTitle),
                        &t(language, crate::localization::TextKey::OnboardingIntro),
                    );
                    if let Some(result) = setup_result {
                        ui.add_space(12.0);
                        super::section_frame(ui, |ui| {
                            ui.label(super::section_heading(t(
                                language,
                                crate::localization::TextKey::OnboardingSetupResultHeading,
                            )));
                            super::wrapped_label(
                                ui,
                                crate::localization::stable_code_display(language, result.code()),
                            );
                        });
                    }
                    ui.add_space(16.0);
                    super::components::status_banner(
                        ui,
                        &t(language, crate::localization::TextKey::OnboardingStep1Title),
                        &t(language, crate::localization::TextKey::OnboardingStep1Body),
                        super::StatusTone::Neutral,
                    );
                    ui.add_space(12.0);
                    super::section_frame(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(super::section_heading(check_heading));
                            if ui
                                .add_enabled(
                                    !snapshot.serial_work_busy,
                                    egui::Button::new(t(
                                        language,
                                        crate::localization::TextKey::HostCheckAgain,
                                    )),
                                )
                                .clicked()
                            {
                                action = OnboardingAction::Refresh;
                            }
                        });
                        for (label, state) in &rows {
                            ui.horizontal_wrapped(|ui| {
                                ui.add_sized(
                                    [160.0, super::scale::CONTROL_H],
                                    egui::Label::new(label.clone()),
                                );
                                ui.colored_label(
                                    state.tone().color(),
                                    format!("{} {}", state.tone().marker(), state.label(language)),
                                );
                                if *state == CheckState::Running {
                                    super::components::loading_spinner(ui);
                                }
                            });
                        }
                        ui.add_space(6.0);
                        super::wrapped_label(ui, super::meta_text(completion_detail));
                    });
                    if super::module_network_check::render(
                        ui,
                        snapshot,
                        now,
                        crate::localization::Language::ZhCn,
                        sink,
                    ) {
                        action = OnboardingAction::OpenRepairs;
                    }
                    ui.add_space(12.0);
                    // Always open: sections are cards, not accordions.
                    ui.label(super::section_heading(t(
                        language,
                        crate::localization::TextKey::OnboardingHostHeading,
                    )));
                    {
                        let (tone, summary) = super::network_assistance::brief(
                            snapshot,
                            now,
                            crate::localization::Language::ZhCn,
                        );
                        super::wrapped_label(
                            ui,
                            egui::RichText::new(format!("{} {summary}", tone.marker()))
                                .color(tone.color()),
                        );
                        if ui
                            .button(t(
                                language,
                                crate::localization::TextKey::OnboardingCheckHost,
                            ))
                            .clicked()
                        {
                            action = OnboardingAction::InspectHostNetwork;
                        }
                        super::wrapped_label(
                            ui,
                            super::meta_text(t(
                                language,
                                crate::localization::TextKey::OnboardingHostNote,
                            )),
                        );
                    }
                    ui.add_space(12.0);
                    ui.label(super::section_heading(t(
                        language,
                        crate::localization::TextKey::OnboardingStep3Title,
                    )));
                    {
                        if driver_fixture.unwrap_or_else(bundled_driver_available) {
                            super::wrapped_label(
                                ui,
                                t(
                                    language,
                                    crate::localization::TextKey::OnboardingBundledDriverNote,
                                ),
                            );
                            if ui
                                .add_enabled(
                                    !super::driver_setup::installation_busy(snapshot),
                                    egui::Button::new(t(
                                        language,
                                        crate::localization::TextKey::OnboardingUseBundledDriver,
                                    )),
                                )
                                .clicked()
                            {
                                action = OnboardingAction::InstallBundledDriver;
                            }
                            super::wrapped_label(
                                ui,
                                super::meta_text(t(
                                    language,
                                    crate::localization::TextKey::OnboardingBundledDriverHint,
                                )),
                            );
                        } else {
                            super::wrapped_label(
                                ui,
                                t(
                                    language,
                                    crate::localization::TextKey::OnboardingNoBundledDriverNote,
                                ),
                            );
                        }
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .button(t(
                                    language,
                                    crate::localization::TextKey::OnboardingOpenWindowsUpdate,
                                ))
                                .clicked()
                            {
                                action = OnboardingAction::OpenWindowsUpdate;
                            }
                            super::components::link(
                                ui,
                                &t(
                                    language,
                                    crate::localization::TextKey::OnboardingDjiCompatibilityLink,
                                ),
                                OFFICIAL_DRIVER_GUIDANCE_URL,
                            );
                            super::components::link(
                                ui,
                                &t(language, crate::localization::TextKey::DriverDjiSupportLink),
                                OFFICIAL_SUPPORT_URL,
                            );
                        });
                        super::wrapped_label(
                            ui,
                            super::meta_text(t(
                                language,
                                crate::localization::TextKey::OnboardingOfficialLinkNote,
                            )),
                        );
                    }
                });
        });
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_missing_and_disabled_evidence_is_never_a_failure() {
        let now = SystemTime::UNIX_EPOCH;
        let mut snapshot = dji4g_application::ReducerState::new(now).snapshot();
        assert!(
            checks(&snapshot, crate::localization::Language::ZhCn, now)
                .iter()
                .all(|(_, state)| *state == CheckState::Waiting)
        );
        snapshot.settings.active_probe = false;
        let rows = checks(&snapshot, crate::localization::Language::ZhCn, now);
        assert_eq!(rows[4].1, CheckState::Disabled);
        assert_eq!(rows[5].1, CheckState::Disabled);
        assert_eq!(check_state(None, true, now), CheckState::Waiting);
    }
    #[test]
    fn successful_checks_expire_instead_of_claiming_current_availability() {
        let now = SystemTime::now();
        let snapshot = dji4g_application::ReducerState::test_ready(now).snapshot();
        assert!(
            checks(&snapshot, crate::localization::Language::ZhCn, now)
                .iter()
                .all(|(_, state)| *state == CheckState::Passed)
        );
        assert!(
            checks(
                &snapshot,
                crate::localization::Language::ZhCn,
                now + std::time::Duration::from_secs(3600)
            )
            .iter()
            .all(|(_, state)| *state == CheckState::Expired)
        );
    }
}

#[test]
fn restart_result_overrides_healthy_checks_and_start_using_copy() {
    use dji4g_windows_platform::driver_setup::DriverSetupOutcome as O;
    for outcome in [O::RestartRequired, O::RestartRequiredAfterFailure] {
        let (heading, button, detail) =
            completion_copy(crate::localization::Language::ZhCn, true, Some(outcome));
        assert!(heading.contains("重启"));
        assert!(button.contains("重启"));
        assert!(detail.contains("重启"));
        assert!(!button.contains(&t(
            crate::localization::Language::ZhCn,
            crate::localization::TextKey::OnboardingStartUsing,
        )));
        assert!(!detail.contains("检查通过"));
    }
    assert!(
        completion_copy(crate::localization::Language::ZhCn, true, Some(O::Ready))
            .1
            .contains(&t(
                crate::localization::Language::ZhCn,
                crate::localization::TextKey::OnboardingStartUsing,
            ))
    );
}
