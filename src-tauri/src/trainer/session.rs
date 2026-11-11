//! The ride as it happens. A tick polls the trainer, the pedals and the
//! strap; every reading goes into the second its own stamp (`Reading.at`)
//! falls in, counted from the ride's start (`origin`, session time — the
//! pairing before it is not part of the ride). A tick closes every second
//! that is over: merges it (noting where power and cadence came from),
//! journals it, and lets the controllers act — the ERG controller and power
//! match plug in here (stages 5 and 6). Ticks may come at any rate and
//! jitter; a late tick closes the seconds it missed, each with its own
//! readings.
//!
//! Merging: power and cadence from the pedals, held for a few seconds
//! between their messages (a CPS power meter speaks about once a
//! revolution), the trainer's otherwise; the trainer's watts are kept apart
//! for power match. Heart rate from the strap only; speed from the trainer,
//! a second of it at a time into distance.
//!
//! The journal is a side channel: a failed write is reported in the tick
//! and the ride, and the trainer's control, go on.
//!
//! `tick` syncs the journal (an fsync a second): run it off the async
//! runtime's workers.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use super::fit::{Device, DeviceKind, Radio, Ride, Sample, SubSport};
use super::journal::{Entry, Journal, Second, Source};
use super::link::{Identity, LinkEvent, Reading, SensorLink, TrainerLink, Transport};
use super::profile::profile_for;

/// The pedals' last power and cadence stand for this many seconds after
/// their last message, unless they are lost.
pub const PEDALS_HOLD: u32 = 3;

/// Something that acts on the trainer as seconds close (ERG, power match).
pub trait Controller: Send {
    /// `second` is the latest second closed, `events` everything the
    /// trainer reported since the controllers' last call (readings,
    /// control outcomes, Lost/Back).
    fn tick(&mut self, now: Duration, second: &Second, events: &[LinkEvent], trainer: &mut dyn TrainerLink);
}

/// What a tick produced.
#[derive(Debug, Default)]
pub struct Tick {
    /// The seconds closed by this tick, in order.
    pub seconds: Vec<Second>,
    /// Everything the links reported, by source, for the screen.
    pub events: Vec<(Source, LinkEvent)>,
    /// What went wrong on the side (journal writes), for the screen.
    pub warnings: Vec<String>,
}

const ROLES: [Source; 3] = [Source::Trainer, Source::Pedals, Source::Strap];

fn role_index(source: Source) -> usize {
    match source {
        Source::Trainer => 0,
        Source::Pedals => 1,
        Source::Strap => 2,
    }
}

/// FNV-1a, to give a BLE address a stable 32-bit serial (never 0).
fn serial_of(address: &str) -> u32 {
    let hash = address.bytes().fold(0x811C_9DC5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193));
    hash.max(1)
}

fn device_of(source: Source, identity: &Identity) -> Device {
    let kind = match source {
        Source::Trainer => DeviceKind::FitnessEquipment,
        Source::Pedals => DeviceKind::BikePower,
        Source::Strap => DeviceKind::HeartRate,
    };
    let radio = match identity.transport {
        Transport::Ant => Some(Radio::Ant),
        Transport::Ble => Some(Radio::Ble),
        Transport::Fake => None,
    };
    let serial = identity.ant_device_number.or_else(|| identity.ble_address.as_deref().map(serial_of));
    Device {
        kind,
        // FIT manufacturer ids are the ANT+ ones.
        manufacturer: identity.ant_manufacturer,
        name: identity.name.clone().or_else(|| identity.model.clone()),
        serial,
        radio,
    }
}

fn mean(values: impl Iterator<Item = f32>) -> Option<f32> {
    let (sum, n) = values.fold((0.0, 0u32), |(s, n), v| (s + v, n + 1));
    (n > 0).then(|| sum / n as f32)
}

pub struct Session {
    origin: Duration,
    start_unix: i64,
    sub_sport: SubSport,
    trainer: Box<dyn TrainerLink>,
    pedals: Option<Box<dyn SensorLink>>,
    strap: Option<Box<dyn SensorLink>>,
    journal: Option<Journal>,
    controllers: Vec<Box<dyn Controller>>,
    seconds: Vec<Second>,
    lap_starts: Vec<usize>,
    devices: [Option<Device>; 3],
    profile: Option<&'static str>,
    /// Readings by the second they fall in, until it closes.
    buckets: BTreeMap<u32, Vec<(Source, Reading)>>,
    /// The first second not closed yet.
    next_second: u32,
    /// Trainer events since the controllers' last call.
    for_controllers: Vec<LinkEvent>,
    /// The pedals' last second with power: (second, watts, cadence).
    last_pedals: Option<(u32, u16, Option<u8>)>,
    pedals_lost: bool,
    /// The strap's last second with a heart rate: (second, bpm).
    last_heart_rate: Option<(u32, u8)>,
    strap_lost: bool,
    /// Readings that came for a second already closed.
    late: u64,
    distance_m: f32,
}

impl Session {
    /// A ride starting at session time `origin`, wall clock `start_unix`
    /// (UTC). The journal, when given, has its header already.
    pub fn new(
        origin: Duration,
        start_unix: i64,
        sub_sport: SubSport,
        trainer: Box<dyn TrainerLink>,
        pedals: Option<Box<dyn SensorLink>>,
        strap: Option<Box<dyn SensorLink>>,
        journal: Option<Journal>,
    ) -> Session {
        Session {
            origin,
            start_unix,
            sub_sport,
            trainer,
            pedals,
            strap,
            journal,
            controllers: Vec::new(),
            seconds: Vec::new(),
            lap_starts: Vec::new(),
            devices: [None, None, None],
            profile: None,
            buckets: BTreeMap::new(),
            next_second: 0,
            for_controllers: Vec::new(),
            last_pedals: None,
            pedals_lost: false,
            last_heart_rate: None,
            strap_lost: false,
            late: 0,
            distance_m: 0.0,
        }
    }

    pub fn add_controller(&mut self, controller: Box<dyn Controller>) {
        self.controllers.push(controller);
    }

    pub fn seconds(&self) -> &[Second] {
        &self.seconds
    }

    /// Readings that came for a second already closed (should stay 0: the
    /// links stamp on arrival).
    pub fn late_readings(&self) -> u64 {
        self.late
    }

    fn journal(&mut self, entry: &Entry, warnings: &mut Vec<String>) {
        if let Some(j) = self.journal.as_mut() {
            if let Err(e) = j.append(entry) {
                warnings.push(e);
            }
        }
    }

    /// Polls every link up to `now` (session time) and closes every second
    /// that is over.
    pub fn tick(&mut self, now: Duration) -> Tick {
        let mut out = self.poll(now);
        let elapsed = now.saturating_sub(self.origin);
        self.close_until(elapsed.as_secs() as u32, &mut out);
        if let Some(last) = out.seconds.last().copied() {
            let events = std::mem::take(&mut self.for_controllers);
            for c in self.controllers.iter_mut() {
                c.tick(now, &last, &events, self.trainer.as_mut());
            }
        }
        out
    }

    fn poll(&mut self, now: Duration) -> Tick {
        let mut out = Tick::default();
        for event in self.trainer.poll(now) {
            self.for_controllers.push(event.clone());
            out.events.push((Source::Trainer, event));
        }
        if let Some(p) = self.pedals.as_mut() {
            out.events.extend(p.poll(now).into_iter().map(|e| (Source::Pedals, e)));
        }
        if let Some(s) = self.strap.as_mut() {
            out.events.extend(s.poll(now).into_iter().map(|e| (Source::Strap, e)));
        }
        let events = std::mem::take(&mut out.events);
        for (source, event) in &events {
            match event {
                LinkEvent::Reading(r) => {
                    let Some(since) = r.at.checked_sub(self.origin) else { continue }; // before the ride
                    let s = since.as_secs() as u32;
                    if s < self.next_second {
                        self.late += 1;
                    } else {
                        self.buckets.entry(s).or_default().push((*source, r.clone()));
                    }
                }
                LinkEvent::Identified { identity, .. } => self.identified(*source, identity, &mut out.warnings),
                LinkEvent::Lost { .. } => self.set_lost(*source, true),
                LinkEvent::Back { .. } | LinkEvent::Connected { .. } => self.set_lost(*source, false),
                _ => {}
            }
        }
        out.events = events;
        out
    }

    /// A lost sensor's last values are not held.
    fn set_lost(&mut self, source: Source, lost: bool) {
        match source {
            Source::Pedals => self.pedals_lost = lost,
            Source::Strap => self.strap_lost = lost,
            Source::Trainer => {}
        }
    }

    fn identified(&mut self, source: Source, identity: &Identity, warnings: &mut Vec<String>) {
        if source == Source::Trainer {
            let profile = profile_for(identity);
            if self.profile != Some(profile.key) {
                self.profile = Some(profile.key);
                self.trainer.use_profile(profile);
            }
        }
        let device = device_of(source, identity);
        let slot = &mut self.devices[role_index(source)];
        if slot.as_ref() != Some(&device) {
            *slot = Some(device.clone());
            self.journal(&Entry::Device(device), warnings);
        }
    }

    /// Closes every second before `upto`.
    fn close_until(&mut self, upto: u32, out: &mut Tick) {
        while self.next_second < upto {
            let second = self.merge(self.next_second);
            self.journal(&Entry::Second(second), &mut out.warnings);
            self.seconds.push(second);
            out.seconds.push(second);
            self.next_second += 1;
        }
    }

    /// Second `s` from the readings that fell in it.
    fn merge(&mut self, s: u32) -> Second {
        let readings = self.buckets.remove(&s).unwrap_or_default();
        let from = |source: Source| readings.iter().filter(move |(src, _)| *src == source).map(|(_, r)| r);
        let watts = |source: Source| mean(from(source).filter_map(|r| r.watts).map(f32::from)).map(|w| w.round() as u16);
        let cadence = |source: Source| from(source).filter_map(|r| r.cadence_rpm).next_back();

        let trainer_watts = watts(Source::Trainer);
        let pedal_watts = watts(Source::Pedals);
        let pedal_cadence = cadence(Source::Pedals);
        if let Some(w) = pedal_watts {
            self.last_pedals = Some((s, w, pedal_cadence));
        }
        let held = self
            .last_pedals
            .filter(|&(at, _, _)| !self.pedals_lost && s.saturating_sub(at) <= PEDALS_HOLD);

        let (watts, power_from) = match (pedal_watts.or(held.map(|h| h.1)), trainer_watts) {
            (Some(w), _) => (Some(w), Some(Source::Pedals)),
            (None, Some(w)) => (Some(w), Some(Source::Trainer)),
            _ => (None, None),
        };
        let (cadence_rpm, cadence_from) = match (pedal_cadence.or(held.and_then(|h| h.2)), cadence(Source::Trainer)) {
            (Some(c), _) => (Some(c), Some(Source::Pedals)),
            (None, Some(c)) => (Some(c), Some(Source::Trainer)),
            _ => (None, None),
        };
        // A strap sends about once a beat: held like the pedals.
        let strap_hr = from(Source::Strap).filter_map(|r| r.heart_rate).next_back();
        if let Some(bpm) = strap_hr {
            self.last_heart_rate = Some((s, bpm));
        }
        let heart_rate = strap_hr.or_else(|| {
            self.last_heart_rate
                .filter(|&(at, _)| !self.strap_lost && s.saturating_sub(at) <= PEDALS_HOLD)
                .map(|(_, bpm)| bpm)
        });
        let speed_mps = mean(from(Source::Trainer).filter_map(|r| r.speed_kmh)).map(|kmh| kmh / 3.6);
        // A second at this second's speed; no speed adds nothing.
        self.distance_m += speed_mps.unwrap_or(0.0);
        Second {
            sample: Sample { t: s, watts, cadence_rpm, heart_rate, speed_mps, distance_m: Some(self.distance_m) },
            power_from,
            cadence_from,
            trainer_watts,
        }
    }

    /// A new lap from the second `now` falls in (the press, not the last
    /// tick: seconds are indexes, one per second from the origin).
    pub fn lap(&mut self, now: Duration) -> Vec<String> {
        let mut warnings = Vec::new();
        let at = now.saturating_sub(self.origin).as_secs() as usize;
        if at > 0 && self.lap_starts.last() != Some(&at) {
            self.lap_starts.push(at);
            self.journal(&Entry::Lap { at }, &mut warnings);
        }
        warnings
    }

    /// The ride so far, for the FIT encoder.
    pub fn ride(&self) -> Ride {
        Ride {
            start_unix: self.start_unix,
            sub_sport: self.sub_sport,
            samples: self.seconds.iter().map(|s| s.sample).collect(),
            lap_starts: self.lap_starts.clone(),
            devices: ROLES.iter().filter_map(|&r| self.devices[role_index(r)].clone()).collect(),
        }
    }

    /// Ends the ride at `now`: the seconds still open are closed (the last,
    /// partial one too), the links dropped. Returns the ride and the
    /// journal's path, to remove once the ride is imported.
    pub fn finish(mut self, now: Duration) -> (Ride, Option<PathBuf>, Vec<String>) {
        let mut out = self.poll(now);
        let elapsed = now.saturating_sub(self.origin);
        let partial = elapsed.subsec_nanos() > 0;
        self.close_until(elapsed.as_secs() as u32 + u32::from(partial), &mut out);
        let ride = self.ride();
        (ride, self.journal.map(|j| j.path().to_path_buf()), out.warnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;
    use crate::trainer::fake::{FakeConfig, FakeTrainer};
    use crate::trainer::journal::{self, fixed, KeyFn};
    use crate::trainer::link::{Control, ControlId};
    use crate::trainer::profile::TrainerProfile;
    use std::sync::{Arc, Mutex};

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    fn ms(m: u64) -> Duration {
        Duration::from_millis(m)
    }

    /// A strap or pedals that say what they are told, every `period_ms`.
    struct Scripted {
        identity: Identity,
        reading: fn(f32) -> Option<Reading>,
        period_ms: u64,
        next_ms: u64,
        /// Identify on every poll, not once.
        chatty: bool,
        said_hello: bool,
    }

    impl SensorLink for Scripted {
        fn identity(&self) -> Option<&Identity> {
            Some(&self.identity)
        }
        fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
            let mut out = Vec::new();
            if !self.said_hello || self.chatty {
                self.said_hello = true;
                out.push(LinkEvent::Connected { at: Duration::ZERO });
                out.push(LinkEvent::Identified { at: Duration::ZERO, identity: self.identity.clone() });
            }
            while self.next_ms <= now.as_millis() as u64 {
                if let Some(mut r) = (self.reading)(self.next_ms as f32 / 1000.0) {
                    r.at = ms(self.next_ms);
                    out.push(LinkEvent::Reading(r));
                }
                self.next_ms += self.period_ms;
            }
            out
        }
    }

    fn scripted(name: &str, period_ms: u64, reading: fn(f32) -> Option<Reading>) -> Scripted {
        Scripted {
            identity: Identity { name: Some(name.into()), ble_address: Some(format!("addr-{name}")), ..Identity::new(Transport::Ble) },
            reading,
            period_ms,
            next_ms: 0,
            chatty: false,
            said_hello: false,
        }
    }

    fn strap() -> Box<dyn SensorLink> {
        Box::new(scripted("HRM-Pro", 1000, |t| Some(Reading { heart_rate: Some(120 + (t / 60.0) as u8), ..Reading::default() })))
    }

    /// Pedals that pedal for the first 30 s, once a second, saying 200 W.
    fn pedals() -> Box<dyn SensorLink> {
        Box::new(scripted("ASSIOMA55430L", 1000, |t| (t < 30.0).then(|| Reading { watts: Some(200), cadence_rpm: Some(90), ..Reading::default() })))
    }

    fn avanti() -> Box<FakeTrainer> {
        Box::new(FakeTrainer::new(FakeConfig::elite_avanti(80)))
    }

    fn session(trainer: Box<dyn TrainerLink>, pedals: Option<Box<dyn SensorLink>>, strap: Option<Box<dyn SensorLink>>) -> Session {
        Session::new(Duration::ZERO, 1_794_420_000, SubSport::IndoorCycling, trainer, pedals, strap, None)
    }

    /// Ticks once a second until seconds 0..n are closed.
    fn ride(s: &mut Session, n: u64) {
        for t in 1..=n {
            s.tick(secs(t));
        }
    }

    #[test]
    fn a_free_ride_records_a_second_a_second_from_every_source() {
        let mut s = session(avanti(), Some(pedals()), Some(strap()));
        ride(&mut s, 60);
        let seconds = s.seconds();
        assert_eq!(seconds.len(), 60);
        assert_eq!((seconds[10].sample.watts, seconds[10].power_from), (Some(200), Some(Source::Pedals)));
        assert_eq!(seconds[10].cadence_from, Some(Source::Pedals));
        assert!(seconds[10].trainer_watts.is_some_and(|w| (220..=228).contains(&w)), "the trainer's own watts kept apart");
        // Held for 3 s after the pedals' last message (at 29 s), then the trainer.
        assert_eq!(seconds[32].power_from, Some(Source::Pedals));
        assert_eq!(seconds[33].power_from, Some(Source::Trainer));
        let later = seconds[40];
        assert!(later.sample.watts.is_some_and(|w| (220..=228).contains(&w)), "riding free at 80 rpm: {later:?}");
        assert_eq!((later.sample.cadence_rpm, later.cadence_from), (Some(80), Some(Source::Trainer)));
        assert_eq!(later.sample.heart_rate, Some(120));
        assert!((later.sample.speed_mps.unwrap_or(0.0) - 8.0).abs() < 0.01, "80 rpm × 0.36 = 28.8 km/h = 8 m/s");
        assert!((seconds[59].sample.distance_m.unwrap_or(0.0) - 8.0 * 60.0).abs() < 1.0);
        let devices = s.ride().devices;
        let kinds: Vec<DeviceKind> = devices.iter().map(|d| d.kind).collect();
        assert_eq!(kinds, vec![DeviceKind::FitnessEquipment, DeviceKind::BikePower, DeviceKind::HeartRate]);
        assert_eq!(devices[1].serial, Some(serial_of("addr-ASSIOMA55430L")));
        assert_eq!(devices[1].radio, Some(Radio::Ble));
        assert_eq!(s.late_readings(), 0);
    }

    #[test]
    fn jittery_ticks_lose_no_second() {
        // Ticks a few ms either side of each second.
        let mut s = session(avanti(), None, None);
        for k in 1..=600u64 {
            let jitter = if k % 2 == 0 { 3 } else { 0 };
            s.tick(ms(k * 1000 + jitter).saturating_sub(ms(if k % 3 == 0 { 3 } else { 0 })));
        }
        s.tick(secs(601));
        assert_eq!(s.seconds().len(), 601);
        let ts: Vec<u32> = s.seconds().iter().map(|x| x.sample.t).collect();
        assert!(ts.windows(2).all(|w| w[1] == w[0] + 1), "every second once, in order");
        assert!((s.seconds()[600].sample.distance_m.unwrap_or(0.0) - 8.0 * 601.0).abs() < 2.0);
    }

    #[test]
    fn a_late_tick_closes_the_seconds_it_missed_each_with_its_readings() {
        let mut s = session(avanti(), None, None);
        s.tick(secs(2));
        let tick = s.tick(secs(7)); // the laptop was busy
        let ts: Vec<u32> = tick.seconds.iter().map(|x| x.sample.t).collect();
        assert_eq!(ts, vec![2, 3, 4, 5, 6]);
        assert!(tick.seconds.iter().all(|x| x.sample.watts.is_some() && x.sample.speed_mps.is_some()));
        assert!((s.seconds()[6].sample.distance_m.unwrap_or(0.0) - 8.0 * 7.0).abs() < 0.5);
    }

    #[test]
    fn the_ride_starts_at_its_origin_not_at_pairing() {
        let mut s = Session::new(secs(37), 1_794_420_000, SubSport::IndoorCycling, avanti(), None, Some(strap()), None);
        s.tick(secs(10)); // pairing: nothing recorded
        assert!(s.seconds().is_empty());
        for t in 38..=97 {
            s.tick(secs(t));
        }
        assert_eq!(s.seconds().len(), 60);
        assert_eq!(s.seconds()[0].sample.t, 0);
        assert!(s.seconds()[0].sample.watts.is_some());
        assert_eq!(s.ride().devices.len(), 2, "who said who they are before the start still counts");
    }

    #[test]
    fn controllers_hear_every_event_whatever_the_tick_rate() {
        struct Count(Arc<Mutex<(usize, usize)>>);
        impl Controller for Count {
            fn tick(&mut self, _: Duration, _: &Second, events: &[LinkEvent], _: &mut dyn TrainerLink) {
                let mut c = self.0.lock().unwrap();
                c.0 += 1;
                c.1 += events.iter().filter(|e| matches!(e, LinkEvent::Reading(_))).count();
            }
        }
        let seen = Arc::new(Mutex::new((0, 0)));
        let mut s = session(avanti(), None, None);
        s.add_controller(Box::new(Count(seen.clone())));
        for q in 1..=240u64 {
            s.tick(ms(q * 250)); // 4 Hz
        }
        let (calls, readings) = *seen.lock().unwrap();
        assert_eq!(calls, 60, "once per second closed");
        assert_eq!(readings, 241, "the trainer's 4 a second for 0–60 s, each once");
    }

    #[test]
    fn a_silent_second_has_no_values_but_still_counts() {
        let trainer = Box::new(FakeTrainer::new(FakeConfig::steady(80).with_drop(secs(5), secs(5))));
        let mut s = session(trainer, None, None);
        ride(&mut s, 12);
        let quiet = s.seconds()[7];
        assert_eq!((quiet.sample.watts, quiet.power_from, quiet.sample.speed_mps), (None, None, None));
        assert_eq!(s.seconds()[9].sample.distance_m, s.seconds()[4].sample.distance_m, "no speed, no distance");
    }

    #[test]
    fn power_meter_messages_once_a_revolution_never_let_the_trainer_in() {
        // A CPS power meter at 57 rpm: a message every 1.05 s.
        let p = Box::new(scripted("ASSIOMA55430L", 1050, |_| Some(Reading { watts: Some(180), cadence_rpm: Some(57), ..Reading::default() })));
        let mut s = session(avanti(), Some(p), None);
        ride(&mut s, 600);
        assert!(s.seconds().iter().all(|x| x.power_from == Some(Source::Pedals) && x.sample.watts == Some(180)));
    }

    #[test]
    fn lost_pedals_hold_nothing() {
        struct Losing(Scripted);
        impl SensorLink for Losing {
            fn identity(&self) -> Option<&Identity> {
                self.0.identity()
            }
            fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
                let mut out = self.0.poll(now);
                if now == secs(11) {
                    out.push(LinkEvent::Lost { at: secs(10) });
                }
                out
            }
        }
        let p = Box::new(Losing(scripted("P", 1000, |t| (t < 10.0).then(|| Reading { watts: Some(200), ..Reading::default() }))));
        assert!(p.identity().is_some());
        let mut s = session(avanti(), Some(p), None);
        ride(&mut s, 15);
        assert_eq!(s.seconds()[11].power_from, Some(Source::Trainer), "no hold once lost");
    }

    #[test]
    fn power_is_the_seconds_mean_and_heart_rate_only_the_straps() {
        // Pedals saying 100 then 200 W in one second, and a heart rate.
        let p = Box::new(scripted("P", 500, |t| {
            Some(Reading { watts: Some(if ((t * 2.0) as u32).is_multiple_of(2) { 100 } else { 200 }), heart_rate: Some(99), ..Reading::default() })
        }));
        let mut s = session(avanti(), Some(p), Some(strap()));
        ride(&mut s, 5);
        assert_eq!(s.seconds()[3].sample.watts, Some(150));
        assert_eq!(s.seconds()[3].sample.heart_rate, Some(120), "not the pedals' 99");
        let mut no_strap = session(avanti(), Some(Box::new(scripted("P", 500, |_| Some(Reading { heart_rate: Some(99), ..Reading::default() })))), None);
        ride(&mut no_strap, 3);
        assert_eq!(no_strap.seconds()[1].sample.heart_rate, None);
    }

    /// Records the profiles the trainer was given.
    struct Recorder(FakeTrainer, Arc<Mutex<Vec<&'static str>>>);
    impl SensorLink for Recorder {
        fn identity(&self) -> Option<&Identity> {
            self.0.identity()
        }
        fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
            let mut out = self.0.poll(now);
            // Say who it is again every poll, as BLE does when more is learned.
            if let Some(id) = self.0.identity().cloned() {
                out.push(LinkEvent::Identified { at: now, identity: id });
            }
            out
        }
    }
    impl TrainerLink for Recorder {
        fn use_profile(&mut self, profile: &'static TrainerProfile) {
            self.1.lock().unwrap().push(profile.key);
            self.0.use_profile(profile);
        }
        fn send(&mut self, control: Control) -> ControlId {
            self.0.send(control)
        }
    }

    #[test]
    fn the_trainer_gets_its_profile_once_and_only_the_trainer_picks_it() {
        let given = Arc::new(Mutex::new(Vec::new()));
        let mut trainer = Box::new(Recorder(FakeTrainer::new(FakeConfig::elite_avanti(80)), given.clone()));
        assert!(trainer.identity().is_none(), "nobody yet");
        trainer.send(Control::TargetPower(100)); // refused: not connected
        let named_like_an_avanti =
            Box::new(scripted("AVANTI COG", 1000, |_| Some(Reading { watts: Some(1), ..Reading::default() })));
        let mut s = session(trainer, Some(named_like_an_avanti), None);
        ride(&mut s, 10);
        assert_eq!(*given.lock().unwrap(), vec!["elite-avanti"]);
    }

    #[test]
    fn controllers_act_and_hold_erg() {
        struct Erg;
        impl Controller for Erg {
            fn tick(&mut self, _: Duration, _: &Second, _: &[LinkEvent], trainer: &mut dyn TrainerLink) {
                trainer.send(Control::TargetPower(150));
            }
        }
        let mut s = session(Box::new(FakeTrainer::new(FakeConfig::steady(80))), None, None);
        s.add_controller(Box::new(Erg));
        ride(&mut s, 40);
        let late: Vec<u16> = s.seconds()[30..].iter().filter_map(|x| x.sample.watts).collect();
        assert!(late.iter().all(|&w| (145..=155).contains(&w)), "{late:?}");
    }

    #[test]
    fn laps_begin_at_the_next_second_and_never_twice_at_one() {
        let mut s = session(avanti(), None, None);
        assert!(s.lap(secs(0)).is_empty()); // before anything: ignored
        ride(&mut s, 10);
        s.lap(secs(10));
        s.lap(ms(10_400));
        assert_eq!(s.ride().lap_starts, vec![10]);
    }

    fn journal_on(vault: &std::path::Path, key: KeyFn) -> Journal {
        Journal::create(vault, 1_794_420_000, SubSport::IndoorCycling, key).unwrap()
    }

    #[test]
    fn a_crash_loses_nothing_the_journal_holds_the_ride() {
        let vault = ScratchDir::new("session_crash");
        let key = Some([3u8; 32]);
        let mut s = Session::new(Duration::ZERO, 1_794_420_000, SubSport::IndoorCycling, avanti(), Some(pedals()), Some(strap()), Some(journal_on(&vault, fixed(key))));
        ride(&mut s, 45);
        s.lap(secs(45));
        for t in 46..=90 {
            s.tick(secs(t));
        }
        let before = s.ride();
        let seconds = s.seconds().to_vec();
        drop(s); // the app dies here
        let path = journal::pending(&vault).pop().expect("the journal is still there");
        let r = journal::read(&path, key.as_ref()).unwrap();
        assert_eq!(r.ride(), before, "every second, lap and device is back");
        assert_eq!(r.seconds, seconds, "sources and the trainer's own watts too");
    }

    #[test]
    fn a_failing_journal_warns_and_the_ride_goes_on() {
        let vault = ScratchDir::new("session_journal_fails");
        let failing = Arc::new(Mutex::new(false));
        let f = failing.clone();
        let key: KeyFn = Box::new(move || if *f.lock().unwrap() { Err("disk full".into()) } else { Ok(None) });
        struct Erg(Arc<Mutex<u32>>);
        impl Controller for Erg {
            fn tick(&mut self, _: Duration, _: &Second, _: &[LinkEvent], trainer: &mut dyn TrainerLink) {
                *self.0.lock().unwrap() += 1;
                trainer.send(Control::TargetPower(150));
            }
        }
        let calls = Arc::new(Mutex::new(0));
        let mut s = Session::new(Duration::ZERO, 1_794_420_000, SubSport::IndoorCycling, avanti(), None, None, Some(journal_on(&vault, key)));
        s.add_controller(Box::new(Erg(calls.clone())));
        ride(&mut s, 5);
        *failing.lock().unwrap() = true;
        let tick = s.tick(secs(6));
        assert_eq!(tick.warnings, vec!["disk full".to_string()]);
        assert_eq!(tick.seconds.len(), 1, "the second is kept");
        assert!(!s.lap(secs(6)).is_empty(), "a lap that could not be journaled says so");
        *failing.lock().unwrap() = false;
        ride(&mut s, 10);
        assert_eq!(s.seconds().len(), 10);
        assert_eq!(*calls.lock().unwrap(), 10, "the trainer stayed under control");
        let r = journal::read(&journal::pending(&vault)[0], None).unwrap();
        assert_eq!(r.seconds.len(), 9, "only the failed second is missing from the journal");
    }

    #[test]
    fn a_device_is_journaled_once_however_often_it_says_who_it_is() {
        let vault = ScratchDir::new("session_device_once");
        let mut chatty = scripted("HRM-Pro", 1000, |_| Some(Reading { heart_rate: Some(100), ..Reading::default() }));
        chatty.chatty = true;
        let mut s = Session::new(Duration::ZERO, 1_794_420_000, SubSport::IndoorCycling, avanti(), None, Some(Box::new(chatty)), Some(journal_on(&vault, fixed(None))));
        ride(&mut s, 20);
        let text = std::fs::read_to_string(&journal::pending(&vault)[0]).unwrap();
        assert_eq!(text.matches("\"device\"").count(), 2, "the trainer and the strap, once each");
    }

    #[test]
    fn finish_closes_the_last_partial_second() {
        let vault = ScratchDir::new("session_finish");
        let mut s = Session::new(Duration::ZERO, 5, SubSport::VirtualActivity, avanti(), None, None, Some(journal_on(&vault, fixed(None))));
        ride(&mut s, 3);
        let (ride, path, warnings) = s.finish(ms(4500));
        assert_eq!(ride.samples.len(), 5, "seconds 0–3 and the half second 4");
        assert_eq!(ride.sub_sport, SubSport::VirtualActivity);
        assert_eq!(path, journal::pending(&vault).pop());
        assert!(warnings.is_empty());

        let (whole, none, _) = session(avanti(), None, None).finish(secs(3));
        assert_eq!(whole.samples.len(), 3, "on a whole second nothing extra");
        assert!(none.is_none());
    }

    #[test]
    fn a_lap_starts_where_it_was_pressed_not_where_the_ticks_are() {
        let mut s = session(avanti(), None, None);
        ride(&mut s, 2);
        s.lap(ms(6_500)); // pressed while the ticks lag
        ride(&mut s, 10);
        assert_eq!(s.ride().lap_starts, vec![6]);
    }

    #[test]
    fn readings_for_a_closed_second_or_before_the_start_are_dropped() {
        struct Late;
        impl SensorLink for Late {
            fn identity(&self) -> Option<&Identity> {
                None
            }
            fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
                let mut out = Vec::new();
                if now >= secs(1) {
                    // Its own reading for this second, said with its seconds.
                    let at = now - ms(500);
                    out.push(LinkEvent::Reading(Reading { at, watts: Some(at.as_secs() as u16), ..Reading::default() }));
                }
                if now == secs(45) {
                    // A straggler for a second long closed.
                    out.push(LinkEvent::Reading(Reading { at: secs(39), watts: Some(999), ..Reading::default() }));
                }
                out
            }
        }
        let late = Late;
        assert!(late.identity().is_none());
        let mut s = Session::new(secs(37), 0, SubSport::IndoorCycling, avanti(), Some(Box::new(late)), None, None);
        for t in 1..=50 {
            s.tick(secs(t));
        }
        // Second 0 holds 37.5 s's reading only; 0–36 s were pairing.
        assert_eq!(s.seconds()[0].sample.watts, Some(37));
        assert_eq!(s.seconds()[2].sample.watts, Some(39), "the straggler did not land in a closed second");
        assert_eq!(s.late_readings(), 1);
    }

    #[test]
    fn controllers_get_the_latest_second_closed() {
        struct Last(Arc<Mutex<Vec<u32>>>);
        impl Controller for Last {
            fn tick(&mut self, _: Duration, second: &Second, _: &[LinkEvent], _: &mut dyn TrainerLink) {
                self.0.lock().unwrap().push(second.sample.t);
            }
        }
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut s = session(avanti(), None, None);
        s.add_controller(Box::new(Last(seen.clone())));
        s.tick(secs(2));
        s.tick(secs(7));
        assert_eq!(*seen.lock().unwrap(), vec![1, 6]);
    }

    #[test]
    fn pedals_back_after_a_drop_are_held_again() {
        struct DropsOnce(Scripted);
        impl SensorLink for DropsOnce {
            fn identity(&self) -> Option<&Identity> {
                self.0.identity()
            }
            fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
                let mut out = self.0.poll(now);
                match now.as_secs() {
                    10 => out.push(LinkEvent::Lost { at: secs(10) }),
                    12 => out.push(LinkEvent::Back { at: secs(12), outage: secs(2) }),
                    _ => {}
                }
                out
            }
        }
        let p = DropsOnce(scripted("P", 1050, |_| Some(Reading { watts: Some(180), ..Reading::default() })));
        assert!(p.identity().is_some());
        let mut s = session(avanti(), Some(Box::new(p)), None);
        ride(&mut s, 120);
        assert!(s.seconds()[20..].iter().all(|x| x.power_from == Some(Source::Pedals)), "the hold works again after Back");
    }

    #[test]
    fn heart_rate_is_held_between_beats_unless_the_strap_is_lost() {
        let beats = Box::new(scripted("HRM", 1050, |_| Some(Reading { heart_rate: Some(140), ..Reading::default() })));
        let mut s = session(avanti(), None, Some(beats));
        ride(&mut s, 600);
        assert!(s.seconds().iter().all(|x| x.sample.heart_rate == Some(140)));

        let gone = Box::new(scripted("HRM", 1000, |t| (t < 5.0).then(|| Reading { heart_rate: Some(140), ..Reading::default() })));
        let mut s = session(avanti(), None, Some(gone));
        ride(&mut s, 12);
        assert_eq!(s.seconds()[7].sample.heart_rate, Some(140), "held 3 s after 4 s");
        assert_eq!(s.seconds()[8].sample.heart_rate, None);
    }

    #[test]
    fn a_lost_strap_holds_nothing_and_is_held_again_once_back() {
        struct Drops(Scripted);
        impl SensorLink for Drops {
            fn identity(&self) -> Option<&Identity> {
                self.0.identity()
            }
            fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
                let mut out = self.0.poll(now);
                match now.as_secs() {
                    10 => out.push(LinkEvent::Lost { at: secs(10) }),
                    20 => out.push(LinkEvent::Back { at: secs(20), outage: secs(10) }),
                    _ => {}
                }
                out
            }
        }
        let strap = Drops(scripted("HRM", 1050, |t| {
            (!(10.0..20.0).contains(&t)).then(|| Reading { heart_rate: Some(140), ..Reading::default() })
        }));
        assert!(strap.identity().is_some());
        let mut s = session(avanti(), None, Some(Box::new(strap)));
        ride(&mut s, 120);
        assert!(s.seconds()[10..20].iter().all(|x| x.sample.heart_rate.is_none()), "no hold once lost");
        assert!(s.seconds()[22..].iter().all(|x| x.sample.heart_rate == Some(140)), "held again after Back");
    }

    #[test]
    fn a_ble_address_gives_a_stable_nonzero_serial() {
        assert_eq!(serial_of("6A6E19E4"), serial_of("6A6E19E4"));
        assert_ne!(serial_of("6A6E19E4"), serial_of("6A6E19E5"));
        assert!(serial_of("") != 0);
        let ant = Identity { ant_device_number: Some(14830), ..Identity::new(Transport::Ant) };
        assert_eq!(device_of(Source::Trainer, &ant).serial, Some(14830));
        assert_eq!(device_of(Source::Trainer, &Identity::new(Transport::Fake)).radio, None);
        let model_only = Identity { model: Some("Avanti".into()), ..Identity::new(Transport::Ble) };
        assert_eq!(device_of(Source::Trainer, &model_only).name.as_deref(), Some("Avanti"));
    }
}
