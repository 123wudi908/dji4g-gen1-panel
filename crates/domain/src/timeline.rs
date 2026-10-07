//! Observed network/device change timeline (research document §5.4, §7.3).
//!
//! Every entry records only what was observed: time, event kind, and a short, non-sensitive
//! detail string. Sampling can only see the changes it actually witnesses, so the model never
//! claims to have captured every radio handover or disconnect.

use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::BoundDnsStatus;
use crate::RegistrationState;

/// One observed transition.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineEvent {
    pub at: SystemTime,
    pub kind: TimelineEventKind,
    /// Closed values of the transition, if it carried any. The event carries no wording: the
    /// presentation layer formats [`TimelineDetail`] against its own catalog, so this crate and
    /// the reducer below it stay language-free.
    pub detail: TimelineDetail,
}

/// Closed set of observed event kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TimelineEventKind {
    /// A different SIM identity was observed (ICCID fingerprint changed).
    SimChanged,
    /// Cellular registration state changed (e.g. 已注册 → 搜索中).
    RegistrationChanged,
    /// The observed serving cell changed.
    CellChanged,
    /// The module stopped being detected (USB removal / unplug).
    DeviceRemoved,
    /// The module reappeared (re-enumeration).
    DeviceArrived,
    /// The bound adapter's link state changed.
    AdapterLinkChanged,
    /// The bound DNS probe changed verdict.
    DnsChanged,
}

/// The values one observed transition carries, if any.
///
/// Nothing here is display text: the wording of a timeline row belongs to the panel's catalog. A
/// variant must agree with its event's [`TimelineEventKind`] — `Registration` is only ever
/// recorded on a `RegistrationChanged` event, and `Dns` only on a `DnsChanged` one.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TimelineDetail {
    /// The transition has no values of its own; the event kind's phrase is the row text.
    Kind,
    /// Cellular registration moved from one closed state to another.
    Registration {
        from: RegistrationState,
        to: RegistrationState,
    },
    /// The bound DNS verdict moved from one closed verdict to another.
    Dns {
        from: BoundDnsStatus,
        to: BoundDnsStatus,
    },
}

/// Bounded timeline ring held by the application state.
///
/// Capacity is intentionally small: this is an operator aid, not a logging system, and the
/// entries must stay exportable in volume.
pub const TIMELINE_CAPACITY: usize = 64;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Timeline {
    events: Vec<TimelineEvent>,
}

impl Timeline {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push the newest event, dropping the oldest when the ring is full.
    pub fn push(&mut self, event: TimelineEvent) {
        if self.events.len() == TIMELINE_CAPACITY {
            self.events.remove(0);
        }
        self.events.push(event);
    }

    /// Events oldest-first.
    #[must_use]
    pub fn events(&self) -> &[TimelineEvent] {
        &self.events
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_is_bounded_and_keeps_the_newest_events() {
        const STATES: [RegistrationState; 6] = [
            RegistrationState::RegisteredHome,
            RegistrationState::RegisteredRoaming,
            RegistrationState::Searching,
            RegistrationState::Denied,
            RegistrationState::NotRegistered,
            RegistrationState::Unknown,
        ];
        let mut timeline = Timeline::new();
        for index in 0..(TIMELINE_CAPACITY + 5) {
            timeline.push(TimelineEvent {
                at: SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(index as u64),
                kind: TimelineEventKind::RegistrationChanged,
                detail: TimelineDetail::Registration {
                    from: RegistrationState::Searching,
                    to: STATES[index % STATES.len()],
                },
            });
        }
        assert_eq!(timeline.events().len(), TIMELINE_CAPACITY);
        assert_eq!(
            timeline.events().last().map(|event| event.at),
            Some(
                SystemTime::UNIX_EPOCH
                    + std::time::Duration::from_secs((TIMELINE_CAPACITY + 4) as u64)
            )
        );
        assert_eq!(
            timeline.events().first().map(|event| event.at),
            Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(5))
        );
    }
}
