//! Test-only FIT encoder: enough of the binary format to build small files
//! for the decoders and the import pipeline — header, definition and data
//! messages, both CRCs. Fixtures are synthetic, so no health data of a real
//! person ever enters the repository (ADR 0002).

use std::collections::HashMap;

/// FIT base types (the byte in a field definition).
pub const ENUM: u8 = 0x00;
pub const UINT8: u8 = 0x02;
pub const SINT16: u8 = 0x83;
pub const UINT16: u8 = 0x84;
pub const SINT32: u8 = 0x85;
pub const UINT32: u8 = 0x86;
pub const UINT32Z: u8 = 0x8C;

/// Global message numbers used by the fixtures.
pub const MSG_FILE_ID: u16 = 0;
pub const MSG_SESSION: u16 = 18;
pub const MSG_LAP: u16 = 19;
pub const MSG_RECORD: u16 = 20;
pub const MSG_DEVICE_INFO: u16 = 23;
pub const MSG_MONITORING: u16 = 55;
pub const MSG_MONITORING_INFO: u16 = 103;
pub const MSG_MONITORING_HR_DATA: u16 = 211;
pub const MSG_SET: u16 = 225;
pub const MSG_STRESS_LEVEL: u16 = 227;
pub const MSG_SPO2_DATA: u16 = 269;
pub const MSG_RESPIRATION_RATE: u16 = 297;

/// `file_id.type` values.
pub const FILE_TYPE_ACTIVITY: u8 = 4;
pub const FILE_TYPE_MONITORING_B: u8 = 32;
pub const FILE_TYPE_SETTINGS: u8 = 2;

/// `activity_type` enum values.
pub const ACTIVITY_WALKING: u8 = 6;
pub const ACTIVITY_SEDENTARY: u8 = 8;

const FIT_EPOCH_UNIX: i64 = 631_065_600;

/// Seconds since the FIT epoch for a unix time.
pub fn fit_ts(unix: i64) -> u32 {
    (unix - FIT_EPOCH_UNIX) as u32
}

/// A field value, encoded little-endian at the size its definition declares.
#[derive(Debug, Clone, Copy)]
pub enum Val {
    U8(u8),
    I16(i16),
    U16(u16),
    U32(u32),
    I32(i32),
    /// A three-element uint16 array — a field defined with size 6.
    U16x3([u16; 3]),
}

/// The FIT CRC-16 (the 4-bit-table variant from the SDK).
pub fn crc16(data: &[u8]) -> u16 {
    const TABLE: [u16; 16] = [
        0x0000, 0xCC01, 0xD801, 0x1400, 0xF001, 0x3C00, 0x2800, 0xE401, 0xA001, 0x6C00, 0x7800,
        0xB401, 0x5000, 0x9C01, 0x8801, 0x4400,
    ];
    let mut crc: u16 = 0;
    for &byte in data {
        let mut tmp = TABLE[(crc & 0xF) as usize];
        crc = (crc >> 4) & 0x0FFF;
        crc ^= tmp ^ TABLE[(byte & 0xF) as usize];
        tmp = TABLE[(crc & 0xF) as usize];
        crc = (crc >> 4) & 0x0FFF;
        crc ^= tmp ^ TABLE[((byte >> 4) & 0xF) as usize];
    }
    crc
}

#[derive(Default)]
pub struct FitBuilder {
    records: Vec<u8>,
    /// Local message type → (field number, size, base type) list.
    defs: HashMap<u8, Vec<(u8, u8, u8)>>,
}

impl FitBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Emit a definition message for local type `local`.
    pub fn define(&mut self, local: u8, global: u16, fields: &[(u8, u8, u8)]) -> &mut Self {
        self.records.push(0x40 | (local & 0x0F));
        self.records.push(0); // reserved
        self.records.push(0); // little-endian
        self.records.extend_from_slice(&global.to_le_bytes());
        self.records.push(fields.len() as u8);
        for &(num, size, base) in fields {
            self.records.extend_from_slice(&[num, size, base]);
        }
        self.defs.insert(local, fields.to_vec());
        self
    }

    /// Emit a data message for local type `local`; `values` follow the
    /// definition's field order and sizes.
    pub fn data(&mut self, local: u8, values: &[Val]) -> &mut Self {
        let def = self.defs.get(&local).expect("data before definition").clone();
        assert_eq!(def.len(), values.len(), "value count must match the definition");
        self.records.push(local & 0x0F);
        for (&(_, size, _), v) in def.iter().zip(values) {
            let bytes: Vec<u8> = match v {
                Val::U8(x) => vec![*x],
                Val::I16(x) => x.to_le_bytes().to_vec(),
                Val::U16(x) => x.to_le_bytes().to_vec(),
                Val::U32(x) => x.to_le_bytes().to_vec(),
                Val::I32(x) => x.to_le_bytes().to_vec(),
                Val::U16x3(xs) => xs.iter().flat_map(|x| x.to_le_bytes()).collect(),
            };
            assert_eq!(bytes.len(), size as usize, "value size must match the definition");
            self.records.extend_from_slice(&bytes);
        }
        self
    }

    /// The complete file: 14-byte header with its CRC, the records, the
    /// file CRC.
    pub fn build(&self) -> Vec<u8> {
        let mut header = Vec::with_capacity(14);
        header.push(14);
        header.push(0x20); // protocol 2.0
        header.extend_from_slice(&2140u16.to_le_bytes()); // profile 21.40
        header.extend_from_slice(&(self.records.len() as u32).to_le_bytes());
        header.extend_from_slice(b".FIT");
        let hcrc = crc16(&header);
        header.extend_from_slice(&hcrc.to_le_bytes());
        let mut out = header;
        out.extend_from_slice(&self.records);
        let crc = crc16(&out);
        out.extend_from_slice(&crc.to_le_bytes());
        out
    }
}

const F_TIMESTAMP: (u8, u8, u8) = (253, 4, UINT32);

/// A file_id message; `kind` is one of the FILE_TYPE_* constants.
fn file_id(b: &mut FitBuilder, kind: u8, serial: u32, created_unix: i64) {
    b.define(
        0,
        MSG_FILE_ID,
        &[(0, 1, ENUM), (1, 2, UINT16), (2, 2, UINT16), (3, 4, UINT32Z), (4, 4, UINT32)],
    );
    b.data(
        0,
        &[
            Val::U8(kind),
            Val::U16(1),
            Val::U16(3291),
            Val::U32(serial),
            Val::U32(fit_ts(created_unix)),
        ],
    );
}

/// A Monitor file of the shape a fenix writes, starting at `midnight`
/// (unix seconds of a local midnight): the head rows stamped with the last
/// sync (22:08 the evening before), a full-stamped heart-rate row as the
/// anchor, 16-bit heart-rate rows (one a "no reading" zero), a running
/// walking total and an active-minutes row on 16-bit stamps, a per-minute
/// intensity mark, stress with one sentinel, respiration, SpO2 and an RHR
/// estimate. The decoder test `decodes_the_synthetic_monitor_file_end_to_end`
/// spells out what must come out of it.
pub fn monitoring_fixture(serial: u32, midnight: i64) -> Vec<u8> {
    let sync = midnight - 6720;
    let low16 = |t: i64| Val::U16((fit_ts(t) & 0xFFFF) as u16);
    let mut b = FitBuilder::new();
    file_id(&mut b, FILE_TYPE_MONITORING_B, serial, sync);
    // monitoring_info: timestamp + local_timestamp (the constant-hour-off value).
    b.define(1, MSG_MONITORING_INFO, &[F_TIMESTAMP, (0, 4, UINT32)]);
    b.data(1, &[Val::U32(fit_ts(sync)), Val::U32(fit_ts(sync + 3600))]);
    // Full-stamped heart rate (anchor).
    b.define(2, MSG_MONITORING, &[F_TIMESTAMP, (27, 1, UINT8)]);
    b.data(2, &[Val::U32(fit_ts(midnight)), Val::U8(52)]);
    // 16-bit heart rates.
    b.define(3, MSG_MONITORING, &[(26, 2, UINT16), (27, 1, UINT8)]);
    b.data(3, &[low16(midnight + 120), Val::U8(58)]);
    b.data(3, &[low16(midnight + 240), Val::U8(0)]);
    b.data(3, &[low16(midnight + 360), Val::U8(55)]);
    // Running walking total on a 16-bit stamp: cycles (steps), active_time
    // (ms), activity_type, active_calories.
    b.define(
        4,
        MSG_MONITORING,
        &[(26, 2, UINT16), (3, 4, UINT32), (4, 4, UINT32), (5, 1, ENUM), (19, 2, UINT16)],
    );
    b.data(
        4,
        &[
            low16(midnight + 600),
            Val::U32(290),
            Val::U32(780_000),
            Val::U8(ACTIVITY_WALKING),
            Val::U16(15),
        ],
    );
    // Active minutes: named increments + the unnamed running totals 37/38.
    b.define(
        5,
        MSG_MONITORING,
        &[(26, 2, UINT16), (33, 2, UINT16), (34, 2, UINT16), (37, 2, UINT16), (38, 2, UINT16)],
    );
    b.data(5, &[low16(midnight + 900), Val::U16(1), Val::U16(3), Val::U16(2), Val::U16(4)]);
    // Per-minute intensity mark.
    b.define(6, MSG_MONITORING, &[F_TIMESTAMP, (5, 1, ENUM), (28, 1, UINT8)]);
    b.data(6, &[Val::U32(fit_ts(midnight + 60)), Val::U8(ACTIVITY_SEDENTARY), Val::U8(0)]);
    // Stress: a sentinel and a reading.
    b.define(7, MSG_STRESS_LEVEL, &[(0, 2, SINT16), (1, 4, UINT32)]);
    b.data(7, &[Val::I16(-1), Val::U32(fit_ts(midnight + 180))]);
    b.data(7, &[Val::I16(23), Val::U32(fit_ts(midnight + 360))]);
    // Garmin's RHR estimate.
    b.define(8, MSG_MONITORING_HR_DATA, &[F_TIMESTAMP, (0, 1, UINT8), (1, 1, UINT8)]);
    b.data(8, &[Val::U32(fit_ts(midnight + 36_000)), Val::U8(62), Val::U8(49)]);
    // Respiration (×100) and SpO2.
    b.define(9, MSG_RESPIRATION_RATE, &[F_TIMESTAMP, (0, 2, SINT16)]);
    b.data(9, &[Val::U32(fit_ts(midnight + 300)), Val::I16(1550)]);
    b.define(10, MSG_SPO2_DATA, &[F_TIMESTAMP, (0, 1, UINT8), (1, 1, UINT8)]);
    b.data(10, &[Val::U32(fit_ts(midnight + 300)), Val::U8(96), Val::U8(12)]);
    b.build()
}

/// A Monitor file with nothing but its identity — what the watch writes
/// after a day in the drawer.
pub fn empty_monitoring_fixture(serial: u32, created_unix: i64) -> Vec<u8> {
    let mut b = FitBuilder::new();
    file_id(&mut b, FILE_TYPE_MONITORING_B, serial, created_unix);
    b.define(1, MSG_MONITORING_INFO, &[F_TIMESTAMP, (0, 4, UINT32)]);
    b.data(1, &[Val::U32(fit_ts(created_unix)), Val::U32(fit_ts(created_unix + 3600))]);
    b.build()
}

/// The sets of a strength activity the way a fenix writes them (#213): each
/// set's repetitions (10, 11, …) and its category as a three-element array
/// padded with the profile's "unknown" (65534), plus the parallel subtype
/// array where one is given.
pub fn strength_fixture(sets: &[([u16; 3], Option<[u16; 3]>)]) -> Vec<u8> {
    let mut b = FitBuilder::new();
    file_id(&mut b, FILE_TYPE_ACTIVITY, 3_314_747_279, 1_788_555_600);
    // set: repetitions, category[3] — and category_subtype[3].
    b.define(1, MSG_SET, &[(3, 2, UINT16), (7, 6, UINT16)]);
    b.define(2, MSG_SET, &[(3, 2, UINT16), (7, 6, UINT16), (8, 6, UINT16)]);
    for (i, (category, subtype)) in sets.iter().enumerate() {
        let repetitions = Val::U16(10 + i as u16);
        match subtype {
            None => b.data(1, &[repetitions, Val::U16x3(*category)]),
            Some(subtype) => {
                b.data(2, &[repetitions, Val::U16x3(*category), Val::U16x3(*subtype)])
            }
        };
    }
    b.build()
}

/// One second of a ride for [`ride_fixture`]: unix time, position in
/// degrees (None = no fix that second), device speed in m/s.
pub type RidePoint = (i64, Option<(f64, f64)>, f64);

/// A road ride the way an Edge writes it: records with position and speed,
/// one session claiming `distance_m` and `max_speed_mps`, and the paired
/// ANT+ sensors (`antplus_device_type` numbers: 11 power, 121 speed and
/// cadence, 123 speed).
pub fn ride_fixture(points: &[RidePoint], distance_m: f64, max_speed_mps: f64, sensors: &[u8]) -> Vec<u8> {
    ride_with_laps_fixture(points, distance_m, max_speed_mps, sensors, &[])
}

/// [`ride_fixture`] with laps given as (start, end) unix times; each claims
/// the session's maximum and its share of the session's distance.
pub fn ride_with_laps_fixture(
    points: &[RidePoint],
    distance_m: f64,
    max_speed_mps: f64,
    sensors: &[u8],
    laps: &[(i64, i64)],
) -> Vec<u8> {
    let start = points.first().map_or(1_788_555_600, |p| p.0);
    let end = points.last().map_or(start, |p| p.0);
    let semicircles = |deg: f64| Val::I32((deg * 2f64.powi(31) / 180.0).round() as i32);
    let mm_s = |v: f64| Val::U16((v * 1000.0).round() as u16);
    let mut b = FitBuilder::new();
    file_id(&mut b, FILE_TYPE_ACTIVITY, 3_314_747_279, start);
    // device_info: device_index, antplus_device_type, serial_number, source_type (antplus).
    b.define(1, MSG_DEVICE_INFO, &[F_TIMESTAMP, (0, 1, UINT8), (1, 1, UINT8), (3, 4, UINT32Z), (25, 1, ENUM)]);
    for (k, &kind) in sensors.iter().enumerate() {
        b.data(1, &[Val::U32(fit_ts(start)), Val::U8(k as u8 + 1), Val::U8(kind), Val::U32(4_000 + k as u32), Val::U8(1)]);
    }
    // record: with a fix (lat, long, speed) and without one (speed only).
    b.define(2, MSG_RECORD, &[F_TIMESTAMP, (0, 4, SINT32), (1, 4, SINT32), (6, 2, UINT16)]);
    b.define(3, MSG_RECORD, &[F_TIMESTAMP, (6, 2, UINT16)]);
    for &(t, pos, v) in points {
        match pos {
            Some((lat, lon)) => b.data(2, &[Val::U32(fit_ts(t)), semicircles(lat), semicircles(lon), mm_s(v)]),
            None => b.data(3, &[Val::U32(fit_ts(t)), mm_s(v)]),
        };
    }
    // lap: start_time, elapsed + timer (ms), distance (cm), max speed (mm/s).
    b.define(5, MSG_LAP, &[F_TIMESTAMP, (2, 4, UINT32), (7, 4, UINT32), (8, 4, UINT32), (9, 4, UINT32), (14, 2, UINT16)]);
    for &(from, to) in laps {
        let ms = (to - from) as u32 * 1000;
        let share = distance_m / laps.len() as f64;
        b.data(
            5,
            &[
                Val::U32(fit_ts(to)),
                Val::U32(fit_ts(from)),
                Val::U32(ms),
                Val::U32(ms),
                Val::U32((share * 100.0).round() as u32),
                mm_s(max_speed_mps),
            ],
        );
    }
    // session: start_time, sport (cycling), elapsed + timer (ms), distance (cm), max speed (mm/s).
    b.define(
        4,
        MSG_SESSION,
        &[F_TIMESTAMP, (2, 4, UINT32), (5, 1, ENUM), (7, 4, UINT32), (8, 4, UINT32), (9, 4, UINT32), (15, 2, UINT16)],
    );
    let secs = (end - start) as u32 * 1000;
    b.data(
        4,
        &[
            Val::U32(fit_ts(end)),
            Val::U32(fit_ts(start)),
            Val::U8(2),
            Val::U32(secs),
            Val::U32(secs),
            Val::U32((distance_m * 100.0).round() as u32),
            mm_s(max_speed_mps),
        ],
    );
    b.build()
}

/// A ride the way a jammed Edge records it (#256): 10 min on the road at
/// 8 m/s, 5 min without a fix, a minute teleported 20 km away at a GPS
/// speed of 27 m/s, then 10 min back on the road. The session claims the
/// teleport in its 30 km distance and a 65 m/s maximum. Cleaned, it is
/// 1200 fixes over 12 472 m (4792 + a 2888 m bridge + 4792).
pub fn jammed_ride_fixture(sensors: &[u8]) -> Vec<u8> {
    jammed_ride_with_laps_fixture(sensors, &[])
}

/// [`jammed_ride_fixture`] with laps at these seconds from its start.
pub fn jammed_ride_with_laps_fixture(sensors: &[u8], laps: &[(i64, i64)]) -> Vec<u8> {
    let t0 = 1_788_555_600;
    let north = |m: f64| (55.0 + m / 111_195.0, 37.0);
    let mut pts: Vec<RidePoint> = (0..600).map(|k| (t0 + k, Some(north(8.0 * k as f64)), 8.0)).collect();
    pts.extend((600..900).map(|k| (t0 + k, None, 0.0)));
    pts.extend((900..960).map(|k| (t0 + k, Some(north(20_000.0 + 27.0 * (k - 900) as f64)), 27.0)));
    pts.extend((960..1560).map(|k| (t0 + k, Some(north(8.0 * k as f64)), 8.0)));
    let laps: Vec<(i64, i64)> = laps.iter().map(|&(a, b)| (t0 + a, t0 + b)).collect();
    ride_with_laps_fixture(&pts, 30_000.0, 65.0, sensors, &laps)
}

/// A FIT file of a type the importer does not handle.
pub fn settings_fixture(serial: u32, created_unix: i64) -> Vec<u8> {
    let mut b = FitBuilder::new();
    file_id(&mut b, FILE_TYPE_SETTINGS, serial, created_unix);
    b.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc16_matches_the_sdk_reference() {
        // Known vector: the CRC of an empty buffer is 0, and of "123456789"
        // under this table is 0xBB3D (CRC-16/ARC).
        assert_eq!(crc16(&[]), 0);
        assert_eq!(crc16(b"123456789"), 0xBB3D);
    }

    #[test]
    fn a_built_file_parses_and_carries_its_file_id() {
        let bytes = settings_fixture(7, 1_788_555_600);
        let messages = fitparser::from_bytes(&bytes).expect("fitparser accepts the file");
        assert_eq!(messages[0].kind(), fitparser::profile::MesgNum::FileId);
    }
}
