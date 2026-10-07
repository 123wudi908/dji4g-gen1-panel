use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct DeviceProfile {
    pub vid: u16,
    pub pid: u16,
}

impl DeviceProfile {
    pub const DJI_GEN1: Self = DJI_GEN1;

    #[must_use]
    pub const fn matches(self, vid: u16, pid: u16) -> bool {
        self.vid == vid && self.pid == pid
    }

    /// The declared profile for one USB VID/PID pair, or `None` when the module is not recognized.
    #[must_use]
    pub fn from_vid_pid(vid: u16, pid: u16) -> Option<Self> {
        SUPPORTED
            .into_iter()
            .find(|profile| profile.matches(vid, pid))
    }

    /// Whether the panel may write to this profile at all.
    ///
    /// Recognition and write eligibility are deliberately separate: a recognized profile is
    /// inspected read-only unless it is proven write-capable.  Today only [`DJI_GEN1`] carries the
    /// reviewed repairs, driver package and controlled AT writes; every other recognized profile
    /// (the generic `2C7C:0125` module) stays read-only, and this predicate is the single place
    /// that decision is made.
    #[must_use]
    pub const fn allows_controlled_actions(self) -> bool {
        self.vid == DJI_GEN1.vid && self.pid == DJI_GEN1.pid
    }
}

pub const DJI_GEN1: DeviceProfile = DeviceProfile {
    vid: 0x2CA3,
    pid: 0x4006,
};

/// A generic Quectel module (`2C7C` is Quectel's USB vendor ID).  It is recognized so the panel
/// can identify it, find its AT interface and run the read-only checks, but it is never eligible
/// for driver installation, controlled repairs, SMS send or storage switching.
pub const QUECTEL_GENERIC: DeviceProfile = DeviceProfile {
    vid: 0x2C7C,
    pid: 0x0125,
};

/// Every module profile the panel recognizes, in a stable order.
pub const SUPPORTED: [DeviceProfile; 2] = [DJI_GEN1, QUECTEL_GENERIC];

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct DeviceEpoch(pub u64);

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StableDeviceIdentity {
    pub container_id: String,
    pub device_instance_id: String,
    pub vid: u16,
    pub pid: u16,
}

impl StableDeviceIdentity {
    #[must_use]
    pub fn is_supported(&self) -> bool {
        self.profile().is_some()
    }

    /// The declared profile this identity matches, or `None` when it is not a recognized module.
    ///
    /// The recorded `vid`/`pid` must agree exactly with the VID/PID parsed out of
    /// `device_instance_id`, so a scalar field can never hide a contradictory instance identity.
    #[must_use]
    pub fn profile(&self) -> Option<DeviceProfile> {
        let (vid, pid) = self.parsed_usb_vid_pid()?;
        if (vid, pid) != (self.vid, self.pid) {
            return None;
        }
        DeviceProfile::from_vid_pid(vid, pid)
    }

    /// Whether this identity names a recognized module the panel may write to.
    ///
    /// A recognized read-only module (the generic `2C7C:0125`) is `is_supported()` but not
    /// `allows_controlled_actions()`; an unrecognized identity is neither.
    #[must_use]
    pub fn allows_controlled_actions(&self) -> bool {
        self.profile()
            .is_some_and(DeviceProfile::allows_controlled_actions)
    }

    fn parsed_usb_vid_pid(&self) -> Option<(u16, u16)> {
        let mut segments = self.device_instance_id.split('\\');
        let bus = segments.next()?;
        let hardware_id = segments.next()?;
        let instance = segments.next()?;
        if !bus.eq_ignore_ascii_case("USB") || instance.is_empty() || segments.next().is_some() {
            return None;
        }

        let mut vid = None;
        let mut pid = None;
        for component in hardware_id.split('&') {
            if let Some(value) = parse_prefixed_hex_u16(component, "VID_") {
                if vid.replace(value).is_some() {
                    return None;
                }
            } else if let Some(value) = parse_prefixed_hex_u16(component, "PID_") {
                if pid.replace(value).is_some() {
                    return None;
                }
            }
        }

        Some((vid?, pid?))
    }
}

fn parse_prefixed_hex_u16(component: &str, prefix: &str) -> Option<u16> {
    if component.len() != prefix.len() + 4
        || !component.get(..prefix.len())?.eq_ignore_ascii_case(prefix)
    {
        return None;
    }

    u16::from_str_radix(component.get(prefix.len()..)?, 16).ok()
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum EvidenceSource {
    Pnp,
    AtControl,
    WindowsAdapter,
    BoundGatewayProbe,
    BoundDnsProbe,
    BoundPublicProbe,
    GlobalRoute,
    GlobalConnectivity,
    Hotspot,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence<T> {
    pub epoch: DeviceEpoch,
    pub observed_at: SystemTime,
    pub ttl: Duration,
    pub source: EvidenceSource,
    pub value: T,
}

impl<T> Evidence<T> {
    #[must_use]
    pub fn is_fresh_for(&self, epoch: DeviceEpoch, now: SystemTime) -> bool {
        if self.epoch != epoch {
            return false;
        }

        now.duration_since(self.observed_at)
            .is_ok_and(|age| age <= self.ttl)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DevicePresence {
    Supported(DeviceProfile),
    NotDetected,
    Unsupported { vid: u16, pid: u16 },
    PermissionDenied,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceSnapshot {
    pub epoch: DeviceEpoch,
    pub identity: StableDeviceIdentity,
    pub problem_code: Option<u32>,
    pub at_port: Option<String>,
    pub adapter_id: Option<String>,
}
