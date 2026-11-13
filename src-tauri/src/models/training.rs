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

/// Whether ANT+ can be used (it needs the ANT+ network key, which is not in
/// the sources).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AntKeyStatus {
    /// This build was given it.
    pub built_in: bool,
    /// It is known: built in, or typed in.
    pub known: bool,
    /// Where Garmin publishes it…
    pub page: String,
    /// …and the agreement its use is subject to.
    pub agreement: String,
}

/// Why an ANT+ stick did not search along.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StickProblem {
    /// Another app holds it (Zwift, Garmin Express).
    InUse,
    /// Anything else, in the stick's own words.
    Other { detail: String },
}

/// What a search found, and why the stick (if one is plugged in) did not
/// search along.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Found {
    pub devices: Vec<FoundDevice>,
    pub stick: Option<StickProblem>,
}

/// How a device is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Radio {
    Ble,
    /// Through an ANT+ USB stick.
    Ant,
}

/// A device that advertised during a search, for the screen to offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FoundDevice {
    pub role: SensorRole,
    /// The same trainer may be found both ways: the user picks.
    pub radio: Radio,
    /// What the OS calls it; a link's `Target.address`.
    pub address: String,
    pub name: Option<String>,
    /// Pedals that are two devices (an Assioma Duo over BLE: one per leg):
    /// the other pedal's address. Both are linked, their power summed.
    pub other_side: Option<String>,
    /// One pedal of such a pair, seen without the other: its power is one
    /// leg's.
    pub lone_side: bool,
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
    StickGone,
    StickBusy,
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

/// What ERG is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErgState {
    /// The target waits for the legs: before the ride, at its start, after
    /// a pause.
    Ready,
    /// Sent, not confirmed yet (or the trainer is out of reach).
    Waiting,
    /// The trainer holds the target (or settles on it).
    Holding,
    /// The watts stayed away from the target: the target goes again.
    OffTarget,
    /// The legs stalled: the brake is let go until they turn.
    LowCadence,
    /// The ride is paused: the brake is let go.
    Paused,
    /// Nothing sends a cadence: held on the trainer's watts, let go only
    /// once they stop (slowing down is not seen).
    NoCadence,
    /// Riding free: the brake is being let go, not confirmed yet
    /// (`target_w` is 0).
    Releasing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ErgLive {
    pub target_w: u16,
    pub state: ErgState,
}

/// Power match: whether ERG is held by the pedals' watts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchState {
    /// Held by the pedals: the trainer is sent a corrected target.
    Matching,
    /// The ride's pedals are not heard (not yet, or lost): held by the
    /// trainer's own watts.
    NoPedals,
}

/// A workout interval, power as a share of FTP.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WorkoutSegment {
    pub secs: u32,
    pub from: f32,
    pub to: f32,
    /// No target: ridden free.
    pub free: bool,
    pub cadence: Option<u8>,
}

/// What was left out of a workout file, or ridden otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkoutNote {
    /// An element this app does not know, without a duration: skipped.
    Unknown { element: String },
    /// An element this app does not know, with a duration (`MaxEffort`,
    /// …): ridden free for as long.
    RiddenFree { element: String },
    /// A value missing or out of range: the interval skipped.
    BadValue { element: String },
    /// A message placed by distance, not time.
    DistanceCue,
    /// A message past the end of its interval.
    CueOutside,
}

/// Why a workout file cannot be ridden.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkoutProblem {
    /// Not XML, or broken.
    Unreadable { detail: String },
    /// XML, but no `<workout_file>` with a `<workout>`.
    NotAWorkout,
    /// A running workout (paces, not powers).
    Run,
    /// Intervals in metres, not seconds.
    Distance,
    /// Nothing that can be ridden in it.
    Empty,
    /// Longer than a day, or more intervals than a workout has.
    TooLong,
    TooBig,
}

/// A workout file in the workouts folder.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorkoutFile {
    /// Within the folder, `/`-separated: how it is started.
    pub path: String,
    pub name: String,
    pub duration_s: u32,
    /// Its power over time, a few dozen points at most (to draw).
    pub profile: Vec<WorkoutSegment>,
    /// What was left out of it.
    pub notes: Vec<WorkoutNote>,
    /// Why it cannot be ridden.
    pub problem: Option<WorkoutProblem>,
}

/// The workouts folder and what is in it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorkoutList {
    pub dir: Option<String>,
    /// The folder picked is gone.
    pub missing: bool,
    pub workouts: Vec<WorkoutFile>,
}

/// The workout under way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkoutLive {
    pub name: String,
    pub elapsed_s: u32,
    pub total_s: u32,
    /// The step under way, from 0; `steps` once it is over.
    pub step: u32,
    pub steps: u32,
    /// Seconds left in the step.
    pub step_left_s: u32,
    /// The step has no target: ridden free.
    pub free: bool,
    /// The cadence the step suggests.
    pub cadence: Option<u8>,
    pub next: Option<NextStep>,
    /// A line the workout shows now.
    pub cue: Option<String>,
    pub done: bool,
}

/// The step after this one (0 W both ways: ridden free).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NextStep {
    pub secs: u32,
    pub from_w: u16,
    pub to_w: u16,
}

/// Power match while ERG holds a target on a ride with pedals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MatchLive {
    pub state: MatchState,
    /// What the trainer is sent for the target.
    pub trainer_target_w: u16,
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
    /// ERG, while a target is set; None riding free.
    pub erg: Option<ErgLive>,
    /// Power match, while ERG holds a target on a ride with pedals.
    pub power_match: Option<MatchLive>,
    /// The workout ERG follows, if one.
    pub workout: Option<WorkoutLive>,
    /// Finishing: the trainer refused letting go (another app may control
    /// it); still asked for a few seconds.
    pub release_unconfirmed: bool,
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
    /// Encrypted, but this vault is not: it was sealed under another
    /// vault's key and cannot be opened here.
    pub sealed_elsewhere: bool,
    /// Why it does not read (another key, a ruined file).
    pub unreadable: Option<String>,
}
