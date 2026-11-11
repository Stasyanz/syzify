//! What the training screen is told (indoor rides, #273).

use serde::{Deserialize, Serialize};

/// Which sensor a value or a link is: also how the ride's journal names
/// the source of a second's power and cadence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensorRole {
    Trainer,
    Pedals,
    Strap,
}

/// A device that advertised during a search, for the screen to offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FoundDevice {
    pub role: SensorRole,
    /// What the OS calls it; a link's `Target.address`.
    pub address: String,
    pub name: Option<String>,
}

/// Where the session is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrainingPhase {
    /// The devices connect; nothing is recorded.
    Pairing,
    Riding,
    Paused,
    /// The ride is over: the trainer is let go, then the ride imported.
    Finishing,
}

/// One device's link as the screen shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    /// Not heard yet.
    Searching,
    Connected,
    /// Gone for now; the link looks for it again (routine for some trainers).
    Lost,
    /// Given up for good; see `problem`.
    Failed,
}

/// Why a link gave up, for the screen to say in its own words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkProblem {
    PermissionDenied,
    RadioOff,
    AdapterGone,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LinkStatus {
    pub role: SensorRole,
    pub state: LinkState,
    /// What the device calls itself, once it said.
    pub name: Option<String>,
    pub problem: Option<LinkProblem>,
    /// The library's own words for `Other`.
    pub detail: Option<String>,
}

/// The ride as it is now, a few times a second.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TrainingLive {
    pub phase: TrainingPhase,
    /// Seconds recorded (the timer: pauses left out).
    pub ride_s: u32,
    /// Seconds recorded since the lap began.
    pub lap_s: u32,
    /// The lap under way, from 1.
    pub lap: u32,
    pub watts: Option<u16>,
    pub power_from: Option<SensorRole>,
    /// The trainer's own watts, apart from the pedals'.
    pub trainer_watts: Option<u16>,
    pub cadence_rpm: Option<u8>,
    pub heart_rate: Option<u8>,
    pub speed_kmh: Option<f32>,
    pub distance_m: f32,
    pub avg_watts: Option<u16>,
    /// The trainer's profile, once it said who it is.
    pub profile: Option<String>,
    pub links: Vec<LinkStatus>,
    /// Journal writes that failed (the ride goes on), newest last.
    pub warnings: Vec<String>,
    /// Seconds the Mac slept during the ride: they are not in it.
    pub slept_s: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishKind {
    Imported,
    /// The same ride was in the library already.
    Duplicate,
    /// Not one value was recorded: nothing to import.
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrainingFinish {
    pub kind: FinishKind,
    /// The new activity, to open.
    pub activity_id: Option<String>,
}

/// A ride a crash or a closed app left unfinished.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PendingRide {
    pub start_unix: i64,
    /// Seconds recorded; None when it does not read.
    pub ride_s: Option<u32>,
    /// Encrypted and the vault locked.
    pub locked: bool,
    /// Why it does not read (another key, a ruined file).
    pub unreadable: Option<String>,
}
