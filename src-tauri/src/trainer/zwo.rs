//! Zwift workout files (`.zwo`): an XML list of intervals, power as a
//! share of FTP.
//!
//! ```xml
//! <workout_file>
//!   <name>Sweet spot 3×10</name>
//!   <workout>
//!     <Warmup Duration="600" PowerLow="0.40" PowerHigh="0.75"/>
//!     <SteadyState Duration="600" Power="0.90" Cadence="90">
//!       <textevent timeoffset="10" message="Settle in"/>
//!     </SteadyState>
//!     <IntervalsT Repeat="3" OnDuration="60" OffDuration="60" OnPower="1.2" OffPower="0.5"/>
//!     <FreeRide Duration="300"/>
//!     <Cooldown Duration="300" PowerLow="0.70" PowerHigh="0.40"/>
//!   </workout>
//! </workout_file>
//! ```
//!
//! What files in the wild hold (checked on ~100 of them):
//!
//! - Warmup, Cooldown and Ramp go from `PowerLow` to `PowerHigh` (a
//!   cooldown lists its higher power first); `SolidState` is the old
//!   `SteadyState`.
//! - An element this app does not know, with a `Duration` (`MaxEffort`…),
//!   is ridden free for as long — the cues after it keep their time — and
//!   noted; without one it is skipped and noted. So is an interval with a
//!   value missing or absurd: a file Zwift rides should ride here.
//! - Cues (text events) are timed from their element's start; one past
//!   its element's end is dropped (some copies of Zwift's own workouts
//!   carry them).
//! - Some are not UTF-8 but Windows-1252 (a dash in the description).
//! - Names match whatever the case.

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use super::erg::{Cue, Step, Workout, CUE_FOR, MAX_TARGET};
use crate::models::training::{WorkoutNote as Note, WorkoutProblem as Problem, WorkoutSegment as Segment};

/// Longer than this is not a workout file.
pub const MAX_BYTES: u64 = 1024 * 1024;
/// A workout at most a day long…
const MAX_SECS: u32 = 24 * 3600;
/// …of at most this many intervals (the longest seen: 137).
const MAX_SEGMENTS: usize = 2000;
/// Power shares beyond this are a typo.
const MAX_SHARE: f32 = 5.0;
/// An IntervalsT repeated more is a typo.
const MAX_REPEAT: u32 = 500;
/// A cue shown longer than this is a typo.
const MAX_CUE_SECS: u32 = 60;
/// Texts are cut at this many characters.
const MAX_NAME: usize = 120;
const MAX_CUE: usize = 300;
/// Notes past this many say nothing more.
const MAX_NOTES: usize = 20;
/// A profile to draw has at most this many points.
pub const PROFILE_POINTS: usize = 120;

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub name: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub segments: Vec<Segment>,
    pub cues: Vec<Cue>,
    pub notes: Vec<Note>,
}

impl Plan {
    pub fn duration(&self) -> u32 {
        self.segments.iter().fold(0u32, |sum, s| sum.saturating_add(s.secs))
    }

    /// The workout in watts for `ftp`.
    pub fn workout(&self, name: &str, ftp: u16) -> Workout {
        let watts = |share: f32| (share * f32::from(ftp)).round().min(f32::from(MAX_TARGET)) as u16;
        let steps = self
            .segments
            .iter()
            .map(|s| match s.free {
                true => Step { secs: s.secs, from: 0, to: 0, cadence: s.cadence },
                false => Step { secs: s.secs, from: watts(s.from), to: watts(s.to), cadence: s.cadence },
            })
            .collect();
        Workout { name: self.name.clone().unwrap_or_else(|| name.to_string()), steps, cues: self.cues.clone() }
    }
}

/// The intervals as at most `PROFILE_POINTS` equal stretches, each its
/// time-weighted power (free when mostly free): enough to draw.
pub fn profile(segments: &[Segment]) -> Vec<Segment> {
    if segments.len() <= PROFILE_POINTS {
        return segments.to_vec();
    }
    // In whole seconds (no position adds up in floats): second t is in
    // stretch t·n/total, so stretch i begins at ⌈i·total/n⌉. More than n
    // intervals of a second or more: every stretch has a second.
    let n = PROFILE_POINTS as u64;
    let total: u64 = segments.iter().map(|s| u64::from(s.secs)).sum();
    let begins = |i: u64| (i * total).div_ceil(n);
    // Per stretch: power × seconds ridden to a target, seconds free.
    let mut bins = vec![(0.0f64, 0u64); PROFILE_POINTS];
    let mut start = 0u64;
    for s in segments.iter().filter(|s| s.secs > 0) {
        let end = start + u64::from(s.secs);
        for i in start * n / total..=(end - 1) * n / total {
            let (a, b) = (start.max(begins(i)), end.min(begins(i + 1)));
            let bin = &mut bins[i as usize];
            if s.free {
                bin.1 += b - a;
            } else {
                let at = ((a + b) as f64 / 2.0 - start as f64) / f64::from(s.secs);
                bin.0 += (f64::from(s.from) + f64::from(s.to - s.from) * at) * (b - a) as f64;
            }
        }
        start = end;
    }
    (0..n)
        .zip(bins)
        .map(|(i, (power, free))| {
            let width = begins(i + 1) - begins(i);
            let held = width - free;
            let share = if held > 0 { (power / held as f64) as f32 } else { 0.0 };
            Segment { secs: 1, from: share, to: share, free: free * 2 > width, cadence: None }
        })
        .collect()
}

/// The file's text: UTF-8 (a byte-order mark dropped), or else
/// Windows-1252.
pub fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|&b| cp1252(b)).collect(),
    }
}

/// A Windows-1252 byte: Latin-1 but for 0x80–0x9F.
fn cp1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž', '\u{8F}', //
        '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}', 'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9F => HIGH[usize::from(b - 0x80)],
        _ => char::from(b),
    }
}

/// At most `max` characters of `text`, trimmed.
fn clip(text: &str, max: usize) -> String {
    text.trim().chars().take(max).collect()
}

/// An element's attributes, by lower-cased name.
struct Attrs(Vec<(String, String)>);

impl Attrs {
    fn of(e: &BytesStart) -> Attrs {
        let attrs = e
            .attributes()
            .flatten()
            .map(|a| {
                let key = String::from_utf8_lossy(a.key.as_ref()).to_ascii_lowercase();
                let value = a.unescape_value().map(|v| v.into_owned()).unwrap_or_default();
                (key, value)
            })
            .collect();
        Attrs(attrs)
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    fn number(&self, key: &str) -> Option<f64> {
        self.get(key)?.trim().parse::<f64>().ok().filter(|v| v.is_finite())
    }

    fn secs(&self, key: &str) -> Option<u32> {
        self.number(key).map(f64::round).filter(|&s| s >= 1.0 && s <= f64::from(MAX_SECS)).map(|s| s as u32)
    }

    fn share(&self, key: &str) -> Option<f32> {
        self.number(key).map(|p| p as f32).filter(|&p| (0.0..=MAX_SHARE).contains(&p))
    }

    /// A cadence: absent is fine, nonsense is dropped.
    fn cadence(&self, key: &str) -> Option<u8> {
        self.number(key).map(f64::round).filter(|&c| (1.0..=250.0).contains(&c)).map(|c| c as u8)
    }

    /// The first of `keys` present.
    fn share_of(&self, keys: &[&str]) -> Option<f32> {
        keys.iter().find_map(|k| self.share(k))
    }

    /// A steady power: `Power`, else between `PowerLow` and `PowerHigh`.
    fn steady(&self) -> Option<f32> {
        self.share("power").or_else(|| match (self.share("powerlow"), self.share("powerhigh")) {
            (Some(low), Some(high)) => Some((low + high) / 2.0),
            (low, high) => low.or(high),
        })
    }
}

/// What an element in `<workout>` rides as.
fn segments(name: &str, a: &Attrs) -> Option<Vec<Segment>> {
    let steady = |secs, power: f32, cadence| Segment { secs, from: power, to: power, free: false, cadence };
    match name {
        "steadystate" | "solidstate" => Some(vec![steady(a.secs("duration")?, a.steady()?, a.cadence("cadence"))]),
        "warmup" | "cooldown" | "ramp" => {
            let (from, to) = (a.share("powerlow")?, a.share("powerhigh")?);
            Some(vec![Segment { secs: a.secs("duration")?, from, to, free: false, cadence: a.cadence("cadence") }])
        }
        "intervalst" => {
            let repeat = a.number("repeat").map(f64::round).filter(|&r| r >= 1.0 && r <= f64::from(MAX_REPEAT))? as usize;
            let on = steady(a.secs("onduration")?, a.share_of(&["onpower", "poweronhigh", "poweronlow"])?, a.cadence("cadence"));
            let off_power = a.share_of(&["offpower", "poweroffhigh", "powerofflow"])?;
            let off = steady(a.secs("offduration")?, off_power, a.cadence("cadenceresting"));
            Some([on, off].repeat(repeat))
        }
        "freeride" => Some(vec![free(a.secs("duration")?, a.cadence("cadence"))]),
        _ => None,
    }
}

fn free(secs: u32, cadence: Option<u8>) -> Segment {
    Segment { secs, from: 0.0, to: 0.0, free: true, cadence }
}

const KNOWN: [&str; 7] = ["steadystate", "solidstate", "warmup", "cooldown", "ramp", "intervalst", "freeride"];

/// Notes, each once, a few at most.
fn note(notes: &mut Vec<Note>, n: Note) {
    if notes.len() < MAX_NOTES && !notes.contains(&n) {
        notes.push(n);
    }
}

/// Reads a `.zwo` file's bytes (the file's size counts, not its text's:
/// Windows-1252 grows in UTF-8).
pub fn parse_bytes(bytes: &[u8]) -> Result<Plan, Problem> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(Problem::TooBig);
    }
    read(&decode(bytes))
}

/// Reads a `.zwo` file's text.
pub fn parse(xml: &str) -> Result<Plan, Problem> {
    if xml.len() as u64 > MAX_BYTES {
        return Err(Problem::TooBig);
    }
    read(xml)
}

fn read(xml: &str) -> Result<Plan, Problem> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut plan = Plan { name: None, author: None, description: None, segments: Vec::new(), cues: Vec::new(), notes: Vec::new() };
    let mut path: Vec<String> = Vec::new();
    let mut seen_workout = false;
    let (mut sport, mut duration_type) = (String::new(), String::new());
    // The text of the field under the root being read.
    let mut text = String::new();
    // The element being read in <workout>: its start and length.
    let mut open: Option<(u32, u32)> = None;
    let mut elapsed: u32 = 0;
    loop {
        let event = reader.read_event().map_err(|e| Problem::Unreadable { detail: e.to_string() })?;
        let (e, empty) = match &event {
            Event::Start(e) => (e, false),
            Event::Empty(e) => (e, true),
            Event::Text(t) if path.len() == 2 => {
                text.push_str(&t.unescape().unwrap_or_default());
                continue;
            }
            Event::CData(t) if path.len() == 2 => {
                text.push_str(&String::from_utf8_lossy(t));
                continue;
            }
            Event::End(_) => {
                if path.len() == 2 {
                    let value = std::mem::take(&mut text);
                    match path[1].as_str() {
                        "name" => plan.name = Some(clip(&value, MAX_NAME)).filter(|n| !n.is_empty()),
                        "author" => plan.author = Some(clip(&value, MAX_NAME)).filter(|n| !n.is_empty()),
                        "description" => plan.description = Some(clip(&value, MAX_CUE)).filter(|n| !n.is_empty()),
                        "sporttype" => sport = value.trim().to_ascii_lowercase(),
                        "durationtype" => duration_type = value.trim().to_ascii_lowercase(),
                        _ => {}
                    }
                }
                path.pop();
                continue;
            }
            // The reader takes a file cut short for one that ended.
            Event::Eof if !path.is_empty() => {
                return Err(Problem::Unreadable { detail: format!("<{}> is never closed", path.join("><")) });
            }
            Event::Eof => break,
            _ => continue,
        };
        let name = String::from_utf8_lossy(e.name().as_ref()).to_ascii_lowercase();
        if path.is_empty() && name != "workout_file" {
            return Err(Problem::NotAWorkout);
        }
        let depth = path.len();
        let in_workout = depth >= 2 && path[1] == "workout";
        seen_workout |= depth == 1 && name == "workout";
        if in_workout && depth == 2 {
            let attrs = Attrs::of(e);
            let found = match segments(&name, &attrs) {
                Some(found) => Some(found),
                None if KNOWN.contains(&name.as_str()) => {
                    note(&mut plan.notes, Note::BadValue { element: element_name(e) });
                    None
                }
                None => match attrs.secs("duration") {
                    Some(secs) => {
                        note(&mut plan.notes, Note::RiddenFree { element: element_name(e) });
                        Some(vec![free(secs, None)])
                    }
                    None => {
                        note(&mut plan.notes, Note::Unknown { element: element_name(e) });
                        None
                    }
                },
            };
            // Skipped: its cues go with it.
            open = None;
            if let Some(found) = found {
                if plan.segments.len() + found.len() > MAX_SEGMENTS {
                    return Err(Problem::TooLong);
                }
                let length = found.iter().fold(0u32, |sum, s| sum.saturating_add(s.secs));
                open = Some((elapsed, length));
                elapsed = elapsed.saturating_add(length);
                if elapsed > MAX_SECS {
                    return Err(Problem::TooLong);
                }
                plan.segments.extend(found);
            }
        } else if in_workout && depth == 3 && name == "textevent" {
            let attrs = Attrs::of(e);
            match (open, attrs.number("timeoffset"), attrs.get("message").map(|m| clip(m, MAX_CUE))) {
                (Some((start, length)), Some(offset), Some(message)) if offset >= 0.0 && !message.is_empty() => {
                    let offset = offset.round() as u32;
                    if offset < length {
                        let secs = attrs.secs("duration").map_or(CUE_FOR, |s| s.min(MAX_CUE_SECS));
                        plan.cues.push(Cue { at: start + offset, secs, text: message });
                    } else {
                        note(&mut plan.notes, Note::CueOutside);
                    }
                }
                _ if attrs.get("distoffset").is_some() => note(&mut plan.notes, Note::DistanceCue),
                _ => {}
            }
        }
        if !empty {
            path.push(name);
        }
    }
    if !seen_workout {
        return Err(Problem::NotAWorkout);
    }
    if sport == "run" {
        return Err(Problem::Run);
    }
    if duration_type == "distance" {
        return Err(Problem::Distance);
    }
    if plan.segments.is_empty() {
        return Err(Problem::Empty);
    }
    plan.cues.sort_by_key(|c| c.at);
    Ok(plan)
}

/// The element's name as the file spells it.
fn element_name(e: &BytesStart) -> String {
    clip(&String::from_utf8_lossy(e.name().as_ref()), MAX_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERYTHING: &str = r#"<?xml version="1.0"?>
<workout_file>
  <author>Coach</author>
  <name>Sweet spot &amp; friends</name>
  <description>Every element once.</description>
  <sportType>bike</sportType>
  <category>Threshold</category>
  <tags><tag name="SS"/><tag>deep text</tag></tags>
  <workout>
    <Warmup Duration="600" PowerLow="0.40" PowerHigh="0.75" Cadence="85"/>
    <SteadyState Duration="300.4" Power="0.90" Cadence="90">
      <textevent timeoffset="10" message="Settle in"/>
      <textevent distoffset="500" message="By distance"/>
    </SteadyState>
    <IntervalsT Repeat="2" OnDuration="60" OffDuration="30" OnPower="1.2" OffPower="0.5" Cadence="100" CadenceResting="80">
      <TextEvent timeoffset="95" message="Second one" duration="5"/>
    </IntervalsT>
    <Ramp Duration="120" PowerLow="0.6" PowerHigh="0.8"/>
    <FreeRide Duration="300" FlatRoad="1" Cadence="95"/>
    <MaxEffort Duration="20"/>
    <Mystery/>
    <SteadyState Duration="60"/>
    <Cooldown Duration="300" PowerLow="0.70" PowerHigh="0.40"/>
  </workout>
</workout_file>"#;

    fn seg(secs: u32, from: f32, to: f32, cadence: Option<u8>) -> Segment {
        Segment { secs, from, to, free: false, cadence }
    }

    fn cue(at: u32, secs: u32, text: &str) -> Cue {
        Cue { at, secs, text: text.into() }
    }

    fn bike(workout: &str) -> String {
        format!("<workout_file><workout>{workout}</workout></workout_file>")
    }

    #[test]
    fn every_element_reads_and_the_rest_is_noted() {
        let plan = parse(EVERYTHING).expect("a workout");
        assert_eq!(plan.name.as_deref(), Some("Sweet spot & friends"));
        assert_eq!(plan.author.as_deref(), Some("Coach"));
        assert_eq!(plan.description.as_deref(), Some("Every element once."));
        assert_eq!(
            plan.segments,
            vec![
                seg(600, 0.40, 0.75, Some(85)),
                seg(300, 0.90, 0.90, Some(90)),
                seg(60, 1.2, 1.2, Some(100)),
                seg(30, 0.5, 0.5, Some(80)),
                seg(60, 1.2, 1.2, Some(100)),
                seg(30, 0.5, 0.5, Some(80)),
                seg(120, 0.6, 0.8, None),
                free(300, Some(95)),
                free(20, None),
                seg(300, 0.70, 0.40, None),
            ]
        );
        assert_eq!(plan.duration(), 1820);
        assert_eq!(plan.cues, vec![cue(610, CUE_FOR, "Settle in"), cue(900 + 95, 5, "Second one")], "timed from their element's start");
        assert_eq!(
            plan.notes,
            vec![
                Note::DistanceCue,
                Note::RiddenFree { element: "MaxEffort".into() },
                Note::Unknown { element: "Mystery".into() },
                Note::BadValue { element: "SteadyState".into() },
            ]
        );
    }

    #[test]
    fn watts_come_from_the_ftp() {
        let plan = parse(EVERYTHING).unwrap();
        let w = plan.workout("file", 250);
        assert_eq!(w.name, "Sweet spot & friends");
        assert_eq!(w.steps[0], Step { secs: 600, from: 100, to: 188, cadence: Some(85) });
        assert_eq!(w.steps[2], Step { secs: 60, from: 300, to: 300, cadence: Some(100) });
        assert!(w.steps[7].free(), "free ride: no target");
        assert_eq!(w.steps[9], Step { secs: 300, from: 175, to: 100, cadence: None }, "a cooldown goes down");
        assert_eq!(w.cues.len(), 2);
        let strong = Plan { segments: vec![seg(60, 5.0, 5.0, None)], ..plan.clone() };
        assert_eq!(strong.workout("x", 600).steps[0].from, MAX_TARGET, "capped");
        let nameless = Plan { name: None, ..plan };
        assert_eq!(nameless.workout("file", 250).name, "file", "the file's name");
    }

    #[test]
    fn variants_of_the_attributes_are_read() {
        let plan = parse(&bike(
            r#"<steadystate duration=" 60 " PowerLow="0.5" PowerHigh="0.75"/>
            <SteadyState Duration="60" PowerHigh="0.8"/>
            <SolidState Duration="60" Power="0.6"/>
            <IntervalsT Repeat="1" OnDuration="30" OffDuration="30" PowerOnHigh="1.1" PowerOffLow="0.4" Cadence="0"/>
            <IntervalsT Repeat="1" OnDuration="30" OffDuration="30" PowerOnLow="1.0" PowerOffHigh="0.45"/>"#,
        ))
        .unwrap();
        assert_eq!(
            plan.segments,
            vec![
                seg(60, 0.625, 0.625, None),
                seg(60, 0.8, 0.8, None),
                seg(60, 0.6, 0.6, None),
                seg(30, 1.1, 1.1, None),
                seg(30, 0.4, 0.4, None),
                seg(30, 1.0, 1.0, None),
                seg(30, 0.45, 0.45, None),
            ]
        );
        assert_eq!(plan.name, None);
        assert!(plan.notes.is_empty());
        // Comments split a name; CDATA is text too.
        let named = "<workout_file><name>A<!--c-->B <![CDATA[& C]]></name><workout><FreeRide Duration='60'/></workout></workout_file>";
        assert_eq!(parse(named).unwrap().name.as_deref(), Some("AB& C"));
        let long = format!("<workout_file><name>{}</name><workout><FreeRide Duration='60'/></workout></workout_file>", "x".repeat(500));
        assert_eq!(parse(&long).unwrap().name.map(|n| n.chars().count()), Some(MAX_NAME));
    }

    #[test]
    fn values_out_of_range_skip_their_interval() {
        let plan = parse(&bike(
            r#"<SteadyState Duration="0" Power="0.5"/>
            <SteadyState Duration="60" Power="9"/>
            <SteadyState Duration="abc" Power="0.5"/>
            <IntervalsT Repeat="100000" OnDuration="30" OffDuration="30" OnPower="1" OffPower="0.5"/>
            <IntervalsT Repeat="0" OnDuration="30" OffDuration="30" OnPower="1" OffPower="0.5"/>
            <Ramp Duration="60" PowerLow="0.5"/>
            <SteadyState Duration="60" Power="0.5" Cadence="999"/>
            <SteadyState Duration="60" Power="0.6"><textevent timeoffset="-5" message="x"/><textevent timeoffset="5" message=" "/></SteadyState>"#,
        ))
        .unwrap();
        assert_eq!(plan.segments, vec![seg(60, 0.5, 0.5, None), seg(60, 0.6, 0.6, None)], "the cadence dropped, not the interval");
        let bad = |element: &str| Note::BadValue { element: element.into() };
        assert_eq!(plan.notes, vec![bad("SteadyState"), bad("IntervalsT"), bad("Ramp")], "each once");
        assert!(plan.cues.is_empty());
    }

    #[test]
    fn cues_stay_in_their_element_in_time_order() {
        let plan = parse(&bike(
            r#"<SteadyState Duration="600" Power="0.5">
              <textevent timeoffset="300" message=" Half way "/>
              <textevent timeoffset="30" message="Early" duration="999"/>
              <textevent timeoffset="600" message="Past it"/>
              <textevent timeoffset="2095" message="Far past it"/>
            </SteadyState>
            <SteadyState Duration="0" Power="0.5"><textevent timeoffset="5" message="Of a bad one"/></SteadyState>
            <MaxEffort><textevent timeoffset="5" message="Of an unknown one"/></MaxEffort>"#,
        ))
        .unwrap();
        assert_eq!(plan.cues, vec![cue(30, MAX_CUE_SECS, "Early"), cue(300, CUE_FOR, "Half way")], "sorted, trimmed, capped");
        assert!(plan.notes.contains(&Note::CueOutside));
    }

    #[test]
    fn a_file_that_is_not_a_bike_workout_by_time_says_why() {
        assert!(matches!(parse("<workout_file><workout>"), Err(Problem::Unreadable { .. })));
        assert!(matches!(parse("<workout_file><workout></oops></workout_file>"), Err(Problem::Unreadable { .. })));
        assert_eq!(parse("<gpx><trk/></gpx>"), Err(Problem::NotAWorkout));
        assert_eq!(parse("<workout_file><name>x</name></workout_file>"), Err(Problem::NotAWorkout));
        assert_eq!(parse("<workout_file><tags><workout/></tags></workout_file>"), Err(Problem::NotAWorkout), "only under the root");
        assert_eq!(parse(""), Err(Problem::NotAWorkout));
        let with = |field: &str| format!("<workout_file>{field}<workout><SteadyState Duration='60' Power='1'/></workout></workout_file>");
        assert_eq!(parse(&with("<sportType> Run </sportType>")), Err(Problem::Run));
        assert_eq!(parse(&with("<durationType>Distance</durationType>")), Err(Problem::Distance));
        assert!(parse(&with("<durationType>time</durationType>")).is_ok());
        assert_eq!(parse(&bike("<Mystery/>")), Err(Problem::Empty));
        let day = r#"<SteadyState Duration="86400" Power="1"/><SteadyState Duration="10" Power="1"/>"#;
        assert_eq!(parse(&bike(day)), Err(Problem::TooLong), "longer than a day");
        let many = r#"<IntervalsT Repeat="500" OnDuration="1" OffDuration="1" OnPower="1" OffPower="0.5"/>"#.repeat(3);
        assert_eq!(parse(&bike(&many)), Err(Problem::TooLong), "more intervals than a workout has: said at once");
        assert_eq!(parse(&" ".repeat(MAX_BYTES as usize + 1)), Err(Problem::TooBig));
        assert_eq!(parse_bytes(&vec![b' '; MAX_BYTES as usize + 1]), Err(Problem::TooBig));
    }

    #[test]
    fn windows_1252_and_a_byte_order_mark_are_read() {
        let mut bytes = b"\xEF\xBB\xBF<workout_file><name>Fight Club</name><description>Hard \x96 harder\x85</description>".to_vec();
        bytes.extend_from_slice(b"<workout><SteadyState Duration='60' Power='1'/></workout></workout_file>");
        // Not UTF-8 after the mark: all of it Windows-1252.
        let plan = parse_bytes(&bytes[3..]).unwrap();
        assert_eq!(plan.description.as_deref(), Some("Hard – harder…"));
        assert_eq!(decode(b"\xEF\xBB\xBFok"), "ok");
        assert_eq!(decode("Ünïcode –".as_bytes()), "Ünïcode –");
        assert_eq!(decode(b"\x80\x8D\xA9\xFF"), "€\u{8D}©ÿ");
    }

    #[test]
    fn a_profile_has_a_few_dozen_points_at_most() {
        let few = vec![seg(60, 0.5, 0.5, None), free(60, None)];
        assert_eq!(profile(&few), few);
        // 300 intervals: a ramp over the first half, free over the second.
        let mut many: Vec<Segment> = (0..150).map(|i| seg(10, i as f32 / 150.0, (i + 1) as f32 / 150.0, None)).collect();
        many.extend((0..150).map(|_| free(10, None)));
        let drawn = profile(&many);
        assert_eq!(drawn.len(), PROFILE_POINTS);
        assert!(drawn[..60].iter().all(|s| !s.free) && drawn[60..].iter().all(|s| s.free));
        assert!((drawn[0].from - 1.0 / 120.0).abs() < 1e-3, "{:?}", drawn[0]);
        assert!((drawn[59].from - 119.0 / 120.0).abs() < 1e-3, "{:?}", drawn[59]);
        assert!(drawn.windows(2).take(59).all(|w| w[1].from > w[0].from), "rising");
    }

    /// Every size of workout ends (floats once left a stretch with nothing
    /// to take, for ever) and keeps the work: the profile's time-weighted
    /// power is the workout's.
    #[test]
    fn a_profile_ends_and_keeps_the_work_for_any_size() {
        let mut sizes: Vec<Vec<Segment>> = vec![
            [vec![seg(9, 0.5, 0.5, None); 129], vec![seg(49, 0.5, 0.5, None)]].concat(),
            vec![seg(20, 1.0, 0.4, None); 137],
        ];
        for count in [121usize, 130, 200, 400, 2000] {
            for secs in [1u32, 7, 9, 13, 30] {
                sizes.push((0..count).map(|i| seg(secs + (i % 3) as u32, 0.3 + (i % 5) as f32 * 0.2, 0.5, None)).collect());
            }
        }
        for segments in sizes {
            let drawn = profile(&segments);
            assert_eq!(drawn.len(), PROFILE_POINTS);
            let total: u64 = segments.iter().map(|s| u64::from(s.secs)).sum();
            let work: f64 = segments.iter().map(|s| f64::from(s.from + s.to) / 2.0 * f64::from(s.secs)).sum();
            let n = PROFILE_POINTS as u64;
            let width = |i: u64| ((i + 1) * total).div_ceil(n) - (i * total).div_ceil(n);
            let drawn_work: f64 = (0..n).zip(&drawn).map(|(i, s)| f64::from(s.from) * width(i) as f64).sum();
            assert!((drawn_work - work).abs() < work * 1e-4, "{} intervals: {drawn_work} vs {work}", segments.len());
        }
    }

    #[test]
    fn a_profile_starts_past_a_hundred_and_twenty_and_weighs_a_stretch_half_free() {
        let exact = vec![seg(5, 0.5, 0.5, None); PROFILE_POINTS];
        assert_eq!(profile(&exact), exact, "120: as they are");
        // 240 seconds, a second held at 1.0 then a second free, over and
        // over: each stretch of 2 s half free.
        let halves: Vec<Segment> = (0..240).map(|i| if i % 2 == 0 { seg(1, 1.0, 1.0, None) } else { free(1, None) }).collect();
        let drawn = profile(&halves);
        assert!(drawn.iter().all(|s| s.from == 1.0 && !s.free), "held at 1.0 while held, and not mostly free: {:?}", drawn[0]);
        let mostly_free: Vec<Segment> = (0..360).map(|i| if i % 3 == 0 { seg(1, 1.0, 1.0, None) } else { free(1, None) }).collect();
        assert!(profile(&mostly_free).iter().all(|s| s.free));
        let all_free = vec![free(10, None); 121];
        assert!(profile(&all_free).iter().all(|s| s.free && s.from == 0.0));
    }

    #[test]
    fn texts_are_the_field_s_own_and_cut_short() {
        let file = |fields: &str| format!("<workout_file>{fields}<workout><FreeRide Duration='60'/></workout></workout_file>");
        assert_eq!(parse(&file("<name>A<b>x</b>B</name>")).unwrap().name.as_deref(), Some("AB"), "not a nested element's");
        assert_eq!(parse(&file("<name> </name>")).unwrap().name, None, "blank: the file's name");
        let long = "y".repeat(1000);
        let plan = parse(&format!(
            "<workout_file><author>{long}</author><description>{long}</description><workout>\
             <SteadyState Duration='60' Power='0.5'><textevent timeoffset='1' message='{long}'/></SteadyState>\
             <{long} Duration='5'/></workout></workout_file>"
        ))
        .unwrap();
        assert_eq!(plan.author.map(|a| a.len()), Some(MAX_NAME));
        assert_eq!(plan.description.map(|d| d.len()), Some(MAX_CUE));
        assert_eq!(plan.cues[0].text.len(), MAX_CUE);
        assert_eq!(plan.notes, vec![Note::RiddenFree { element: "y".repeat(MAX_NAME) }]);
        let many: String = (0..30).map(|i| format!("<Odd{i}/>")).collect();
        assert_eq!(parse(&bike(&format!("{many}<FreeRide Duration='60'/>"))).unwrap().notes.len(), MAX_NOTES);
        // The reader trims text, not CDATA.
        let spaced = "<workout_file><durationType><![CDATA[ Distance ]]></durationType><workout><FreeRide Duration='60'/></workout></workout_file>";
        assert_eq!(parse(spaced), Err(Problem::Distance));
        assert_eq!(parse(&file("<name>A<b><![CDATA[x]]></b>B</name>")).unwrap().name.as_deref(), Some("AB"), "nor a nested CDATA");
    }

    #[test]
    fn the_limits_hold_to_the_second_and_the_interval() {
        let day = bike(r#"<SteadyState Duration="86399" Power="1"/><SteadyState Duration="1" Power="1"/>"#);
        assert_eq!(parse(&day).map(|p| p.duration()), Ok(MAX_SECS), "a day exactly");
        let full = r#"<IntervalsT Repeat="500" OnDuration="1" OffDuration="1" OnPower="1" OffPower="0.5"/>"#.repeat(2);
        assert_eq!(parse(&bike(&full)).map(|p| p.segments.len()), Ok(MAX_SEGMENTS), "2000 exactly");
        let over = format!("{full}<FreeRide Duration='1'/>");
        assert_eq!(parse(&bike(&over)), Err(Problem::TooLong));
        // A file under the limit whose text grows past it in UTF-8.
        let mut bytes = b"<workout_file><description>".to_vec();
        bytes.extend(std::iter::repeat_n(0x96u8, 600 * 1024));
        bytes.extend_from_slice(b"</description><workout><FreeRide Duration='60'/></workout></workout_file>");
        assert!(parse_bytes(&bytes).is_ok(), "the file's size counts");
        assert_eq!(decode(b"\x9F"), "Ÿ");
    }
}
