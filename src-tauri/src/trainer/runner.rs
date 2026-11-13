//! The ride on a thread of its own: ticks the session four times a second
//! (it syncs the journal, so never on the async runtime), takes the
//! screen's orders between ticks and tells the screen what it sees.
//!
//! Everything the app around it provides — the vault, the import, the
//! window — comes through [`Host`], and time through [`Clock`], so a whole
//! ride, from pairing to the import, runs in a test against the fake
//! trainer in milliseconds.
//!
//! The end of a ride, finished or discarded, first lets the trainer go
//! (`Control::Release`, retried while the trainer answers but does not
//! confirm, within the profile's release budget), then drops the links,
//! then imports or throws the ride away.

use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::erg::{Erg, ErgHandle, Workout};
use super::fit::{Ride, SubSport};
use super::journal::{Journal, Source};
use super::link::{Control, ControlId, ControlOutcome, LinkEvent, LinkFailure};
use super::session::{Session, Tick, PEDALS_HOLD};
use crate::error::CommandError;
use crate::models::training::{LinkProblem, LinkState, LinkStatus, TrainingFinish, TrainingLive, TrainingPhase};

/// How often the session is ticked.
pub const TICK: Duration = Duration::from_millis(250);
/// A value older than this is not shown (the ride holds the pedals as long).
const FRESH: Duration = Duration::from_secs(PEDALS_HOLD as u64);
/// Journal failures kept for the screen.
const WARNINGS: usize = 3;
/// The wall clock running ahead of the session's by more than this is a
/// sleep (`Instant` does not count it on macOS).
const SLEEP_SLACK: f64 = 5.0;
/// Between two tries to let the trainer go.
const RELEASE_RETRY: Duration = Duration::from_secs(1);

/// Session time, the wall clock, and waiting.
pub trait Clock {
    fn now(&self) -> Duration;
    fn wall_unix(&self) -> f64;
    fn sleep(&self, d: Duration);
}

/// The real one: session time from the session's start `Instant`.
pub struct SystemClock {
    epoch: Instant,
}

impl SystemClock {
    pub fn new(epoch: Instant) -> SystemClock {
        SystemClock { epoch }
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.epoch.elapsed()
    }
    fn wall_unix(&self) -> f64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
    }
    fn sleep(&self, d: Duration) {
        std::thread::sleep(d);
    }
}

pub type Reply<T> = Sender<Result<T, CommandError>>;

/// What the screen asks for.
pub enum Order {
    /// Start the ride's clock.
    Begin(Reply<()>),
    Lap,
    Pause,
    Resume,
    /// ERG: hold these watts; None rides free.
    SetTarget(Option<u16>),
    /// ERG: follow this workout from the next second (from the start of
    /// the ride, set before it).
    SetWorkout(Workout),
    /// End the ride, let the trainer go, import it.
    Finish(Reply<TrainingFinish>),
    /// End the ride, let the trainer go, throw it away.
    Discard(Reply<()>),
    /// Drop the links before a ride began.
    Leave(Reply<()>),
}

/// What the app does for a ride.
pub trait Host {
    /// The ride begins: claim the vault, open the journal, keep the Mac
    /// awake. An error leaves the session pairing.
    fn begin(&mut self, start_unix: i64, sub_sport: SubSport) -> Result<Option<Journal>, CommandError>;
    /// The ride is over and the trainer let go: import it (and remove the
    /// journal).
    fn finish(&mut self, ride: &Ride, journal: Option<&Path>) -> Result<TrainingFinish, CommandError>;
    /// Throw the ride away.
    fn discard(&mut self, journal: Option<&Path>) -> Result<(), CommandError>;
    fn show(&mut self, live: &TrainingLive);
    /// The session is over, however it ended: let go of the vault and the
    /// Mac.
    fn done(&mut self);
}

/// The refusal to leave a ride that runs.
pub fn riding() -> CommandError {
    CommandError::coded("training.riding", "Finish or discard the ride first")
}

/// What the rest of the app hears while a ride runs: it holds the vault
/// and the trainer until it is finished or discarded.
pub fn ride_under_way() -> CommandError {
    CommandError::coded("training.rideUnderWay", "An indoor ride is under way: finish or discard it first")
}

/// The last value of one kind and when it came.
type Stamped<T> = Option<(T, Duration)>;

#[derive(Default, Clone, Copy)]
struct Latest {
    watts: Stamped<u16>,
    cadence: Stamped<u8>,
    heart_rate: Stamped<u8>,
    speed: Stamped<f32>,
}

/// What the screen is shown, kept up from the ticks.
struct View {
    links: Vec<LinkStatus>,
    latest: [Latest; 3],
    warnings: Vec<String>,
    /// Wall clock and session time at the ride's start.
    began: Option<(f64, Duration)>,
    watts_sum: u64,
    watts_n: u64,
    counted: usize,
    erg: ErgHandle,
    /// Letting go at the end: the last answer was a refusal.
    release_unconfirmed: bool,
}

fn slot(role: Source) -> usize {
    match role {
        Source::Trainer => 0,
        Source::Pedals => 1,
        Source::Strap => 2,
    }
}

fn problem_of(reason: &LinkFailure) -> (LinkProblem, Option<String>) {
    match reason {
        LinkFailure::PermissionDenied => (LinkProblem::PermissionDenied, None),
        LinkFailure::RadioOff => (LinkProblem::RadioOff, None),
        LinkFailure::AdapterGone => (LinkProblem::AdapterGone, None),
        LinkFailure::StickGone => (LinkProblem::StickGone, None),
        LinkFailure::StickBusy => (LinkProblem::StickBusy, None),
        LinkFailure::Other(why) => (LinkProblem::Other, Some(why.clone())),
    }
}

impl View {
    fn new(roles: &[Source], erg: ErgHandle) -> View {
        View {
            erg,
            release_unconfirmed: false,
            links: roles
                .iter()
                .map(|&role| LinkStatus { role, state: LinkState::Searching, name: None, problem: None, detail: None })
                .collect(),
            latest: [Latest::default(); 3],
            warnings: Vec::new(),
            began: None,
            watts_sum: 0,
            watts_n: 0,
            counted: 0,
        }
    }

    fn warn(&mut self, warnings: Vec<String>) {
        for w in warnings {
            // A failing disk says the same every second: once is enough.
            if !self.warnings.contains(&w) {
                self.warnings.push(w);
            }
        }
        let extra = self.warnings.len().saturating_sub(WARNINGS);
        self.warnings.drain(..extra);
    }

    fn link(&mut self, role: Source) -> Option<&mut LinkStatus> {
        self.links.iter_mut().find(|l| l.role == role)
    }

    fn absorb(&mut self, tick: Tick) {
        for (role, event) in tick.events {
            let Some(link) = self.link(role) else { continue };
            match event {
                LinkEvent::Connected { .. } | LinkEvent::Back { .. } => link.state = LinkState::Connected,
                LinkEvent::Lost { .. } => link.state = LinkState::Lost,
                LinkEvent::Failed { reason, .. } => {
                    let (problem, detail) = problem_of(&reason);
                    link.state = LinkState::Failed;
                    link.problem = Some(problem);
                    link.detail = detail;
                }
                LinkEvent::Identified { identity, .. } => {
                    link.name = identity.name.or(identity.model).or(link.name.take());
                }
                LinkEvent::Reading(r) => {
                    let l = &mut self.latest[slot(role)];
                    let at = r.at;
                    if let Some(w) = r.watts {
                        l.watts = Some((w, at));
                    }
                    if let Some(c) = r.cadence_rpm {
                        l.cadence = Some((c, at));
                    }
                    if let Some(h) = r.heart_rate {
                        l.heart_rate = Some((h, at));
                    }
                    if let Some(s) = r.speed_kmh {
                        l.speed = Some((s, at));
                    }
                }
                LinkEvent::Control { .. } => {}
            }
        }
        self.warn(tick.warnings);
    }

    fn connected(&self, role: Source) -> bool {
        self.state_of(role) == Some(LinkState::Connected)
    }

    fn state_of(&self, role: Source) -> Option<LinkState> {
        self.links.iter().find(|l| l.role == role).map(|l| l.state)
    }

    /// A value from `role` if it is fresh and its device connected.
    fn fresh<T: Copy>(&self, role: Source, now: Duration, pick: impl Fn(&Latest) -> Stamped<T>) -> Option<T> {
        if !self.connected(role) {
            return None;
        }
        pick(&self.latest[slot(role)]).filter(|&(_, at)| now.saturating_sub(at) <= FRESH).map(|(v, _)| v)
    }

    fn live(&mut self, session: &Session, phase: TrainingPhase, clock: &dyn Clock) -> TrainingLive {
        let now = clock.now();
        for s in &session.seconds()[self.counted..] {
            if let Some(w) = s.sample.watts {
                self.watts_sum += u64::from(w);
                self.watts_n += 1;
            }
        }
        self.counted = session.seconds().len();
        let pedal_watts = self.fresh(Source::Pedals, now, |l| l.watts);
        let trainer_watts = self.fresh(Source::Trainer, now, |l| l.watts);
        let (watts, power_from) = match (pedal_watts, trainer_watts) {
            (Some(w), _) => (Some(w), Some(Source::Pedals)),
            (None, Some(w)) => (Some(w), Some(Source::Trainer)),
            _ => (None, None),
        };
        let cadence_rpm = self.fresh(Source::Pedals, now, |l| l.cadence).or(self.fresh(Source::Trainer, now, |l| l.cadence));
        let lap_began = session.lap_began();
        let slept = self.began.map_or(0.0, |(wall, at)| (clock.wall_unix() - wall) - now.saturating_sub(at).as_secs_f64());
        TrainingLive {
            phase,
            ride_s: session.seconds().len() as u32,
            lap_s: session.seconds().iter().rev().take_while(|s| s.sample.t >= lap_began).count() as u32,
            lap: session.laps() as u32,
            watts,
            power_from,
            trainer_watts,
            cadence_rpm,
            heart_rate: self.fresh(Source::Strap, now, |l| l.heart_rate),
            speed_kmh: self.fresh(Source::Trainer, now, |l| l.speed),
            distance_m: session.seconds().last().and_then(|s| s.sample.distance_m).unwrap_or(0.0),
            avg_watts: (self.watts_n > 0).then(|| (self.watts_sum as f64 / self.watts_n as f64).round() as u16),
            profile: session.identified_profile().map(|p| p.name.to_string()),
            links: self.links.clone(),
            warnings: self.warnings.clone(),
            slept_s: if slept > SLEEP_SLACK { slept as u32 } else { 0 },
            release_unconfirmed: self.release_unconfirmed,
            erg: match phase {
                TrainingPhase::Pairing if session.target_power() == Some(false) => None,
                TrainingPhase::Pairing => self.erg.ready(),
                // Letting go at the end is the runner's, not ERG's.
                TrainingPhase::Finishing => None,
                _ => self.erg.view(),
            },
            // Said before the ride too, while another trainer can be picked.
            erg_unsupported: session.target_power() == Some(false),
            // ERG says nothing before the ride; the end is the runner's.
            power_match: self.erg.matching().filter(|_| phase != TrainingPhase::Finishing),
            workout: match phase {
                TrainingPhase::Finishing => None,
                _ => self.erg.workout_at(session.seconds().len() as u32),
            },
        }
    }
}

fn phase_of(session: &Session) -> TrainingPhase {
    match (session.riding(), session.paused()) {
        (false, _) => TrainingPhase::Pairing,
        (true, true) => TrainingPhase::Paused,
        (true, false) => TrainingPhase::Riding,
    }
}

/// Lets the trainer go before its link is dropped, within the profile's
/// release budget: asked until it confirms, again after an answer without
/// confirmation, and — a trainer lost for a moment keeps its brake — at
/// once when it is back. A trainer never heard, or gone for good, is not
/// waited for.
fn let_go(session: &mut Session, view: &mut View, clock: &dyn Clock, host: &mut dyn Host) {
    if matches!(view.state_of(Source::Trainer), None | Some(LinkState::Searching | LinkState::Failed)) {
        return;
    }
    let until = clock.now() + session.profile().release_budget;
    let mut asked: Option<ControlId> = None;
    let mut again_at = clock.now();
    while clock.now() < until {
        if asked.is_none() && clock.now() >= again_at && view.connected(Source::Trainer) {
            asked = Some(session.control(Control::Release));
        }
        let tick = session.tick(clock.now());
        let mut over = false;
        for (role, event) in &tick.events {
            match event {
                LinkEvent::Control { id, outcome, .. } if Some(*id) == asked => {
                    asked = None;
                    // Refused while connected: the trainer would not take
                    // it (BLE "not permitted": another app holds it). Late
                    // or unconfirmed answers are usual on some trainers.
                    view.release_unconfirmed = *outcome == ControlOutcome::Refused;
                    match outcome {
                        ControlOutcome::Acknowledged => over = true,
                        _ => again_at = clock.now() + RELEASE_RETRY,
                    }
                }
                LinkEvent::Back { .. } if *role == Source::Trainer => again_at = clock.now(),
                LinkEvent::Failed { .. } if *role == Source::Trainer => over = true,
                _ => {}
            }
        }
        view.absorb(tick);
        let live = view.live(session, TrainingPhase::Finishing, clock);
        host.show(&live);
        if over {
            return;
        }
        clock.sleep(TICK);
    }
}

/// Runs the session until it is finished, discarded or left, or the app
/// lets go of its orders (it quit: a ride's journal stays for next time).
pub fn run(mut session: Session, orders: Receiver<Order>, clock: &dyn Clock, host: &mut dyn Host) {
    let erg = ErgHandle::default();
    session.add_controller(Box::new(Erg::new(erg.clone())));
    let mut view = View::new(&session.roles(), erg.clone());
    loop {
        loop {
            let order = match orders.try_recv() {
                Ok(order) => order,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    host.done();
                    return;
                }
            };
            let now = clock.now();
            match order {
                Order::Begin(reply) => {
                    let result = if session.riding() {
                        Ok(())
                    } else {
                        let start_unix = clock.wall_unix().floor() as i64;
                        host.begin(start_unix, SubSport::IndoorCycling).map(|journal| {
                            let warnings = session.begin(clock.now(), start_unix, SubSport::IndoorCycling, journal);
                            view.began = Some((clock.wall_unix(), clock.now()));
                            view.warn(warnings);
                        })
                    };
                    let _ = reply.send(result);
                }
                Order::Lap => view.warn(session.lap(now)),
                Order::SetTarget(watts) => erg.set_target(watts),
                Order::SetWorkout(plan) => erg.set_workout(plan),
                Order::Pause => view.warn(session.pause(now)),
                Order::Resume => view.warn(session.resume(now)),
                Order::Leave(reply) => {
                    if session.riding() {
                        let _ = reply.send(Err(riding()));
                        continue;
                    }
                    drop(session);
                    host.done();
                    let _ = reply.send(Ok(()));
                    return;
                }
                // The answer goes last: whoever waits for it finds the
                // session over and the vault free.
                Order::Finish(reply) => {
                    let (ride, journal, warnings) = session.end(now);
                    view.warn(warnings);
                    let_go(&mut session, &mut view, clock, host);
                    drop(session);
                    let result = host.finish(&ride, journal.as_deref());
                    host.done();
                    let _ = reply.send(result);
                    return;
                }
                Order::Discard(reply) => {
                    let (_, journal, _) = session.end(now);
                    let_go(&mut session, &mut view, clock, host);
                    drop(session);
                    let result = host.discard(journal.as_deref());
                    host.done();
                    let _ = reply.send(result);
                    return;
                }
            }
        }
        let tick = session.tick(clock.now());
        view.absorb(tick);
        let live = view.live(&session, phase_of(&session), clock);
        host.show(&live);
        clock.sleep(TICK);
    }
}

/// Time for tests: every sleep moves it on at once, and it can be stopped
/// at a moment so a test can act there.
#[cfg(test)]
pub mod test_clock {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    /// Session time in milliseconds; the wall clock is `START` plus it and
    /// any sleep the test adds.
    #[derive(Clone, Default)]
    pub struct StepClock {
        pub ms: Arc<AtomicU64>,
        /// Seconds the "Mac slept": the wall clock moves, session time not.
        pub slept: Arc<AtomicU64>,
        /// Sleeps end here until the test moves it (u64::MAX: never stop).
        pub stop_at: Arc<AtomicU64>,
    }

    pub const START: f64 = 1_794_420_000.0;

    impl StepClock {
        pub fn new() -> StepClock {
            let c = StepClock::default();
            c.stop_at.store(u64::MAX, Ordering::SeqCst);
            c
        }
    }

    impl Clock for StepClock {
        fn now(&self) -> Duration {
            Duration::from_millis(self.ms.load(Ordering::SeqCst))
        }
        fn wall_unix(&self) -> f64 {
            START + self.ms.load(Ordering::SeqCst) as f64 / 1000.0 + self.slept.load(Ordering::SeqCst) as f64
        }
        fn sleep(&self, d: Duration) {
            let next = self.ms.load(Ordering::SeqCst) + d.as_millis() as u64;
            if next > self.stop_at.load(Ordering::SeqCst) {
                // Held: let the other thread act, without moving time.
                std::thread::sleep(Duration::from_micros(200));
                return;
            }
            self.ms.store(next, Ordering::SeqCst);
            // Let the other thread see each step.
            std::thread::yield_now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_clock::{StepClock, START};
    use super::*;
    use crate::models::training::FinishKind;
    use crate::test_support::ScratchDir;
    use crate::trainer::fake::{FakeConfig, FakeTrainer};
    use crate::trainer::journal::{self, fixed};
    use crate::trainer::link::{ControlId, Identity, Reading, SensorLink, TrainerLink, Transport};
    use crate::trainer::profile::TrainerProfile;
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc::channel;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Seen {
        last: Option<TrainingLive>,
        /// How many times the screen was shown something.
        shows: u64,
        /// Each phase once, in the order they came.
        phases: Vec<TrainingPhase>,
        finished: Vec<Ride>,
        calls: Vec<&'static str>,
    }

    /// A host that writes the journal into a scratch vault and records
    /// what it was asked.
    struct Recording {
        vault: PathBuf,
        seen: Arc<Mutex<Seen>>,
        refuse_begin: bool,
    }

    impl Host for Recording {
        fn begin(&mut self, start_unix: i64, sub_sport: SubSport) -> Result<Option<Journal>, CommandError> {
            self.seen.lock().unwrap().calls.push("begin");
            if self.refuse_begin {
                return Err(CommandError::coded("vault.busy", "Another vault operation is already in progress"));
            }
            Ok(Some(Journal::create(&self.vault, start_unix, sub_sport, fixed(None))?))
        }
        fn finish(&mut self, ride: &Ride, journal: Option<&Path>) -> Result<TrainingFinish, CommandError> {
            let mut seen = self.seen.lock().unwrap();
            seen.calls.push("finish");
            seen.finished.push(ride.clone());
            if let Some(p) = journal {
                journal::discard(p)?;
            }
            Ok(TrainingFinish { kind: FinishKind::Imported, activity_id: Some("new".into()) })
        }
        fn discard(&mut self, journal: Option<&Path>) -> Result<(), CommandError> {
            self.seen.lock().unwrap().calls.push("discard");
            if let Some(p) = journal {
                journal::discard(p)?;
            }
            Ok(())
        }
        fn show(&mut self, live: &TrainingLive) {
            let mut seen = self.seen.lock().unwrap();
            if seen.phases.last() != Some(&live.phase) {
                seen.phases.push(live.phase);
            }
            seen.last = Some(live.clone());
            seen.shows += 1;
        }
        fn done(&mut self) {
            self.seen.lock().unwrap().calls.push("done");
        }
    }

    /// A session running on its thread with time under the test's hand.
    struct Running {
        clock: StepClock,
        orders: Option<Sender<Order>>,
        seen: Arc<Mutex<Seen>>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Running {
        fn start(session: Session, vault: &Path, refuse_begin: bool) -> Running {
            let clock = StepClock::new();
            clock.stop_at.store(0, Ordering::SeqCst);
            let (orders, rx) = channel();
            let seen = Arc::new(Mutex::new(Seen::default()));
            let mut host = Recording { vault: vault.to_path_buf(), seen: seen.clone(), refuse_begin };
            let c = clock.clone();
            let thread = std::thread::spawn(move || run(session, rx, &c, &mut host));
            Running { clock, orders: Some(orders), seen, thread: Some(thread) }
        }

        /// Lets session time run to `ms` and waits until it is there and
        /// shown.
        fn run_to(&self, ms: u64) {
            self.clock.stop_at.store(ms, Ordering::SeqCst);
            while self.clock.ms.load(Ordering::SeqCst) + TICK.as_millis() as u64 <= ms {
                std::thread::yield_now();
            }
            // Two more rounds, so what that moment brought (and the orders
            // sent before) is shown.
            let shows = || self.seen.lock().unwrap().shows;
            let before = shows();
            while shows() < before + 2 {
                std::thread::yield_now();
            }
        }

        fn free(&self) {
            self.clock.stop_at.store(u64::MAX, Ordering::SeqCst);
        }

        fn last(&self) -> TrainingLive {
            self.seen.lock().unwrap().last.clone().expect("shown")
        }

        fn tell(&self, order: Order) {
            self.orders.as_ref().unwrap().send(order).unwrap();
        }

        fn ask<T>(&self, order: impl FnOnce(Reply<T>) -> Order) -> Result<T, CommandError> {
            let (tx, rx) = channel();
            self.tell(order(tx));
            rx.recv().unwrap()
        }

        /// Waits for the thread to end; returns what it did.
        fn join(mut self) -> Seen {
            self.thread.take().unwrap().join().unwrap();
            std::mem::take(&mut *self.seen.lock().unwrap())
        }
    }

    fn avanti() -> Box<FakeTrainer> {
        Box::new(FakeTrainer::new(FakeConfig::elite_avanti(80)))
    }

    /// Pedals at 200 W and 90 rpm once a second; silent from 100 to 110 s.
    struct Pedals(u64);
    impl SensorLink for Pedals {
        fn identity(&self) -> Option<&Identity> {
            None
        }
        fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
            let mut out = Vec::new();
            if self.0 == 0 {
                out.push(LinkEvent::Connected { at: Duration::ZERO });
                out.push(LinkEvent::Identified {
                    at: Duration::ZERO,
                    identity: Identity { name: Some("ASSIOMA".into()), ..Identity::new(Transport::Ble) },
                });
            }
            while self.0 * 1000 <= now.as_millis() as u64 {
                if !(100..110).contains(&self.0) {
                    let at = Duration::from_secs(self.0);
                    out.push(LinkEvent::Reading(Reading { at, watts: Some(200), cadence_rpm: Some(90), ..Reading::default() }));
                }
                self.0 += 1;
            }
            out
        }
    }

    /// A strap that cannot get the radio.
    struct Denied;
    impl SensorLink for Denied {
        fn identity(&self) -> Option<&Identity> {
            None
        }
        fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
            vec![LinkEvent::Failed { at: now, reason: LinkFailure::PermissionDenied }]
        }
    }

    #[test]
    fn a_ride_from_pairing_to_the_import() {
        let vault = ScratchDir::new("runner_ride");
        assert!(Denied.identity().is_none() && Pedals(0).identity().is_none());
        let session = Session::pairing(avanti(), Some(Box::new(Pedals(0))), Some(Box::new(Denied)));
        let r = Running::start(session, &vault, false);

        r.run_to(10_000);
        let pairing = r.last();
        assert_eq!(pairing.phase, TrainingPhase::Pairing);
        assert_eq!((pairing.watts, pairing.power_from), (Some(200), Some(Source::Pedals)), "readings show while pairing");
        assert!(pairing.trainer_watts.is_some_and(|w| w > 150), "{pairing:?}");
        assert_eq!(pairing.cadence_rpm, Some(90));
        assert!(pairing.speed_kmh.is_some());
        assert_eq!((pairing.ride_s, pairing.lap, pairing.avg_watts), (0, 1, None));
        assert_eq!(pairing.profile.as_deref(), Some("Elite Avanti"));
        let states: Vec<(Source, LinkState)> = pairing.links.iter().map(|l| (l.role, l.state)).collect();
        assert_eq!(
            states,
            vec![(Source::Trainer, LinkState::Connected), (Source::Pedals, LinkState::Connected), (Source::Strap, LinkState::Failed)]
        );
        assert_eq!(pairing.links[1].name.as_deref(), Some("ASSIOMA"));
        assert_eq!(pairing.links[2].problem, Some(LinkProblem::PermissionDenied));
        assert_eq!(pairing.heart_rate, None);

        r.ask(Order::Begin).unwrap();
        r.ask(Order::Begin).unwrap(); // again: the ride runs already
        assert_eq!(r.ask(Order::Leave).unwrap_err().code, Some("training.riding"));
        r.run_to(70_000);
        let riding = r.last();
        assert_eq!((riding.phase, riding.ride_s), (TrainingPhase::Riding, 60));
        assert_eq!(riding.avg_watts, Some(200));
        assert!(riding.distance_m > 400.0, "{}", riding.distance_m);

        r.run_to(105_000);
        assert_eq!(r.last().power_from, Some(Source::Trainer), "the pedals went quiet at 100 s");

        r.run_to(120_000);
        r.tell(Order::Lap);
        r.run_to(150_000);
        let lap = r.last();
        assert_eq!((lap.lap, lap.lap_s, lap.ride_s), (2, 30, 140));

        r.tell(Order::Pause);
        r.run_to(200_000);
        let paused = r.last();
        assert_eq!((paused.phase, paused.ride_s), (TrainingPhase::Paused, 141), "second 140 was ridden to its end");
        r.tell(Order::Resume);
        r.clock.slept.store(30, Ordering::SeqCst); // the lid was shut
        r.run_to(260_000);
        let resumed = r.last();
        assert_eq!((resumed.phase, resumed.ride_s), (TrainingPhase::Riding, 141 + 60));
        assert_eq!(resumed.slept_s, 30);

        r.free();
        let finished = r.ask(Order::Finish).unwrap();
        assert_eq!(finished, TrainingFinish { kind: FinishKind::Imported, activity_id: Some("new".into()) });
        let seen = r.join();
        assert_eq!(seen.calls, vec!["begin", "finish", "done"]);
        assert!(seen.last.as_ref().is_some_and(|l| !l.release_unconfirmed), "the Avanti confirmed letting go");
        assert_eq!(
            seen.phases,
            vec![TrainingPhase::Pairing, TrainingPhase::Riding, TrainingPhase::Paused, TrainingPhase::Riding, TrainingPhase::Finishing]
        );
        let ride = &seen.finished[0];
        assert_eq!(ride.pauses, vec![(141, 190)]);
        assert_eq!(ride.start_unix, (START + 10.0) as i64);
        assert_eq!(ride.lap_starts, vec![110]);
        assert!(ride.samples.len() >= 201);
        assert!(journal::pending(&vault).is_empty());
    }

    #[test]
    fn leaving_before_the_ride_drops_the_links() {
        let vault = ScratchDir::new("runner_leave");
        let r = Running::start(Session::pairing(avanti(), None, None), &vault, false);
        r.run_to(3_000);
        r.ask(Order::Leave).unwrap();
        assert_eq!(r.join().calls, vec!["done"]);
    }

    #[test]
    fn a_refused_begin_leaves_the_session_pairing() {
        let vault = ScratchDir::new("runner_refused");
        let r = Running::start(Session::pairing(avanti(), None, None), &vault, true);
        r.run_to(3_000);
        assert_eq!(r.ask(Order::Begin).unwrap_err().code, Some("vault.busy"));
        r.run_to(6_000);
        assert_eq!(r.last().phase, TrainingPhase::Pairing);
        r.free();
        r.ask(Order::Discard).unwrap();
        let seen = r.join();
        assert_eq!(seen.calls, vec!["begin", "discard", "done"]);
    }

    /// A trainer that said it cannot hold watts: the screen says so while
    /// pairing, when another can still be picked (#292).
    #[test]
    fn a_trainer_without_erg_is_said_before_the_ride() {
        let vault = ScratchDir::new("runner_no_erg");
        let mut cfg = FakeConfig::elite_avanti(80);
        cfg.identity.target_power = Some(false);
        cfg.identify_after = Duration::ZERO;
        let r = Running::start(Session::pairing(Box::new(FakeTrainer::new(cfg)), None, None), &vault, false);
        r.run_to(1_000);
        assert!(r.last().erg_unsupported, "said with no target set");
        r.tell(Order::SetTarget(Some(160)));
        r.run_to(2_000);
        let live = r.last();
        assert!(live.erg_unsupported);
        assert_eq!(live.erg, None, "no target is held with it");
        r.ask(Order::Leave).unwrap();
        r.join();
    }

    #[test]
    fn the_screen_sets_the_erg_target_and_sees_what_erg_does() {
        use crate::models::training::{ErgLive, ErgState, MatchLive, MatchState};
        let vault = ScratchDir::new("runner_erg");
        let mut trainer = avanti();
        let pedals = trainer.pedals(Vec::new(), Vec::new());
        let r = Running::start(Session::pairing(trainer, Some(Box::new(pedals)), None), &vault, false);
        r.run_to(3_000);
        r.tell(Order::SetTarget(Some(160))); // set while pairing: held from the start
        r.run_to(4_000);
        assert_eq!(r.last().erg, Some(ErgLive { target_w: 160, state: ErgState::Ready }), "held from the start of the ride");
        assert_eq!(r.last().power_match, None);
        r.ask(Order::Begin).unwrap();
        r.run_to(40_000);
        let riding = r.last();
        assert_eq!(riding.erg, Some(ErgLive { target_w: 160, state: ErgState::Holding }));
        assert_eq!(riding.power_match, Some(MatchLive { state: MatchState::Matching, trainer_target_w: 160 }), "true pedals: 0 W off");
        assert!(riding.trainer_watts.is_some_and(|w| w.abs_diff(160) <= 16), "{riding:?}");
        r.tell(Order::Pause);
        r.run_to(45_000);
        assert_eq!(r.last().erg.map(|e| e.state), Some(ErgState::Paused));
        r.tell(Order::Resume);
        r.tell(Order::SetTarget(None));
        r.run_to(50_000);
        assert_eq!(r.last().erg, None, "riding free");
        r.tell(Order::SetTarget(Some(200)));
        r.run_to(55_000);
        assert_eq!(r.last().workout, None);
        let steps = vec![crate::trainer::erg::Step { secs: 600, from: 180, to: 180, cadence: None }];
        r.tell(Order::SetWorkout(Workout { name: "Steady".into(), steps, cues: Vec::new() }));
        r.run_to(60_000);
        let following = r.last();
        assert_eq!(following.erg.map(|e| e.target_w), Some(180));
        assert_eq!(following.workout.map(|w| (w.name, w.step, w.done)), Some(("Steady".to_string(), 0, false)));
        r.free();
        r.ask(Order::Finish).unwrap();
        let seen = r.join();
        let last = seen.last.expect("shown");
        assert_eq!((last.erg, last.power_match, last.workout), (None, None, None), "letting go at the end is not ERG's");
    }

    #[test]
    fn a_discarded_ride_lets_the_trainer_go_and_leaves_no_journal() {
        let vault = ScratchDir::new("runner_discard");
        let r = Running::start(Session::pairing(avanti(), None, None), &vault, false);
        r.run_to(3_000);
        r.ask(Order::Begin).unwrap();
        r.run_to(20_000);
        assert_eq!(journal::pending(&vault).len(), 1);
        r.free();
        r.ask(Order::Discard).unwrap();
        let seen = r.join();
        assert_eq!(seen.calls, vec!["begin", "discard", "done"]);
        assert_eq!(seen.phases.last(), Some(&TrainingPhase::Finishing));
        assert!(journal::pending(&vault).is_empty());
    }

    #[test]
    fn an_app_that_quits_leaves_the_journal_for_next_time() {
        let vault = ScratchDir::new("runner_quit");
        let mut r = Running::start(Session::pairing(avanti(), None, None), &vault, false);
        r.run_to(3_000);
        r.ask(Order::Begin).unwrap();
        r.run_to(10_000);
        r.orders = None; // the app let go of its orders
        r.free();
        assert_eq!(r.join().calls, vec!["begin", "done"]);
        assert_eq!(journal::pending(&vault).len(), 1);
    }

    /// A trainer that answers the first release as `first`, the next ones
    /// as `second` (None: the link fails), and records how often it was
    /// asked.
    struct Stubborn {
        first: ControlOutcome,
        second: Option<ControlOutcome>,
        asked: Arc<Mutex<u32>>,
        owed: Vec<ControlId>,
        connected: bool,
    }
    impl SensorLink for Stubborn {
        fn identity(&self) -> Option<&Identity> {
            None
        }
        fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
            let mut out = Vec::new();
            if !self.connected {
                self.connected = true;
                out.push(LinkEvent::Connected { at: now });
            }
            for id in self.owed.drain(..) {
                let n = *self.asked.lock().unwrap();
                match (n, self.second) {
                    (1, _) => out.push(LinkEvent::Control { id, at: now, control: Control::Release, outcome: self.first }),
                    (_, Some(outcome)) => out.push(LinkEvent::Control { id, at: now, control: Control::Release, outcome }),
                    (_, None) => out.push(LinkEvent::Failed { at: now, reason: LinkFailure::AdapterGone }),
                }
            }
            out
        }
    }
    impl TrainerLink for Stubborn {
        fn use_profile(&mut self, _: &'static TrainerProfile) {}
        fn send(&mut self, _: Control) -> ControlId {
            let mut asked = self.asked.lock().unwrap();
            *asked += 1;
            let id = ControlId(u64::from(*asked));
            self.owed.push(id);
            id
        }
    }

    /// How many releases went out, and whether the screen ended up saying
    /// the trainer would not take them.
    fn released(first: ControlOutcome, second: Option<ControlOutcome>) -> (u32, bool) {
        let vault = ScratchDir::new("runner_release");
        let asked = Arc::new(Mutex::new(0));
        let mut trainer = Stubborn { first, second, asked: asked.clone(), owed: Vec::new(), connected: false };
        assert!(trainer.identity().is_none());
        trainer.use_profile(&crate::trainer::profile::GENERIC);
        let r = Running::start(Session::pairing(Box::new(trainer), None, None), &vault, false);
        r.run_to(2_000);
        r.free();
        assert_eq!(r.ask(Order::Finish).unwrap().kind, FinishKind::Imported);
        let seen = r.join();
        let refused = seen.last.expect("shown").release_unconfirmed;
        let n = *asked.lock().unwrap();
        (n, refused)
    }

    #[test]
    fn the_release_is_asked_again_until_the_trainer_confirms_or_goes() {
        use ControlOutcome::{Acknowledged, NotAcknowledged, Refused, TimedOut};
        // A time-out is usual (a slow answer): nothing said.
        assert_eq!(released(TimedOut, Some(Acknowledged)), (2, false), "timed out, then confirmed");
        // Refused, then taken: the warning goes with the confirmation.
        assert_eq!(released(Refused, Some(Acknowledged)), (2, false));
        // Refused: asked again within the budget, and the screen says why.
        let (n, refused) = released(TimedOut, Some(Refused));
        assert!(n >= 10 && refused, "{n} {refused}");
        assert_eq!(released(TimedOut, None), (2, false), "the link failed while letting go");
        // Never confirmed: asked once a second until the budget is spent.
        let generic = crate::trainer::profile::GENERIC.release_budget.as_secs() as u32;
        let (n, refused) = released(TimedOut, Some(NotAcknowledged));
        assert!(n >= generic / 2 && !refused, "{n} {refused}");
    }

    /// Finished during a drop: the trainer keeps its brake through a short
    /// one, so the release waits for it to come back.
    #[test]
    fn a_ride_finished_in_a_drop_lets_go_once_the_trainer_is_back() {
        let vault = ScratchDir::new("runner_drop_finish");
        let trainer = FakeTrainer::new(FakeConfig::elite_avanti(85).with_drop(Duration::from_secs(19), Duration::from_secs(6)));
        let r = Running::start(Session::pairing(Box::new(trainer), None, None), &vault, false);
        r.run_to(3_000);
        r.ask(Order::Begin).unwrap();
        r.run_to(20_500);
        assert_eq!(r.last().links[0].state, LinkState::Lost);
        r.free();
        let clock = r.clock.clone();
        assert_eq!(r.ask(Order::Finish).unwrap().kind, FinishKind::Imported);
        let seen = r.join();
        let last = seen.last.expect("shown");
        assert_eq!(last.links[0].state, LinkState::Connected, "it waited for the trainer to come back");
        let ended = clock.ms.load(Ordering::SeqCst);
        assert!((25_000..36_000).contains(&ended), "back at 25 s, let go then, not the whole budget: {ended}");
    }

    /// A trainer never heard is not asked to let go.
    #[test]
    fn a_trainer_never_heard_is_not_asked() {
        let vault = ScratchDir::new("runner_silent");
        struct Silent(Arc<Mutex<u32>>);
        impl SensorLink for Silent {
            fn identity(&self) -> Option<&Identity> {
                None
            }
            fn poll(&mut self, _: Duration) -> Vec<LinkEvent> {
                Vec::new()
            }
        }
        impl TrainerLink for Silent {
            fn use_profile(&mut self, _: &'static TrainerProfile) {}
            fn send(&mut self, _: Control) -> ControlId {
                *self.0.lock().unwrap() += 1;
                ControlId(0)
            }
        }
        let asked = Arc::new(Mutex::new(0));
        let mut silent = Silent(asked.clone());
        assert!(silent.identity().is_none());
        silent.use_profile(&crate::trainer::profile::GENERIC);
        silent.send(Control::Release); // the count starts at 1
        let r = Running::start(Session::pairing(Box::new(silent), None, None), &vault, false);
        r.run_to(1_000);
        assert_eq!(r.last().links[0].state, LinkState::Searching);
        r.free();
        r.ask(Order::Finish).unwrap();
        r.join();
        assert_eq!(*asked.lock().unwrap(), 1, "the ride's end asked nothing more");
    }

    /// A strap beating once a second.
    struct Strap(u64);
    impl SensorLink for Strap {
        fn identity(&self) -> Option<&Identity> {
            None
        }
        fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
            let mut out = Vec::new();
            if self.0 == 0 {
                out.push(LinkEvent::Connected { at: Duration::ZERO });
            }
            while self.0 * 1000 <= now.as_millis() as u64 {
                let at = Duration::from_secs(self.0);
                out.push(LinkEvent::Reading(Reading { at, heart_rate: Some(131), ..Reading::default() }));
                self.0 += 1;
            }
            out
        }
    }

    /// A trainer that drops for 15 s shows "reconnecting", then its watts
    /// again; the ride goes on through it.
    #[test]
    fn a_trainer_that_drops_and_comes_back() {
        let vault = ScratchDir::new("runner_drop");
        assert!(Strap(0).identity().is_none());
        let trainer = FakeTrainer::new(FakeConfig::elite_avanti(80).with_drop(Duration::from_secs(20), Duration::from_secs(15)));
        let r = Running::start(Session::pairing(Box::new(trainer), None, Some(Box::new(Strap(0)))), &vault, false);
        r.run_to(5_000);
        r.ask(Order::Begin).unwrap();
        r.run_to(28_000);
        let lost = r.last();
        assert_eq!(lost.links[0].state, LinkState::Lost);
        assert_eq!((lost.watts, lost.speed_kmh), (None, None), "nothing stale is shown");
        assert_eq!(lost.heart_rate, Some(131), "the strap goes on");
        r.run_to(50_000);
        let back = r.last();
        assert_eq!(back.links[0].state, LinkState::Connected);
        assert!(back.watts.is_some_and(|w| w > 150), "{back:?}");
        assert_eq!(back.ride_s, 45, "every second counted through the drop");
        r.free();
        r.ask(Order::Finish).unwrap();
        let seen = r.join();
        let ride = &seen.finished[0];
        let without_power = ride.samples.iter().filter(|s| s.watts.is_none()).count();
        assert!((10..=16).contains(&without_power), "the drop's seconds have no power: {without_power}");
        // The beat at 5.0 s went with the last pairing tick: second 0 has
        // none, every later one has.
        assert!(ride.samples[1..].iter().all(|s| s.heart_rate == Some(131)));
    }

    #[test]
    fn warnings_are_kept_once_and_the_newest_few() {
        let mut v = View::new(&[Source::Trainer], ErgHandle::default());
        v.warn(vec!["disk full".into(), "disk full".into()]);
        v.warn(vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(v.warnings, vec!["a", "b", "c"]);
        v.warn(vec!["b".into()]);
        assert_eq!(v.warnings, vec!["a", "b", "c"], "a known one is not added again");
    }

    #[test]
    fn a_link_failure_is_told_by_kind() {
        assert_eq!(problem_of(&LinkFailure::RadioOff), (LinkProblem::RadioOff, None));
        assert_eq!(problem_of(&LinkFailure::AdapterGone), (LinkProblem::AdapterGone, None));
        assert_eq!(problem_of(&LinkFailure::StickGone), (LinkProblem::StickGone, None));
        assert_eq!(problem_of(&LinkFailure::StickBusy), (LinkProblem::StickBusy, None));
        assert_eq!(problem_of(&LinkFailure::Other("boom".into())), (LinkProblem::Other, Some("boom".into())));
        let clock = SystemClock::new(Instant::now());
        clock.sleep(Duration::from_millis(1));
        assert!(clock.now() >= Duration::from_millis(1) && clock.wall_unix() > 1_700_000_000.0);
    }
}
