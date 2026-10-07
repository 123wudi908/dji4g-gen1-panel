//! Device tools page: the three-tier AT terminal (预设 / 查询 / 专家) and its bounded transcript.
//!
//! The page only projects [`DeviceToolsSnapshot`] and dispatches closed [`UiCommand`] values
//! through the panel's sink. Request and response text lives exclusively in this page's on-screen
//! projection: nothing here writes it to `crate::logging`, a diagnostic export, or a `tracing`
//! field, and the expert tab freezes every non-whitelisted line for its own per-command
//! confirmation before anything is written to the module.

use std::time::{Duration, SystemTime};

use dji4g_application::{
    ControlledRepairError, ControlledRepairRequest, ControllerSnapshot, DeviceToolsSnapshot,
    PendingExpertTool, ToolHistoryEntry, ToolOperationKind, ToolOutcome, ToolPhase, UiCommand,
    UiSendError, UsbNetReading, urc_transcript_payload,
};
use dji4g_at_protocol::{
    PdpContextState, PdpType, ToolInputError, ToolReadId, ToolWriteId, ValidatedToolLine,
    VerifiedUsbNetProfile, classify_known_write,
};
use dji4g_domain::{DeviceEpoch, FeatureStatus, StableDeviceIdentity};
use eframe::egui::{self, RichText, Ui};

use super::{
    StatusTone, detail_text, field_label, info_grid, meta_text, scale, section_frame,
    section_heading, wrapped_label,
};
use crate::localization::Language;

/// One catalog string in the language this page was rendered with.
fn t(language: crate::localization::Language, key: crate::localization::TextKey) -> String {
    crate::localization::LocalizedText::new(language, key).text
}

/// Which tier of the terminal is on screen.
///
/// The tab row itself never locks: the user may keep reading and switching while a task runs, and
/// only the action buttons inside a tab are gated by [`DeviceToolsSnapshot::busy`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ToolTab {
    #[default]
    Preset,
    Query,
    Expert,
}

/// UI-local state of the tools page.
///
/// Never persisted and never serialized. The expert unlock is deliberately session-local and is
/// reset as soon as the device or SIM context changes, so a switch flipped while looking at one
/// module can never carry over to another.
#[derive(Default)]
pub struct DeviceToolsState {
    pub tab: ToolTab,
    expert_unlocked: bool,
    expert_input: String,
    query_input: String,
    query_selected: Option<ToolReadId>,
    point_to_expert: bool,
    apn_cid: String,
    apn_value: String,
    error: Option<String>,
    notice: Option<String>,
    context: Option<(DeviceEpoch, u64)>,
}

impl DeviceToolsState {
    /// Forget everything typed or unlocked against a previous device/SIM context.
    fn observe_context(&mut self, snapshot: &ControllerSnapshot) {
        let context = snapshot
            .app
            .device
            .as_ref()
            .map(|device| (device.epoch, snapshot.sim_epoch));
        if self.context == context {
            return;
        }
        self.context = context;
        self.expert_unlocked = false;
        self.expert_input.clear();
        self.apn_cid.clear();
        self.apn_value.clear();
        self.error = None;
        self.notice = None;
    }

    /// Debug-only tab selector for the screenshot harness; it never changes any other state.
    #[cfg(debug_assertions)]
    pub(crate) fn set_review_tab(&mut self, tab: usize) {
        self.tab = match tab {
            1 => ToolTab::Query,
            2 => ToolTab::Expert,
            _ => ToolTab::Preset,
        };
    }
}

// ---------------------------------------------------------------------------------------------
// Closed display vocabulary
// ---------------------------------------------------------------------------------------------

/// Status of one capability row. `NotProbed` reads 未查询; it is never presented as a failure.
#[must_use]
pub fn feature_status_text(status: FeatureStatus, language: Language) -> (String, StatusTone) {
    match status {
        FeatureStatus::NotProbed => (
            t(language, crate::localization::TextKey::ToolStateNotQueried),
            StatusTone::Neutral,
        ),
        FeatureStatus::Supported => (
            t(language, crate::localization::TextKey::ToolStateAvailable),
            StatusTone::Positive,
        ),
        FeatureStatus::Empty => (
            t(language, crate::localization::TextKey::ToolStateNoData),
            StatusTone::Neutral,
        ),
        FeatureStatus::UnsupportedConfirmed => (
            t(language, crate::localization::TextKey::ToolStateUnsupported),
            StatusTone::Negative,
        ),
        FeatureStatus::TemporarilyUnavailable => (
            t(
                language,
                crate::localization::TextKey::ToolStateTemporarilyUnavailable,
            ),
            StatusTone::Caution,
        ),
        FeatureStatus::FormatMismatch => (
            t(
                language,
                crate::localization::TextKey::ToolStateFormatMismatch,
            ),
            StatusTone::Caution,
        ),
        FeatureStatus::TransportFailure => (
            t(language, crate::localization::TextKey::ToolStateTimeout),
            StatusTone::Negative,
        ),
    }
}

/// Localized description of one tool outcome. The stable code is shown next to it, so a report can
/// name the exact result without ever quoting the response text.
#[must_use]
pub fn tool_outcome_text(outcome: ToolOutcome, language: Language) -> String {
    match outcome {
        ToolOutcome::Ok => t(language, crate::localization::TextKey::ToolResultOk),
        ToolOutcome::Rejected => t(language, crate::localization::TextKey::ToolResultRejected),
        ToolOutcome::Unsupported => t(
            language,
            crate::localization::TextKey::ToolResultUnsupported,
        ),
        ToolOutcome::TransportFailure => {
            t(language, crate::localization::TextKey::ToolResultNoAnswer)
        }
        ToolOutcome::FormatMismatch => t(
            language,
            crate::localization::TextKey::ToolResultUnrecognized,
        ),
        ToolOutcome::CancelledBeforeWrite => {
            t(language, crate::localization::TextKey::ToolResultCancelled)
        }
        ToolOutcome::OutcomeUnknown => t(
            language,
            crate::localization::TextKey::ToolResultMaybeWritten,
        ),
        ToolOutcome::ContextChanged => t(
            language,
            crate::localization::TextKey::ToolResultInvalidated,
        ),
    }
}

/// Presentation tone of one tool outcome: only a proven refusal or a lost transport reads
/// negative, and an unknown effect stays a caution instead of being rounded either way.
#[must_use]
pub fn tool_outcome_tone(outcome: ToolOutcome) -> StatusTone {
    match outcome {
        ToolOutcome::Ok => StatusTone::Positive,
        ToolOutcome::Rejected | ToolOutcome::Unsupported | ToolOutcome::TransportFailure => {
            StatusTone::Negative
        }
        ToolOutcome::FormatMismatch | ToolOutcome::OutcomeUnknown => StatusTone::Caution,
        ToolOutcome::CancelledBeforeWrite | ToolOutcome::ContextChanged => StatusTone::Neutral,
    }
}

/// Localized description of an input refusal; the parser's stable code stays the machine-readable
/// form and is rendered beside this text.
#[must_use]
pub fn tool_input_error_text(error: ToolInputError, language: Language) -> String {
    match error {
        ToolInputError::Empty => t(language, crate::localization::TextKey::ToolInputEmpty),
        ToolInputError::TooLong => t(language, crate::localization::TextKey::ToolInputTooLong),
        ToolInputError::NonAscii => t(language, crate::localization::TextKey::ToolInputNotAscii),
        ToolInputError::ControlCharacter => t(
            language,
            crate::localization::TextKey::ToolInputControlChars,
        ),
        ToolInputError::ChainedCommand => {
            t(language, crate::localization::TextKey::ToolInputSemicolon)
        }
        ToolInputError::InvalidPrefix => {
            t(language, crate::localization::TextKey::ToolInputMustStartAt)
        }
        ToolInputError::NotWhitelisted => t(
            language,
            crate::localization::TextKey::ToolInputNotWhitelisted,
        ),
        ToolInputError::InteractiveCommand => t(
            language,
            crate::localization::TextKey::ToolInputNeedsInteractive,
        ),
    }
}

/// Display label of one whitelisted read, including the AT request it maps to.
#[must_use]
pub fn tool_read_text(id: ToolReadId, language: Language) -> String {
    match id {
        ToolReadId::Attention => t(language, crate::localization::TextKey::ToolPresetAttention),
        ToolReadId::Manufacturer => t(
            language,
            crate::localization::TextKey::ToolPresetManufacturer,
        ),
        ToolReadId::Model => t(language, crate::localization::TextKey::ToolPresetModel),
        ToolReadId::Revision => t(language, crate::localization::TextKey::ToolPresetFirmware),
        ToolReadId::SimState => t(language, crate::localization::TextKey::ToolPresetSim),
        ToolReadId::SignalQuality => t(language, crate::localization::TextKey::ToolPresetSignal),
        ToolReadId::Operator => t(language, crate::localization::TextKey::ToolPresetCarrier),
        ToolReadId::EpsRegistration => t(
            language,
            crate::localization::TextKey::ToolPresetRegistration,
        ),
        ToolReadId::PacketAttach => t(language, crate::localization::TextKey::ToolPresetAttach),
        ToolReadId::PdpContexts => t(
            language,
            crate::localization::TextKey::ToolPresetPdpContexts,
        ),
        ToolReadId::PdpActivation => t(language, crate::localization::TextKey::ToolPresetPdpActive),
        ToolReadId::PdpAddresses => t(language, crate::localization::TextKey::ToolPresetPdpAddress),
        ToolReadId::UsbNet => t(language, crate::localization::TextKey::ToolPresetUsbMode),
        ToolReadId::Temperature => t(
            language,
            crate::localization::TextKey::ToolPresetTemperature,
        ),
        ToolReadId::ServingCell => t(
            language,
            crate::localization::TextKey::ToolPresetServingCell,
        ),
        ToolReadId::SmsFormat => t(language, crate::localization::TextKey::ToolPresetSmsFormat),
        ToolReadId::SmsStorage => t(language, crate::localization::TextKey::ToolPresetSmsStorage),
    }
}

/// Label of one operation kind, shared by the task strip and the history list.
#[must_use]
pub fn tool_operation_text(kind: ToolOperationKind, language: Language) -> String {
    match kind {
        ToolOperationKind::Read(id) => tool_read_text(id, language),
        ToolOperationKind::ProbeAll => t(language, crate::localization::TextKey::ToolBatchPresets),
        ToolOperationKind::Expert => t(language, crate::localization::TextKey::ToolAdvancedAt),
    }
}

/// Phase label and tone of the task strip.
#[must_use]
pub fn tool_phase_text(phase: ToolPhase, language: Language) -> (String, StatusTone) {
    match phase {
        ToolPhase::Idle => (
            t(language, crate::localization::TextKey::ToolTaskIdle),
            StatusTone::Neutral,
        ),
        ToolPhase::Queued => (
            t(language, crate::localization::TextKey::ToolTaskQueued),
            StatusTone::Progress,
        ),
        ToolPhase::Running => (
            t(language, crate::localization::TextKey::ToolTaskRunning),
            StatusTone::Progress,
        ),
        ToolPhase::Cancelling => (
            t(language, crate::localization::TextKey::ToolTaskCancelling),
            StatusTone::Caution,
        ),
        ToolPhase::Finished => (
            t(language, crate::localization::TextKey::ToolTaskFinished),
            StatusTone::Neutral,
        ),
    }
}

/// Human elapsed time of one task or history entry, in the band that reads honestly.
#[must_use]
pub fn format_elapsed(elapsed: Duration, language: Language) -> String {
    if elapsed.as_secs() >= 60 {
        crate::localization::format_positional(
            language,
            crate::localization::TextKey::ToolElapsedMinutes,
            &[
                &(elapsed.as_secs() / 60).to_string(),
                &(elapsed.as_secs() % 60).to_string(),
            ],
        )
    } else if elapsed.as_millis() >= 1_000 {
        crate::localization::format_positional(
            language,
            crate::localization::TextKey::ToolElapsedSeconds,
            &[&format!("{:.1}", elapsed.as_secs_f64())],
        )
    } else {
        crate::localization::format_positional(
            language,
            crate::localization::TextKey::ToolElapsedMillis,
            &[&elapsed.as_millis().to_string()],
        )
    }
}

/// Last four characters of the device's container id (or instance path when the container id is
/// absent). The full value never reaches the screen.
#[must_use]
pub fn masked_device_id(identity: &StableDeviceIdentity, language: Language) -> String {
    let source = if identity.container_id.trim().is_empty() {
        identity.device_instance_id.as_str()
    } else {
        identity.container_id.as_str()
    };
    let tail: String = {
        let chars: Vec<char> = source.trim().chars().collect();
        chars[chars.len().saturating_sub(4)..].iter().collect()
    };
    if tail.trim().is_empty() {
        t(language, crate::localization::TextKey::ValueNotAvailable)
    } else {
        format!("…{}", tail.trim())
    }
}

/// The verified USB network mode's display name.
#[must_use]
pub fn usb_profile_text(profile: VerifiedUsbNetProfile, language: Language) -> String {
    match profile {
        VerifiedUsbNetProfile::DjiNdis => {
            t(language, crate::localization::TextKey::ToolUsbModeDjiNdis)
        }
        VerifiedUsbNetProfile::Ecm => "ECM".to_owned(),
    }
}

fn usb_profile_raw_value(profile: VerifiedUsbNetProfile) -> u8 {
    match profile {
        VerifiedUsbNetProfile::DjiNdis => 0,
        VerifiedUsbNetProfile::Ecm => 1,
    }
}

/// The exact normalized line the reviewed repair flow puts on the wire for a known write. Shown in
/// the expert tab so a recognized write is confirmed against what will really run.
#[must_use]
pub fn normalized_write_text(write: &ToolWriteId) -> String {
    match write {
        ToolWriteId::RestartModule => "AT+CFUN=1,1".to_owned(),
        ToolWriteId::SetApn { cid, apn } => {
            format!("AT+CGDCONT={},\"IP\",\"{}\"", cid.get(), apn.as_str())
        }
        ToolWriteId::SetUsbNetProfile(profile) => {
            format!("AT+QCFG=\"usbnet\",{}", usb_profile_raw_value(*profile))
        }
    }
}

/// The write request a recognized expert line maps onto, or `None` when it is not a known write.
fn known_write_request(write: &ToolWriteId) -> ControlledRepairRequest {
    match write {
        ToolWriteId::RestartModule => ControlledRepairRequest::RestartModule,
        ToolWriteId::SetApn { cid, apn } => ControlledRepairRequest::SetApn {
            cid: *cid,
            apn: apn.clone(),
        },
        ToolWriteId::SetUsbNetProfile(profile) => {
            ControlledRepairRequest::SetUsbNetProfile { profile: *profile }
        }
    }
}

/// Localized text of one PDP type; these are protocol names and stay verbatim.
#[must_use]
pub fn pdp_type_text(pdp_type: PdpType) -> String {
    match pdp_type {
        PdpType::Ip => "IP".to_owned(),
        PdpType::Ipv6 => "IPv6".to_owned(),
        PdpType::Ipv4v6 => "IPv4v6".to_owned(),
    }
}

fn pdp_state_text(state: PdpContextState, language: Language) -> String {
    match state {
        PdpContextState::Active => t(language, crate::localization::TextKey::PdpActive),
        PdpContextState::Inactive => t(language, crate::localization::TextKey::PdpInactive),
    }
}

fn send_error_text(error: UiSendError, language: Language) -> String {
    match error {
        UiSendError::QueueFull => {
            t(language, crate::localization::TextKey::ToolQueueFull).to_owned()
        }
        UiSendError::Closed => {
            t(language, crate::localization::TextKey::ToolChannelClosed).to_owned()
        }
    }
}

fn controlled_repair_error_text(error: ControlledRepairError, language: Language) -> String {
    match error {
        ControlledRepairError::InvalidPdpContextId => {
            t(language, crate::localization::TextKey::ToolApnContextRange).to_owned()
        }
        ControlledRepairError::InvalidApn => {
            t(language, crate::localization::TextKey::ToolApnInvalid).to_owned()
        }
        ControlledRepairError::InvalidDnsProfile | ControlledRepairError::UnsupportedAction => t(
            language,
            crate::localization::TextKey::ToolControlledUnavailable,
        )
        .to_owned(),
    }
}

fn badge(ui: &mut Ui, text: impl Into<String>, tone: StatusTone) {
    egui::Frame::none()
        .fill(scale::surface_sunken())
        .rounding(6.0)
        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text.into()).size(12.0).color(tone.color()));
        });
}

fn value_text(value: Option<&str>, language: Language) -> RichText {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => RichText::new(value).size(scale::BODY).color(scale::ink()),
        None => RichText::new(t(
            language,
            crate::localization::TextKey::ToolStateNotQueried,
        ))
        .size(scale::META)
        .color(scale::faint()),
    }
}

fn observed_time_text(at: Option<SystemTime>, now: SystemTime, language: Language) -> String {
    let Some(at) = at else {
        return t(language, crate::localization::TextKey::ToolStateNotQueried);
    };
    let clock = super::clock_hms(at)
        .unwrap_or_else(|| t(language, crate::localization::TextKey::ToolTimeUnknown).to_owned());
    match now.duration_since(at) {
        Ok(age) => crate::localization::format_positional(
            language,
            crate::localization::TextKey::ToolClockAgo,
            &[clock.as_str(), &super::format_age(age, language).text],
        ),
        Err(_) => clock,
    }
}

// ---------------------------------------------------------------------------------------------
// Page rendering
// ---------------------------------------------------------------------------------------------

pub(crate) fn render(
    ui: &mut Ui,
    snapshot: &ControllerSnapshot,
    language: Language,
    sink: &dyn crate::app::PanelCommandSink,
    state: &mut DeviceToolsState,
) {
    state.observe_context(snapshot);
    let tools = &snapshot.device_tools;
    let now = SystemTime::now();
    let device_present = snapshot.app.device.is_some();
    // A tool task and a repair operation share the serial actor, so both disable every write
    // button while the tab row itself stays usable.
    let busy = snapshot.serial_work_busy
        || tools.busy()
        || snapshot.operation.as_ref().is_some_and(|operation| {
            matches!(
                operation.state,
                dji4g_application::OperationState::Running { .. }
            )
        });
    let can_act = device_present && !busy;

    render_header(ui, language, snapshot, tools);
    ui.add_space(8.0);
    let previous_tab = state.tab;
    render_tabs(ui, language, state);
    if state.tab != previous_tab {
        state.error = None;
        state.point_to_expert = false;
    }
    ui.add_space(6.0);
    if tools.task.is_some() {
        render_task_strip(ui, language, tools, sink, state);
        ui.add_space(8.0);
    }
    match state.tab {
        ToolTab::Preset => render_preset(ui, snapshot, sink, state, now, language, can_act),
        ToolTab::Query => render_query(ui, language, tools, sink, state, can_act),
        ToolTab::Expert => render_expert(ui, snapshot, sink, state, can_act, language),
    }
    render_feedback(ui, state);
    ui.add_space(8.0);
    render_history(ui, language, tools, state, sink);
}

/// Current target: identity and AT port, with device/SIM epoch available on hover. Nothing here is rendered from a
/// fabricated value — an absent device says so and every action stays disabled.
fn render_header(
    ui: &mut Ui,
    language: Language,
    snapshot: &ControllerSnapshot,
    tools: &DeviceToolsSnapshot,
) {
    super::components::page_heading(
        ui,
        &t(language, crate::localization::TextKey::ToolsTitle),
        &t(language, crate::localization::TextKey::ToolsIntro),
    );
    let device = snapshot.app.device.as_ref();
    let identity = device.map(|device| &device.identity).or_else(|| {
        tools
            .profile
            .context
            .as_ref()
            .map(|context| &context.identity)
    });
    ui.horizontal_wrapped(|ui| match device {
        Some(device) => {
            badge(
                ui,
                t(language, crate::localization::TextKey::ToolsDeviceConnected),
                StatusTone::Positive,
            );
            if let Some(identity) = identity {
                super::wrapped_label(
                    ui,
                    RichText::new(crate::localization::format_positional(
                        language,
                        crate::localization::TextKey::ToolsDeviceIdentity,
                        &[
                            &format!("{:04X}", identity.vid),
                            &format!("{:04X}", identity.pid),
                            &masked_device_id(identity, language),
                        ],
                    ))
                    .size(scale::BODY)
                    .strong()
                    .color(scale::ink()),
                );
            }
            let port = device
                .at_port
                .as_deref()
                .map(str::to_owned)
                .unwrap_or_else(|| t(language, crate::localization::TextKey::ValueNotAvailable));
            ui.label(meta_text(crate::localization::format_positional(
                language,
                crate::localization::TextKey::ToolsAtPort,
                &[&port],
            )))
            .on_hover_text(crate::localization::format_positional(
                language,
                crate::localization::TextKey::ToolsDeviceEpoch,
                &[&device.epoch.0.to_string(), &snapshot.sim_epoch.to_string()],
            ));
        }
        None => {
            badge(
                ui,
                t(language, crate::localization::TextKey::ToolsNoDevice),
                StatusTone::Negative,
            );
            ui.label(meta_text(t(
                language,
                crate::localization::TextKey::ToolsNoDeviceHint,
            )))
            .on_hover_text(crate::localization::format_positional(
                language,
                crate::localization::TextKey::ToolsSimSession,
                &[&snapshot.sim_epoch.to_string()],
            ));
        }
    });
}

fn render_tabs(ui: &mut Ui, language: Language, state: &mut DeviceToolsState) {
    super::components::page_tabs(
        ui,
        egui::Id::new("device-tools-tabs"),
        &mut state.tab,
        &[
            super::components::TabItem::new(
                ToolTab::Preset,
                t(language, crate::localization::TextKey::ToolsTabPresets),
            ),
            super::components::TabItem::new(
                ToolTab::Query,
                t(language, crate::localization::TextKey::ToolsTabReadOnly),
            ),
            super::components::TabItem::new(
                ToolTab::Expert,
                t(language, crate::localization::TextKey::ToolAdvancedAt),
            ),
        ],
    );
    ui.label(meta_text(t(
        language,
        crate::localization::TextKey::ToolsTabSwitchHint,
    )));
}

fn render_task_strip(
    ui: &mut Ui,
    language: Language,
    tools: &DeviceToolsSnapshot,
    sink: &dyn crate::app::PanelCommandSink,
    state: &mut DeviceToolsState,
) {
    egui::Frame::none()
        .fill(scale::surface_sunken())
        .rounding(10.0)
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(section_heading(t(language, crate::localization::TextKey::ToolsTaskProgress)));
                match &tools.task {
                    Some(task) => {
                        let (phase, tone) = tool_phase_text(task.phase, language);
                        badge(ui, phase, tone);
                        ui.label(
                            RichText::new(tool_operation_text(task.operation, language)).color(scale::ink()),
                        );
                        if task.total_items > 0 {
                            ui.label(meta_text(format!(
                                "{}/{}",
                                task.completed_items, task.total_items
                            )));
                        }
                        if task.phase == ToolPhase::Finished {
                            match task.outcome {
                                Some(outcome) => {
                                    ui.label(
                                        RichText::new(
                                            if task.operation == ToolOperationKind::ProbeAll
                                                && outcome == ToolOutcome::Ok
                                            {
                                                t(language, crate::localization::TextKey::ToolsBatchFinished)
                                            } else {
                                                tool_outcome_text(outcome, language)
                                            },
                                        )
                                        .color(tool_outcome_tone(outcome).color()),
                                    );
                                    ui.label(meta_text(outcome.code()));
                                }
                                None => {
                                    ui.label(meta_text(t(language, crate::localization::TextKey::ToolsFinishedUnknown)));
                                }
                            }
                        }
                        if task.phase.is_active()
                            && ui
                                .add_enabled(
                                    task.phase != ToolPhase::Cancelling,
                                    egui::Button::new(t(language, crate::localization::TextKey::ButtonCancel)),
                                )
                                .clicked()
                        {
                            send_tool_command(
                                language,
                                sink,
                                state,
                                UiCommand::CancelDeviceTool { id: task.id },
                            );
                        }
                    }
                    None => {
                        ui.label(meta_text(t(language, crate::localization::TextKey::ToolsNoTask)));
                    }
                }
            });
            if tools
                .task
                .as_ref()
                .is_some_and(|task| task.phase.is_active())
            {
                wrapped_label(
                    ui,
                    meta_text(
                        t(language, crate::localization::TextKey::ToolsCancelNote),
                    ),
                );
            }
            if let Some(refusal) = tools.last_refusal {
                wrapped_label(
                    ui,
                    RichText::new(crate::localization::format_positional(
                        language,
                        crate::localization::TextKey::ToolsLastRejected,
                        &[&tool_outcome_text(refusal, language), refusal.code()],
                    ))
                    .color(tool_outcome_tone(refusal).color()),
                );
            }
        });
}

fn render_preset(
    ui: &mut Ui,
    snapshot: &ControllerSnapshot,
    sink: &dyn crate::app::PanelCommandSink,
    state: &mut DeviceToolsState,
    now: SystemTime,
    language: Language,
    can_act: bool,
) {
    let tools = &snapshot.device_tools;
    let profile = &tools.profile;
    render_profile_section(ui, tools, sink, state, now, language, can_act);
    render_capability_section(ui, language, tools, now, sink, state, can_act);
    render_connection_section(ui, language, profile);
    render_controlled_actions(ui, snapshot, profile, sink, state, now, language);
}

fn render_profile_section(
    ui: &mut Ui,
    tools: &DeviceToolsSnapshot,
    sink: &dyn crate::app::PanelCommandSink,
    state: &mut DeviceToolsState,
    now: SystemTime,
    language: Language,
    can_act: bool,
) {
    let profile = &tools.profile;
    section_frame(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(section_heading(t(
                language,
                crate::localization::TextKey::ToolsProfileHeading,
            )));
            if ui
                .add_enabled(
                    can_act,
                    egui::Button::new(t(
                        language,
                        crate::localization::TextKey::ToolsRefreshProfile,
                    )),
                )
                .on_hover_text(t(
                    language,
                    crate::localization::TextKey::ToolsRefreshProfileHint,
                ))
                .clicked()
            {
                state.notice = None;
                state.error = sink
                    .try_send(UiCommand::ProbeDeviceTools)
                    .err()
                    .map(|error| send_error_text(error, language));
            }
        });
        info_grid(ui, "device-tools-profile-grid", |ui| {
            ui.label(field_label(t(
                language,
                crate::localization::TextKey::FieldManufacturer,
            )));
            ui.label(value_text(profile.manufacturer.as_deref(), language));
            ui.end_row();
            ui.label(field_label(t(
                language,
                crate::localization::TextKey::FieldModel,
            )));
            ui.label(value_text(profile.model.as_deref(), language));
            ui.end_row();
            ui.label(field_label(t(
                language,
                crate::localization::TextKey::FieldFirmwareVersion,
            )));
            ui.label(value_text(profile.revision.as_deref(), language));
            ui.end_row();
            ui.label(field_label(t(
                language,
                crate::localization::TextKey::FieldUsbNetworkMode,
            )));
            match profile.usb_net {
                Some(UsbNetReading::Verified(profile)) => {
                    ui.label(
                        RichText::new(usb_profile_text(profile, language)).color(scale::ink()),
                    );
                }
                Some(UsbNetReading::Unrecognised) => {
                    ui.label(
                        RichText::new(t(language, crate::localization::TextKey::ToolNotRecognized))
                            .color(StatusTone::Caution.color()),
                    );
                }
                None => {
                    ui.label(meta_text(t(
                        language,
                        crate::localization::TextKey::ToolStateNotQueried,
                    )));
                }
            }
            ui.end_row();
            ui.label(field_label(t(
                language,
                crate::localization::TextKey::FieldCapturedAt,
            )));
            ui.label(value_text(
                Some(observed_time_text(profile.observed_at, now, language).as_str()),
                language,
            ));
            ui.end_row();
        });
        if matches!(profile.usb_net, Some(UsbNetReading::Unrecognised)) {
            wrapped_label(
                ui,
                RichText::new(t(
                    language,
                    crate::localization::TextKey::ToolsUsbModeUnverified,
                ))
                .color(StatusTone::Caution.color()),
            );
        }
        if profile.is_empty() {
            wrapped_label(
                ui,
                meta_text(t(language, crate::localization::TextKey::ToolsNoProfileYet)),
            );
        }
    });
}

fn render_capability_section(
    ui: &mut Ui,
    language: Language,
    tools: &DeviceToolsSnapshot,
    now: SystemTime,
    sink: &dyn crate::app::PanelCommandSink,
    state: &mut DeviceToolsState,
    can_act: bool,
) {
    section_frame(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(section_heading(t(
                language,
                crate::localization::TextKey::ToolsEvidenceHeading,
            )));
            ui.label(meta_text(t(
                language,
                crate::localization::TextKey::ToolsEvidenceNote,
            )));
        });
        // Two columns on a desktop-width card: each capability is a short block, so one stacked
        // column left most of the card's width unused and made the section twice as tall as it
        // needed to be.
        //
        // Two explicit columns rather than a `Grid`: the grid reserved a tall empty first row,
        // which showed as a hole between the heading and the first item. Each column is pinned to
        // its width so a detail line wraps inside its own column instead of running past the edge.
        let two_up = ui.available_width() >= 720.0;
        let column_width = if two_up {
            ((ui.available_width() - scale::BLOCK_GAP) / 2.0).floor()
        } else {
            ui.available_width()
        };
        let ids = ToolReadId::ALL;
        let mut draw = |ui: &mut Ui, id: ToolReadId| {
            ui.set_max_width(column_width);
            let row = tools.capability(id);
            let querying = tools.task.as_ref().is_some_and(|task| {
                task.phase.is_active() && task.operation == ToolOperationKind::Read(id)
            });
            let (status_text, tone) = if querying {
                (
                    t(language, crate::localization::TextKey::ToolsQuerying),
                    StatusTone::Progress,
                )
            } else {
                row.map_or(
                    (
                        t(language, crate::localization::TextKey::ToolStateNotQueried),
                        StatusTone::Neutral,
                    ),
                    |row| feature_status_text(row.status, language),
                )
            };
            // Always open: the panel has no disclosure triangles, so a capability row shows its
            // status and its query action together instead of hiding them behind a header.
            ui.label(
                RichText::new(format!(
                    "{}    {} {}",
                    tool_read_text(id, language),
                    tone.marker(),
                    status_text
                ))
                .size(scale::HEADING)
                .strong()
                .color(tone.color()),
            );
            {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(
                            can_act,
                            egui::Button::new(if row.is_some() {
                                t(language, crate::localization::TextKey::ToolsQueryAgain)
                            } else {
                                t(language, crate::localization::TextKey::ToolsQueryItem)
                            }),
                        )
                        .clicked()
                    {
                        send_tool_command(language, sink, state, UiCommand::RunToolRead { id });
                    }
                    if querying {
                        super::components::loading_spinner(ui);
                        ui.label(meta_text(t(
                            language,
                            crate::localization::TextKey::ToolsQueryingKeepLast,
                        )));
                    }
                });
                match row {
                    Some(row) => {
                        wrapped_label(
                            ui,
                            detail_text(crate::localization::format_positional(
                                language,
                                crate::localization::TextKey::ToolsReason,
                                &[&tool_outcome_text(row.reason, language), row.reason.code()],
                            )),
                        );
                        wrapped_label(
                            ui,
                            detail_text(crate::localization::format_positional(
                                language,
                                crate::localization::TextKey::ToolsCaptured,
                                &[
                                    &observed_time_text(Some(row.observed_at), now, language),
                                    &row.context.device_epoch.0.to_string(),
                                    &row.context.sim_epoch.to_string(),
                                ],
                            )),
                        );
                        if id == ToolReadId::SmsStorage
                            && matches!(row.status, FeatureStatus::Supported | FeatureStatus::Empty)
                        {
                            wrapped_label(
                                ui,
                                meta_text(t(
                                    language,
                                    crate::localization::TextKey::ToolsStorageNote,
                                )),
                            );
                        }
                    }
                    None => {
                        wrapped_label(
                            ui,
                            meta_text(t(language, crate::localization::TextKey::ToolsNotRunYet)),
                        );
                    }
                }
            }
            ui.separator();
        };
        if two_up {
            let half = ids.len().div_ceil(2);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = scale::BLOCK_GAP;
                ui.allocate_ui_with_layout(
                    egui::vec2(column_width, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_max_width(column_width);
                        for id in &ids[..half] {
                            draw(ui, *id);
                        }
                    },
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(column_width, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_max_width(column_width);
                        for id in &ids[half..] {
                            draw(ui, *id);
                        }
                    },
                );
            });
        } else {
            for id in ids {
                draw(ui, id);
            }
        }
        wrapped_label(
            ui,
            meta_text(t(
                language,
                crate::localization::TextKey::ToolsStorageCaution,
            )),
        );
    });
}

fn render_connection_section(
    ui: &mut Ui,
    language: Language,
    profile: &dji4g_application::ModuleProfile,
) {
    section_frame(ui, |ui| {
        ui.label(section_heading(t(
            language,
            crate::localization::TextKey::ToolsConnectionHeading,
        )));
        if profile.pdp_contexts.is_empty() {
            wrapped_label(
                ui,
                meta_text(t(language, crate::localization::TextKey::ToolsNoPdpYet)),
            );
        } else {
            info_grid(ui, "device-tools-pdp-grid", |ui| {
                for context in &profile.pdp_contexts {
                    ui.label(field_label(format!("CID {}", context.cid().get())));
                    ui.horizontal_wrapped(|ui| {
                        ui.label(meta_text(pdp_type_text(context.pdp_type())));
                        ui.label(meta_text(pdp_state_text(context.state(), language)));
                        ui.label(
                            RichText::new(format!("APN {}", context.apn().as_str()))
                                .color(scale::ink()),
                        );
                    });
                    ui.end_row();
                }
            });
        }
        if profile.temperature.is_empty() {
            wrapped_label(
                ui,
                meta_text(t(
                    language,
                    crate::localization::TextKey::ToolsNoTemperature,
                )),
            );
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for (index, reading) in profile.temperature.iter().enumerate() {
                    // A firmware channel without a name is shown by its position — the module gave
                    // no identity for it, and this page never invents one.
                    let label = reading.name.clone().unwrap_or_else(|| {
                        crate::localization::format_positional(
                            language,
                            crate::localization::TextKey::ToolsSensor,
                            &[&(index + 1).to_string()],
                        )
                    });
                    badge(
                        ui,
                        format!("{label} {} ℃", reading.celsius),
                        StatusTone::Neutral,
                    );
                }
            });
            wrapped_label(
                ui,
                meta_text(t(language, crate::localization::TextKey::ToolsSensorNote)),
            );
        }
    });
}

fn render_controlled_actions(
    ui: &mut Ui,
    snapshot: &ControllerSnapshot,
    profile: &dji4g_application::ModuleProfile,
    sink: &dyn crate::app::PanelCommandSink,
    state: &mut DeviceToolsState,
    now: SystemTime,
    language: Language,
) {
    use dji4g_application::ActionReadinessKey as Key;
    let apn = super::action_availability::repair_action_availability(
        snapshot,
        Key::EditApn,
        now,
        language,
    );
    let usb = super::action_availability::repair_action_availability(
        snapshot,
        Key::SetUsbNetworkProfile,
        now,
        language,
    );
    let restart = super::action_availability::repair_action_availability(
        snapshot,
        Key::RestartModule,
        now,
        language,
    );
    section_frame(ui, |ui| {
        ui.label(section_heading(t(
            language,
            crate::localization::TextKey::ToolsControlledHeading,
        )));
        wrapped_label(
            ui,
            meta_text(t(
                language,
                crate::localization::TextKey::ToolsControlledNote,
            )),
        );
        ui.add_space(6.0);
        // 修改 APN：cid + apn 两个输入，校验通过后交给受控修复流程。
        ui.horizontal_wrapped(|ui| {
            ui.label(field_label(t(
                language,
                crate::localization::TextKey::FieldPdpContextShort,
            )));
            ui.add(
                egui::TextEdit::singleline(&mut state.apn_cid)
                    .desired_width(44.0)
                    .hint_text("1"),
            );
            ui.label(field_label("APN"));
            ui.add(
                egui::TextEdit::singleline(&mut state.apn_value)
                    .desired_width(180.0)
                    .hint_text(t(language, crate::localization::TextKey::ToolsApnExample)),
            );
            let filled = !state.apn_cid.trim().is_empty() && !state.apn_value.trim().is_empty();
            if ui
                .add_enabled(
                    apn.enabled && filled,
                    egui::Button::new(t(language, crate::localization::TextKey::ToolsEditApn)),
                )
                .clicked()
            {
                state.notice = None;
                match state.apn_cid.trim().parse::<u8>() {
                    Ok(cid) => {
                        match ControlledRepairRequest::try_apn(cid, state.apn_value.trim()) {
                            Ok(request) => {
                                // The reviewed confirmation box opens now, at click time, exactly as
                                // it does on the repairs page; the plan it prepares usually arrives
                                // while the user is still reading it.
                                sink.prepare_repair_now(request);
                                state.notice = Some(crate::localization::format_positional(
                                    language,
                                    crate::localization::TextKey::ToolsApnConfirm,
                                    &[&cid.to_string(), state.apn_value.trim()],
                                ));
                            }
                            Err(error) => {
                                state.error = Some(controlled_repair_error_text(error, language));
                            }
                        }
                    }
                    Err(_) => {
                        state.error = Some(
                            t(language, crate::localization::TextKey::ToolsApnRange).to_owned(),
                        );
                    }
                }
            }
        });
        if let Some(reason) = &apn.reason {
            wrapped_label(ui, meta_text(&reason.text));
        }
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(field_label(t(
                language,
                crate::localization::TextKey::FieldUsbNetworkMode,
            )));
            match profile.usb_net {
                Some(UsbNetReading::Verified(current)) => {
                    let target = match current {
                        VerifiedUsbNetProfile::DjiNdis => VerifiedUsbNetProfile::Ecm,
                        VerifiedUsbNetProfile::Ecm => VerifiedUsbNetProfile::DjiNdis,
                    };
                    ui.label(meta_text(usb_profile_text(current, language)));
                    if ui
                        .add_enabled(
                            usb.enabled,
                            egui::Button::new(crate::localization::format_positional(
                                language,
                                crate::localization::TextKey::ToolsSwitchTo,
                                &[&usb_profile_text(target, language)],
                            )),
                        )
                        .on_hover_text(t(language, crate::localization::TextKey::ToolsSwitchHint))
                        .clicked()
                    {
                        sink.prepare_repair_now(ControlledRepairRequest::SetUsbNetProfile {
                            profile: target,
                        });
                        state.notice = Some(crate::localization::format_positional(
                            language,
                            crate::localization::TextKey::ToolsConfirmRuns,
                            &[&normalized_write_text(&ToolWriteId::SetUsbNetProfile(
                                target,
                            ))],
                        ));
                    }
                }
                Some(UsbNetReading::Unrecognised) => {
                    ui.label(
                        RichText::new(t(language, crate::localization::TextKey::ToolNotRecognized))
                            .color(StatusTone::Caution.color()),
                    );
                    ui.label(meta_text(t(
                        language,
                        crate::localization::TextKey::ToolsCurrentUnrecognized,
                    )));
                }
                None => {
                    ui.label(meta_text(t(
                        language,
                        crate::localization::TextKey::ToolsNotQueriedRefresh,
                    )));
                }
            }
        });
        if let Some(reason) = &usb.reason {
            wrapped_label(ui, meta_text(&reason.text));
        }
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    restart.enabled,
                    egui::Button::new(t(
                        language,
                        crate::localization::TextKey::ToolsRestartModule,
                    )),
                )
                .on_hover_text(t(
                    language,
                    crate::localization::TextKey::ToolsRestartCommand,
                ))
                .clicked()
            {
                sink.prepare_repair_now(ControlledRepairRequest::RestartModule);
                state.notice =
                    Some(t(language, crate::localization::TextKey::ToolsRestartConfirm).to_owned());
            }
            ui.label(meta_text(t(
                language,
                crate::localization::TextKey::ToolsRestartNote,
            )));
        });
        if let Some(reason) = &restart.reason {
            wrapped_label(ui, meta_text(&reason.text));
        }
    });
}

fn render_query(
    ui: &mut Ui,
    language: Language,
    tools: &DeviceToolsSnapshot,
    sink: &dyn crate::app::PanelCommandSink,
    state: &mut DeviceToolsState,
    can_act: bool,
) {
    section_frame(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(section_heading(t(
                language,
                crate::localization::TextKey::ToolsReadOnlyHeading,
            )));
            ui.label(meta_text(t(
                language,
                crate::localization::TextKey::ToolsReadOnlyNote,
            )));
        });
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("device-tools-read-preset")
                .selected_text(state.query_selected.map_or_else(
                    || t(language, crate::localization::TextKey::ToolsChoosePreset),
                    |id| tool_read_text(id, language),
                ))
                .width(280.0)
                .show_ui(ui, |ui| {
                    for id in ToolReadId::ALL {
                        if ui
                            .selectable_label(
                                state.query_selected == Some(id),
                                tool_read_text(id, language),
                            )
                            .clicked()
                        {
                            state.query_selected = Some(id);
                            state.query_input.clear();
                            state.error = None;
                            state.point_to_expert = false;
                        }
                    }
                });
            let has_input = state.query_selected.is_some() || !state.query_input.trim().is_empty();
            if ui
                .add_enabled(
                    can_act && has_input,
                    egui::Button::new(t(language, crate::localization::TextKey::ToolsRunQuery)),
                )
                .clicked()
            {
                run_query(language, sink, state);
            }
        });
        ui.add(
            egui::TextEdit::singleline(&mut state.query_input)
                .hint_text(t(
                    language,
                    crate::localization::TextKey::ToolsWhitelistHint,
                ))
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace),
        );
        if state.point_to_expert {
            ui.horizontal_wrapped(|ui| {
                ui.label(meta_text(t(
                    language,
                    crate::localization::TextKey::ToolsNotInList,
                )));
                if ui
                    .button(t(language, crate::localization::TextKey::ToolsOpenAdvanced))
                    .clicked()
                {
                    state.tab = ToolTab::Expert;
                    state.point_to_expert = false;
                }
            });
        }
        if tools
            .task
            .as_ref()
            .is_some_and(|task| task.phase.is_active())
        {
            wrapped_label(
                ui,
                meta_text(t(language, crate::localization::TextKey::ToolsBusyReadOnly)),
            );
        }
    });
}

fn run_query(
    language: Language,
    sink: &dyn crate::app::PanelCommandSink,
    state: &mut DeviceToolsState,
) {
    state.notice = None;
    state.point_to_expert = false;
    let text = state.query_input.trim();
    if text.is_empty() {
        if let Some(id) = state.query_selected {
            state.error = sink
                .try_send(UiCommand::RunToolRead { id })
                .err()
                .map(|error| send_error_text(error, language));
        }
        return;
    }
    match ValidatedToolLine::parse_read_only(text) {
        Ok((_line, id)) => {
            state.error = sink
                .try_send(UiCommand::RunToolRead { id })
                .err()
                .map(|error| send_error_text(error, language));
        }
        Err(ToolInputError::NotWhitelisted) => {
            state.error = Some(format!(
                "{}（{}）。",
                tool_input_error_text(ToolInputError::NotWhitelisted, language),
                ToolInputError::NotWhitelisted.code()
            ));
            state.point_to_expert = true;
        }
        Err(error) => {
            state.error = Some(crate::localization::format_positional(
                language,
                crate::localization::TextKey::ToolsInvalidInput,
                &[&tool_input_error_text(error, language), error.code()],
            ));
        }
    }
}

fn render_expert(
    ui: &mut Ui,
    snapshot: &ControllerSnapshot,
    sink: &dyn crate::app::PanelCommandSink,
    state: &mut DeviceToolsState,
    can_act: bool,
    language: Language,
) {
    let tools = &snapshot.device_tools;
    section_frame(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(section_heading(t(
                language,
                crate::localization::TextKey::ToolAdvancedAt,
            )));
            if state.expert_unlocked {
                badge(
                    ui,
                    t(language, crate::localization::TextKey::ToolsSessionUnlocked),
                    StatusTone::Caution,
                );
            } else {
                if ui
                    .add_enabled(
                        can_act,
                        egui::Button::new(t(
                            language,
                            crate::localization::TextKey::ToolsEnableAtInput,
                        )),
                    )
                    .clicked()
                {
                    state.expert_unlocked = true;
                }
                wrapped_label(
                    ui,
                    meta_text(t(language, crate::localization::TextKey::ToolsUnlockScope)),
                );
            }
        });
        wrapped_label(
            ui,
            meta_text(t(language, crate::localization::TextKey::ToolsAdvancedNote)),
        );
        if state.expert_unlocked {
            ui.add_space(4.0);
            ui.add(
                egui::TextEdit::singleline(&mut state.expert_input)
                    .hint_text(t(language, crate::localization::TextKey::ToolsAtHint))
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace),
            );
            let write_availability = ValidatedToolLine::parse(state.expert_input.trim())
                .ok()
                .and_then(|line| classify_known_write(&line))
                .map(|write| {
                    use dji4g_application::ActionReadinessKey as Key;
                    let key = match write {
                        ToolWriteId::RestartModule => Key::RestartModule,
                        ToolWriteId::SetUsbNetProfile(_) => Key::SetUsbNetworkProfile,
                        ToolWriteId::SetApn { .. } => Key::EditApn,
                    };
                    super::action_availability::repair_action_availability(
                        snapshot,
                        key,
                        SystemTime::now(),
                        language,
                    )
                });
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        can_act
                            && write_availability
                                .as_ref()
                                .is_none_or(|value| value.enabled)
                            && !state.expert_input.trim().is_empty(),
                        egui::Button::new(t(language, crate::localization::TextKey::ToolsExecute)),
                    )
                    .clicked()
                {
                    submit_expert(language, sink, state);
                }
                if ui
                    .button(t(language, crate::localization::TextKey::ToolsClearInput))
                    .clicked()
                {
                    state.expert_input.clear();
                    state.error = None;
                    state.notice = None;
                }
            });
            if let Some(reason) = write_availability.and_then(|value| value.reason) {
                wrapped_label(ui, meta_text(reason.text));
            }
        } else {
            wrapped_label(
                ui,
                meta_text(t(language, crate::localization::TextKey::ToolsLocked)),
            );
        }
        // The per-command confirmation is independent of the unlock switch: opening the terminal
        // never substitutes for approving one exact command.
        if let Some(pending) = &tools.pending_expert {
            ui.add_space(8.0);
            render_pending_expert(ui, language, pending, sink, state, can_act);
        }
    });
}

fn submit_expert(
    language: Language,
    sink: &dyn crate::app::PanelCommandSink,
    state: &mut DeviceToolsState,
) {
    state.notice = None;
    let text = state.expert_input.trim();
    match ValidatedToolLine::parse(text) {
        Err(error) => {
            state.error = Some(crate::localization::format_positional(
                language,
                crate::localization::TextKey::ToolsCheckFailed,
                &[&tool_input_error_text(error, language), error.code()],
            ));
        }
        Ok(line) => match classify_known_write(&line) {
            Some(write) => {
                // A recognized write is routed through the reviewed repair flow, and the exact
                // normalized line is shown before/while that flow runs.
                state.notice = Some(crate::localization::format_positional(
                    language,
                    crate::localization::TextKey::ToolsNormalizedWrite,
                    &[&normalized_write_text(&write)],
                ));
                // One frozen confirmation for this action, never two: the expert switch does not
                // add a second dialog on top of the repair confirmation.
                sink.prepare_repair_now(known_write_request(&write));
            }
            None => {
                state.error = sink
                    .try_send(UiCommand::PrepareExpertTool { line })
                    .err()
                    .map(|error| send_error_text(error, language));
                if state.error.is_none() {
                    state.notice = Some(
                        t(language, crate::localization::TextKey::ToolsCommandFrozen).to_owned(),
                    );
                }
            }
        },
    }
}

fn render_pending_expert(
    ui: &mut Ui,
    language: Language,
    pending: &PendingExpertTool,
    sink: &dyn crate::app::PanelCommandSink,
    state: &mut DeviceToolsState,
    can_act: bool,
) {
    let remaining = pending
        .expires_at
        .duration_since(SystemTime::now())
        .unwrap_or_default();
    egui::Frame::none()
        .fill(scale::surface_sunken())
        .rounding(10.0)
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(section_heading(t(
                language,
                crate::localization::TextKey::ToolsAtPending,
            )));
            wrapped_label(
                ui,
                meta_text(t(language, crate::localization::TextKey::ToolsFrozenList)),
            );
            egui::Frame::none()
                .fill(scale::surface_sunken())
                .rounding(8.0)
                .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    wrapped_label(
                        ui,
                        RichText::new(pending.line.expose_for_confirmation())
                            .monospace()
                            .color(scale::ink()),
                    );
                });
            wrapped_label(
                ui,
                RichText::new(t(
                    language,
                    crate::localization::TextKey::ToolsUnknownEffect,
                ))
                .color(StatusTone::Caution.color()),
            );
            ui.horizontal_wrapped(|ui| {
                if remaining.is_zero() {
                    ui.label(meta_text(t(
                        language,
                        crate::localization::TextKey::ToolsPlanExpired,
                    )));
                } else {
                    ui.label(meta_text(crate::localization::format_positional(
                        language,
                        crate::localization::TextKey::ToolsPlanRemaining,
                        &[&remaining.as_secs().to_string()],
                    )));
                }
                if ui
                    .add_enabled(
                        can_act && !remaining.is_zero(),
                        egui::Button::new(t(
                            language,
                            crate::localization::TextKey::ConfirmationTitle,
                        )),
                    )
                    .clicked()
                {
                    send_tool_command(
                        language,
                        sink,
                        state,
                        UiCommand::ConfirmExpertTool { id: pending.id },
                    );
                }
                if ui
                    .button(t(language, crate::localization::TextKey::ButtonCancel))
                    .clicked()
                {
                    send_tool_command(
                        language,
                        sink,
                        state,
                        UiCommand::CancelExpertToolPlan { id: pending.id },
                    );
                }
            });
        });
}

fn render_feedback(ui: &mut Ui, state: &DeviceToolsState) {
    if let Some(notice) = &state.notice {
        ui.add_space(10.0);
        wrapped_label(
            ui,
            RichText::new(notice).color(StatusTone::Positive.color()),
        );
    }
    if let Some(error) = &state.error {
        ui.add_space(10.0);
        wrapped_label(ui, RichText::new(error).color(StatusTone::Negative.color()));
    }
}

// ---------------------------------------------------------------------------------------------
// Terminal output
// ---------------------------------------------------------------------------------------------

fn render_history(
    ui: &mut Ui,
    language: Language,
    tools: &DeviceToolsSnapshot,
    state: &mut DeviceToolsState,
    sink: &dyn crate::app::PanelCommandSink,
) {
    section_frame(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(section_heading(t(
                language,
                crate::localization::TextKey::ToolsLogHeading,
            )));
            ui.label(meta_text(t(
                language,
                crate::localization::TextKey::ToolsLogNote,
            )));
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !tools.history.is_empty(),
                    egui::Button::new(t(language, crate::localization::TextKey::ToolsCopySummary)),
                )
                .on_hover_text(t(
                    language,
                    crate::localization::TextKey::ToolsCopySummaryNote,
                ))
                .clicked()
            {
                copy_diagnostic_summary(ui, language, tools);
            }
            if ui
                .add_enabled(
                    !tools.history.is_empty(),
                    egui::Button::new(t(language, crate::localization::TextKey::ToolsCopyRaw)),
                )
                .on_hover_text(t(language, crate::localization::TextKey::ToolsCopyRawNote))
                .clicked()
            {
                copy_raw_response(ui, language, tools);
            }
            if ui
                .add_enabled(
                    !tools.history.is_empty(),
                    egui::Button::new(t(language, crate::localization::TextKey::ToolsClearLog)),
                )
                .on_hover_text(t(language, crate::localization::TextKey::ToolsClearLogNote))
                .clicked()
            {
                send_tool_command(language, sink, state, UiCommand::ClearToolHistory);
            }
        });
        wrapped_label(
            ui,
            meta_text(t(language, crate::localization::TextKey::ToolsShareCaution)),
        );
        if tools.history.is_empty() {
            wrapped_label(
                ui,
                meta_text(t(language, crate::localization::TextKey::ToolsNoLog)),
            );
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("device-tools-history")
            .auto_shrink([false, false])
            .max_height(320.0)
            .show(ui, |ui| {
                // Newest first: the entry the user just triggered stays in view.
                for entry in tools.history.entries().iter().rev() {
                    render_history_entry(ui, language, entry);
                }
            });
    });
}

fn send_tool_command(
    language: Language,
    sink: &dyn crate::app::PanelCommandSink,
    state: &mut DeviceToolsState,
    command: UiCommand,
) {
    state.notice = None;
    state.error = sink
        .try_send(command)
        .err()
        .map(|error| send_error_text(error, language));
}

fn render_history_entry(ui: &mut Ui, language: Language, entry: &ToolHistoryEntry) {
    egui::Frame::none()
        .fill(scale::surface_sunken())
        .rounding(8.0)
        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(tool_operation_text(entry.operation, language))
                        .strong()
                        .color(scale::ink()),
                );
                ui.label(
                    RichText::new(tool_outcome_text(entry.outcome, language))
                        .color(tool_outcome_tone(entry.outcome).color()),
                );
                ui.label(meta_text(format!(
                    "{} · {}",
                    entry.outcome.code(),
                    format_elapsed(entry.elapsed, language)
                )));
                if let Some(clock) = super::clock_hms(entry.finished_at) {
                    ui.label(meta_text(clock));
                }
            });
            for line in entry.transcript.lines() {
                wrapped_label(
                    ui,
                    RichText::new(transcript_line_text(language, line))
                        .monospace()
                        .size(scale::META)
                        .color(scale::secondary()),
                );
            }
            if entry.transcript.is_empty() {
                ui.label(meta_text(t(
                    language,
                    crate::localization::TextKey::ToolsEmptyResponse,
                )));
            }
            if entry.transcript.is_truncated() {
                ui.label(
                    RichText::new(t(
                        language,
                        crate::localization::TextKey::ToolsResponseTruncated,
                    ))
                    .size(scale::META)
                    .color(StatusTone::Caution.color()),
                );
            }
        });
    ui.add_space(6.0);
}

/// One transcript line as a person reads it: a module-initiated report is marked with the
/// catalog's own label, so the transport sentinel never reaches the screen or the clipboard.
fn transcript_line_text(language: Language, line: &str) -> String {
    match urc_transcript_payload(line) {
        Some(payload) => crate::localization::format_positional(
            language,
            crate::localization::TextKey::ToolUrcLine,
            &[payload],
        ),
        None => line.to_owned(),
    }
}

/// Diagnostic summary: operation type, elapsed time and the stable outcome code only — never the
/// response text.
fn copy_diagnostic_summary(ui: &mut Ui, language: Language, tools: &DeviceToolsSnapshot) {
    let mut text = t(language, crate::localization::TextKey::ToolsHistoryHeader);
    for entry in tools.history.entries() {
        text.push_str(&format!(
            "- {} | {} | {}\n",
            tool_operation_text(entry.operation, language),
            entry.outcome.code(),
            format_elapsed(entry.elapsed, language)
        ));
    }
    ui.ctx().copy_text(text);
}

/// Raw response copy: a separate, explicitly labelled action whose output may contain device or
/// account information.
fn copy_raw_response(ui: &mut Ui, language: Language, tools: &DeviceToolsSnapshot) {
    let mut text = String::new();
    for entry in tools.history.entries() {
        text.push_str(&format!(
            "### {} | {} | {}\n",
            tool_operation_text(entry.operation, language),
            entry.outcome.code(),
            format_elapsed(entry.elapsed, language)
        ));
        for line in entry.transcript.lines() {
            text.push_str(&transcript_line_text(language, line));
            text.push('\n');
        }
        if entry.transcript.is_truncated() {
            text.push_str(&t(
                language,
                crate::localization::TextKey::ToolsTruncatedMark,
            ));
        }
    }
    ui.ctx().copy_text(text);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use dji4g_application::{ToolCapabilityRow, ToolContext, ToolHistoryEntry, ToolTranscript};
    use dji4g_at_protocol::SensorTemperature;
    use dji4g_domain::StableDeviceIdentity;

    fn identity() -> StableDeviceIdentity {
        StableDeviceIdentity {
            container_id: "SWD\\VID_2CA3&PID_4006\\5&2A1B3C4D&0&1".to_owned(),
            device_instance_id: "USB\\VID_2CA3&PID_4006\\1234567890AB".to_owned(),
            vid: 0x2CA3,
            pid: 0x4006,
        }
    }

    fn context() -> ToolContext {
        ToolContext {
            device_epoch: DeviceEpoch(4),
            sim_epoch: 2,
            identity: identity(),
            at_port: "COM7".to_owned(),
        }
    }

    fn snapshot_with_tools(tools: DeviceToolsSnapshot) -> ControllerSnapshot {
        let mut snapshot = dji4g_application::ReducerState::new(SystemTime::UNIX_EPOCH).snapshot();
        snapshot.device_tools = tools;
        snapshot
    }

    fn tools_fixture() -> DeviceToolsSnapshot {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let mut tools = DeviceToolsSnapshot {
            profile: dji4g_application::ModuleProfile {
                manufacturer: Some("Quectel".to_owned()),
                model: Some("EC200A-CN".to_owned()),
                revision: Some("EC200ACNAAR02A05M08".to_owned()),
                usb_net: Some(UsbNetReading::Verified(VerifiedUsbNetProfile::DjiNdis)),
                pdp_contexts: Vec::new(),
                temperature: vec![SensorTemperature {
                    name: Some("cpu".to_owned()),
                    celsius: 42,
                }],
                observed_at: Some(now),
                context: Some(context()),
            },
            ..DeviceToolsSnapshot::default()
        };
        tools.record_capability(ToolCapabilityRow::new(
            ToolReadId::Manufacturer,
            ToolOutcome::Ok,
            context(),
            now,
        ));
        tools.record_capability(ToolCapabilityRow::empty(
            ToolReadId::SmsStorage,
            context(),
            now,
        ));
        tools.record_capability(ToolCapabilityRow::new(
            ToolReadId::ServingCell,
            ToolOutcome::TransportFailure,
            context(),
            now,
        ));
        tools.history.push(ToolHistoryEntry {
            id: 1,
            operation: ToolOperationKind::Read(ToolReadId::Manufacturer),
            outcome: ToolOutcome::Ok,
            elapsed: Duration::from_millis(420),
            finished_at: now,
            transcript: Arc::new(ToolTranscript::from_lines(vec![
                "+CGMI: \"Quectel\"".to_owned(),
                "OK".to_owned(),
            ])),
        });
        tools
    }

    fn render_into(tools: DeviceToolsSnapshot, state: &mut DeviceToolsState) {
        struct NoopSink;
        impl crate::app::PanelCommandSink for NoopSink {
            fn try_send(&self, _command: UiCommand) -> Result<(), dji4g_application::UiSendError> {
                Ok(())
            }

            fn prepare_repair_now(&self, _request: ControlledRepairRequest) {}

            fn prepare_action_now(&self, _request: dji4g_application::ActionRequest) {}
        }
        let snapshot = snapshot_with_tools(tools);
        let context = egui::Context::default();
        let _ = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 900.0),
                )),
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    render(ui, &snapshot, Language::ZhCn, &NoopSink, state);
                });
            },
        );
    }

    #[test]
    fn the_masked_device_id_keeps_only_a_short_tail() {
        let masked = masked_device_id(&identity(), Language::ZhCn);
        assert_eq!(masked, "…&0&1");
        assert!(!masked.contains("2A1B3C4D"));
        assert_eq!(
            masked_device_id(
                &StableDeviceIdentity {
                    container_id: String::new(),
                    device_instance_id: "USB\\VID_2CA3&PID_4006\\ABCDEF".to_owned(),
                    vid: 0x2CA3,
                    pid: 0x4006,
                },
                Language::ZhCn,
            ),
            "…CDEF"
        );
    }

    #[test]
    fn known_writes_show_the_normalized_command_the_executor_runs() {
        assert_eq!(
            normalized_write_text(&ToolWriteId::RestartModule),
            "AT+CFUN=1,1"
        );
        assert_eq!(
            normalized_write_text(&ToolWriteId::SetUsbNetProfile(VerifiedUsbNetProfile::Ecm)),
            "AT+QCFG=\"usbnet\",1"
        );
        let line = ValidatedToolLine::parse("AT+CGDCONT=1,\"IP\",\"internet\"")
            .expect("the canonical CGDCONT write validates");
        match classify_known_write(&line) {
            Some(write @ ToolWriteId::SetApn { .. }) => {
                assert_eq!(
                    normalized_write_text(&write),
                    "AT+CGDCONT=1,\"IP\",\"internet\""
                );
            }
            other => panic!("expected a known APN write, got {other:?}"),
        }
    }

    #[test]
    fn every_outcome_and_input_error_has_a_closed_description() {
        for outcome in [
            ToolOutcome::Ok,
            ToolOutcome::Rejected,
            ToolOutcome::Unsupported,
            ToolOutcome::TransportFailure,
            ToolOutcome::FormatMismatch,
            ToolOutcome::CancelledBeforeWrite,
            ToolOutcome::OutcomeUnknown,
            ToolOutcome::ContextChanged,
        ] {
            assert!(!tool_outcome_text(outcome, Language::ZhCn).trim().is_empty());
            assert!(!outcome.code().trim().is_empty());
        }
        for error in [
            ToolInputError::Empty,
            ToolInputError::TooLong,
            ToolInputError::NonAscii,
            ToolInputError::ControlCharacter,
            ToolInputError::ChainedCommand,
            ToolInputError::InvalidPrefix,
            ToolInputError::NotWhitelisted,
            ToolInputError::InteractiveCommand,
        ] {
            assert!(
                !tool_input_error_text(error, Language::ZhCn)
                    .trim()
                    .is_empty()
            );
            assert!(!error.code().trim().is_empty());
        }
        for status in [
            FeatureStatus::NotProbed,
            FeatureStatus::Supported,
            FeatureStatus::Empty,
            FeatureStatus::UnsupportedConfirmed,
            FeatureStatus::TemporarilyUnavailable,
            FeatureStatus::FormatMismatch,
            FeatureStatus::TransportFailure,
        ] {
            assert!(
                !feature_status_text(status, Language::ZhCn)
                    .0
                    .trim()
                    .is_empty()
            );
        }
    }

    #[test]
    fn the_query_helper_text_names_the_advanced_terminal() {
        assert_eq!(
            ValidatedToolLine::parse_read_only("AT+CFUN=1,1"),
            Err(ToolInputError::NotWhitelisted)
        );
        assert!(
            tool_input_error_text(ToolInputError::NotWhitelisted, Language::ZhCn)
                .contains("白名单")
        );
    }

    /// Records both the plain commands and the controlled writes the page prepared, so a test can
    /// tell a refused command from a confirmed one.
    #[derive(Default)]
    struct RecordingSink {
        sent: std::sync::Mutex<Vec<UiCommand>>,
        repairs: std::sync::Mutex<Vec<ControlledRepairRequest>>,
    }

    #[test]
    fn capability_retry_sends_only_that_read_and_clear_is_a_real_command() {
        let sink = RecordingSink::default();
        let mut state = DeviceToolsState::default();
        send_tool_command(
            Language::ZhCn,
            &sink,
            &mut state,
            UiCommand::RunToolRead {
                id: ToolReadId::Temperature,
            },
        );
        send_tool_command(
            Language::ZhCn,
            &sink,
            &mut state,
            UiCommand::ClearToolHistory,
        );
        assert!(matches!(
            sink.sent.lock().unwrap().as_slice(),
            [
                UiCommand::RunToolRead {
                    id: ToolReadId::Temperature
                },
                UiCommand::ClearToolHistory
            ]
        ));
        assert!(sink.repairs.lock().unwrap().is_empty());
    }

    #[test]
    fn rejected_clear_reports_error_without_claiming_the_history_was_cleared() {
        struct Full;
        impl crate::app::PanelCommandSink for Full {
            fn try_send(&self, _: UiCommand) -> Result<(), UiSendError> {
                Err(UiSendError::QueueFull)
            }
            fn prepare_repair_now(&self, _: ControlledRepairRequest) {}
            fn prepare_action_now(&self, _: dji4g_application::ActionRequest) {}
        }
        let mut state = DeviceToolsState {
            notice: Some("上次提示".into()),
            ..Default::default()
        };
        send_tool_command(
            Language::ZhCn,
            &Full,
            &mut state,
            UiCommand::ClearToolHistory,
        );
        assert!(state.notice.is_none());
        assert!(state.error.as_deref().unwrap().contains("队列"));
    }

    /// Click the actual rendered button so tests cover the dispatch wiring, including disabled
    /// confirmation controls, rather than only calling the shared submission helper.
    fn click_button(label: &str, mut render: impl FnMut(&mut Ui)) {
        let ctx = egui::Context::default();
        super::super::apply_style(&ctx);
        let mut point = None;
        for tick in 0..4 {
            let events = if tick >= 2 {
                let pos = point.expect("the requested button must be visible");
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
                Vec::new()
            };
            let output = ctx.run(
                egui::RawInput {
                    events,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 700.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        render(ui);
                    });
                },
            );
            if tick < 2 {
                point = output.shapes.iter().find_map(|shape| {
                    if let egui::Shape::Text(text) = &shape.shape
                        && text.galley.text() == label
                    {
                        Some(text.pos + text.galley.size() * 0.5)
                    } else {
                        None
                    }
                });
            }
        }
    }

    struct RejectingSink {
        error: UiSendError,
        attempts: std::sync::atomic::AtomicUsize,
    }

    impl crate::app::PanelCommandSink for RejectingSink {
        fn try_send(&self, _: UiCommand) -> Result<(), UiSendError> {
            self.attempts
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err(self.error.clone())
        }
        fn prepare_repair_now(&self, _: ControlledRepairRequest) {}
        fn prepare_action_now(&self, _: dji4g_application::ActionRequest) {}
    }

    fn pending_fixture(expires_at: SystemTime) -> PendingExpertTool {
        PendingExpertTool {
            id: 7,
            line: ValidatedToolLine::parse("AT+VENDOR=1").unwrap(),
            expires_at,
        }
    }

    #[test]
    fn tool_cancel_and_expert_buttons_show_actual_submission_failures() {
        for error in [UiSendError::QueueFull, UiSendError::Closed] {
            for action in ["task-cancel", "expert-confirm", "expert-cancel"] {
                let sink = RejectingSink {
                    error: error.clone(),
                    attempts: std::sync::atomic::AtomicUsize::new(0),
                };
                let mut state = DeviceToolsState {
                    notice: Some("old notice".into()),
                    ..Default::default()
                };
                if action == "task-cancel" {
                    let mut controller =
                        dji4g_application::Controller::for_test(SystemTime::UNIX_EPOCH);
                    controller
                        .handle_command(UiCommand::RunToolRead {
                            id: ToolReadId::Model,
                        })
                        .unwrap();
                    let tools = controller.snapshot().device_tools;
                    click_button(
                        &t(Language::ZhCn, crate::localization::TextKey::ButtonCancel),
                        |ui| {
                            render_task_strip(ui, Language::ZhCn, &tools, &sink, &mut state);
                        },
                    );
                } else {
                    let pending = pending_fixture(SystemTime::now() + Duration::from_secs(60));
                    let label = if action == "expert-confirm" {
                        t(
                            Language::ZhCn,
                            crate::localization::TextKey::ConfirmationTitle,
                        )
                    } else {
                        t(Language::ZhCn, crate::localization::TextKey::ButtonCancel)
                    };
                    click_button(&label, |ui| {
                        render_pending_expert(
                            ui,
                            Language::ZhCn,
                            &pending,
                            &sink,
                            &mut state,
                            true,
                        );
                    });
                }
                assert_eq!(
                    sink.attempts.load(std::sync::atomic::Ordering::SeqCst),
                    1,
                    "{action}"
                );
                assert!(state.notice.is_none(), "{action}");
                assert_eq!(
                    state.error.as_deref(),
                    Some(send_error_text(error.clone(), Language::ZhCn).as_str()),
                    "{action}"
                );
            }
        }
    }

    #[test]
    fn an_expired_or_busy_expert_confirmation_cannot_dispatch() {
        for (expired, can_act) in [(true, true), (false, false)] {
            let sink = RejectingSink {
                error: UiSendError::QueueFull,
                attempts: std::sync::atomic::AtomicUsize::new(0),
            };
            let mut state = DeviceToolsState::default();
            let pending = pending_fixture(if expired {
                SystemTime::UNIX_EPOCH
            } else {
                SystemTime::now() + Duration::from_secs(60)
            });
            click_button(
                &t(
                    Language::ZhCn,
                    crate::localization::TextKey::ConfirmationTitle,
                ),
                |ui| {
                    render_pending_expert(ui, Language::ZhCn, &pending, &sink, &mut state, can_act);
                },
            );
            assert_eq!(sink.attempts.load(std::sync::atomic::Ordering::SeqCst), 0);
            assert!(state.error.is_none());
        }
    }

    impl crate::app::PanelCommandSink for RecordingSink {
        fn try_send(&self, command: UiCommand) -> Result<(), dji4g_application::UiSendError> {
            self.sent.lock().expect("healthy lock").push(command);
            Ok(())
        }

        fn prepare_repair_now(&self, request: ControlledRepairRequest) {
            self.repairs.lock().expect("healthy lock").push(request);
        }

        fn prepare_action_now(&self, _request: dji4g_application::ActionRequest) {}
    }

    #[test]
    fn the_query_tab_runs_only_whitelisted_reads() {
        let sink = RecordingSink::default();
        let mut state = DeviceToolsState {
            query_input: "AT+CSQ".to_owned(),
            ..DeviceToolsState::default()
        };
        run_query(Language::ZhCn, &sink, &mut state);
        assert!(state.error.is_none());
        assert!(matches!(
            sink.sent.lock().expect("healthy lock").as_slice(),
            [UiCommand::RunToolRead {
                id: ToolReadId::SignalQuality
            }]
        ));

        // The ComboBox selection runs its preset with no free text.
        let sink = RecordingSink::default();
        let mut state = DeviceToolsState {
            query_selected: Some(ToolReadId::SmsStorage),
            ..DeviceToolsState::default()
        };
        run_query(Language::ZhCn, &sink, &mut state);
        assert!(matches!(
            sink.sent.lock().expect("healthy lock").as_slice(),
            [UiCommand::RunToolRead {
                id: ToolReadId::SmsStorage
            }]
        ));

        // A non-whitelisted line never leaves this tab; it points at the expert terminal instead.
        let sink = RecordingSink::default();
        let mut state = DeviceToolsState {
            query_input: "AT+CFUN=1,1".to_owned(),
            ..DeviceToolsState::default()
        };
        run_query(Language::ZhCn, &sink, &mut state);
        assert!(
            sink.sent.lock().expect("healthy lock").is_empty(),
            "the query tab must never dispatch a non-whitelisted line"
        );
        assert!(state.point_to_expert);
        assert!(
            state
                .error
                .as_deref()
                .is_some_and(|text| text.contains("白名单"))
        );
    }

    #[test]
    fn the_expert_tab_routes_known_writes_to_the_repair_flow() {
        let sink = RecordingSink::default();
        let mut state = DeviceToolsState {
            expert_input: "AT+CFUN=1,1".to_owned(),
            ..DeviceToolsState::default()
        };
        submit_expert(Language::ZhCn, &sink, &mut state);
        // A recognized write goes through the reviewed repair flow's own confirmation; it must not
        // be queued as a plain command, so it cannot bypass that confirmation.
        assert!(sink.sent.lock().expect("healthy lock").is_empty());
        assert!(matches!(
            sink.repairs.lock().expect("healthy lock").as_slice(),
            [ControlledRepairRequest::RestartModule]
        ));
        assert!(
            state
                .notice
                .as_deref()
                .is_some_and(|text| text.contains("AT+CFUN=1,1"))
        );

        // Anything the repair flow does not already know is frozen for its own confirmation.
        let sink = RecordingSink::default();
        let mut state = DeviceToolsState {
            expert_input: "AT+QCFG=\"usbnet\"".to_owned(),
            ..DeviceToolsState::default()
        };
        submit_expert(Language::ZhCn, &sink, &mut state);
        match sink.sent.lock().expect("healthy lock").as_slice() {
            [UiCommand::PrepareExpertTool { line }] => {
                assert_eq!(line.expose_for_confirmation(), "AT+QCFG=\"usbnet\"");
            }
            other => panic!("expected a frozen expert plan, got {other:?}"),
        }
        assert!(state.error.is_none());

        // An interactive family is refused by the parser itself and never reaches the sink.
        let sink = RecordingSink::default();
        let mut state = DeviceToolsState {
            expert_input: "AT+CMGS=12".to_owned(),
            ..DeviceToolsState::default()
        };
        submit_expert(Language::ZhCn, &sink, &mut state);
        assert!(sink.sent.lock().expect("healthy lock").is_empty());
        assert!(
            state
                .error
                .as_deref()
                .is_some_and(|text| text.contains("交互"))
        );
    }

    #[test]
    fn all_three_tabs_render_without_panicking() {
        for tab in [ToolTab::Preset, ToolTab::Query, ToolTab::Expert] {
            let mut state = DeviceToolsState {
                tab,
                ..DeviceToolsState::default()
            };
            render_into(tools_fixture(), &mut state);
        }
    }

    #[test]
    fn the_expert_tab_stays_locked_until_an_explicit_unlock() {
        let mut state = DeviceToolsState::default();
        assert!(!state.expert_unlocked);
        state.expert_unlocked = true;
        assert!(state.expert_input.is_empty());
    }

    #[test]
    fn a_device_or_sim_change_relocks_the_expert_terminal() {
        let mut ready =
            dji4g_application::ReducerState::test_ready(SystemTime::UNIX_EPOCH).snapshot();
        let mut state = DeviceToolsState::default();
        state.observe_context(&ready);
        state.expert_unlocked = true;
        state.expert_input = "AT+CSQ".to_owned();
        state.apn_value = "internet".to_owned();

        state.observe_context(&ready);
        assert!(state.expert_unlocked, "the same context keeps the unlock");

        let device = ready.app.device.clone();
        ready.app = Arc::new(dji4g_domain::AppSnapshot {
            device: None,
            ..(*ready.app).clone()
        });
        state.observe_context(&ready);
        assert!(
            !state.expert_unlocked,
            "a changed context relocks the terminal"
        );
        assert!(state.expert_input.is_empty());
        assert!(state.apn_value.is_empty());

        let mut restored = ready;
        restored.app = Arc::new(dji4g_domain::AppSnapshot {
            device,
            ..(*restored.app).clone()
        });
        state.observe_context(&restored);
        assert!(
            !state.expert_unlocked,
            "returning to a device is a new context and must relock the terminal"
        );
    }
}
