//! Garmin **Monitor** FIT files (`FileId.type = monitoring_b`) — the
//! all-day files next to the activities: heart rate every 1–2 min, stress
//! every ~3 min, respiration, SpO2, Garmin's resting heart rate, steps and
//! active minutes (ADR 0002).
//!
//! Layering: a THIN decoder turns fitparser messages into [`Event`]s in file
//! order, and everything with logic in it — unrolling `timestamp_16`,
//! dropping sentinels, confirming the time zone — is a pure function over
//! those events, testable without a FIT fixture (the byte-level decoder gets
//! its own fixture in stage 1b).
//!
//! The traps, all encoded below:
//! - Heart rate AND the incremental activity rows (steps, active time,
//!   moderate/vigorous minutes) ride on `timestamp_16`, the low 16 bits of
//!   the FIT timestamp, which wraps every 18.2 h; it unrolls against the
//!   last full timestamp seen in file order; the anchor advances with
//!   every unrolled reading the way Garmin's SDK does, and every full
//!   timestamp resets it (a full timestamp can step BACK a minute in these
//!   files, which the unroll tolerates; a single wild 16-bit reading must
//!   not poison the rest of the file, which the reset guarantees).
//! - `heart_rate = 0` is "no reading" and 255 the invalid marker;
//!   `stress_level_value` −1/−2 is "unavailable"; neither is a sample.
//! - The activity counters (`steps`/`cycles`, `distance`, `active_time`,
//!   `active_calories`) are RUNNING DAY TOTALS per activity type, on the
//!   16-bit rows as much as on the full-stamped sync rows — never
//!   increments. A day's value is the maximum, not the sum (summing gave a
//!   tenfold step count). Moderate/vigorous minutes are the opposite: the
//!   named fields are one-minute increments, and the running totals sit in
//!   the unnamed fields 37/38 — which fitparser names in only some files,
//!   so the totals are read from the unnamed fields directly.
//! - The time zone: NOT from `MonitoringInfo.local_timestamp` (the device
//!   writes it a constant hour off UTC, fitparser shows it 2 h off local).
//!   The caller passes the offset it believes in — the nearest activity's
//!   explicit offset from the vault, else the machine's — and the file can
//!   only CONFIRM it: Garmin cuts these files at local midnight (the
//!   `…0000.FIT` file ENDS at 00:00, the next one starts there), so a file
//!   end sits at midnight under the right offset. A sample near a quarter
//!   hour is not evidence on its own (a watch put on at 18:28 starts a
//!   series there too), so no offset is ever derived from the data — see
//!   [`sits_at_local_midnight`]. Unconfirmed is normal: a file cut at a
//!   sync time confirms nothing.

use fitparser::profile::MesgNum;
use fitparser::{FitDataRecord, Value};

/// Unix time of the FIT epoch (1989-12-31T00:00:00Z).
pub const FIT_EPOCH_UNIX: i64 = 631_065_600;

/// Heart-rate readings outside this range are the device's invalid markers
/// (0 = no reading, 255 = invalid) or nonsense, never a person.
const HR_VALID: std::ops::RangeInclusive<u8> = 20..=230;

/// What a FIT byte buffer is, from its `file_id` message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FitFileType {
    Activity,
    MonitoringB,
    /// Anything else (`settings`, `sleep`, `course`, …), by its FIT name.
    Other(String),
}

/// One reading with its unix timestamp (UTC).
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub ts: i64,
    pub value: f64,
    /// SpO2 reading confidence as the device reports it — a device-specific
    /// scale (the fenix 6 writes 0–65, median ≈ 10), not a percentage.
    pub confidence: Option<i64>,
}

/// Garmin's resting-heart-rate estimates, written a few times a day.
#[derive(Debug, Clone, PartialEq)]
pub struct RhrReading {
    pub ts: i64,
    pub current_day: Option<i64>,
    pub seven_day: Option<i64>,
}

/// A running day-so-far total for one activity type (`generic`, `walking`,
/// `running`, …) at `ts`. Garmin writes these on the 16-bit rows as the
/// day goes AND on the full-stamped rows at sync time; the day's value is
/// the MAXIMUM per (day, activity type), never a sum.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ActivityTotal {
    pub ts: i64,
    pub activity_type: Option<String>,
    /// `steps`, or `cycles` × 2 where the device wrote that instead
    /// (fitparser applies the profile's 0.5 scale to `cycles`; for walking
    /// and running rows one raw unit is one step — a swim's strokes would
    /// need their own rule).
    pub steps: Option<f64>,
    pub distance_m: Option<f64>,
    pub active_calories: Option<f64>,
    pub active_time_s: Option<f64>,
}

impl ActivityTotal {
    fn has_data(&self) -> bool {
        self.steps.is_some()
            || self.distance_m.is_some()
            || self.active_calories.is_some()
            || self.active_time_s.is_some()
    }
}

/// Garmin's per-minute activity marker: 0 = sedentary … 7 = highly active.
#[derive(Debug, Clone, PartialEq)]
pub struct IntensityMark {
    pub ts: i64,
    pub activity_type: Option<String>,
    pub intensity: i64,
}

/// Moderate / vigorous activity minutes at `ts`: the day-so-far totals
/// (unnamed fields 37/38 — see the module docs) and, where the device
/// wrote them, the one-minute increments (`moderate_activity_minutes`,
/// `vigorous_activity_minutes`). The day's value is the maximum of the
/// totals; the increments are a fallback to sum when no total exists.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ActiveMinutes {
    pub ts: i64,
    pub moderate_total: Option<f64>,
    pub vigorous_total: Option<f64>,
    pub moderate_inc: Option<f64>,
    pub vigorous_inc: Option<f64>,
}

impl ActiveMinutes {
    fn has_data(&self) -> bool {
        self.moderate_total.is_some()
            || self.vigorous_total.is_some()
            || self.moderate_inc.is_some()
            || self.vigorous_inc.is_some()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedMonitoring {
    pub device_serial: Option<String>,
    pub device_product: Option<String>,
    /// UTC offset of the device's local time, seconds east of UTC — the
    /// caller's offset, echoed back.
    pub tz_offset_s: i32,
    /// A file end (first/last full timestamp, first/last sample) sits at
    /// local midnight under that offset: the file was cut there, which is
    /// what Garmin does, so the offset is consistent with the data. False
    /// for a file cut at a sync time — normal, not an error.
    pub tz_confirmed: bool,
    /// Span of everything timestamped in the file — full timestamps and
    /// unrolled readings alike — unix seconds.
    pub first_ts: Option<i64>,
    pub last_ts: Option<i64>,
    pub hr: Vec<Sample>,
    pub stress: Vec<Sample>,
    pub respiration: Vec<Sample>,
    pub spo2: Vec<Sample>,
    pub rhr: Vec<RhrReading>,
    pub totals: Vec<ActivityTotal>,
    pub intensity: Vec<IntensityMark>,
    pub active_minutes: Vec<ActiveMinutes>,
}

/// When a reading happened: its own full timestamp, or the low 16 bits to
/// unroll against the running anchor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stamp {
    Full(i64),
    Low16(u16),
}

/// Decoder-level events, in FILE ORDER. The pure [`assemble`] consumes them.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    FileId {
        serial: Option<String>,
        product: Option<String>,
    },
    /// A full `timestamp` on a message that carries nothing else we keep —
    /// still an anchor for the 16-bit stamps that follow.
    FullTimestamp(i64),
    Hr {
        at: Stamp,
        bpm: u8,
    },
    Stress {
        ts: i64,
        value: i64,
    },
    Respiration {
        ts: i64,
        rate: f64,
    },
    Spo2 {
        ts: i64,
        spo2: f64,
        confidence: Option<i64>,
    },
    Rhr {
        ts: i64,
        current_day: Option<i64>,
        seven_day: Option<i64>,
    },
    /// `ts` inside is filled in by [`assemble`] from `at`.
    Total {
        at: Stamp,
        total: ActivityTotal,
    },
    Intensity {
        at: Stamp,
        activity_type: Option<String>,
        intensity: i64,
    },
    ActiveMinutes {
        at: Stamp,
        minutes: ActiveMinutes,
    },
}

/// Decode a FIT buffer once; the pipeline routes on [`file_type_of`] and
/// then hands the same messages to [`parse_monitoring_messages`] (or to
/// the activity parser) without parsing twice.
pub fn parse_fit_messages(data: &[u8]) -> Result<Vec<FitDataRecord>, String> {
    fitparser::from_bytes(data).map_err(|e| format!("Failed to parse FIT: {}", e))
}

/// The file's kind from its `file_id` message.
pub fn file_type_of(messages: &[FitDataRecord]) -> Result<FitFileType, String> {
    let file_id = messages
        .iter()
        .find(|m| m.kind() == MesgNum::FileId)
        .ok_or_else(|| "FIT file has no file_id message".to_string())?;
    let kind = field(file_id, "type")
        .map(|f| format!("{}", f.value()))
        .unwrap_or_default();
    Ok(match kind.as_str() {
        "activity" => FitFileType::Activity,
        "monitoring_b" => FitFileType::MonitoringB,
        other => FitFileType::Other(other.to_string()),
    })
}

/// Convenience over [`parse_fit_messages`] + [`file_type_of`].
pub fn detect_fit_file_type(data: &[u8]) -> Result<FitFileType, String> {
    file_type_of(&parse_fit_messages(data)?)
}

/// Assemble already-decoded messages. `tz_offset_s` is the offset the
/// caller believes in (seconds east of UTC) — the nearest activity's
/// explicit offset from the vault, else the machine's.
pub fn parse_monitoring_messages(
    messages: &[FitDataRecord],
    tz_offset_s: i32,
) -> ParsedMonitoring {
    assemble(decode(messages), tz_offset_s)
}

/// Decode + assemble a Monitor file in one go.
pub fn parse_monitoring_bytes(data: &[u8], tz_offset_s: i32) -> Result<ParsedMonitoring, String> {
    Ok(parse_monitoring_messages(&parse_fit_messages(data)?, tz_offset_s))
}

/// A `timestamp_16` more than this far "ahead" of the anchor is really a
/// reading a little BEFORE it (the anchor stepped back), not 18 h later.
const BACKSTEP_TOLERANCE_S: i64 = 3600;

/// Unroll a `timestamp_16` (low 16 bits of the FIT timestamp) against the
/// running anchor. Readings are at or after the anchor and within 18.2 h
/// of it, so the wrapped difference is the delta — except that Garmin's
/// full timestamps occasionally step back a minute, which would make a
/// reading just before the new anchor look like one 18 h later; a delta in
/// the last hour of the wrap is therefore read as a small step back.
pub fn unroll_timestamp16(anchor_unix: i64, t16: u16) -> i64 {
    let anchor_fit = anchor_unix - FIT_EPOCH_UNIX;
    let delta = (i64::from(t16) - (anchor_fit & 0xFFFF)) & 0xFFFF;
    let signed = if delta > 0xFFFF - BACKSTEP_TOLERANCE_S {
        delta - 0x1_0000
    } else {
        delta
    };
    anchor_fit + signed + FIT_EPOCH_UNIX
}

/// The local calendar day a RUNNING TOTAL at `ts` belongs to, as days
/// since the Unix epoch. Garmin cuts a file at local midnight and stamps
/// its closing totals with exactly 00:00 of the NEXT day — so a total at
/// midnight is the previous day's final value, not the new day's first.
/// Aggregating by the naive local date put 4123 steps of June 20 onto
/// June 21 and overstated 79 days of walking by a quarter.
pub fn local_day_of_total(ts: i64, tz_offset_s: i32) -> i64 {
    const DAY: i64 = 86_400;
    let local = ts + i64::from(tz_offset_s);
    // 00:00:00 exactly (and up to 3 min after, the same file-cut slack as
    // the midnight check) closes the day before.
    (local - 181).div_euclid(DAY)
}

/// The local calendar day of an ordinary reading at `ts` — a heart-rate
/// or stress sample at 00:01 belongs to the new day.
pub fn local_day_of_sample(ts: i64, tz_offset_s: i32) -> i64 {
    (ts + i64::from(tz_offset_s)).div_euclid(86_400)
}

/// Whether `ts` falls within 3 minutes of local midnight under
/// `tz_offset_s`. Garmin cuts monitoring files at local midnight, so a
/// file whose ends sit there is consistent with the offset. A sample near
/// a quarter hour proves nothing else — the offset is never derived from
/// the data, only confirmed.
pub fn sits_at_local_midnight(ts: i64, tz_offset_s: i32) -> bool {
    const DAY: i64 = 86_400;
    let local = (ts + i64::from(tz_offset_s)).rem_euclid(DAY);
    local <= 180 || local >= DAY - 180
}

/// Pure assembly of decoder events into a [`ParsedMonitoring`].
pub fn assemble(events: Vec<Event>, tz_offset_s: i32) -> ParsedMonitoring {
    let mut out = ParsedMonitoring {
        device_serial: None,
        device_product: None,
        tz_offset_s,
        tz_confirmed: false,
        first_ts: None,
        last_ts: None,
        hr: Vec::new(),
        stress: Vec::new(),
        respiration: Vec::new(),
        spo2: Vec::new(),
        rhr: Vec::new(),
        totals: Vec::new(),
        intensity: Vec::new(),
        active_minutes: Vec::new(),
    };
    // The running anchor for 16-bit stamps (see `resolve`).
    let mut anchor: Option<i64> = None;
    for ev in events {
        match ev {
            Event::FileId { serial, product } => {
                out.device_serial = serial;
                out.device_product = product;
            }
            Event::FullTimestamp(ts) => {
                resolve(Stamp::Full(ts), &mut anchor);
                out.span(ts);
            }
            Event::Hr { at, bpm } => {
                let Some(ts) = resolve(at, &mut anchor) else { continue };
                out.span(ts);
                if HR_VALID.contains(&bpm) {
                    out.hr.push(Sample { ts, value: f64::from(bpm), confidence: None });
                }
            }
            Event::Total { at, mut total } => {
                let Some(ts) = resolve(at, &mut anchor) else { continue };
                out.span(ts);
                if total.has_data() {
                    total.ts = ts;
                    out.totals.push(total);
                }
            }
            Event::Intensity { at, activity_type, intensity } => {
                let Some(ts) = resolve(at, &mut anchor) else { continue };
                out.span(ts);
                out.intensity.push(IntensityMark { ts, activity_type, intensity });
            }
            Event::ActiveMinutes { at, mut minutes } => {
                let Some(ts) = resolve(at, &mut anchor) else { continue };
                out.span(ts);
                if minutes.has_data() {
                    minutes.ts = ts;
                    out.active_minutes.push(minutes);
                }
            }
            Event::Stress { ts, value } => {
                out.span(ts);
                if (0..=100).contains(&value) {
                    out.stress.push(Sample { ts, value: value as f64, confidence: None });
                }
            }
            Event::Respiration { ts, rate } => {
                out.span(ts);
                if rate.is_finite() && rate > 0.0 {
                    out.respiration.push(Sample { ts, value: rate, confidence: None });
                }
            }
            Event::Spo2 { ts, spo2, confidence } => {
                out.span(ts);
                if spo2.is_finite() && spo2 > 0.0 {
                    out.spo2.push(Sample { ts, value: spo2, confidence });
                }
            }
            Event::Rhr { ts, current_day, seven_day } => {
                out.span(ts);
                out.rhr.push(RhrReading { ts, current_day, seven_day });
            }
        }
    }
    // Canonical order, one reading per timestamp. Within one file Garmin
    // never repeats a timestamp in practice; the cross-file overlap (the
    // daily file and the sync-time file of one day) is resolved by the
    // database's unique key in stage 2, not here.
    canonicalize(&mut out.hr);
    canonicalize(&mut out.stress);
    canonicalize(&mut out.respiration);
    canonicalize(&mut out.spo2);
    out.totals.sort_by_key(|r| r.ts);
    out.intensity.sort_by_key(|r| r.ts);
    out.active_minutes.sort_by_key(|r| r.ts);
    // A file is cut at a sync or at local midnight, so its ends are where
    // the cut shows: the full-timestamp span, and the sample series (the
    // watch can be worn across midnight while the head rows carry the last
    // sync time).
    let series = out.hr.iter().chain(out.stress.iter()).map(|s| s.ts);
    let (first, last) =
        series.fold((None, None), |(lo, hi): (Option<i64>, Option<i64>), t| {
            (Some(lo.map_or(t, |l| l.min(t))), Some(hi.map_or(t, |h| h.max(t))))
        });
    out.tz_confirmed = [out.first_ts, out.last_ts, first, last]
        .into_iter()
        .flatten()
        .any(|t| sits_at_local_midnight(t, tz_offset_s));
    out
}

impl ParsedMonitoring {
    fn span(&mut self, ts: i64) {
        self.first_ts = Some(self.first_ts.map_or(ts, |f| f.min(ts)));
        self.last_ts = Some(self.last_ts.map_or(ts, |l| l.max(ts)));
    }
}

/// Resolve a stamp against the anchor. A full timestamp is authoritative
/// and RESETS the anchor (so one wild 16-bit reading cannot poison the
/// rest of the file); an unrolled reading only advances it, never steps it
/// back. None when a 16-bit stamp arrives before any anchor — such a
/// reading has no absolute time, and inventing one would be worse than
/// dropping it.
fn resolve(at: Stamp, anchor: &mut Option<i64>) -> Option<i64> {
    match at {
        Stamp::Full(ts) => {
            *anchor = Some(ts);
            Some(ts)
        }
        Stamp::Low16(t16) => {
            let ts = unroll_timestamp16((*anchor)?, t16);
            *anchor = Some(anchor.map_or(ts, |a| a.max(ts)));
            Some(ts)
        }
    }
}

/// Sort by time and keep the LAST reading per timestamp (stable sort keeps
/// file order within equal timestamps, so a later write wins).
fn canonicalize(samples: &mut Vec<Sample>) {
    samples.sort_by_key(|s| s.ts);
    let mut kept: Vec<Sample> = Vec::with_capacity(samples.len());
    for s in samples.drain(..) {
        match kept.last_mut() {
            Some(last) if last.ts == s.ts => *last = s,
            _ => kept.push(s),
        }
    }
    *samples = kept;
}

// ---- thin decoder -----------------------------------------------------------

fn decode(messages: &[FitDataRecord]) -> Vec<Event> {
    let mut events = Vec::new();
    for msg in messages {
        match msg.kind() {
            MesgNum::FileId => {
                let serial = field(msg, "serial_number").map(|f| format!("{}", f.value()));
                let product = field(msg, "garmin_product")
                    .or_else(|| field(msg, "product"))
                    .map(|f| format!("{}", f.value()));
                events.push(Event::FileId { serial, product });
            }
            MesgNum::Monitoring => decode_monitoring(msg, &mut events),
            MesgNum::MonitoringHrData => {
                if let Some(ts) = field_unix(msg, "timestamp") {
                    events.push(Event::Rhr {
                        ts,
                        current_day: field_i64(msg, "current_day_resting_heart_rate"),
                        seven_day: field_i64(msg, "resting_heart_rate"),
                    });
                }
            }
            MesgNum::StressLevel => {
                if let (Some(ts), Some(value)) = (
                    field_unix(msg, "stress_level_time"),
                    field_i64(msg, "stress_level_value"),
                ) {
                    events.push(Event::Stress { ts, value });
                }
            }
            MesgNum::RespirationRate => {
                if let (Some(ts), Some(rate)) =
                    (field_unix(msg, "timestamp"), field_f64(msg, "respiration_rate"))
                {
                    events.push(Event::Respiration { ts, rate });
                }
            }
            MesgNum::Spo2Data => {
                if let (Some(ts), Some(spo2)) =
                    (field_unix(msg, "timestamp"), field_f64(msg, "reading_spo2"))
                {
                    events.push(Event::Spo2 {
                        ts,
                        spo2,
                        confidence: field_i64(msg, "reading_confidence"),
                    });
                }
            }
            _ => {
                // Any other message with a full timestamp (MonitoringInfo,
                // DeviceInfo, OhrSettings, …) still anchors the 16-bit
                // stamps that follow it.
                if let Some(ts) = field_unix(msg, "timestamp") {
                    events.push(Event::FullTimestamp(ts));
                }
            }
        }
    }
    events
}

fn decode_monitoring(msg: &FitDataRecord, events: &mut Vec<Event>) {
    let at = match (field_unix(msg, "timestamp"), field_i64(msg, "timestamp_16")) {
        (Some(ts), _) => Stamp::Full(ts),
        (None, Some(t16)) => match u16::try_from(t16) {
            Ok(t16) => Stamp::Low16(t16),
            Err(_) => return,
        },
        (None, None) => return,
    };
    let mut carried = false;
    if let Some(bpm) = field_i64(msg, "heart_rate").and_then(|v| u8::try_from(v).ok()) {
        events.push(Event::Hr { at, bpm });
        carried = true;
    }
    let activity_type = field_string(msg, "activity_type");
    let total = ActivityTotal {
        ts: 0,
        activity_type: activity_type.clone(),
        steps: field_f64(msg, "steps").or_else(|| field_f64(msg, "cycles").map(|c| c * 2.0)),
        distance_m: field_f64(msg, "distance"),
        active_calories: field_f64(msg, "active_calories"),
        active_time_s: field_f64(msg, "active_time"),
    };
    if total.has_data() {
        events.push(Event::Total { at, total });
        carried = true;
    }
    if let Some(intensity) = field_i64(msg, "intensity") {
        events.push(Event::Intensity { at, activity_type, intensity });
        carried = true;
    }
    // Running totals live in the profile-unnamed fields 37/38 — read by
    // field NUMBER, so a fitparser release that learns their names cannot
    // silently turn them into nothing. fitparser resolves the named
    // increment fields in only some files, and one message may carry both.
    let minutes = ActiveMinutes {
        ts: 0,
        moderate_total: field_f64_by_number(msg, 37),
        vigorous_total: field_f64_by_number(msg, 38),
        moderate_inc: field_f64(msg, "moderate_activity_minutes"),
        vigorous_inc: field_f64(msg, "vigorous_activity_minutes"),
    };
    if minutes.has_data() {
        events.push(Event::ActiveMinutes { at, minutes });
        carried = true;
    }
    if !carried {
        if let Stamp::Full(ts) = at {
            events.push(Event::FullTimestamp(ts));
        }
    }
}

fn field<'a>(msg: &'a FitDataRecord, name: &str) -> Option<&'a fitparser::FitDataField> {
    msg.fields().iter().find(|f| f.name() == name)
}

fn field_unix(msg: &FitDataRecord, name: &str) -> Option<i64> {
    match field(msg, name)?.value() {
        Value::Timestamp(dt) => Some(dt.timestamp()),
        Value::UInt32(secs) => Some(i64::from(*secs) + FIT_EPOCH_UNIX),
        _ => None,
    }
}

fn field_f64(msg: &FitDataRecord, name: &str) -> Option<f64> {
    value_f64(field(msg, name)?.value())
}

fn field_f64_by_number(msg: &FitDataRecord, number: u8) -> Option<f64> {
    msg.fields()
        .iter()
        .find(|f| f.number() == number)
        .and_then(|f| value_f64(f.value()))
}

fn field_i64(msg: &FitDataRecord, name: &str) -> Option<i64> {
    value_f64(field(msg, name)?.value()).map(|v| v as i64)
}

fn field_string(msg: &FitDataRecord, name: &str) -> Option<String> {
    match field(msg, name)?.value() {
        Value::String(s) => Some(s.clone()),
        v @ Value::Enum(_) => Some(format!("{}", v)),
        _ => None,
    }
}

fn value_f64(v: &Value) -> Option<f64> {
    match v {
        Value::SInt8(x) => Some(f64::from(*x)),
        Value::UInt8(x) => Some(f64::from(*x)),
        Value::SInt16(x) => Some(f64::from(*x)),
        Value::UInt16(x) => Some(f64::from(*x)),
        Value::SInt32(x) => Some(f64::from(*x)),
        Value::UInt32(x) => Some(f64::from(*x)),
        Value::SInt64(x) => Some(*x as f64),
        Value::UInt64(x) => Some(*x as f64),
        Value::Float32(x) => Some(f64::from(*x)),
        Value::Float64(x) => Some(*x),
        Value::UInt8z(x) => Some(f64::from(*x)),
        Value::UInt16z(x) => Some(f64::from(*x)),
        Value::UInt32z(x) => Some(f64::from(*x)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-09-04T21:00:00Z = local midnight of 2026-09-05 at +03:00.
    const MIDNIGHT_PLUS3: i64 = 1_788_555_600;
    const PLUS3: i32 = 3 * 3600;

    fn low16(ts: i64) -> u16 {
        ((ts - FIT_EPOCH_UNIX) & 0xFFFF) as u16
    }

    fn hr(ts: i64, bpm: u8) -> Event {
        Event::Hr { at: Stamp::Full(ts), bpm }
    }

    fn hr16(ts: i64, bpm: u8) -> Event {
        Event::Hr { at: Stamp::Low16(low16(ts)), bpm }
    }

    fn readings(out: &ParsedMonitoring) -> Vec<(i64, f64)> {
        out.hr.iter().map(|s| (s.ts, s.value)).collect()
    }

    #[test]
    fn detect_rejects_garbage() {
        assert!(detect_fit_file_type(b"not a fit file").is_err());
        assert!(detect_fit_file_type(&[]).is_err());
    }

    #[test]
    fn timestamp16_unrolls_forward_within_the_wrap() {
        let anchor = FIT_EPOCH_UNIX + 100_000; // low 16 bits = 34_464
        assert_eq!(unroll_timestamp16(anchor, 34_464), anchor);
        assert_eq!(unroll_timestamp16(anchor, 34_464 + 90), anchor + 90);
        // Past 0xFFFF: 100_000 + 40_000 = 140_000 → low bits 8_928.
        assert_eq!(unroll_timestamp16(anchor, 8_928), anchor + 40_000);
        // Up to 17.2 h ahead is still ahead.
        assert_eq!(unroll_timestamp16(anchor, low16(anchor + 61_000)), anchor + 61_000);
    }

    #[test]
    fn timestamp16_reads_the_last_hour_of_the_wrap_as_a_step_back() {
        let anchor = FIT_EPOCH_UNIX + 100_000;
        // A reading 60 s before the anchor (the anchor stepped back a
        // minute) is 60 s back, not 18 h ahead.
        assert_eq!(unroll_timestamp16(anchor, low16(anchor - 60)), anchor - 60);
        assert_eq!(unroll_timestamp16(anchor, low16(anchor - 3599)), anchor - 3599);
        // Beyond the tolerance it is what it says: far ahead.
        assert_eq!(
            unroll_timestamp16(anchor, low16(anchor - 3601)),
            anchor - 3601 + 0x1_0000
        );
    }

    #[test]
    fn a_total_stamped_at_midnight_belongs_to_the_day_it_closes() {
        // 2026-09-05 00:00 +03:00 = the cut that closes 2026-09-04.
        let sep4 = local_day_of_sample(MIDNIGHT_PLUS3 - 3600, PLUS3);
        let sep5 = sep4 + 1;
        assert_eq!(local_day_of_total(MIDNIGHT_PLUS3, PLUS3), sep4);
        assert_eq!(local_day_of_total(MIDNIGHT_PLUS3 + 120, PLUS3), sep4);
        assert_eq!(local_day_of_total(MIDNIGHT_PLUS3 + 600, PLUS3), sep5);
        assert_eq!(local_day_of_total(MIDNIGHT_PLUS3 - 60, PLUS3), sep4);
        // A plain sample at 00:01 is already the new day.
        assert_eq!(local_day_of_sample(MIDNIGHT_PLUS3 + 60, PLUS3), sep5);
        // The rule follows the offset, not UTC: the same instant at UTC is
        // still 21:00 of the day before.
        assert_eq!(local_day_of_total(MIDNIGHT_PLUS3, 0), sep4);
        // Two totals of one type on either side of the cut: the closing one
        // is the earlier day's maximum, not the later day's.
        let totals = [
            (MIDNIGHT_PLUS3 - 7200, 3000.0),
            (MIDNIGHT_PLUS3, 4123.0),
            (MIDNIGHT_PLUS3 + 900, 12.0),
        ];
        let mut by_day = std::collections::BTreeMap::new();
        for (ts, steps) in totals {
            let e = by_day.entry(local_day_of_total(ts, PLUS3)).or_insert(0.0_f64);
            *e = e.max(steps);
        }
        assert_eq!(by_day.get(&sep4), Some(&4123.0));
        assert_eq!(by_day.get(&sep5), Some(&12.0));
    }

    #[test]
    fn midnight_check_is_a_3_minute_window_under_the_given_offset() {
        assert!(sits_at_local_midnight(MIDNIGHT_PLUS3, PLUS3));
        assert!(sits_at_local_midnight(MIDNIGHT_PLUS3 + 179, PLUS3));
        assert!(sits_at_local_midnight(MIDNIGHT_PLUS3 - 120, PLUS3));
        assert!(!sits_at_local_midnight(MIDNIGHT_PLUS3 + 181, PLUS3));
        // The same instant is 21:00 at UTC and 19:00 at −02:00.
        assert!(!sits_at_local_midnight(MIDNIGHT_PLUS3, 0));
        assert!(sits_at_local_midnight(MIDNIGHT_PLUS3 + 8 * 3600, -5 * 3600));
    }

    #[test]
    fn assemble_anchors_16bit_stamps_and_drops_invalid_heart_rates() {
        let anchor = MIDNIGHT_PLUS3;
        let out = assemble(
            vec![
                hr16(anchor, 70), // before any anchor — dropped
                Event::FullTimestamp(anchor),
                hr16(anchor + 60, 0),   // no reading
                hr16(anchor + 90, 255), // invalid marker
                hr16(anchor + 120, 58),
                hr(anchor + 3600, 55),
                hr16(anchor + 3660, 57),
            ],
            PLUS3,
        );
        assert_eq!(
            readings(&out),
            vec![(anchor + 120, 58.0), (anchor + 3600, 55.0), (anchor + 3660, 57.0)]
        );
        // The span covers unrolled readings too.
        assert_eq!((out.first_ts, out.last_ts), (Some(anchor), Some(anchor + 3660)));
    }

    #[test]
    fn assemble_keeps_the_anchor_from_stepping_back() {
        let t = MIDNIGHT_PLUS3;
        let out = assemble(
            vec![
                Event::FullTimestamp(t + 600),
                hr16(t + 660, 60),
                // A full timestamp a minute EARLIER than the last reading —
                // Garmin does this. It resets the anchor, and the unroll's
                // step-back tolerance keeps the next reading in place…
                Event::FullTimestamp(t + 600),
                hr16(t + 720, 61),
                // And a reading a few seconds before the anchor is a step
                // back, not a wrap ahead.
                hr16(t + 700, 62),
            ],
            PLUS3,
        );
        assert_eq!(readings(&out), vec![(t + 660, 60.0), (t + 700, 62.0), (t + 720, 61.0)]);
    }

    #[test]
    fn assemble_drops_stress_sentinels_and_bad_readings() {
        let t = MIDNIGHT_PLUS3;
        let out = assemble(
            vec![
                Event::Stress { ts: t, value: -1 },
                Event::Stress { ts: t + 180, value: -2 },
                Event::Stress { ts: t + 360, value: 23 },
                Event::Stress { ts: t + 540, value: 101 },
                Event::Respiration { ts: t, rate: 0.0 },
                Event::Respiration { ts: t + 60, rate: 15.5 },
                Event::Spo2 { ts: t, spo2: 0.0, confidence: Some(0) },
                Event::Spo2 { ts: t + 60, spo2: 96.0, confidence: Some(12) },
            ],
            PLUS3,
        );
        assert_eq!(out.stress, vec![Sample { ts: t + 360, value: 23.0, confidence: None }]);
        assert_eq!(
            out.respiration,
            vec![Sample { ts: t + 60, value: 15.5, confidence: None }]
        );
        assert_eq!(out.spo2, vec![Sample { ts: t + 60, value: 96.0, confidence: Some(12) }]);
    }

    #[test]
    fn assemble_drops_invalid_heart_rates_on_full_stamps_too() {
        let t = MIDNIGHT_PLUS3;
        let out = assemble(
            vec![hr(t, 0), hr(t + 60, 255), hr(t + 120, 19), hr(t + 180, 48)],
            PLUS3,
        );
        assert_eq!(readings(&out), vec![(t + 180, 48.0)]);
        // Dropped readings still count toward the span — they were stamped.
        assert_eq!((out.first_ts, out.last_ts), (Some(t), Some(t + 180)));
    }

    #[test]
    fn assemble_keeps_the_later_reading_per_timestamp_and_sorts() {
        let t = MIDNIGHT_PLUS3;
        let out = assemble(vec![hr(t + 120, 60), hr(t, 52), hr(t + 120, 61)], PLUS3);
        assert_eq!(readings(&out), vec![(t, 52.0), (t + 120, 61.0)]);
    }

    #[test]
    fn assemble_keeps_totals_intensity_and_minutes_apart_and_drops_empty_rows() {
        let t = MIDNIGHT_PLUS3;
        let walking = ActivityTotal {
            activity_type: Some("walking".into()),
            steps: Some(290.0),
            active_time_s: Some(780.0),
            active_calories: Some(15.0),
            ..Default::default()
        };
        let minutes = ActiveMinutes {
            moderate_total: Some(2.0),
            vigorous_total: Some(4.0),
            moderate_inc: Some(1.0),
            vigorous_inc: Some(3.0),
            ..Default::default()
        };
        let out = assemble(
            vec![
                Event::FullTimestamp(t),
                Event::Total { at: Stamp::Low16(low16(t + 780)), total: walking.clone() },
                Event::Total { at: Stamp::Full(t + 60), total: ActivityTotal::default() },
                Event::Intensity {
                    at: Stamp::Full(t + 120),
                    activity_type: Some("sedentary".into()),
                    intensity: 0,
                },
                Event::ActiveMinutes { at: Stamp::Low16(low16(t + 900)), minutes: minutes.clone() },
                Event::ActiveMinutes { at: Stamp::Full(t + 30), minutes: ActiveMinutes::default() },
            ],
            PLUS3,
        );
        assert_eq!(out.totals, vec![ActivityTotal { ts: t + 780, ..walking }]);
        assert_eq!(
            out.intensity,
            vec![IntensityMark {
                ts: t + 120,
                activity_type: Some("sedentary".into()),
                intensity: 0
            }]
        );
        assert_eq!(out.active_minutes, vec![ActiveMinutes { ts: t + 900, ..minutes }]);
    }

    #[test]
    fn a_full_stamp_resets_a_poisoned_anchor() {
        let t = MIDNIGHT_PLUS3;
        let out = assemble(
            vec![
                Event::FullTimestamp(t),
                // A wild reading 10 h ahead advances the anchor…
                hr16(t + 36_000, 60),
                // …but the next full timestamp brings it back, so readings
                // after it land where they belong.
                Event::FullTimestamp(t + 120),
                hr16(t + 180, 61),
            ],
            PLUS3,
        );
        assert_eq!(readings(&out), vec![(t + 180, 61.0), (t + 36_000, 60.0)]);
    }

    #[test]
    fn assemble_confirms_the_offset_only_when_a_file_end_sits_at_midnight() {
        // Head rows are stamped with the last sync (22:08 the evening before)
        // — never evidence. The series ending at 00:00 is (the `…0000` file).
        let head = MIDNIGHT_PLUS3 - 6720;
        let ends_at_midnight = assemble(
            vec![Event::FullTimestamp(head), hr(head + 120, 70), hr(MIDNIGHT_PLUS3 - 60, 52)],
            PLUS3,
        );
        assert_eq!(
            (ends_at_midnight.tz_offset_s, ends_at_midnight.tz_confirmed),
            (PLUS3, true)
        );
        // The next file starts at 00:00 — confirmed too.
        assert!(assemble(vec![hr(MIDNIGHT_PLUS3 + 60, 52)], PLUS3).tz_confirmed);
        // Same data under a wrong offset: not confirmed, offset echoed as given.
        let wrong = assemble(vec![hr(MIDNIGHT_PLUS3 + 60, 52)], 7200);
        assert_eq!((wrong.tz_offset_s, wrong.tz_confirmed), (7200, false));
        // Watch put on at 18:28: near a quarter hour, still no confirmation.
        let evening = assemble(vec![hr(MIDNIGHT_PLUS3 + 15 * 3600 + 28 * 60, 70)], PLUS3);
        assert!(!evening.tz_confirmed);
        // No samples and a sync-time head → nothing to confirm…
        assert!(!assemble(vec![Event::FullTimestamp(head)], PLUS3).tz_confirmed);
        // …but a file whose full timestamps run midnight to midnight is cut
        // there even with the watch off at night.
        let cut = assemble(
            vec![
                Event::FullTimestamp(MIDNIGHT_PLUS3),
                Event::FullTimestamp(MIDNIGHT_PLUS3 + 86_400),
            ],
            PLUS3,
        );
        assert!(cut.tz_confirmed);
    }

    #[test]
    fn assemble_carries_identity_and_rhr() {
        let t = MIDNIGHT_PLUS3;
        let out = assemble(
            vec![
                Event::FileId { serial: Some("123".into()), product: Some("fenix6x".into()) },
                Event::Rhr { ts: t + 300, current_day: Some(49), seven_day: Some(62) },
            ],
            PLUS3,
        );
        assert_eq!(out.device_serial.as_deref(), Some("123"));
        assert_eq!(out.device_product.as_deref(), Some("fenix6x"));
        assert_eq!(
            out.rhr,
            vec![RhrReading { ts: t + 300, current_day: Some(49), seven_day: Some(62) }]
        );
        assert_eq!((out.first_ts, out.last_ts), (Some(t + 300), Some(t + 300)));
    }

    /// Smoke test on a real Monitor file — run by hand:
    /// `SYZIFY_MONITOR_FIT=/path/M9500000.FIT SYZIFY_TZ_OFFSET=10800 \
    ///  cargo test --lib monitoring -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn smoke_real_monitor_file() {
        let Ok(path) = std::env::var("SYZIFY_MONITOR_FIT") else {
            eprintln!("SYZIFY_MONITOR_FIT not set — skipping");
            return;
        };
        let data = std::fs::read(&path).expect("read monitor file");
        let messages = parse_fit_messages(&data).expect("parse");
        assert_eq!(file_type_of(&messages).unwrap(), FitFileType::MonitoringB);
        let tz: i32 = std::env::var("SYZIFY_TZ_OFFSET")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let out = parse_monitoring_messages(&messages, tz);
        // Day values = MAX of the running totals per activity type.
        let mut steps_by_type: std::collections::BTreeMap<String, f64> = Default::default();
        for r in &out.totals {
            if let (Some(ty), Some(st)) = (&r.activity_type, r.steps) {
                let e = steps_by_type.entry(ty.clone()).or_insert(0.0);
                *e = e.max(st);
            }
        }
        let vigorous = out
            .active_minutes
            .iter()
            .filter_map(|m| m.vigorous_total)
            .fold(None, |a: Option<f64>, v| Some(a.map_or(v, |x| x.max(v))));
        eprintln!(
            "steps by type (max of running totals): {steps_by_type:?}; \
             vigorous total: {vigorous:?}"
        );
        eprintln!(
            "{path}: device {:?}/{:?} tz {:+}s confirmed={} span {:?}..{:?} \
             hr {} stress {} resp {} spo2 {} rhr {} totals {} intensity {} minutes {}",
            out.device_product,
            out.device_serial,
            out.tz_offset_s,
            out.tz_confirmed,
            out.first_ts,
            out.last_ts,
            out.hr.len(),
            out.stress.len(),
            out.respiration.len(),
            out.spo2.len(),
            out.rhr.len(),
            out.totals.len(),
            out.intensity.len(),
            out.active_minutes.len()
        );
        assert!(!out.hr.is_empty());
        // The 2026-09-05 13:37 sync file, counted independently from a
        // message dump: a lost or duplicated reading shows up here.
        if path.ends_with("M95D3745.FIT") {
            let counts = (
                out.hr.len(),
                out.stress.len(),
                out.respiration.len(),
                out.spo2.len(),
                out.rhr.len(),
            );
            assert_eq!(counts, (452, 528, 528, 276, 1));
            assert!(out.tz_confirmed);
        }
    }
}
