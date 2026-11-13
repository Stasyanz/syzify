//! ANT+ data pages: what a trainer (FE-C), power pedals (Bicycle Power)
//! and a strap (Heart Rate) send, what they say about themselves (common
//! pages 0x50/0x51, the strap's own 2/3), and the pages that control a
//! trainer.

use std::time::Duration;

use crate::trainer::link::{Control, Identity, Reading, Transport};

/// Device types (ANT+ device profiles).
pub const KIND_POWER: u8 = 11;
pub const KIND_FEC: u8 = 17;
pub const KIND_HEART: u8 = 120;

/// The channel period each profile sends at (32768 / period messages a
/// second).
pub fn period(kind: u8) -> u16 {
    match kind {
        KIND_POWER => 8182,
        KIND_HEART => 8070,
        _ => 8192,
    }
}

/// A counter that moved further than this between two pages heard may
/// have wrapped: the page's own value is taken instead…
const MAX_EVENTS: u8 = 16;
/// …and so after a longer silence (the crank period wraps every 32 s).
const MAX_GAP: Duration = Duration::from_secs(20);
/// More watts than this from the counters is a counter gone wrong (a
/// sensor that restarted): the page's own value is taken.
const MAX_WATTS: u16 = 4000;
/// …and more rpm than this.
const MAX_RPM: f32 = 200.0;
/// Pedals whose event count has not moved this long stand still (their
/// page may go on saying the last watts).
const IDLE: Duration = Duration::from_secs(3);

/// Who sent a data message: the 20-bit device number (16 bits plus the
/// transmission type's upper nibble — the ANT ID a Garmin shows) and type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sender {
    pub number: u32,
    pub kind: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub channel: u8,
    pub page: [u8; 8],
    pub from: Option<Sender>,
}

/// A data message `[channel, 8 bytes, flag, extended…]`; the extended part
/// is there because lib config asked for the channel id.
pub fn packet(data: &[u8]) -> Option<Packet> {
    let channel = *data.first()?;
    let page: [u8; 8] = data.get(1..9)?.try_into().ok()?;
    let from = match (data.get(9), data.get(10..14)) {
        (Some(flag), Some(ext)) if flag & 0x80 != 0 => Some(Sender {
            number: u32::from(u16::from_le_bytes([ext[0], ext[1]])) | (u32::from(ext[3] >> 4) << 16),
            kind: ext[2] & 0x7F,
        }),
        _ => None,
    };
    Some(Packet { channel, page, from })
}

/// Two counted pages may be compared: little time and few events apart.
fn comparable(before_at: Duration, at: Duration, events: u8) -> bool {
    at.saturating_sub(before_at) <= MAX_GAP && (1..=MAX_EVENTS).contains(&events)
}

/// Whether an event count stands still.
#[derive(Debug, Default)]
struct Still {
    events: Option<u8>,
    moved: Duration,
}

impl Still {
    /// True when `events` has not moved for `IDLE`.
    fn still(&mut self, events: u8, at: Duration) -> bool {
        if self.events != Some(events) {
            self.events = Some(events);
            self.moved = at;
            return false;
        }
        at.saturating_sub(self.moved) >= IDLE
    }
}

/// Turns one channel's pages into readings: watts come from the counters
/// between two pages heard (a page lost costs nothing) — accumulated power,
/// or the pedals' crank torque when they send it — else from the page's
/// own instantaneous value.
#[derive(Debug, Default)]
pub struct Decoder {
    /// Events, accumulated power and when, of the last power page.
    power: Option<(u8, u16, Duration)>,
    /// Events, revolutions, period, torque and when, of the last crank
    /// torque page.
    crank: Option<(u8, u8, u16, u16, Duration)>,
    power_still: Still,
    crank_still: Still,
    /// The pedals send crank torque: their watts come from it.
    torque: bool,
}

impl Decoder {
    /// The reading in `page` from a device of type `kind`, if any.
    pub fn reading(&mut self, kind: u8, at: Duration, p: &[u8; 8]) -> Option<Reading> {
        let mut r = Reading { at, ..Reading::default() };
        match (kind, p[0]) {
            (KIND_FEC, 0x19) => {
                let instant = u16::from(p[5]) | (u16::from(p[6] & 0x0F) << 8);
                r.watts = self.watts(p[1], u16::from_le_bytes([p[3], p[4]]), at, (instant != 0xFFF).then_some(instant));
                r.cadence_rpm = (p[2] != 0xFF).then_some(p[2]);
                r.reports_on_target = Some(p[7] & 0x0F == 0);
            }
            (KIND_FEC, 0x10) => {
                let speed = u16::from_le_bytes([p[4], p[5]]);
                r.speed_kmh = (speed != 0xFFFF).then(|| f32::from(speed) * 0.0036);
            }
            (KIND_FEC, 0x31) => r.reported_target = Some(u16::from_le_bytes([p[6], p[7]]) / 4),
            (KIND_POWER, 0x10) => {
                let instant = u16::from_le_bytes([p[6], p[7]]);
                let watts = self.watts(p[1], u16::from_le_bytes([p[4], p[5]]), at, Some(instant));
                // Pedals that send crank torque are counted by it alone (it
                // says standing still too).
                r.watts = match (self.torque, self.power_still.still(p[1], at)) {
                    (true, _) => None,
                    (false, true) => Some(0),
                    (false, false) => watts,
                };
                r.cadence_rpm = (p[3] != 0xFF).then_some(p[3]);
                r.balance_right = (p[2] != 0xFF && p[2] & 0x80 != 0).then_some(p[2] & 0x7F);
            }
            (KIND_POWER, 0x12) => {
                self.torque = true;
                let counted = self.crank(p, at);
                (r.watts, r.cadence_rpm) = match (self.crank_still.still(p[1], at), counted) {
                    (true, _) => (Some(0), Some(0)),
                    (false, Some(counted)) => counted,
                    (false, None) => return None,
                };
            }
            (KIND_HEART, _) => r.heart_rate = Some(p[7]).filter(|&b| b > 0),
            _ => return None,
        }
        Some(r)
    }

    /// Δ accumulated power / Δ events since the last page, when the two are
    /// comparable and the result is a sane one; the instantaneous value
    /// otherwise (the first page, a long gap, a counter reset).
    fn watts(&mut self, events: u8, accumulated: u16, at: Duration, instant: Option<u16>) -> Option<u16> {
        let counted = match self.power.replace((events, accumulated, at)) {
            Some((e, a, then)) if comparable(then, at, events.wrapping_sub(e)) => {
                accumulated.wrapping_sub(a) / u16::from(events.wrapping_sub(e))
            }
            _ => return instant,
        };
        // A sensor that restarted counts from nothing: far off its own
        // instantaneous value (when it gives one) is no average.
        let near = |i: u16| counted.abs_diff(i) <= i.max(300);
        match instant {
            _ if counted > MAX_WATTS => instant,
            Some(i) if !near(i) => Some(i),
            _ => Some(counted),
        }
    }

    /// Watts and rpm between two crank torque pages: P = 128π · Δtorque /
    /// Δperiod (the ANT+ power profile), when they are comparable; each
    /// None when it is not sane.
    fn crank(&mut self, p: &[u8; 8], at: Duration) -> Option<(Option<u16>, Option<u8>)> {
        let now = (p[1], p[2], u16::from_le_bytes([p[4], p[5]]), u16::from_le_bytes([p[6], p[7]]), at);
        let (e, revs, period, torque, then) = self.crank.replace(now)?;
        let dperiod = f32::from(now.2.wrapping_sub(period));
        if !comparable(then, at, now.0.wrapping_sub(e)) || dperiod == 0.0 {
            return None;
        }
        let watts = 128.0 * std::f32::consts::PI * f32::from(now.3.wrapping_sub(torque)) / dperiod;
        let rpm = f32::from(now.1.wrapping_sub(revs)) * 60.0 * 2048.0 / dperiod;
        Some((Some(watts.round() as u16).filter(|&w| w <= MAX_WATTS), Some(rpm.round()).filter(|&r| r <= MAX_RPM).map(|r| r as u8)))
    }
}

/// What a page says about the device; true when it is known who it is
/// (maker and model) and this told something new.
pub fn absorb(identity: &mut Identity, kind: u8, p: &[u8; 8]) -> bool {
    let before = identity.clone();
    match (kind, p[0] & 0x7F, p[0]) {
        // A strap predates the common pages: its page number carries a
        // toggle bit, and 2/3 are its own.
        (KIND_HEART, 0x02, _) => identity.ant_manufacturer = Some(u16::from(p[1])),
        (KIND_HEART, 0x03, _) => {
            identity.firmware = Some(p[2].to_string());
            identity.ant_model = Some(u16::from(p[3]));
        }
        (KIND_HEART, _, _) => {}
        (_, _, 0x50) => {
            identity.ant_manufacturer = Some(u16::from_le_bytes([p[4], p[5]]));
            identity.ant_model = Some(u16::from_le_bytes([p[6], p[7]]));
        }
        (_, _, 0x51) => identity.firmware = Some(firmware(p[2], p[3])),
        // FE-C capabilities: byte 7 bit 1, target power (ERG).
        (KIND_FEC, _, PAGE_CAPABILITIES) => identity.target_power = Some(p[7] & 0x02 != 0),
        _ => {}
    }
    if let (Some(maker), None) = (identity.ant_manufacturer, &identity.manufacturer) {
        identity.manufacturer = maker_name(maker).map(str::to_string);
    }
    *identity != before && identity.ant_manufacturer.is_some() && identity.ant_model.is_some()
}

/// Who a device is before it said more: its number.
pub fn identity_of(sender: Sender) -> Identity {
    Identity { ant_device_number: Some(sender.number), ..Identity::new(Transport::Ant) }
}

/// Main revision 37 reads 3.7; a supplemental byte adds two digits (37 +
/// 05 → 3.705).
fn firmware(supplemental: u8, main: u8) -> String {
    match supplemental {
        0xFF => format!("{:.1}", f32::from(main) / 10.0),
        s => format!("{:.3}", (u32::from(main) * 100 + u32::from(s)) as f32 / 1000.0),
    }
}

/// Makers seen on trainers, pedals and straps.
pub fn maker_name(id: u16) -> Option<&'static str> {
    Some(match id {
        1 => "Garmin",
        32 => "Wahoo",
        86 => "Elite",
        89 => "Tacx",
        263 => "Favero",
        _ => return None,
    })
}

/// The FE-C page a control goes as: 0x31 target power (0.25 W), 0x30
/// basic resistance (0.5 %); letting go is 0 % resistance.
pub fn control_page(control: Control) -> [u8; 8] {
    match control {
        Control::TargetPower(w) => {
            let [lo, hi] = ((u32::from(w) * 4).min(0xFFFF) as u16).to_le_bytes();
            [0x31, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, lo, hi]
        }
        Control::Resistance(pct) => [0x30, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, pct.min(100) * 2],
        Control::Release => [0x30, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0],
    }
}

/// FE-C page 0x36: what the trainer can be told to do.
pub const PAGE_CAPABILITIES: u8 = 0x36;

/// Common page 0x46: ask the device to broadcast `page` four times (four,
/// because a third of a trainer's packets do not arrive).
pub fn request_page(page: u8) -> [u8; 8] {
    [0x46, 0xFF, 0xFF, 0xFF, 0xFF, 0x04, page, 0x01]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    /// FE-C 0x19: events, cadence, accumulated power, instantaneous, flags.
    fn fec(events: u8, cadence: u8, acc: u16, instant: u16, flag: u8) -> [u8; 8] {
        let [a0, a1] = acc.to_le_bytes();
        [0x19, events, cadence, a0, a1, (instant & 0xFF) as u8, (instant >> 8) as u8 | 0x30, flag]
    }

    #[test]
    fn a_data_message_names_its_sender() {
        // Channel 2, page, extended flag, number 0x39EE (14830) with the
        // transmission type's upper nibble 1 → 0x139EE.
        let data = [2, 0x19, 1, 2, 3, 4, 5, 6, 7, 0x80, 0xEE, 0x39, 0x11, 0x15];
        let p = packet(&data).unwrap();
        assert_eq!((p.channel, p.page[0]), (2, 0x19));
        assert_eq!(p.from, Some(Sender { number: 0x139EE, kind: 17 }));
        assert_eq!(packet(&data[..9]).unwrap().from, None, "no extended part");
        assert_eq!(packet(&data[..10]).unwrap().from, None, "a flag, the part cut");
        assert_eq!(packet(&[0, 0x19, 1, 2, 3, 4, 5, 6, 7, 0x00, 1, 2, 3, 4]).unwrap().from, None, "flag off");
        assert_eq!(packet(&data[..5]), None);
    }

    #[test]
    fn trainer_pages_read_as_power_cadence_speed_and_its_target() {
        let mut d = Decoder::default();
        let first = d.reading(KIND_FEC, at(1), &fec(10, 85, 1000, 150, 0x30)).unwrap();
        assert_eq!((first.watts, first.cadence_rpm, first.reports_on_target), (Some(150), Some(85), Some(true)), "first: its own value");
        // Two pages later (one lost): 2 events, 320 W more → 160 W each.
        let next = d.reading(KIND_FEC, at(2), &fec(12, 86, 1320, 170, 0x31)).unwrap();
        assert_eq!((next.watts, next.reports_on_target), (Some(160), Some(false)));
        // The counter wrapped: still right.
        let mut w = Decoder::default();
        w.reading(KIND_FEC, at(1), &fec(255, 85, 65500, 150, 0));
        assert_eq!(w.reading(KIND_FEC, at(2), &fec(1, 85, 264, 150, 0)).unwrap().watts, Some(150));
        // A long gap or standing still: the page's value; none known: none.
        assert_eq!(w.reading(KIND_FEC, at(9), &fec(60, 0xFF, 9000, 0xFFF, 0)).unwrap(), Reading {
            at: at(9),
            reports_on_target: Some(true),
            ..Reading::default()
        });
        // 16 events apart is counted, 17 is the page's own…
        let mut window = Decoder::default();
        window.reading(KIND_FEC, at(1), &fec(0, 85, 0, 150, 0));
        assert_eq!(window.reading(KIND_FEC, at(5), &fec(16, 85, 3200, 150, 0)).unwrap().watts, Some(200));
        assert_eq!(window.reading(KIND_FEC, at(9), &fec(33, 85, 6600, 150, 0)).unwrap().watts, Some(150));
        // …so is a page more than 20 s on (the count may have wrapped
        // round in between) — 20 s is still counted — and a sum no rider
        // makes (a restart).
        assert_eq!(window.reading(KIND_FEC, at(29), &fec(34, 85, 6800, 150, 0)).unwrap().watts, Some(200), "20 s on");
        assert_eq!(window.reading(KIND_FEC, at(50), &fec(35, 85, 7000, 150, 0)).unwrap().watts, Some(150), "21 s on");
        assert_eq!(window.reading(KIND_FEC, at(51), &fec(36, 85, 60000, 150, 0)).unwrap().watts, Some(150));
        // Without a value of its own: 4000 W is the most counted.
        let mut lone = Decoder::default();
        lone.reading(KIND_FEC, at(1), &fec(0, 85, 0, 0xFFF, 0));
        assert_eq!(lone.reading(KIND_FEC, at(2), &fec(1, 85, 4000, 0xFFF, 0)).unwrap().watts, Some(4000));
        assert_eq!(lone.reading(KIND_FEC, at(3), &fec(2, 85, 8001, 0xFFF, 0)).unwrap().watts, None);
        // A sensor that restarted, its average plausible: far off what the
        // page says, so the page's own value.
        let mut restarted = Decoder::default();
        restarted.reading(KIND_FEC, at(1), &fec(250, 85, 60000, 200, 0));
        assert_eq!(restarted.reading(KIND_FEC, at(2), &fec(3, 85, 600, 200, 0)).unwrap().watts, Some(200));
        // Within 300 W of it, or within its own size, the average stands
        // (an easing off, a sprint's first seconds).
        let mut eased = Decoder::default();
        eased.reading(KIND_FEC, at(1), &fec(0, 85, 0, 100, 0));
        assert_eq!(eased.reading(KIND_FEC, at(2), &fec(1, 85, 350, 100, 0)).unwrap().watts, Some(350));
        let mut sprint = Decoder::default();
        sprint.reading(KIND_FEC, at(1), &fec(0, 85, 0, 900, 0));
        assert_eq!(sprint.reading(KIND_FEC, at(2), &fec(1, 85, 450, 900, 0)).unwrap().watts, Some(450));
        assert_eq!(window.reading(KIND_FEC, at(31), &fec(36, 85, 60200, 150, 0x34)).unwrap().reports_on_target, Some(false), "any target status but 0");
        let speed = d.reading(KIND_FEC, at(3), &[0x10, 25, 0, 0, 0x10, 0x27, 0, 0]).unwrap();
        assert!((speed.speed_kmh.unwrap() - 36.0).abs() < 0.01, "10000 mm/s = 36 km/h");
        assert_eq!(d.reading(KIND_FEC, at(3), &[0x10, 25, 0, 0, 0xFF, 0xFF, 0, 0]).unwrap().speed_kmh, None);
        assert_eq!(d.reading(KIND_FEC, at(3), &control_page(Control::TargetPower(150))).unwrap().reported_target, Some(150));
        assert_eq!(d.reading(KIND_FEC, at(3), &[0x36; 8]), None, "a page with no reading");
    }

    #[test]
    fn pedal_pages_read_as_power_cadence_and_balance() {
        let mut d = Decoder::default();
        let power = |events: u8, acc: u16, instant: u16, balance: u8, cadence: u8| {
            let [a0, a1] = acc.to_le_bytes();
            let [i0, i1] = instant.to_le_bytes();
            [0x10, events, balance, cadence, a0, a1, i0, i1]
        };
        let r = d.reading(KIND_POWER, at(1), &power(5, 500, 210, 0x80 | 52, 90)).unwrap();
        assert_eq!((r.watts, r.cadence_rpm, r.balance_right), (Some(210), Some(90), Some(52)));
        let r = d.reading(KIND_POWER, at(2), &power(6, 700, 199, 52, 0xFF)).unwrap();
        assert_eq!((r.watts, r.cadence_rpm, r.balance_right), (Some(200), None, None), "one leg's share: not the right's");
        // Coasting: the count stays and the page may keep its last watts; a
        // little while, then none.
        assert_eq!(d.reading(KIND_POWER, at(4), &power(6, 700, 199, 0xFF, 0)).unwrap().watts, Some(199));
        assert_eq!(d.reading(KIND_POWER, at(5), &power(6, 700, 199, 0xFF, 0)).unwrap().watts, Some(0), "3 s still");
    }

    /// Crank torque: events, revolutions, period (1/2048 s), torque (1/32 N·m).
    fn torque(events: u8, revs: u8, period: u16, torque: u16) -> [u8; 8] {
        let [p0, p1] = period.to_le_bytes();
        let [t0, t1] = torque.to_le_bytes();
        [0x12, events, revs, 0xFF, p0, p1, t0, t1]
    }

    #[test]
    fn pedals_that_send_crank_torque_are_counted_by_it() {
        let mut d = Decoder::default();
        assert_eq!(d.reading(KIND_POWER, at(1), &torque(1, 1, 0, 0)), None, "nothing to compare yet");
        // One revolution in 0.75 s (1536/2048) at 1200/32 = 37.5 N·m:
        // 128π · 1200 / 1536 ≈ 314 W at 80 rpm.
        let r = d.reading(KIND_POWER, at(2), &torque(2, 2, 1536, 1200)).unwrap();
        assert_eq!((r.watts, r.cadence_rpm), (Some(314), Some(80)));
        // The power page now says the balance and cadence, not the watts —
        // not even a still count's 0 (the torque says standing still).
        let p = d.reading(KIND_POWER, at(2), &[0x10, 9, 0x80 | 50, 81, 0, 0, 44, 1]).unwrap();
        assert_eq!((p.watts, p.balance_right, p.cadence_rpm), (None, Some(50), Some(81)));
        assert_eq!(d.reading(KIND_POWER, at(6), &[0x10, 9, 0x80 | 50, 81, 0, 0, 44, 1]).unwrap().watts, None);
        // A torque no rider makes is no watts; the cadence stands.
        let wild = d.reading(KIND_POWER, at(3), &torque(3, 3, 3072, 60000)).unwrap();
        assert_eq!((wild.watts, wild.cadence_rpm), (None, Some(80)));
        // A revolution in a few ticks: no cadence either.
        let mut spun = Decoder::default();
        spun.reading(KIND_POWER, at(1), &torque(1, 1, 0, 0));
        let r = spun.reading(KIND_POWER, at(2), &torque(2, 2, 500, 100)).unwrap();
        assert_eq!((r.watts, r.cadence_rpm), (Some(80), None), "246 rpm");
        let r = spun.reading(KIND_POWER, at(3), &torque(3, 3, 1114, 200)).unwrap();
        assert_eq!(r.cadence_rpm, Some(200), "200 rpm, the most");
        // A page 20 s on, or with no time between: nothing said.
        assert_eq!(d.reading(KIND_POWER, at(30), &torque(4, 4, 4608, 61200)), None);
        assert_eq!(d.reading(KIND_POWER, at(31), &torque(5, 5, 4608, 62400)), None);
        // Still for 3 s: none.
        assert!(d.reading(KIND_POWER, at(32), &torque(6, 6, 6144, 63600)).unwrap().watts.is_some());
        let still = d.reading(KIND_POWER, at(35), &torque(6, 6, 6144, 63600)).unwrap();
        assert_eq!((still.watts, still.cadence_rpm), (Some(0), Some(0)));
    }

    #[test]
    fn a_strap_reads_its_heart_rate_whatever_the_page() {
        let mut d = Decoder::default();
        assert_eq!(d.reading(KIND_HEART, at(1), &[0x84, 0, 0, 0, 0, 0, 0, 141]).unwrap().heart_rate, Some(141));
        assert_eq!(d.reading(KIND_HEART, at(1), &[0x04, 0, 0, 0, 0, 0, 0, 0]).unwrap().heart_rate, None, "no contact");
        assert_eq!(d.reading(99, at(1), &[0; 8]), None, "a device of no kind here");
    }

    #[test]
    fn devices_tell_who_they_are() {
        let mut avanti = identity_of(Sender { number: 14830, kind: KIND_FEC });
        assert_eq!((avanti.transport, avanti.ant_device_number), (Transport::Ant, Some(14830)));
        assert!(!absorb(&mut avanti, KIND_FEC, &[0x51, 0xFF, 0xFF, 37, 1, 2, 3, 4]));
        assert_eq!(avanti.firmware.as_deref(), Some("3.7"));
        assert!(absorb(&mut avanti, KIND_FEC, &[0x50, 0xFF, 0xFF, 1, 86, 0, 45, 0]), "known now");
        assert_eq!((avanti.ant_manufacturer, avanti.ant_model, avanti.manufacturer.as_deref()), (Some(86), Some(45), Some("Elite")));
        assert!(!absorb(&mut avanti, KIND_FEC, &[0x50, 0xFF, 0xFF, 1, 86, 0, 45, 0]), "nothing new");
        assert!(absorb(&mut avanti, KIND_FEC, &[0x51, 0xFF, 5, 37, 1, 2, 3, 4]), "known, and more");
        assert_eq!(avanti.firmware.as_deref(), Some("3.705"));
        // 0x36 as the Avanti sends it: basic, target power, simulation.
        assert!(absorb(&mut avanti, KIND_FEC, &[0x36, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x07]), "it can hold watts");
        assert_eq!(avanti.target_power, Some(true));
        assert!(absorb(&mut avanti, KIND_FEC, &[0x36, 0xFF, 0xFF, 0xFF, 0xFF, 0xD0, 0x07, 0x05]), "it cannot");
        assert_eq!(avanti.target_power, Some(false));
        let mut pedals = identity_of(Sender { number: 2, kind: KIND_POWER });
        absorb(&mut pedals, KIND_POWER, &[0x36, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x07]);
        assert_eq!(pedals.target_power, None, "0x36 is a trainer's page only");

        let mut strap = identity_of(Sender { number: 7, kind: KIND_HEART });
        assert!(!absorb(&mut strap, KIND_HEART, &[0x82, 1, 0, 0, 0, 0, 0, 120]));
        assert!(absorb(&mut strap, KIND_HEART, &[0x83, 2, 9, 7, 0, 0, 0, 120]));
        assert_eq!((strap.ant_manufacturer, strap.ant_model, strap.firmware.as_deref()), (Some(1), Some(7), Some("9")));
        assert!(!absorb(&mut strap, KIND_HEART, &[0x50, 0, 0, 0, 9, 0, 9, 0]), "a strap's 0x50 is a heart rate");
        assert_eq!((strap.ant_manufacturer, strap.ant_model), (Some(1), Some(7)));
        let mut odd = identity_of(Sender { number: 1, kind: KIND_POWER });
        absorb(&mut odd, KIND_POWER, &[0x50, 0xFF, 0xFF, 1, 0x34, 0x12, 1, 0]);
        assert_eq!((odd.ant_manufacturer, odd.manufacturer.as_deref()), (Some(0x1234), None), "a maker without a name here");
        assert!(!absorb(&mut odd, KIND_POWER, &[0x12; 8]));
        let names: Vec<Option<&str>> = [1, 32, 86, 89, 263, 7].into_iter().map(maker_name).collect();
        assert_eq!(names, vec![Some("Garmin"), Some("Wahoo"), Some("Elite"), Some("Tacx"), Some("Favero"), None]);
    }

    #[test]
    fn controls_go_as_fe_c_pages() {
        assert_eq!(control_page(Control::TargetPower(150)), [0x31, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x58, 0x02]);
        assert_eq!(control_page(Control::TargetPower(u16::MAX))[6..], [0xFF, 0xFF], "capped");
        assert_eq!(control_page(Control::Resistance(40))[7], 80);
        assert_eq!(control_page(Control::Resistance(250))[7], 200, "above 100 counts as 100");
        assert_eq!(control_page(Control::Release), [0x30, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0]);
        assert_eq!(request_page(PAGE_CAPABILITIES), [0x46, 0xFF, 0xFF, 0xFF, 0xFF, 0x04, 0x36, 0x01]);
        assert_eq!((period(KIND_FEC), period(KIND_POWER), period(KIND_HEART)), (8192, 8182, 8070));
    }
}
