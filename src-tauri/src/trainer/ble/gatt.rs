//! The GATT side of the BLE profiles a ride needs: FTMS (the trainer),
//! Cycling Power (pedals), Heart Rate, Device Information. Pure: bytes in,
//! values out. The parsers were proven on an Elite Avanti and Favero
//! Assioma pedals by the ftms-spike tool.

use std::time::Duration;

use crate::trainer::link::Control;

// Services.
pub const FITNESS_MACHINE: u16 = 0x1826;
pub const CYCLING_POWER: u16 = 0x1818;
pub const HEART_RATE: u16 = 0x180D;

// Characteristics.
pub const INDOOR_BIKE_DATA: u16 = 0x2AD2;
pub const FTMS_CONTROL_POINT: u16 = 0x2AD9;
pub const CYCLING_POWER_MEASUREMENT: u16 = 0x2A63;
pub const HEART_RATE_MEASUREMENT: u16 = 0x2A37;
pub const MANUFACTURER_NAME: u16 = 0x2A29;
pub const MODEL_NUMBER: u16 = 0x2A24;
pub const FIRMWARE_REVISION: u16 = 0x2A26;
pub const FTMS_FEATURE: u16 = 0x2ACC;

/// FTMS Fitness Machine Feature (0x2ACC): the machine's features (u32),
/// then its target setting features (u32), whose bit 3 is "power target
/// setting supported" — ERG. None when the value is too short to say.
pub fn parse_target_power_supported(value: &[u8]) -> Option<bool> {
    let settings = value.get(4..8)?;
    Some(settings[0] & 0x08 != 0)
}

/// The Bluetooth base UUID with the 16-bit part zeroed.
const BASE_UUID: u128 = 0x0000_0000_0000_1000_8000_0080_5F9B_34FB;
const SHORT_MASK: u128 = 0xFFFF_FFFF_u128 << 96;

/// The 16-bit id of a UUID built on the Bluetooth base; vendor UUIDs give
/// None.
pub fn short_uuid(uuid: u128) -> Option<u16> {
    let short = (uuid >> 96) as u32;
    (uuid & !SHORT_MASK == BASE_UUID && short <= u16::MAX as u32).then_some(short as u16)
}

/// The full UUID of a 16-bit id.
pub fn full_uuid(short: u16) -> u128 {
    BASE_UUID | ((short as u128) << 96)
}

fn u16_at(v: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*v.get(i)?, *v.get(i + 1)?]))
}

fn i16_at(v: &[u8], i: usize) -> Option<i16> {
    Some(i16::from_le_bytes([*v.get(i)?, *v.get(i + 1)?]))
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct IndoorBike {
    pub speed_kmh: Option<f32>,
    pub cadence_rpm: Option<f32>,
    pub watts: Option<i16>,
}

/// FTMS Indoor Bike Data (0x2AD2): a flags word, then the fields the flags
/// announce, in order. Bit 0 set means "more data": the speed is left out.
pub fn parse_indoor_bike_data(v: &[u8]) -> Option<IndoorBike> {
    let flags = u16_at(v, 0)?;
    let has = |bit: u16| flags & (1 << bit) != 0;
    let mut i = 2;
    let mut out = IndoorBike::default();
    if !has(0) {
        out.speed_kmh = Some(u16_at(v, i)? as f32 / 100.0);
        i += 2;
    }
    if has(1) {
        i += 2; // average speed
    }
    if has(2) {
        out.cadence_rpm = Some(u16_at(v, i)? as f32 / 2.0);
        i += 2;
    }
    if has(3) {
        i += 2; // average cadence
    }
    if has(4) {
        i += 3; // total distance
    }
    if has(5) {
        i += 2; // resistance level
    }
    if has(6) {
        out.watts = Some(i16_at(v, i)?);
    }
    Some(out)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CyclingPower {
    pub watts: i16,
    /// The right pedal's share, percent, when the device says whose share
    /// its balance is.
    pub balance_right: Option<f32>,
    /// Cumulative crank revolutions and the last crank event, 1/1024 s.
    pub crank: Option<(u16, u16)>,
}

/// Cycling Power Measurement (0x2A63): flags, instantaneous power, then
/// optional fields; the crank data sits behind the balance, the
/// accumulated torque and the wheel data when those are present.
pub fn parse_cycling_power(v: &[u8]) -> Option<CyclingPower> {
    let flags = u16_at(v, 0)?;
    let watts = i16_at(v, 2)?;
    let has = |bit: u16| flags & (1 << bit) != 0;
    let mut i = 4;
    let mut balance_right = None;
    if has(0) {
        // In half percent, of the left pedal when bit 1 says so; with an
        // unknown reference it is nobody's share.
        let share = *v.get(i)? as f32 / 2.0;
        if has(1) {
            balance_right = Some(100.0 - share);
        }
        i += 1;
    }
    if has(2) {
        i += 2; // accumulated torque
    }
    if has(4) {
        i += 6; // wheel revolutions + last wheel event
    }
    let crank = if has(5) { Some((u16_at(v, i)?, u16_at(v, i + 2)?)) } else { None };
    Some(CyclingPower { watts, balance_right, crank })
}

/// Heart Rate Measurement (0x2A37): bpm as u8, or u16 when flag bit 0 is
/// set. None when the strap says it has no skin contact (bits 1–2: contact
/// supported, not detected) or reads 0: an unknown is never zero.
pub fn parse_heart_rate(v: &[u8]) -> Option<u16> {
    let flags = *v.first()?;
    let contact_supported = flags & 0b100 != 0;
    let contact = flags & 0b010 != 0;
    if contact_supported && !contact {
        return None;
    }
    let bpm = if flags & 1 != 0 { u16_at(v, 1)? } else { *v.get(1)? as u16 };
    (bpm > 0).then_some(bpm)
}

/// Cadence from successive crank events. A repeated event (no new
/// revolution) keeps the last value; none for `IDLE` reads 0.
#[derive(Debug, Default)]
pub struct CrankCadence {
    last: Option<(u16, u16)>,
    rpm: Option<f32>,
    moved_at: Option<Duration>,
}

impl CrankCadence {
    pub const IDLE: Duration = Duration::from_secs(3);

    pub fn push(&mut self, at: Duration, crank: (u16, u16)) -> Option<f32> {
        let (revs, time) = crank;
        if let Some((r0, t0)) = self.last {
            let dr = revs.wrapping_sub(r0);
            let dt = time.wrapping_sub(t0);
            if dr > 0 && dt > 0 {
                self.rpm = Some(dr as f32 * 1024.0 * 60.0 / dt as f32);
                self.moved_at = Some(at);
            }
        }
        self.last = Some(crank);
        match self.moved_at {
            Some(m) if at.saturating_sub(m) >= Self::IDLE => Some(0.0),
            _ => self.rpm,
        }
    }
}

// FTMS control point op codes.
pub const OP_REQUEST_CONTROL: u8 = 0x00;
pub const OP_RESET: u8 = 0x01;
pub const OP_SET_TARGET_RESISTANCE: u8 = 0x04;
pub const OP_SET_TARGET_POWER: u8 = 0x05;
pub const OP_START: u8 = 0x07;
const OP_RESPONSE: u8 = 0x80;
pub const RESULT_SUCCESS: u8 = 0x01;
/// Another client holds the trainer, or a Reset took our control away.
pub const RESULT_CONTROL_NOT_PERMITTED: u8 = 0x05;

/// The control point write for a control: ERG is Set Target Power (W,
/// SINT16); resistance is Set Target Resistance Level (UINT8, 0.1 per
/// unit, so 0–100 % reads 0.0–10.0 — trainers scale it their own way, and
/// the MVP does not use it); letting go is Reset, proven on the Avanti,
/// which also takes our control away: the link asks for it again before
/// the next control.
pub fn control_write(control: Control) -> Vec<u8> {
    match control {
        Control::TargetPower(w) => {
            let [lo, hi] = (w.min(i16::MAX as u16) as i16).to_le_bytes();
            vec![OP_SET_TARGET_POWER, lo, hi]
        }
        Control::Resistance(pct) => vec![OP_SET_TARGET_RESISTANCE, pct.min(100)],
        Control::Release => vec![OP_RESET],
    }
}

/// A control point indication `[0x80, request op code, result]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlResponse {
    pub request: u8,
    pub result: u8,
}

impl ControlResponse {
    pub fn success(self) -> bool {
        self.result == RESULT_SUCCESS
    }
}

pub fn parse_control_response(v: &[u8]) -> Option<ControlResponse> {
    match v {
        [OP_RESPONSE, request, result, ..] => Some(ControlResponse { request: *request, result: *result }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_and_full_uuids() {
        assert_eq!(full_uuid(FITNESS_MACHINE), 0x0000_1826_0000_1000_8000_0080_5F9B_34FB);
        assert_eq!(short_uuid(full_uuid(INDOOR_BIKE_DATA)), Some(INDOOR_BIKE_DATA));
        // Zwift's vendor service on the Avanti COG.
        assert_eq!(short_uuid(0x0000_0001_19CA_4651_86E5_FA29_DCDD_09D1), None);
        assert_eq!(short_uuid(0x0001_1826_0000_1000_8000_0080_5F9B_34FB), None, "32-bit, not 16");
    }

    #[test]
    fn indoor_bike_data_with_speed_cadence_and_power() {
        // flags 0x0044: cadence (bit 2) + power (bit 6), speed present (bit 0 clear).
        let v = [0x44, 0x00, 0xB8, 0x0B, 0xB4, 0x00, 0xC8, 0x00];
        assert_eq!(
            parse_indoor_bike_data(&v),
            Some(IndoorBike { speed_kmh: Some(30.0), cadence_rpm: Some(90.0), watts: Some(200) })
        );
    }

    #[test]
    fn indoor_bike_data_skips_the_fields_before_power() {
        // more data (no speed) + distance (3 bytes) + resistance (2) + power.
        let v = [0x71, 0x00, 0x10, 0x27, 0x00, 0x05, 0x00, 0x2C, 0x01];
        assert_eq!(parse_indoor_bike_data(&v), Some(IndoorBike { speed_kmh: None, cadence_rpm: None, watts: Some(300) }));
        // average speed and average cadence are skipped too.
        let v = [0x0F, 0x00, 0x01, 0x00, 0xA0, 0x00, 0x02, 0x00];
        assert_eq!(parse_indoor_bike_data(&v), Some(IndoorBike { speed_kmh: None, cadence_rpm: Some(80.0), watts: None }));
        assert_eq!(parse_indoor_bike_data(&[0x40, 0x00, 0x01]), None, "truncated");
        assert_eq!(parse_indoor_bike_data(&[0x40]), None, "no flags");
    }

    #[test]
    fn cycling_power_with_balance_and_crank_data() {
        // flags 0x0023: balance (bit 0) of the left pedal (bit 1) + crank data (bit 5).
        let v = [0x23, 0x00, 0xFA, 0x00, 0x68, 0x0A, 0x00, 0x00, 0x04];
        let cp = parse_cycling_power(&v).unwrap();
        assert_eq!(cp, CyclingPower { watts: 250, balance_right: Some(48.0), crank: Some((10, 1024)) });
        // A balance with no reference says nothing about the right leg.
        let v = [0x21, 0x00, 0xFA, 0x00, 0x64, 0x0A, 0x00, 0x00, 0x04];
        assert_eq!(parse_cycling_power(&v).unwrap().balance_right, None);
    }

    #[test]
    fn cycling_power_finds_the_crank_behind_torque_and_wheel_data() {
        // flags 0x0034: torque (bit 2) + wheel (bit 4) + crank (bit 5).
        let v = [0x34, 0x00, 0x96, 0x00, 1, 2, 1, 2, 3, 4, 5, 6, 0x05, 0x00, 0x00, 0x08];
        assert_eq!(parse_cycling_power(&v).unwrap().crank, Some((5, 2048)));
        // Assioma over BLE: crank data only, one leg each.
        let v = [0x20, 0x00, 0x4B, 0x00, 0x07, 0x00, 0x00, 0x04];
        assert_eq!(parse_cycling_power(&v), Some(CyclingPower { watts: 75, balance_right: None, crank: Some((7, 1024)) }));
        assert_eq!(parse_cycling_power(&[0x20, 0x00, 0x4B, 0x00, 0x07]), None, "truncated crank data");
    }

    #[test]
    fn heart_rate_in_one_or_two_bytes() {
        assert_eq!(parse_heart_rate(&[0x00, 72]), Some(72));
        assert_eq!(parse_heart_rate(&[0x01, 0x2C, 0x01]), Some(300));
        assert_eq!(parse_heart_rate(&[0x00]), None);
        assert_eq!(parse_heart_rate(&[0x01, 0x2C]), None, "truncated u16");
        assert_eq!(parse_heart_rate(&[]), None);
    }

    #[test]
    fn heart_rate_without_skin_contact_or_zero_is_no_reading() {
        assert_eq!(parse_heart_rate(&[0b100, 0]), None, "contact supported, not detected");
        assert_eq!(parse_heart_rate(&[0b100, 80]), None, "a number without contact is not a heart rate");
        assert_eq!(parse_heart_rate(&[0b110, 80]), Some(80), "contact detected");
        assert_eq!(parse_heart_rate(&[0b000, 0]), None, "zero");
    }

    #[test]
    fn the_feature_word_says_whether_a_power_target_is_taken() {
        assert_eq!(parse_target_power_supported(&[0x83, 0x14, 0, 0, 0x0C, 0x20, 0, 0]), Some(true));
        assert_eq!(parse_target_power_supported(&[0xFF, 0xFF, 0xFF, 0xFF, 0x04, 0, 0, 0]), Some(false), "resistance only");
        assert_eq!(parse_target_power_supported(&[0x83, 0x14, 0, 0, 0x08]), None, "too short");
    }

    #[test]
    fn indoor_bike_data_skips_average_cadence_before_power() {
        // more data (no speed) + average cadence (bit 3) + power (bit 6).
        let v = [0x49, 0x00, 0xA0, 0x00, 0x96, 0x00];
        assert_eq!(parse_indoor_bike_data(&v), Some(IndoorBike { speed_kmh: None, cadence_rpm: None, watts: Some(150) }));
    }

    #[test]
    fn cadence_from_crank_events_wraps_and_goes_idle() {
        let mut c = CrankCadence::default();
        let s = Duration::from_secs;
        assert_eq!(c.push(s(0), (65535, 64512)), None, "one event is no cadence");
        // One revolution in 1024/1024 s across both counters' wrap: 60 rpm.
        assert_eq!(c.push(s(1), (0, 0)), Some(60.0));
        assert_eq!(c.push(s(2), (0, 0)), Some(60.0), "the same event repeated keeps the value");
        assert_eq!(c.push(s(4), (0, 0)), Some(0.0), "no revolution for 3 s");
    }

    #[test]
    fn control_writes() {
        assert_eq!(control_write(Control::TargetPower(150)), vec![0x05, 0x96, 0x00]);
        assert_eq!(control_write(Control::TargetPower(u16::MAX)), vec![0x05, 0xFF, 0x7F], "clamped to SINT16");
        assert_eq!(control_write(Control::Resistance(250)), vec![0x04, 100]);
        assert_eq!(control_write(Control::Release), vec![0x01]);
    }

    #[test]
    fn control_responses() {
        let ok = parse_control_response(&[0x80, 0x05, 0x01]).unwrap();
        assert_eq!(ok, ControlResponse { request: 0x05, result: RESULT_SUCCESS });
        assert!(ok.success());
        // Start after a reconnect on the Avanti: "failed".
        let failed = parse_control_response(&[0x80, 0x07, 0x04]).unwrap();
        assert_eq!((failed.request, failed.success()), (0x07, false));
        assert_eq!(parse_control_response(&[0x05, 0x96]), None, "not a response");
    }
}
