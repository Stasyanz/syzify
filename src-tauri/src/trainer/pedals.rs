//! Power pedals that are two devices — an Assioma Duo over BLE is one per
//! leg, each sending its own leg's power — made one power source. Linked
//! alone, one side records half the ride's power; the spike summed them.
//!
//! The pair speaks for both or not at all: its power is left + right while
//! both are heard (within `PAIR_FRESH` of each other), and nothing when one
//! is lost or late — the trainer's watts stand in, never one leg's. It is
//! connected (and says who it is) once both are, lost when either side is
//! after that, back when both are.
//!
//! Order: a poll gives the left's events, then the right's — each side in
//! order, the two not interleaved by time. The session files readings by
//! their own time stamps, so that is enough.
//!
//! `new(left, right)`: which is which is the caller's word (the device
//! picker pairs them by name, left first).

use std::time::Duration;

use super::link::{Identity, LinkEvent, Reading, SensorLink};

/// The two sides' latest readings count together only this close in time.
pub const PAIR_FRESH: Duration = Duration::from_secs(2);

/// Left or right, by an advertised name's last letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// A pedal's side and its name without it, from its own: `ASSIOMA55430L`
/// is a left, `ASSIOMA55430`. The name before the letter must end in a
/// digit (a serial), so a `KICKR` is not the right of a `KICK`. Each pedal
/// has its own serial (`ASSIOMA55430L` pairs with `ASSIOMA61026R`): the
/// pair is told by [`model_of`].
pub fn side_of(name: &str) -> Option<(&str, Side)> {
    let name = name.trim();
    let last = name.chars().next_back()?;
    let base = &name[..name.len() - last.len_utf8()];
    if !base.ends_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    match last {
        'L' | 'l' => Some((base, Side::Left)),
        'R' | 'r' => Some((base, Side::Right)),
        _ => None,
    }
}

/// A side's model: its name without the side and the serial
/// (`ASSIOMA55430L` → `ASSIOMA`); None when nothing is left.
pub fn model_of(name: &str) -> Option<&str> {
    let (base, _) = side_of(name)?;
    Some(base.trim_end_matches(|c: char| c.is_ascii_digit())).filter(|m| !m.is_empty())
}

pub struct PedalPair {
    /// Left, right.
    sides: [Box<dyn SensorLink>; 2],
    last: [Option<Reading>; 2],
    heard: [bool; 2],
    lost: [bool; 2],
    connected: bool,
    /// When the pair went silent (the first side lost).
    lost_at: Option<Duration>,
    identity: Option<Identity>,
    failed: bool,
}

impl PedalPair {
    pub fn new(left: Box<dyn SensorLink>, right: Box<dyn SensorLink>) -> PedalPair {
        PedalPair {
            sides: [left, right],
            last: [None, None],
            heard: [false; 2],
            lost: [false; 2],
            connected: false,
            lost_at: None,
            identity: None,
            failed: false,
        }
    }

    /// The pair as one device: the left's identity (else the right's),
    /// named for both.
    fn combined(&self) -> Option<Identity> {
        let mut id = self.sides[0].identity().or(self.sides[1].identity())?.clone();
        if let Some((base, _)) = id.name.as_deref().and_then(side_of) {
            id.name = Some(base.to_string());
        }
        Some(id)
    }

    /// Connected, the first time both sides are heard and neither is lost;
    /// then who it is.
    fn up(&mut self, at: Duration, out: &mut Vec<LinkEvent>) {
        if !self.connected && self.heard == [true, true] && self.lost == [false, false] {
            self.connected = true;
            out.push(LinkEvent::Connected { at });
            self.say_who(at, out);
        }
    }

    /// Identified, when what the pair is changed.
    fn say_who(&mut self, at: Duration, out: &mut Vec<LinkEvent>) {
        let id = self.combined();
        if id != self.identity {
            self.identity = id.clone();
            out.extend(id.map(|identity| LinkEvent::Identified { at, identity }));
        }
    }

    /// Side `i`'s reading `r` with the other side's latest: both legs, or
    /// nothing for power.
    fn both(&self, i: usize, r: &Reading) -> Reading {
        let other = self.last[1 - i].as_ref().filter(|o| !self.lost[1 - i] && o.at.abs_diff(r.at) <= PAIR_FRESH);
        let watts = |o: &Reading| r.watts.zip(o.watts);
        let (left, right) = match other.and_then(watts) {
            Some((mine, theirs)) if i == 0 => (Some(mine), Some(theirs)),
            Some((mine, theirs)) => (Some(theirs), Some(mine)),
            None => (None, None),
        };
        let total = left.zip(right).map(|(l, r)| l.saturating_add(r));
        Reading {
            at: r.at,
            watts: total,
            cadence_rpm: r.cadence_rpm.or(other.and_then(|o| o.cadence_rpm)),
            balance_right: total
                .filter(|&t| t > 0)
                .zip(right)
                .map(|(t, r)| (u32::from(r) * 100 / u32::from(t)) as u8),
            ..Reading::default()
        }
    }
}

impl SensorLink for PedalPair {
    fn identity(&self) -> Option<&Identity> {
        self.identity.as_ref()
    }

    fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
        let mut out = Vec::new();
        for i in 0..2 {
            for event in self.sides[i].poll(now) {
                if self.failed {
                    return out;
                }
                match event {
                    LinkEvent::Connected { at } => {
                        self.heard[i] = true;
                        self.lost[i] = false;
                        self.up(at, &mut out);
                    }
                    LinkEvent::Identified { at, .. } => {
                        if self.connected {
                            self.say_who(at, &mut out);
                        }
                    }
                    LinkEvent::Lost { at } => {
                        self.lost[i] = true;
                        self.last[i] = None;
                        // Lost before it was ever up is just not up yet.
                        if self.connected && self.lost_at.is_none() {
                            self.lost_at = Some(at);
                            out.push(LinkEvent::Lost { at });
                        }
                    }
                    LinkEvent::Back { at, .. } => {
                        self.lost[i] = false;
                        if self.lost == [false, false] {
                            match self.lost_at.take() {
                                Some(since) => out.push(LinkEvent::Back { at, outage: at.saturating_sub(since) }),
                                None => self.up(at, &mut out),
                            }
                        }
                    }
                    LinkEvent::Reading(r) => {
                        let both = self.both(i, &r);
                        self.last[i] = Some(r);
                        out.push(LinkEvent::Reading(both));
                    }
                    LinkEvent::Failed { at, reason } => {
                        // One pedal gone for good: the pair is.
                        self.failed = true;
                        out.push(LinkEvent::Failed { at, reason });
                    }
                    // Pedals take no controls.
                    LinkEvent::Control { .. } => {}
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trainer::link::{ControlId, ControlOutcome, Control, LinkFailure, Transport};

    fn ms(m: u64) -> Duration {
        Duration::from_millis(m)
    }

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    /// One pedal: hello on the first poll, its watts every second from
    /// `first_ms` (none in `quiet`), and `script` events at their times.
    struct Leg {
        identity: Identity,
        watts: u16,
        cadence: Option<u8>,
        next_ms: u64,
        hello: bool,
        quiet: Option<(Duration, Duration)>,
        script: Vec<LinkEvent>,
    }

    fn at(e: &LinkEvent) -> Duration {
        match e {
            LinkEvent::Connected { at } | LinkEvent::Lost { at } | LinkEvent::Back { at, .. } | LinkEvent::Failed { at, .. } => *at,
            LinkEvent::Identified { at, .. } | LinkEvent::Control { at, .. } => *at,
            LinkEvent::Reading(r) => r.at,
        }
    }

    impl SensorLink for Leg {
        fn identity(&self) -> Option<&Identity> {
            self.hello.then_some(&self.identity)
        }
        fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
            let mut out = Vec::new();
            if !self.hello {
                self.hello = true;
                out.push(LinkEvent::Connected { at: Duration::ZERO });
                out.push(LinkEvent::Identified { at: Duration::ZERO, identity: self.identity.clone() });
            }
            while self.next_ms <= now.as_millis() as u64 {
                let t = ms(self.next_ms);
                if !self.quiet.is_some_and(|(from, to)| (from..to).contains(&t)) {
                    out.push(LinkEvent::Reading(Reading { at: t, watts: Some(self.watts), cadence_rpm: self.cadence, ..Reading::default() }));
                }
                self.next_ms += 1000;
            }
            let (due, later): (Vec<LinkEvent>, Vec<LinkEvent>) = self.script.drain(..).partition(|e| at(e) <= now);
            self.script = later;
            out.extend(due);
            out
        }
    }

    fn leg(name: &str, address: &str, watts: u16, first_ms: u64) -> Leg {
        Leg {
            identity: Identity { name: Some(name.into()), ble_address: Some(address.into()), ..Identity::new(Transport::Ble) },
            watts,
            cadence: Some(88),
            next_ms: first_ms,
            hello: false,
            quiet: None,
            script: Vec::new(),
        }
    }

    fn pair(left: Leg, right: Leg) -> PedalPair {
        PedalPair::new(Box::new(left), Box::new(right))
    }

    /// Polls once a second to `to` s; every event the pair said.
    fn ride(p: &mut PedalPair, from: u64, to: u64) -> Vec<LinkEvent> {
        (from..=to).flat_map(|s| p.poll(secs(s))).collect()
    }

    fn watts(events: &[LinkEvent]) -> Vec<Option<u16>> {
        events.iter().filter_map(|e| if let LinkEvent::Reading(r) = e { Some(r.watts) } else { None }).collect()
    }

    #[test]
    fn a_side_is_told_by_its_name() {
        assert_eq!(side_of("ASSIOMA55430L"), Some(("ASSIOMA55430", Side::Left)));
        assert_eq!(side_of(" assioma1r "), Some(("assioma1", Side::Right)));
        assert_eq!(side_of("KICKR"), None, "no serial before the letter");
        assert_eq!(side_of("ASSIOMA55430"), None);
        assert_eq!(side_of(""), None);
        assert_eq!(side_of("12Ä"), None, "a letter that is not one byte is no panic");
        assert_eq!(model_of("ASSIOMA55430L"), Some("ASSIOMA"));
        assert_eq!(model_of("ASSIOMA61026R"), Some("ASSIOMA"), "the other pedal's serial: one model");
        assert_eq!(model_of("12345L"), None, "a serial alone is no model");
        assert_eq!(model_of("KICKR"), None);
    }

    #[test]
    fn the_pair_is_one_device_with_both_legs_power() {
        let mut p = pair(leg("ASSIOMA55430L", "AL", 110, 0), leg("ASSIOMA61026R", "AR", 90, 500));
        assert!(p.identity().is_none());
        let events = ride(&mut p, 0, 10);
        let connected = events.iter().filter(|e| matches!(e, LinkEvent::Connected { .. })).count();
        assert_eq!(connected, 1, "connected once both are");
        let ids: Vec<&Identity> = events.iter().filter_map(|e| if let LinkEvent::Identified { identity, .. } = e { Some(identity) } else { None }).collect();
        assert_eq!(ids.len(), 1, "one identity, though both said theirs");
        assert_eq!(ids[0].name.as_deref(), Some("ASSIOMA55430"));
        assert_eq!(ids[0].ble_address.as_deref(), Some("AL"), "the left's: its serial in the ride");
        assert_eq!(p.identity().and_then(|i| i.name.as_deref()), Some("ASSIOMA55430"));
        let w = watts(&events);
        assert_eq!(w[..2], [None, None], "a side alone at first (the right's first comes later): not half the power");
        assert!(w[2..].iter().all(|&x| x == Some(200)), "{w:?}");
        let balance: Vec<Option<u8>> = events.iter().filter_map(|e| if let LinkEvent::Reading(r) = e { Some(r.balance_right) } else { None }).collect();
        assert_eq!(balance.last(), Some(&Some(45)), "90 of 200 from the right");
        assert!(events.iter().all(|e| !matches!(e, LinkEvent::Reading(r) if r.cadence_rpm != Some(88))));
    }

    #[test]
    fn a_lost_side_gives_no_power_and_the_pair_is_back_with_both() {
        let mut right = leg("ASSIOMA61026R", "AR", 90, 500);
        right.quiet = Some((secs(10), secs(20)));
        right.script = vec![LinkEvent::Lost { at: secs(10) }, LinkEvent::Back { at: secs(20), outage: secs(10) }];
        let mut left = leg("ASSIOMA55430L", "AL", 110, 0);
        left.script = vec![LinkEvent::Lost { at: secs(12) }, LinkEvent::Back { at: secs(14), outage: secs(2) }];
        let mut p = pair(left, right);
        ride(&mut p, 0, 9);
        let lost = ride(&mut p, 10, 19);
        let losts: Vec<Duration> = lost.iter().filter_map(|e| if let LinkEvent::Lost { at } = e { Some(*at) } else { None }).collect();
        assert_eq!(losts, vec![secs(10)], "lost once, when the first side went");
        assert!(!lost.iter().any(|e| matches!(e, LinkEvent::Back { .. })), "the left back alone is not the pair back");
        let after: Vec<Option<u16>> = lost
            .iter()
            .filter_map(|e| if let LinkEvent::Reading(r) = e { (r.at > secs(10)).then_some(r.watts) } else { None })
            .collect();
        assert!(!after.is_empty() && after.iter().all(|w| w.is_none()), "one leg is never the power: {after:?}");
        let back = ride(&mut p, 20, 25);
        let backs: Vec<(Duration, Duration)> =
            back.iter().filter_map(|e| if let LinkEvent::Back { at, outage } = e { Some((*at, *outage)) } else { None }).collect();
        assert_eq!(backs, vec![(secs(20), secs(10))]);
        assert_eq!(watts(&back).last(), Some(&Some(200)));
    }

    #[test]
    fn a_side_gone_quiet_without_a_word_counts_no_more() {
        let mut right = leg("ASSIOMA61026R", "AR", 90, 500);
        right.quiet = Some((secs(5), secs(60)));
        let mut p = pair(leg("ASSIOMA55430L", "AL", 110, 0), right);
        let events = ride(&mut p, 0, 15);
        let w = watts(&events);
        assert!(w[w.len() - 5..].iter().all(|x| x.is_none()), "stale after PAIR_FRESH: {w:?}");
    }

    #[test]
    fn a_side_that_fails_ends_the_pair() {
        let mut left = leg("ASSIOMA55430L", "AL", 110, 0);
        left.script = vec![LinkEvent::Failed { at: secs(3), reason: LinkFailure::PermissionDenied }];
        let mut p = pair(left, leg("ASSIOMA61026R", "AR", 90, 500));
        let events = ride(&mut p, 0, 3);
        assert!(matches!(events.last(), Some(LinkEvent::Failed { .. })));
        assert!(ride(&mut p, 4, 10).is_empty(), "nothing after Failed");
        // A control outcome (none should come) is ignored.
        let mut stray = leg("ASSIOMA55430L", "AL", 110, 0);
        stray.script = vec![
            LinkEvent::Control { id: ControlId(1), at: secs(1), control: Control::Release, outcome: ControlOutcome::Refused },
            // A late reading of its own (as a radio may bring one).
            LinkEvent::Reading(Reading { at: ms(1200), watts: Some(120), ..Reading::default() }),
        ];
        let mut p = pair(stray, leg("ASSIOMA61026R", "AR", 90, 500));
        let events = ride(&mut p, 0, 2);
        assert!(!events.iter().any(|e| matches!(e, LinkEvent::Control { .. })));
        assert!(events.iter().any(|e| matches!(e, LinkEvent::Reading(r) if r.at == ms(1200) && r.watts == Some(210))));
    }

    /// In a ride: both legs' power is the second's, and with one side lost
    /// the trainer's stands in, never half.
    #[test]
    fn a_ride_records_both_legs_or_the_trainer() {
        use crate::trainer::fake::{FakeConfig, FakeTrainer};
        use crate::trainer::fit::SubSport;
        use crate::trainer::journal::Source;
        use crate::trainer::session::Session;
        let mut right = leg("ASSIOMA61026R", "AR", 90, 500);
        right.quiet = Some((secs(30), secs(40)));
        right.script = vec![LinkEvent::Lost { at: secs(30) }, LinkEvent::Back { at: secs(40), outage: secs(10) }];
        let pedals = Box::new(pair(leg("ASSIOMA55430L", "AL", 110, 0), right));
        let trainer = Box::new(FakeTrainer::new(FakeConfig::steady(80)));
        let mut s = Session::new(Duration::ZERO, 0, SubSport::IndoorCycling, trainer, Some(pedals), None, None);
        for q in 1..=240 {
            s.tick(ms(q * 250));
        }
        let at = |t: usize| (s.seconds()[t].sample.watts, s.seconds()[t].power_from);
        assert_eq!(at(10), (Some(200), Some(Source::Pedals)));
        let lost = at(35);
        assert_eq!(lost.1, Some(Source::Trainer), "one side lost: the trainer's, not 110 W");
        assert!(lost.0.is_some_and(|w| w > 150));
        assert_eq!(at(50), (Some(200), Some(Source::Pedals)));
        assert_eq!(s.ride().devices[1].name.as_deref(), Some("ASSIOMA55430"));
    }

    #[test]
    fn coasting_is_zero_watts_with_no_balance() {
        let mut p = pair(leg("ASSIOMA55430L", "AL", 0, 0), leg("ASSIOMA61026R", "AR", 0, 500));
        let events = ride(&mut p, 0, 5);
        let readings: Vec<&Reading> = events.iter().filter_map(|e| if let LinkEvent::Reading(r) = e { Some(r) } else { None }).collect();
        let last = readings.last().unwrap();
        assert_eq!((last.watts, last.balance_right), (Some(0), None), "no division by zero");
    }

    #[test]
    fn each_side_gives_the_balance_and_the_cadence_the_other_lacks() {
        let mut right = leg("ASSIOMA61026R", "AR", 90, 500);
        right.cadence = None;
        let mut p = pair(leg("ASSIOMA55430L", "AL", 110, 0), right);
        let events = ride(&mut p, 0, 6);
        let at = |t: Duration| {
            events.iter().find_map(|e| if let LinkEvent::Reading(r) = e { (r.at == t).then(|| r.clone()) } else { None }).unwrap()
        };
        let on_left = at(secs(5));
        assert_eq!((on_left.watts, on_left.balance_right), (Some(200), Some(45)), "summed on the left's reading too");
        let on_right = at(ms(5500));
        assert_eq!(on_right.cadence_rpm, Some(88), "the right says none: the left's");
    }

    /// The left came, went, and the right came meanwhile: not connected
    /// until both are there, and the identity only then — the left's.
    #[test]
    fn the_pair_is_up_only_with_both_sides_there() {
        let mut left = leg("ASSIOMA55430L", "AL", 110, 0);
        left.quiet = Some((secs(2), secs(8)));
        left.script = vec![LinkEvent::Lost { at: secs(2) }, LinkEvent::Back { at: secs(8), outage: secs(6) }];
        let mut right = leg("ASSIOMA61026R", "AR", 90, 500);
        right.hello = true; // says hello later, by script
        right.script = vec![
            LinkEvent::Connected { at: secs(5) },
            LinkEvent::Identified { at: secs(5), identity: right.identity.clone() },
        ];
        right.next_ms = 5500;
        let mut p = pair(left, right);
        let early = ride(&mut p, 0, 7);
        assert!(
            !early.iter().any(|e| matches!(e, LinkEvent::Connected { .. } | LinkEvent::Identified { .. } | LinkEvent::Lost { .. })),
            "nothing to say before both are there: {early:?}"
        );
        assert!(p.identity().is_none());
        let up = ride(&mut p, 8, 9);
        let connected: Vec<Duration> = up.iter().filter_map(|e| if let LinkEvent::Connected { at } = e { Some(*at) } else { None }).collect();
        assert_eq!(connected, vec![secs(8)], "up once the left is back: {up:?}");
        assert!(!up.iter().any(|e| matches!(e, LinkEvent::Back { .. })), "it never was up: no Back");
        assert_eq!(p.identity().and_then(|i| i.ble_address.as_deref()), Some("AL"));
        assert_eq!(p.identity().and_then(|i| i.name.as_deref()), Some("ASSIOMA55430"));
    }

    /// A side with no name keeps the pair's identity as it is.
    #[test]
    fn a_nameless_side_is_named_as_it_is() {
        let mut left = leg("x", "AL", 100, 0);
        left.identity.name = None;
        let mut p = pair(left, leg("ASSIOMA61026R", "AR", 100, 500));
        ride(&mut p, 0, 2);
        assert_eq!(p.identity().and_then(|i| i.ble_address.as_deref()), Some("AL"));
        assert_eq!(p.identity().and_then(|i| i.name.as_deref()), None);
    }
}
