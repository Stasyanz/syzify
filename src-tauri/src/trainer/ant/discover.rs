//! Finding ANT+ devices to pick from: the stick in Rx Scan Mode hears
//! every device in the room, each message tagged with its sender. The USB
//! work is the driver's; here is what the stick is told and what a scan
//! makes of what it heard.

use std::collections::BTreeMap;
use std::time::Duration;

use super::frame::*;
use super::machine::{Input, Progress, Setup, ANT_PLUS_FREQ};
use super::pages::{self, Sender, KIND_FEC, KIND_HEART, KIND_POWER};
use crate::models::training::{FoundDevice, Radio, SensorRole};
use crate::trainer::link::Identity;

/// Fewer messages than this from one sender: a stray (scan mode now and
/// then tags a packet with a mix of two senders' ids).
const MIN_HEARD: u32 = 3;

/// An ANT+ device's address for the picker: `ant:<type>:<number>`.
pub fn address(sender: Sender) -> String {
    format!("ant:{}:{}", sender.kind, sender.number)
}

/// The device an address names, if it is an ANT+ one.
pub fn parse_address(address: &str) -> Option<Sender> {
    let mut parts = address.strip_prefix("ant:")?.split(':');
    let kind = parts.next()?.parse().ok()?;
    let number = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some(Sender { number, kind })
}

fn role_of(kind: u8) -> Option<SensorRole> {
    match kind {
        KIND_FEC => Some(SensorRole::Trainer),
        KIND_POWER => Some(SensorRole::Pedals),
        KIND_HEART => Some(SensorRole::Strap),
        _ => None,
    }
}

#[derive(Debug)]
struct Heard {
    count: u32,
    identity: Identity,
}

#[derive(Debug)]
pub struct Scan {
    setup: Option<Setup>,
    failed: Option<String>,
    heard: BTreeMap<Sender, Heard>,
}

impl Scan {
    /// A scan on network `key`: returns it and the first message (a reset).
    pub fn new(now: Duration, key: &[u8; 8]) -> (Scan, Msg) {
        let mut queue = Setup::network(key);
        queue.extend([
            // Channel 0, any device, on the ANT+ frequency, listening all
            // the time.
            Msg::new(MSG_ASSIGN, &[0, 0x00, 0]),
            Msg::new(MSG_CHANNEL_ID, &[0, 0, 0, 0, 0]),
            Msg::new(MSG_RF_FREQ, &[0, ANT_PLUS_FREQ]),
            Msg::new(MSG_OPEN_SCAN, &[0]),
        ]);
        let (setup, reset) = Setup::new(now, queue);
        (Scan { setup: Some(setup), failed: None, heard: BTreeMap::new() }, reset)
    }

    /// Why the stick could not scan.
    pub fn failed(&self) -> Option<&str> {
        self.failed.as_deref()
    }

    pub fn handle(&mut self, now: Duration, input: Input) -> Vec<Msg> {
        let progress = match (self.setup.as_mut(), input) {
            (_, Input::Gone) => Progress::Failed("the ANT stick was unplugged".into()),
            (Some(setup), Input::Message(m)) => setup.on_message(now, &m),
            (Some(setup), Input::Tick) => setup.on_tick(now),
            (None, Input::Message(m)) => {
                self.heard_from(&m);
                Progress::Wait
            }
            (None, Input::Tick) => Progress::Wait,
        };
        match progress {
            Progress::Send(m) => vec![m],
            Progress::Wait => Vec::new(),
            Progress::Done => {
                self.setup = None;
                Vec::new()
            }
            Progress::Failed(why) => {
                self.failed.get_or_insert(why);
                Vec::new()
            }
        }
    }

    fn heard_from(&mut self, m: &Msg) {
        if !matches!(m.id, MSG_BROADCAST | MSG_ACKNOWLEDGED | MSG_BURST) {
            return;
        }
        let Some(pages::Packet { page, from: Some(sender), .. }) = pages::packet(&m.data) else { return };
        let heard = self.heard.entry(sender).or_insert_with(|| Heard { count: 0, identity: pages::identity_of(sender) });
        heard.count += 1;
        pages::absorb(&mut heard.identity, sender.kind, &page);
    }

    /// Trainers, pedals and straps heard enough, by role then number, each
    /// named by its maker when it said.
    pub fn found(&self) -> Vec<FoundDevice> {
        let mut found: Vec<(SensorRole, Sender, &Heard)> = self
            .heard
            .iter()
            .filter(|(_, h)| h.count >= MIN_HEARD)
            .filter_map(|(&s, h)| Some((role_of(s.kind)?, s, h)))
            .collect();
        let rank = |r: SensorRole| match r {
            SensorRole::Trainer => 0,
            SensorRole::Pedals => 1,
            SensorRole::Strap => 2,
        };
        found.sort_by_key(|(role, s, _)| (rank(*role), s.number));
        found
            .into_iter()
            .map(|(role, sender, h)| FoundDevice {
                role,
                radio: Radio::Ant,
                address: address(sender),
                name: Some(format!("{} {}", h.identity.manufacturer.as_deref().unwrap_or("ANT+"), sender.number)),
                other_side: None,
                lone_side: false,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trainer::ant::machine::tests::{data, ok, KEY};

    fn ms(m: u64) -> Duration {
        Duration::from_millis(m)
    }

    #[test]
    fn addresses_name_the_type_and_number() {
        let avanti = Sender { number: 14830, kind: KIND_FEC };
        assert_eq!(address(avanti), "ant:17:14830");
        assert_eq!(parse_address("ant:17:14830"), Some(avanti));
        for not in ["17:14830", "ant:17", "ant:x:1", "ant:17:x", "ant:17:1:2", "1C2D3E4F-AB"] {
            assert_eq!(parse_address(not), None, "{not}");
        }
    }

    #[test]
    fn a_scan_lists_what_it_heard_enough_by_role() {
        let (mut scan, reset) = Scan::new(ms(0), &KEY);
        assert_eq!(reset.id, MSG_RESET);
        let mut sent = Vec::new();
        let mut out = scan.handle(ms(10), Input::Message(Msg::new(MSG_STARTUP, &[0])));
        while let Some(next) = out.pop() {
            sent.push(next.id);
            out = scan.handle(ms(20), Input::Message(ok(&next)));
        }
        assert_eq!(sent, vec![MSG_SET_KEY, MSG_LIB_CONFIG, MSG_ASSIGN, MSG_CHANNEL_ID, MSG_RF_FREQ, MSG_OPEN_SCAN]);
        scan.handle(ms(30), Input::Tick);
        let fec = [0x19, 1, 85, 0, 0, 150, 0x30, 0x30];
        for _ in 0..3 {
            scan.handle(ms(100), Input::Message(data(0, 14830, KIND_HEART, [0x84, 0, 0, 0, 0, 0, 0, 120])));
            scan.handle(ms(100), Input::Message(data(0, 55430, KIND_POWER, [0x10, 1, 0xFF, 90, 0, 0, 200, 0])));
            scan.handle(ms(100), Input::Message(data(0, 14830, KIND_FEC, fec)));
            scan.handle(ms(100), Input::Message(data(0, 9, 121, fec)));
        }
        scan.handle(ms(200), Input::Message(data(0, 14830, KIND_FEC, [0x50, 0xFF, 0xFF, 1, 86, 0, 45, 0])));
        scan.handle(ms(200), Input::Message(data(0, 777, KIND_FEC, fec)));
        scan.handle(ms(200), Input::Message(data(0, 777, KIND_FEC, fec)));
        scan.handle(ms(200), Input::Message(Msg::new(MSG_BROADCAST, &[0, 1, 2, 3, 4, 5, 6, 7, 8])));
        scan.handle(ms(200), Input::Message(Msg::new(MSG_CHANNEL_EVENT, &[0, 1, 2])));
        let found = scan.found();
        let listed: Vec<(SensorRole, &str, &str)> =
            found.iter().map(|f| (f.role, f.address.as_str(), f.name.as_deref().unwrap_or_default())).collect();
        assert_eq!(
            listed,
            vec![
                (SensorRole::Trainer, "ant:17:14830", "Elite 14830"),
                (SensorRole::Pedals, "ant:11:55430", "ANT+ 55430"),
                (SensorRole::Strap, "ant:120:14830", "ANT+ 14830"),
            ],
            "a speed sensor, one heard only twice, an anonymous message: not listed"
        );
        assert!(found.iter().all(|f| f.radio == Radio::Ant && f.other_side.is_none() && !f.lone_side));
        assert_eq!(scan.failed(), None);
        scan.handle(ms(300), Input::Gone);
        scan.handle(ms(300), Input::Gone);
        assert_eq!(scan.failed(), Some("the ANT stick was unplugged"));
    }

    #[test]
    fn a_stick_that_refuses_the_scan_says_why() {
        let (mut scan, _) = Scan::new(ms(0), &KEY);
        scan.handle(ms(1000), Input::Tick);
        scan.handle(ms(1100), Input::Message(Msg::new(MSG_CHANNEL_EVENT, &[0, MSG_SET_KEY, 0x28])));
        assert!(scan.failed().is_some_and(|why| why.contains("refused")));
        assert!(scan.found().is_empty());
    }
}
