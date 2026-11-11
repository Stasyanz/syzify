//! A FIT encoder for rides recorded in Syzify: just enough of the FIT
//! protocol (Garmin's FIT SDK, protocol 2.0) to write an activity that
//! Syzify's own import, Garmin Connect and Strava read — file_id,
//! device_info per sensor, timer events, a record per second, laps, a
//! session and the activity. No dependency: fixed little-endian
//! definitions, invalid values for what a second does not carry, and the
//! SDK's CRC-16.

use serde::{Deserialize, Serialize};

/// Seconds between the Unix epoch and the FIT epoch (1989-12-31 00:00 UTC).
pub const FIT_EPOCH: i64 = 631_065_600;

// Global message numbers.
const FILE_ID: u16 = 0;
const SESSION: u16 = 18;
const LAP: u16 = 19;
const RECORD: u16 = 20;
const EVENT: u16 = 21;
const DEVICE_INFO: u16 = 23;
const ACTIVITY: u16 = 34;

// Base types.
const ENUM: u8 = 0x00;
const UINT8: u8 = 0x02;
const UINT16: u8 = 0x84;
const UINT32: u8 = 0x86;
const UINT32Z: u8 = 0x8C;
const STRING: u8 = 0x07;

/// The file's creator as importers show it (file_id product_name).
pub const PRODUCT_NAME: &str = "Syzify";
const SOURCE_ANTPLUS: u8 = 1;
const SOURCE_BLE: u8 = 3;

// Profile values.
const FILE_ACTIVITY: u8 = 4;
const MANUFACTURER_DEVELOPMENT: u16 = 255;
const SPORT_CYCLING: u8 = 2;
const EVENT_TIMER: u8 = 0;
const EVENT_ACTIVITY: u8 = 26;
const EVENT_LAP: u8 = 9;
const EVENT_SESSION: u8 = 8;
const EVENT_TYPE_START: u8 = 0;
const EVENT_TYPE_STOP: u8 = 1;
const EVENT_TYPE_STOP_ALL: u8 = 4;
const ACTIVITY_MANUAL: u8 = 0;
const LAP_TRIGGER_MANUAL: u8 = 0;
const LAP_TRIGGER_SESSION_END: u8 = 7;

/// Device names are written in this many bytes, NUL-padded.
const NAME_BYTES: u8 = 20;

/// FIT sub-sport of a ride on a trainer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubSport {
    /// A trainer ride: free ride or ERG.
    IndoorCycling = 6,
    /// A ride through a virtual world or route.
    VirtualActivity = 58,
}

/// What a sensor is. FIT names it in the ANT+ device types, or for a BLE
/// sensor in its own BLE enum (`ble_device_type`): see `device_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    BikePower = 11,
    FitnessEquipment = 17,
    HeartRate = 120,
}

/// How a sensor was reached (FIT device_info source_type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Radio {
    Ant,
    Ble,
}

/// A sensor that took part, for device_info.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub kind: DeviceKind,
    /// FIT manufacturer id when known (86 = Elite), development otherwise.
    pub manufacturer: Option<u16>,
    pub name: Option<String>,
    /// What tells this unit from another of its kind: the ANT+ device
    /// number, or a hash of the BLE address. Importers list a sensor only
    /// with one (gear rules key on it).
    #[serde(default)]
    pub serial: Option<u32>,
    #[serde(default)]
    pub radio: Option<Radio>,
}

/// One second of the ride.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    /// Seconds since the start.
    pub t: u32,
    pub watts: Option<u16>,
    pub cadence_rpm: Option<u8>,
    pub heart_rate: Option<u8>,
    pub speed_mps: Option<f32>,
    /// Metres since the start.
    pub distance_m: Option<f32>,
}

/// What the encoder writes.
#[derive(Debug, Clone, PartialEq)]
pub struct Ride {
    /// Unix seconds (UTC) of the first sample.
    pub start_unix: i64,
    pub sub_sport: SubSport,
    pub samples: Vec<Sample>,
    /// Where each lap after the first begins: indexes into `samples`.
    pub lap_starts: Vec<usize>,
    pub devices: Vec<Device>,
    /// Paused spans `[stop, start)` in seconds since the start, in order:
    /// no samples in them, the timer stopped.
    pub pauses: Vec<(u32, u32)>,
}

/// Indexes into `samples` of the laps that begin at the seconds `at`: the
/// first sample at or after each. Seconds are not places once a pause or
/// a lost journal line leaves a gap.
pub fn lap_indexes(samples: &[Sample], at: &[usize]) -> Vec<usize> {
    at.iter().map(|&t| samples.partition_point(|s| (s.t as usize) < t)).collect()
}

/// Seconds of `[start, end)` the timer ran: the span less its pauses.
fn timer_secs(start: u32, end: u32, pauses: &[(u32, u32)]) -> u32 {
    let paused: u32 = pauses.iter().map(|&(a, b)| b.min(end).saturating_sub(a.max(start))).sum();
    (end - start).saturating_sub(paused)
}

/// The FIT SDK's CRC-16.
fn crc16(crc: u16, byte: u8) -> u16 {
    const TABLE: [u16; 16] = [
        0x0000, 0xCC01, 0xD801, 0x1400, 0xF001, 0x3C00, 0x2800, 0xE401, 0xA001, 0x6C00, 0x7800, 0xB401, 0x5000, 0x9C01,
        0x8801, 0x4400,
    ];
    let mut crc = crc;
    let tmp = TABLE[(crc & 0xF) as usize];
    crc = (crc >> 4) & 0x0FFF;
    crc = crc ^ tmp ^ TABLE[(byte & 0xF) as usize];
    let tmp = TABLE[(crc & 0xF) as usize];
    crc = (crc >> 4) & 0x0FFF;
    crc ^ tmp ^ TABLE[((byte >> 4) & 0xF) as usize]
}

fn crc_of(bytes: &[u8]) -> u16 {
    bytes.iter().fold(0, |crc, &b| crc16(crc, b))
}

/// A field value; `None` writes the base type's invalid value.
#[derive(Debug, Clone)]
enum V {
    U8(Option<u8>),
    U16(Option<u16>),
    U32(Option<u32>),
    /// uint32z: 0 is the invalid value.
    U32Z(Option<u32>),
    Str(Option<String>),
}

/// device_info field 1, which FIT reads in the enum the source type picks:
/// ANT+ device types, or for BLE its own (heart_rate 1, bike_power 2,
/// bike_trainer 7).
fn device_type(d: &Device) -> u8 {
    match (d.radio, d.kind) {
        (Some(Radio::Ble), DeviceKind::HeartRate) => 1,
        (Some(Radio::Ble), DeviceKind::BikePower) => 2,
        (Some(Radio::Ble), DeviceKind::FitnessEquipment) => 7,
        (_, kind) => kind as u8,
    }
}

/// The string cut to fit `NAME_BYTES` with its NUL, on a character boundary.
fn fitted(s: &str) -> &str {
    let mut end = s.len().min(NAME_BYTES as usize - 1);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

struct Writer {
    data: Vec<u8>,
}

impl Writer {
    fn define(&mut self, local: u8, global: u16, fields: &[(u8, u8, u8)]) {
        self.data.push(0x40 | local);
        self.data.push(0); // reserved
        self.data.push(0); // little endian
        self.data.extend_from_slice(&global.to_le_bytes());
        self.data.push(fields.len() as u8);
        for &(num, size, base) in fields {
            self.data.extend_from_slice(&[num, size, base]);
        }
    }

    fn write(&mut self, local: u8, values: &[V]) {
        self.data.push(local);
        for v in values {
            match v {
                V::U8(x) => self.data.push(x.unwrap_or(0xFF)),
                V::U16(x) => self.data.extend_from_slice(&x.unwrap_or(0xFFFF).to_le_bytes()),
                V::U32(x) => self.data.extend_from_slice(&x.unwrap_or(0xFFFF_FFFF).to_le_bytes()),
                V::U32Z(x) => self.data.extend_from_slice(&x.unwrap_or(0).to_le_bytes()),
                V::Str(s) => {
                    let mut bytes = fitted(s.as_deref().unwrap_or("")).as_bytes().to_vec();
                    bytes.resize(NAME_BYTES as usize, 0);
                    self.data.extend_from_slice(&bytes);
                }
            }
        }
    }
}

/// FIT time of a Unix time; FIT cannot say anything before its epoch.
fn fit_time(unix: i64) -> u32 {
    (unix - FIT_EPOCH).clamp(0, u32::MAX as i64 - 1) as u32
}

fn scaled(x: Option<f32>, scale: f32, max: u32) -> Option<u32> {
    x.filter(|v| v.is_finite() && *v >= 0.0).map(|v| ((v * scale).round() as u64).min(max as u64) as u32)
}

/// Totals of a run of samples, for a lap or the session.
struct Totals {
    start: u32,
    end: u32,
    distance: Option<f32>,
    avg_speed: Option<f32>,
    max_speed: Option<f32>,
    avg_hr: Option<u8>,
    max_hr: Option<u8>,
    avg_cadence: Option<u8>,
    max_cadence: Option<u8>,
    avg_power: Option<u16>,
    max_power: Option<u16>,
}

fn mean<T: Copy + Into<f64>>(values: &[T]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().map(|&v| v.into()).sum::<f64>() / values.len() as f64)
}

/// Totals of `samples`; `base` is the distance where they begin (the end of
/// the previous lap, 0 for the ride), so laps add up to the session.
fn totals(samples: &[Sample], base: f32) -> Totals {
    let first = samples.first().map_or(0, |s| s.t);
    let last = samples.last().map_or(0, |s| s.t);
    let watts: Vec<u16> = samples.iter().filter_map(|s| s.watts).filter(|&w| w != u16::MAX).collect();
    let hr: Vec<u8> = samples.iter().filter_map(|s| s.heart_rate).filter(|&h| h != u8::MAX).collect();
    // Coasting is no cadence: averages are over pedalling seconds, as on a
    // head unit.
    let cadence: Vec<u8> = samples.iter().filter_map(|s| s.cadence_rpm).filter(|&c| c > 0 && c != u8::MAX).collect();
    let speed: Vec<f32> = samples.iter().filter_map(|s| s.speed_mps).collect();
    let distance = samples.iter().rev().find_map(|s| s.distance_m).map(|d| (d - base).max(0.0));
    Totals {
        start: first,
        // A sample covers the second after it.
        end: last + 1,
        distance,
        avg_speed: mean(&speed).map(|v| v as f32),
        max_speed: speed.iter().copied().reduce(f32::max),
        avg_hr: mean(&hr).map(|v| v.round() as u8),
        max_hr: hr.iter().copied().max(),
        avg_cadence: mean(&cadence).map(|v| v.round() as u8),
        max_cadence: cadence.iter().copied().max(),
        avg_power: mean(&watts).map(|v| v.round() as u16),
        max_power: watts.iter().copied().max(),
    }
}

/// The ride as a FIT activity file.
pub fn encode(ride: &Ride) -> Vec<u8> {
    let at = |t: u32| fit_time(ride.start_unix + t as i64);
    // The timeline starts at the first second recorded (a journal that
    // missed its first writes starts later than 0).
    let first = ride.samples.first().map_or(0, |s| s.t);
    let mut w = Writer { data: Vec::new() };

    w.define(0, FILE_ID, &[(0, 1, ENUM), (1, 2, UINT16), (2, 2, UINT16), (4, 4, UINT32), (8, NAME_BYTES, STRING)]);
    w.write(
        0,
        &[
            V::U8(Some(FILE_ACTIVITY)),
            V::U16(Some(MANUFACTURER_DEVELOPMENT)),
            V::U16(Some(0)),
            V::U32(Some(at(first))),
            V::Str(Some(PRODUCT_NAME.into())),
        ],
    );

    w.define(
        1,
        DEVICE_INFO,
        &[
            (253, 4, UINT32),
            (0, 1, UINT8),
            (1, 1, UINT8),
            (2, 2, UINT16),
            (3, 4, UINT32Z),
            (25, 1, ENUM),
            (27, NAME_BYTES, STRING),
        ],
    );
    for (i, d) in ride.devices.iter().enumerate() {
        w.write(
            1,
            &[
                V::U32(Some(at(first))),
                // Index 0 is the creator (Syzify): sensors from 1.
                V::U8(Some(i as u8 + 1)),
                V::U8(Some(device_type(d))),
                // Unknown is left out, not "development": that is the
                // creator's, and labels would say so.
                V::U16(d.manufacturer),
                V::U32Z(d.serial.filter(|&s| s != 0)),
                V::U8(d.radio.map(|r| match r {
                    Radio::Ant => SOURCE_ANTPLUS,
                    Radio::Ble => SOURCE_BLE,
                })),
                V::Str(d.name.clone()),
            ],
        );
    }

    w.define(2, EVENT, &[(253, 4, UINT32), (0, 1, ENUM), (1, 1, ENUM)]);
    w.write(2, &[V::U32(Some(at(first))), V::U8(Some(EVENT_TIMER)), V::U8(Some(EVENT_TYPE_START))]);

    let end = ride.samples.last().map_or(0, |s| s.t + 1);
    // Only pauses between samples stop the timer; one at the edge is no
    // more than where the ride starts or ends.
    let pauses: Vec<(u32, u32)> = ride.pauses.iter().copied().filter(|&(a, b)| a < b && a > first && b < end).collect();
    let mut pending = pauses.iter().peekable();

    w.define(3, RECORD, &[(253, 4, UINT32), (7, 2, UINT16), (4, 1, UINT8), (3, 1, UINT8), (6, 2, UINT16), (5, 4, UINT32)]);
    for s in &ride.samples {
        while let Some(&&(stop, start)) = pending.peek() {
            if start > s.t {
                break;
            }
            pending.next();
            w.write(2, &[V::U32(Some(at(stop))), V::U8(Some(EVENT_TIMER)), V::U8(Some(EVENT_TYPE_STOP_ALL))]);
            w.write(2, &[V::U32(Some(at(start))), V::U8(Some(EVENT_TIMER)), V::U8(Some(EVENT_TYPE_START))]);
        }
        w.write(
            3,
            &[
                V::U32(Some(at(s.t))),
                V::U16(s.watts.filter(|&p| p != u16::MAX)),
                V::U8(s.cadence_rpm.filter(|&c| c != u8::MAX)),
                V::U8(s.heart_rate.filter(|&h| h != u8::MAX)),
                V::U16(scaled(s.speed_mps, 1000.0, 0xFFFE).map(|v| v as u16)),
                V::U32(scaled(s.distance_m, 100.0, 0xFFFF_FFFE)),
            ],
        );
    }

    w.write(2, &[V::U32(Some(at(end))), V::U8(Some(EVENT_TIMER)), V::U8(Some(EVENT_TYPE_STOP_ALL))]);

    // Laps: the bounds, clipped to the samples, in order, never empty.
    let mut bounds: Vec<usize> = ride.lap_starts.iter().copied().filter(|&i| i > 0 && i < ride.samples.len()).collect();
    bounds.sort_unstable();
    bounds.dedup();
    bounds.insert(0, 0);
    bounds.push(ride.samples.len());
    let laps: Vec<&[Sample]> = bounds.windows(2).map(|b| &ride.samples[b[0]..b[1]]).filter(|l| !l.is_empty()).collect();
    let laps: Vec<&[Sample]> = if laps.is_empty() { vec![&ride.samples[..]] } else { laps };

    let summary = |w: &mut Writer, local: u8, t: &Totals, extra: Vec<V>| {
        let mut values = vec![
            V::U32(Some(at(t.end))),
            V::U32(Some(at(t.start))),
            V::U32(Some((t.end - t.start) * 1000)),
            V::U32(Some(timer_secs(t.start, t.end, &pauses) * 1000)),
            V::U32(scaled(t.distance, 100.0, 0xFFFF_FFFE)),
            V::U16(scaled(t.avg_speed, 1000.0, 0xFFFE).map(|v| v as u16)),
            V::U16(scaled(t.max_speed, 1000.0, 0xFFFE).map(|v| v as u16)),
            V::U8(t.avg_hr),
            V::U8(t.max_hr),
            V::U8(t.avg_cadence),
            V::U8(t.max_cadence),
            V::U16(t.avg_power),
            V::U16(t.max_power),
        ];
        values.extend(extra);
        w.write(local, &values);
    };

    // Lap and session share their first fields, numbered apart.
    w.define(
        4,
        LAP,
        &[
            (253, 4, UINT32),
            (2, 4, UINT32),
            (7, 4, UINT32),
            (8, 4, UINT32),
            (9, 4, UINT32),
            (13, 2, UINT16),
            (14, 2, UINT16),
            (15, 1, UINT8),
            (16, 1, UINT8),
            (17, 1, UINT8),
            (18, 1, UINT8),
            (19, 2, UINT16),
            (20, 2, UINT16),
            (254, 2, UINT16),
            (0, 1, ENUM),
            (1, 1, ENUM),
            (24, 1, ENUM),
            (25, 1, ENUM),
            (39, 1, ENUM),
        ],
    );
    let mut base = 0.0;
    for (i, lap) in laps.iter().enumerate() {
        let trigger = if i + 1 == laps.len() { LAP_TRIGGER_SESSION_END } else { LAP_TRIGGER_MANUAL };
        let t = totals(lap, base);
        base = lap.iter().rev().find_map(|s| s.distance_m).unwrap_or(base);
        summary(
            &mut w,
            4,
            &t,
            vec![
                V::U16(Some(i as u16)),
                V::U8(Some(EVENT_LAP)),
                V::U8(Some(EVENT_TYPE_STOP)),
                V::U8(Some(trigger)),
                V::U8(Some(SPORT_CYCLING)),
                V::U8(Some(ride.sub_sport as u8)),
            ],
        );
    }

    w.define(
        5,
        SESSION,
        &[
            (253, 4, UINT32),
            (2, 4, UINT32),
            (7, 4, UINT32),
            (8, 4, UINT32),
            (9, 4, UINT32),
            (14, 2, UINT16),
            (15, 2, UINT16),
            (16, 1, UINT8),
            (17, 1, UINT8),
            (18, 1, UINT8),
            (19, 1, UINT8),
            (20, 2, UINT16),
            (21, 2, UINT16),
            (254, 2, UINT16),
            (0, 1, ENUM),
            (1, 1, ENUM),
            (5, 1, ENUM),
            (6, 1, ENUM),
            (25, 2, UINT16),
            (26, 2, UINT16),
        ],
    );
    summary(
        &mut w,
        5,
        &totals(&ride.samples, 0.0),
        vec![
            V::U16(Some(0)),
            V::U8(Some(EVENT_SESSION)),
            V::U8(Some(EVENT_TYPE_STOP)),
            V::U8(Some(SPORT_CYCLING)),
            V::U8(Some(ride.sub_sport as u8)),
            V::U16(Some(0)),
            V::U16(Some(laps.len() as u16)),
        ],
    );

    w.define(6, ACTIVITY, &[(253, 4, UINT32), (0, 4, UINT32), (1, 2, UINT16), (2, 1, ENUM), (3, 1, ENUM), (4, 1, ENUM)]);
    w.write(
        6,
        &[
            V::U32(Some(at(end))),
            V::U32(Some(timer_secs(first, end, &pauses) * 1000)),
            V::U16(Some(1)),
            V::U8(Some(ACTIVITY_MANUAL)),
            V::U8(Some(EVENT_ACTIVITY)),
            V::U8(Some(EVENT_TYPE_STOP)),
        ],
    );

    let mut out = Vec::with_capacity(w.data.len() + 16);
    out.push(14);
    out.push(0x20); // protocol 2.0
    out.extend_from_slice(&2132u16.to_le_bytes()); // profile 21.32
    out.extend_from_slice(&(w.data.len() as u32).to_le_bytes());
    out.extend_from_slice(b".FIT");
    let header_crc = crc_of(&out);
    out.extend_from_slice(&header_crc.to_le_bytes());
    out.extend_from_slice(&w.data);
    let file_crc = crc_of(&out);
    out.extend_from_slice(&file_crc.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use fitparser::profile::MesgNum;
    use fitparser::{FitDataRecord, Value};

    /// 2026-11-11 18:00:00 UTC.
    const START: i64 = 1_794_420_000;

    fn ride(seconds: u32) -> Ride {
        let samples = (0..seconds)
            .map(|t| Sample {
                t,
                watts: Some(150 + (t % 10) as u16),
                cadence_rpm: Some(85),
                heart_rate: Some(120 + (t / 60) as u8),
                speed_mps: Some(8.5),
                distance_m: Some(8.5 * t as f32),
            })
            .collect();
        Ride {
            start_unix: START,
            sub_sport: SubSport::IndoorCycling,
            samples,
            lap_starts: vec![],
            pauses: vec![],
            devices: vec![
                Device {
                    kind: DeviceKind::FitnessEquipment,
                    manufacturer: Some(86),
                    name: Some("Elite Avanti".into()),
                    serial: Some(14830),
                    radio: Some(Radio::Ant),
                },
                Device { kind: DeviceKind::HeartRate, manufacturer: None, name: None, serial: None, radio: Some(Radio::Ble) },
            ],
        }
    }

    fn of(records: &[FitDataRecord], kind: MesgNum) -> Vec<&FitDataRecord> {
        records.iter().filter(|r| r.kind() == kind).collect()
    }

    fn field<'a>(r: &'a FitDataRecord, name: &str) -> Option<&'a Value> {
        r.fields().iter().find(|f| f.name() == name).map(|f| f.value())
    }

    fn number(v: Option<&Value>) -> Option<f64> {
        match v? {
            Value::UInt8(x) => Some(*x as f64),
            Value::UInt16(x) => Some(*x as f64),
            Value::UInt32(x) => Some(*x as f64),
            Value::Float64(x) => Some(*x),
            Value::Float32(x) => Some(*x as f64),
            _ => None,
        }
    }

    #[test]
    fn the_crc_matches_the_sdk() {
        // The CRC-16/ARC check value, which the FIT SDK's algorithm is.
        assert_eq!(crc_of(b"123456789"), 0xBB3D);
        // A buffer followed by its CRC (little endian) checks to zero.
        let data = b"Syzify rides indoors";
        let crc = crc_of(data);
        let mut with = data.to_vec();
        with.extend_from_slice(&crc.to_le_bytes());
        assert_eq!(crc_of(&with), 0);
        assert_eq!(crc_of(&[]), 0);
    }

    #[test]
    fn a_ride_reads_back_through_fitparser() {
        let bytes = encode(&ride(120));
        let records = fitparser::from_bytes(&bytes).expect("a valid FIT file");
        let samples = of(&records, MesgNum::Record);
        assert_eq!(samples.len(), 120);
        assert_eq!(number(field(samples[3], "power")), Some(153.0));
        assert_eq!(number(field(samples[3], "cadence")), Some(85.0));
        assert_eq!(number(field(samples[3], "heart_rate")), Some(120.0));
        // fitparser expands speed into its enhanced component.
        let speed = number(field(samples[3], "speed").or(field(samples[3], "enhanced_speed"))).unwrap_or(0.0);
        assert!((speed - 8.5).abs() < 0.01, "{speed}");
        let distance = number(field(samples[119], "distance")).unwrap_or(0.0);
        assert!((distance - 8.5 * 119.0).abs() < 0.1, "{distance}");

        let session = of(&records, MesgNum::Session);
        assert_eq!(session.len(), 1);
        assert_eq!(field(session[0], "sport"), Some(&Value::String("cycling".into())));
        assert_eq!(field(session[0], "sub_sport"), Some(&Value::String("indoor_cycling".into())));
        assert_eq!(number(field(session[0], "total_timer_time")), Some(120.0));
        assert_eq!(number(field(session[0], "avg_power")), Some(155.0)); // 150..159 repeating, mean 154.5
        assert_eq!(number(field(session[0], "max_power")), Some(159.0));
        assert_eq!(number(field(session[0], "num_laps")), Some(1.0));
        assert_eq!(of(&records, MesgNum::Activity).len(), 1);
        assert_eq!(of(&records, MesgNum::DeviceInfo).len(), 2);
        let file_id = of(&records, MesgNum::FileId);
        assert_eq!(field(file_id[0], "type"), Some(&Value::String("activity".into())));
    }

    #[test]
    fn laps_split_the_samples() {
        let mut r = ride(600);
        r.lap_starts = vec![300, 0, 9999, 300, 120]; // out of order, out of range, repeated
        let records = fitparser::from_bytes(&encode(&r)).expect("valid");
        let laps = of(&records, MesgNum::Lap);
        let times: Vec<Option<f64>> = laps.iter().map(|l| number(field(l, "total_timer_time"))).collect();
        assert_eq!(times, vec![Some(120.0), Some(180.0), Some(300.0)]);
        let session = of(&records, MesgNum::Session);
        assert_eq!(number(field(session[0], "num_laps")), Some(3.0));
        // The second lap's heart rate: minutes 2–4 → 122..124.
        assert_eq!(number(field(laps[1], "max_heart_rate")), Some(124.0));
    }

    #[test]
    fn missing_values_are_left_out_not_zero() {
        let mut r = ride(5);
        r.samples[2].watts = None;
        r.samples[2].heart_rate = None;
        r.samples[2].speed_mps = Some(f32::NAN);
        r.sub_sport = SubSport::VirtualActivity;
        r.devices.clear();
        let records = fitparser::from_bytes(&encode(&r)).expect("valid");
        let samples = of(&records, MesgNum::Record);
        assert_eq!(field(samples[2], "power"), None);
        assert_eq!(field(samples[2], "heart_rate"), None);
        assert_eq!(field(samples[2], "speed").or(field(samples[2], "enhanced_speed")), None);
        let session = of(&records, MesgNum::Session);
        assert_eq!(field(session[0], "sub_sport"), Some(&Value::String("virtual_activity".into())));
        assert_eq!(number(field(session[0], "avg_power")), Some(152.0), "over the seconds that have power");
    }

    #[test]
    fn an_empty_ride_is_still_a_valid_file() {
        let mut r = ride(0);
        r.lap_starts = vec![5];
        let records = fitparser::from_bytes(&encode(&r)).expect("valid");
        assert_eq!(of(&records, MesgNum::Record).len(), 0);
        assert_eq!(of(&records, MesgNum::Session).len(), 1);
    }

    #[test]
    fn coasting_does_not_pull_the_average_cadence_down() {
        let mut r = ride(10);
        for s in &mut r.samples[5..] {
            s.cadence_rpm = Some(0);
        }
        let records = fitparser::from_bytes(&encode(&r)).expect("valid");
        let session = of(&records, MesgNum::Session);
        assert_eq!(number(field(session[0], "avg_cadence")), Some(85.0));
    }

    fn unix(v: Option<&Value>) -> Option<i64> {
        match v? {
            Value::Timestamp(t) => Some(t.timestamp()),
            _ => None,
        }
    }

    fn text(v: Option<&Value>) -> Option<String> {
        v.map(|v| format!("{v}"))
    }

    #[test]
    fn the_file_names_syzify_and_its_sensors() {
        let records = fitparser::from_bytes(&encode(&ride(10))).expect("valid");
        let file_id = of(&records, MesgNum::FileId);
        assert_eq!(text(field(file_id[0], "product_name")).as_deref(), Some(PRODUCT_NAME));
        let devices = of(&records, MesgNum::DeviceInfo);
        let trainer = devices[0];
        assert_eq!(text(field(trainer, "serial_number")).as_deref(), Some("14830"));
        assert_eq!(text(field(trainer, "source_type")).as_deref(), Some("antplus"));
        assert_eq!(text(field(trainer, "antplus_device_type")).as_deref(), Some("fitness_equipment"));
        assert_eq!(text(field(trainer, "manufacturer")).as_deref(), Some("elite"));
        assert_eq!(text(field(trainer, "device_index")).as_deref(), Some("1"));
        let strap = devices[1];
        assert_eq!(field(strap, "serial_number"), None, "no serial: invalid, not 0");
        assert_eq!(text(field(strap, "source_type")).as_deref(), Some("bluetooth_low_energy"));
    }

    #[test]
    fn a_pause_stops_the_timer_and_leaves_the_elapsed_time() {
        // 120 s of riding with seconds 40–99 paused: 60 s ridden.
        let mut r = ride(160);
        r.samples.retain(|s| !(40..100).contains(&s.t));
        r.pauses = vec![(40, 100), (0, 0), (150, 400)]; // empty and past-the-end ones go
        r.lap_starts = lap_indexes(&r.samples, &[130]);
        assert_eq!(r.lap_starts, vec![70], "the lap at t = 130 is the 71st sample");
        let records = fitparser::from_bytes(&encode(&r)).expect("valid");
        let events = of(&records, MesgNum::Event);
        let timeline: Vec<(Option<i64>, Option<String>)> =
            events.iter().map(|e| (unix(field(e, "timestamp")), text(field(e, "event_type")))).collect();
        let kind = |k: &str| Some(k.to_string());
        assert_eq!(
            timeline,
            vec![
                (Some(START), kind("start")),
                (Some(START + 40), kind("stop_all")),
                (Some(START + 100), kind("start")),
                (Some(START + 160), kind("stop_all")),
            ]
        );
        let session = of(&records, MesgNum::Session);
        assert_eq!(number(field(session[0], "total_timer_time")), Some(100.0));
        assert_eq!(number(field(session[0], "total_elapsed_time")), Some(160.0));
        let activity = of(&records, MesgNum::Activity);
        assert_eq!(number(field(activity[0], "total_timer_time")), Some(100.0));
        let laps = of(&records, MesgNum::Lap);
        let timers: Vec<Option<f64>> = laps.iter().map(|l| number(field(l, "total_timer_time"))).collect();
        assert_eq!(timers, vec![Some(70.0), Some(30.0)], "the pause is in the first lap's span, not its timer");
        let elapsed: Vec<Option<f64>> = laps.iter().map(|l| number(field(l, "total_elapsed_time"))).collect();
        assert_eq!(elapsed, vec![Some(130.0), Some(30.0)]);
    }

    #[test]
    fn timer_events_and_the_activity_span_the_ride() {
        let records = fitparser::from_bytes(&encode(&ride(120))).expect("valid");
        let events = of(&records, MesgNum::Event);
        let times: Vec<Option<i64>> = events.iter().map(|e| unix(field(e, "timestamp"))).collect();
        assert_eq!(times, vec![Some(START), Some(START + 120)]);
        assert_eq!(text(field(events[1], "event_type")).as_deref(), Some("stop_all"));
        let activity = of(&records, MesgNum::Activity);
        assert_eq!(number(field(activity[0], "total_timer_time")), Some(120.0));
        assert_eq!(unix(field(activity[0], "timestamp")), Some(START + 120));
        assert_eq!(number(field(activity[0], "num_sessions")), Some(1.0));
    }

    #[test]
    fn laps_end_with_the_session_and_add_up_to_it() {
        let mut r = ride(300);
        r.lap_starts = vec![100, 200];
        let records = fitparser::from_bytes(&encode(&r)).expect("valid");
        let laps = of(&records, MesgNum::Lap);
        let triggers: Vec<Option<String>> = laps.iter().map(|l| text(field(l, "lap_trigger"))).collect();
        assert_eq!(triggers.iter().map(|t| t.as_deref()).collect::<Vec<_>>(), vec![Some("manual"), Some("manual"), Some("session_end")]);
        let distances: Vec<f64> = laps.iter().filter_map(|l| number(field(l, "total_distance"))).collect();
        let session = number(field(of(&records, MesgNum::Session)[0], "total_distance")).unwrap_or(0.0);
        // Distance after 299 s at 8.5 m/s, measured from 0.
        assert!((session - 8.5 * 299.0).abs() < 0.1, "{session}");
        assert!((distances.iter().sum::<f64>() - session).abs() < 0.1, "laps {distances:?} vs session {session}");
        assert!((distances[0] - 8.5 * 99.0).abs() < 0.1, "the first lap from 0: {distances:?}");
        assert!((distances[1] - 8.5 * 100.0).abs() < 0.1, "the next from where the last ended: {distances:?}");
    }

    #[test]
    fn invalid_sentinels_are_never_written_as_values() {
        let mut r = ride(3);
        r.samples[1].heart_rate = Some(u8::MAX);
        r.samples[1].watts = Some(u16::MAX);
        r.samples[1].cadence_rpm = Some(u8::MAX);
        let records = fitparser::from_bytes(&encode(&r)).expect("valid");
        let s = of(&records, MesgNum::Record)[1];
        assert_eq!((field(s, "heart_rate"), field(s, "power"), field(s, "cadence")), (None, None, None));
    }

    #[test]
    fn ble_sensors_are_named_in_the_ble_enum_and_unknown_makers_left_out() {
        let mut r = ride(5);
        r.devices = vec![
            Device { kind: DeviceKind::BikePower, manufacturer: None, name: None, serial: Some(7), radio: Some(Radio::Ble) },
            Device { kind: DeviceKind::FitnessEquipment, manufacturer: None, name: None, serial: Some(8), radio: Some(Radio::Ble) },
            Device { kind: DeviceKind::HeartRate, manufacturer: None, name: None, serial: Some(9), radio: Some(Radio::Ble) },
        ];
        let records = fitparser::from_bytes(&encode(&r)).expect("valid");
        let devices = of(&records, MesgNum::DeviceInfo);
        let types: Vec<Option<String>> = devices.iter().map(|d| text(field(d, "ble_device_type"))).collect();
        assert_eq!(
            types.iter().map(|t| t.as_deref()).collect::<Vec<_>>(),
            vec![Some("bike_power"), Some("bike_trainer"), Some("heart_rate")]
        );
        assert!(devices.iter().all(|d| field(d, "manufacturer").is_none()), "not «development»");
    }

    #[test]
    fn a_ride_whose_first_seconds_are_missing_starts_at_its_first_second() {
        let mut r = ride(60);
        r.samples.drain(..3);
        let records = fitparser::from_bytes(&encode(&r)).expect("valid");
        let file_id = of(&records, MesgNum::FileId);
        assert_eq!(unix(field(file_id[0], "time_created")), Some(START + 3));
        assert_eq!(unix(field(of(&records, MesgNum::Event)[0], "timestamp")), Some(START + 3));
        let session = number(field(of(&records, MesgNum::Session)[0], "total_timer_time"));
        let activity = number(field(of(&records, MesgNum::Activity)[0], "total_timer_time"));
        assert_eq!((session, activity), (Some(57.0), Some(57.0)), "the two agree");
    }

    #[test]
    fn sentinels_stay_out_of_the_totals() {
        let mut r = ride(10);
        r.samples[4].heart_rate = Some(u8::MAX);
        r.samples[4].watts = Some(u16::MAX);
        r.samples[4].cadence_rpm = Some(u8::MAX);
        let records = fitparser::from_bytes(&encode(&r)).expect("valid");
        let session = of(&records, MesgNum::Session)[0];
        assert_eq!(number(field(session, "max_heart_rate")), Some(120.0));
        assert_eq!(number(field(session, "max_power")), Some(159.0));
        assert_eq!(number(field(session, "max_cadence")), Some(85.0));
    }

    #[test]
    fn a_name_is_cut_on_a_character_boundary() {
        assert_eq!(fitted("ASSIOMA55430L"), "ASSIOMA55430L");
        // 18 ASCII bytes then a 2-byte "Ä": byte 19 falls inside it.
        let name = "ASSIOMA-DUO-PEDALSÄ";
        assert_eq!(fitted(name), "ASSIOMA-DUO-PEDALS");
        assert!(fitted("ÄÄÄÄÄÄÄÄÄÄÄÄÄ").len() <= 19);
    }

    #[test]
    fn long_names_and_times_before_the_fit_epoch_are_safe() {
        let mut r = ride(2);
        r.start_unix = 0;
        r.devices[0].name = Some("A name far longer than twenty bytes".into());
        assert!(fitparser::from_bytes(&encode(&r)).is_ok());
    }
}
