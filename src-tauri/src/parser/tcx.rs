use quick_xml::events::Event;
use quick_xml::Reader;

use crate::models::lap::Lap;
use crate::models::trackpoint::TrackPoint;
use crate::parser::{ParsedActivity, SessionMetrics};

/// Test convenience: the app itself always parses in-memory bytes (the import
/// pipeline reads + size-gates files before parsing).
#[cfg(test)]
pub fn parse_tcx(path: &str, activity_id: &str) -> Result<ParsedActivity, String> {
    let xml = std::fs::read_to_string(path).map_err(|e| format!("Failed to read TCX file: {}", e))?;
    parse_tcx_bytes(xml.as_bytes(), activity_id)
}

/// Parse TCX from in-memory bytes (e.g. decompressed from a .gz import).
pub fn parse_tcx_bytes(bytes: &[u8], activity_id: &str) -> Result<ParsedActivity, String> {
    let xml = std::str::from_utf8(bytes).map_err(|e| format!("TCX is not valid UTF-8: {}", e))?;
    let mut reader = Reader::from_str(xml);

    let mut trackpoints: Vec<TrackPoint> = Vec::new();
    let mut start_time: Option<String> = None;
    let mut sport_type: Option<String> = None;
    let title: Option<String> = None;
    let mut source_device: Option<String> = None;
    let mut session_metrics = SessionMetrics::default();

    // State machine for XML parsing
    let mut in_trackpoint = false;
    let mut in_lap = false;
    let mut in_creator = false;
    let mut current_tag = String::new();
    let mut current_tp = empty_tp(activity_id);
    let mut in_hr = false;
    let mut in_cadence_ext = false;
    let mut _lap_count = 0;
    let mut total_distance = 0.0;
    let mut total_time = 0.0;
    let mut total_ascent = 0.0f64;
    let mut max_speed = 0.0f64;
    let mut hr_sum = 0.0;
    let mut hr_count = 0u32;
    let mut max_hr = 0.0f64;
    let mut total_calories = 0.0f64;
    let mut laps: Vec<Lap> = Vec::new();
    let mut lap_start_time: Option<String> = None;
    let mut lap_time = 0.0f64;
    let mut lap_distance = 0.0f64;
    let mut lap_calories = 0.0f64;
    let mut prev_alt: Option<f64> = None;

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                current_tag = name.clone();

                match name.as_str() {
                    "Activity" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"Sport" {
                                sport_type = Some(
                                    String::from_utf8_lossy(&attr.value).to_string(),
                                );
                            }
                        }
                    }
                    "Lap" => {
                        in_lap = true;
                        _lap_count += 1;
                        lap_time = 0.0;
                        lap_distance = 0.0;
                        lap_calories = 0.0;
                        lap_start_time = None;
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"StartTime" {
                                let st = String::from_utf8_lossy(&attr.value).to_string();
                                lap_start_time = Some(st.clone());
                                if start_time.is_none() {
                                    start_time = Some(st);
                                }
                            }
                        }
                    }
                    "Trackpoint" => {
                        in_trackpoint = true;
                        current_tp = empty_tp(activity_id);
                    }
                    "HeartRateBpm" => {
                        in_hr = true;
                    }
                    "Extensions" => {}
                    "TPX" | "ns3:TPX" => {
                        in_cadence_ext = true;
                    }
                    "Creator" => {
                        in_creator = true;
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "Trackpoint" => {
                        // Track elevation gain
                        if let Some(alt) = current_tp.altitude_m {
                            if let Some(prev) = prev_alt {
                                let diff = alt - prev;
                                if diff > 0.5 {
                                    total_ascent += diff;
                                }
                            }
                            prev_alt = Some(alt);
                        }
                        // Track HR stats
                        if let Some(hr) = current_tp.hr {
                            hr_sum += hr as f64;
                            hr_count += 1;
                            if hr as f64 > max_hr {
                                max_hr = hr as f64;
                            }
                        }

                        trackpoints.push(current_tp.clone());
                        in_trackpoint = false;
                    }
                    "Lap" => {
                        laps.push(Lap {
                            id: None,
                            activity_id: activity_id.to_string(),
                            lap_number: _lap_count,
                            start_time: lap_start_time.clone(),
                            total_elapsed_time_s: if lap_time > 0.0 { Some(lap_time) } else { None },
                            total_timer_time_s: None,
                            total_distance_m: if lap_distance > 0.0 { Some(lap_distance) } else { None },
                            avg_speed_mps: if lap_time > 0.0 && lap_distance > 0.0 { Some(lap_distance / lap_time) } else { None },
                            max_speed_mps: None,
                            avg_hr: None,
                            max_hr: None,
                            avg_cadence: None,
                            max_cadence: None,
                            total_ascent_m: None,
                            total_descent_m: None,
                            total_calories: if lap_calories > 0.0 { Some(lap_calories) } else { None },
                            avg_power_w: None,
                            max_power_w: None,
                            normalized_power_w: None,
                            avg_vertical_oscillation_mm: None,
                            avg_stance_time_ms: None,
                            avg_step_length_mm: None,
                        });
                        in_lap = false;
                    }
                    "HeartRateBpm" => {
                        in_hr = false;
                    }
                    "TPX" | "ns3:TPX" => {
                        in_cadence_ext = false;
                    }
                    "Creator" => {
                        in_creator = false;
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                let text = e.unescape().unwrap_or_default().trim().to_string();
                if text.is_empty() {
                    buf.clear();
                    continue;
                }

                if in_trackpoint {
                    match current_tag.as_str() {
                        "Time" => {
                            if start_time.is_none() {
                                start_time = Some(text.clone());
                            }
                            current_tp.t = Some(text);
                        }
                        "LatitudeDegrees" => {
                            current_tp.lat = text.parse().ok();
                        }
                        "LongitudeDegrees" => {
                            current_tp.lon = text.parse().ok();
                        }
                        "AltitudeMeters" => {
                            current_tp.altitude_m = text.parse().ok();
                        }
                        "DistanceMeters" if !in_hr => {
                            // Track max distance for total
                            if let Ok(d) = text.parse::<f64>() {
                                if d > total_distance {
                                    total_distance = d;
                                }
                            }
                        }
                        "Speed" | "ns3:Speed" => {
                            let spd: Option<f64> = text.parse().ok();
                            current_tp.speed_mps = spd;
                            if let Some(s) = spd {
                                if s > max_speed {
                                    max_speed = s;
                                }
                            }
                        }
                        "Value" if in_hr => {
                            current_tp.hr = text.parse().ok();
                        }
                        "RunCadence" | "ns3:RunCadence" => {
                            current_tp.cadence = text.parse().ok();
                        }
                        "Cadence" if in_cadence_ext => {
                            current_tp.cadence = text.parse().ok();
                        }
                        "Watts" | "ns3:Watts" => {
                            current_tp.power_w = text.parse().ok();
                        }
                        _ => {}
                    }
                } else if in_lap && !in_trackpoint {
                    match current_tag.as_str() {
                        "TotalTimeSeconds" => {
                            if let Ok(t) = text.parse::<f64>() {
                                total_time += t;
                                lap_time = t;
                            }
                        }
                        "DistanceMeters" => {
                            if let Ok(d) = text.parse::<f64>() {
                                lap_distance = d;
                            }
                        }
                        "Calories" => {
                            if let Ok(c) = text.parse::<f64>() {
                                total_calories += c;
                                lap_calories = c;
                            }
                        }
                        _ => {}
                    }
                } else if in_creator {
                    if current_tag == "Name" && source_device.is_none() {
                        source_device = Some(text);
                    }
                } else if current_tag == "Id" && title.is_none() {
                    // TCX <Id> is usually the start time, use as fallback title
                    if start_time.is_none() {
                        start_time = Some(text);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("TCX parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    // Build session metrics
    if total_distance > 0.0 || total_time > 0.0 {
        session_metrics.total_distance_m = Some(total_distance);
        session_metrics.total_elapsed_time_s = Some(total_time);
        session_metrics.total_ascent_m = Some(total_ascent);
        if max_speed > 0.0 {
            session_metrics.max_speed_mps = Some(max_speed);
        }
        if total_time > 0.0 && total_distance > 0.0 {
            session_metrics.avg_speed_mps = Some(total_distance / total_time);
        }
        if hr_count > 0 {
            session_metrics.avg_hr = Some(hr_sum / hr_count as f64);
            session_metrics.max_hr = Some(max_hr);
        }
        if total_calories > 0.0 {
            session_metrics.total_calories = Some(total_calories);
        }
    }

    Ok(ParsedActivity {
        start_time,
        sport_type,
        title,
        source_device,
        trackpoints,
        session_metrics: if total_distance > 0.0 || total_time > 0.0 {
            Some(session_metrics)
        } else {
            None
        },
        laps,
        lengths: Vec::new(),
        sets: Vec::new(),
        time_in_zones: Vec::new(),
        hrv_samples: Vec::new(),
        legs: Vec::new(),
    })
}

fn empty_tp(activity_id: &str) -> TrackPoint {
    TrackPoint {
        activity_id: activity_id.to_string(),
        t: None,
        lat: None,
        lon: None,
        altitude_m: None,
        speed_mps: None,
        hr: None,
        cadence: None,
        power_w: None,
        temperature_c: None,
        vertical_oscillation_mm: None,
        stance_time_ms: None,
        stance_time_percent: None,
        step_length_mm: None,
        grade_percent: None,
        left_right_balance: None,
        left_torque_effectiveness: None,
        right_torque_effectiveness: None,
        left_pedal_smoothness: None,
        right_pedal_smoothness: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn sample_tcx() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<TrainingCenterDatabase xmlns="http://www.garmin.com/xmlschemas/TrainingCenterDatabase/v2">
  <Activities>
    <Activity Sport="Running">
      <Id>2025-06-01T08:00:00Z</Id>
      <Lap StartTime="2025-06-01T08:00:00Z">
        <TotalTimeSeconds>1800</TotalTimeSeconds>
        <Calories>250</Calories>
        <Trackpoint>
          <Time>2025-06-01T08:00:00Z</Time>
          <Position>
            <LatitudeDegrees>55.75</LatitudeDegrees>
            <LongitudeDegrees>37.62</LongitudeDegrees>
          </Position>
          <AltitudeMeters>150.0</AltitudeMeters>
          <DistanceMeters>0</DistanceMeters>
          <HeartRateBpm><Value>140</Value></HeartRateBpm>
        </Trackpoint>
        <Trackpoint>
          <Time>2025-06-01T08:00:10Z</Time>
          <Position>
            <LatitudeDegrees>55.7501</LatitudeDegrees>
            <LongitudeDegrees>37.6201</LongitudeDegrees>
          </Position>
          <AltitudeMeters>155.0</AltitudeMeters>
          <DistanceMeters>100</DistanceMeters>
          <HeartRateBpm><Value>150</Value></HeartRateBpm>
        </Trackpoint>
      </Lap>
      <Creator>
        <Name>Garmin FR265</Name>
      </Creator>
    </Activity>
  </Activities>
</TrainingCenterDatabase>"#.to_string()
    }

    #[test]
    fn parse_tcx_basic() {
        let tmp = std::env::temp_dir().join("tv_test.tcx");
        let mut f = std::fs::File::create(&tmp).unwrap();
        f.write_all(sample_tcx().as_bytes()).unwrap();

        let result = parse_tcx(tmp.to_str().unwrap(), "test-tcx").unwrap();

        assert_eq!(result.sport_type, Some("Running".to_string()));
        assert_eq!(result.start_time, Some("2025-06-01T08:00:00Z".to_string()));
        assert_eq!(result.source_device, Some("Garmin FR265".to_string()));
        assert_eq!(result.trackpoints.len(), 2);

        let tp0 = &result.trackpoints[0];
        assert_eq!(tp0.lat, Some(55.75));
        assert_eq!(tp0.lon, Some(37.62));
        assert_eq!(tp0.altitude_m, Some(150.0));
        assert_eq!(tp0.hr, Some(140));

        let sm = result.session_metrics.unwrap();
        assert_eq!(sm.total_distance_m, Some(100.0));
        assert_eq!(sm.total_elapsed_time_s, Some(1800.0));
        assert!(sm.avg_hr.unwrap() > 140.0);
        assert_eq!(sm.max_hr, Some(150.0));

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn parse_tcx_extracts_calories() {
        let tmp = std::env::temp_dir().join("tv_test_cal.tcx");
        let mut f = std::fs::File::create(&tmp).unwrap();
        f.write_all(sample_tcx().as_bytes()).unwrap();

        let result = parse_tcx(tmp.to_str().unwrap(), "test-cal").unwrap();
        let sm = result.session_metrics.unwrap();
        assert_eq!(sm.total_calories, Some(250.0));

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn parse_tcx_extracts_laps() {
        let tmp = std::env::temp_dir().join("tv_test_laps.tcx");
        let mut f = std::fs::File::create(&tmp).unwrap();
        f.write_all(sample_tcx().as_bytes()).unwrap();

        let result = parse_tcx(tmp.to_str().unwrap(), "test-laps").unwrap();
        assert_eq!(result.laps.len(), 1);
        let lap = &result.laps[0];
        assert_eq!(lap.lap_number, 1);
        assert_eq!(lap.total_elapsed_time_s, Some(1800.0));
        assert_eq!(lap.total_calories, Some(250.0));
        assert!(lap.start_time.is_some());

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn parse_tcx_empty_file() {
        let tmp = std::env::temp_dir().join("tv_test_empty.tcx");
        let mut f = std::fs::File::create(&tmp).unwrap();
        f.write_all(b"<?xml version=\"1.0\"?><TrainingCenterDatabase></TrainingCenterDatabase>").unwrap();

        let result = parse_tcx(tmp.to_str().unwrap(), "test-empty").unwrap();
        assert!(result.trackpoints.is_empty());
        assert!(result.session_metrics.is_none());

        std::fs::remove_file(&tmp).ok();
    }
}
