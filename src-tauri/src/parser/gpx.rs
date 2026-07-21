use crate::models::trackpoint::TrackPoint;
use crate::parser::ParsedActivity;

/// Remove a `<time>…</time>` that sits directly inside `<trk>` (immediately
/// before `<trkseg>`). Runkeeper and some other exporters emit a track-level
/// time, which is not valid per the GPX 1.1 schema, so the strict `gpx` crate
/// rejects the whole file. Trackpoint/metadata `<time>` (followed by `</trkpt>`
/// / `</metadata>` etc.) is left untouched, and the start time is anyway derived
/// from the first trackpoint.
fn strip_track_level_time(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(open) = rest.find("<time") {
        // Confirm it's a real <time> tag (not e.g. <timestamp>): the char after
        // "<time" must end or open the tag.
        let is_time_tag = matches!(
            rest.as_bytes().get(open + 5),
            Some(b'>' | b' ' | b'\t' | b'\n' | b'\r' | b'/')
        );
        if !is_time_tag {
            let keep = open + 5;
            out.push_str(&rest[..keep]);
            rest = &rest[keep..];
            continue;
        }
        // End of the opening tag.
        let Some(gt_rel) = rest[open..].find('>') else { break };
        let tag_end = open + gt_rel + 1;
        let self_closing = rest.as_bytes().get(open + gt_rel - 1) == Some(&b'/');
        let elem_end = if self_closing {
            tag_end // <time …/> — no separate closing tag
        } else {
            match rest[tag_end..].find("</time>") {
                Some(close_rel) => tag_end + close_rel + "</time>".len(),
                None => break, // unterminated — leave the remainder untouched
            }
        };
        if rest[elem_end..].trim_start().starts_with("<trkseg") {
            // Track-level time (Runkeeper, off-spec) — drop it.
            out.push_str(&rest[..open]);
        } else {
            // A real (trackpoint/metadata) time — keep the whole element.
            out.push_str(&rest[..elem_end]);
        }
        rest = &rest[elem_end..]; // advance past the element (no re-scan)
    }
    out.push_str(rest);
    out
}

/// Test convenience: the app itself always parses in-memory bytes (the import
/// pipeline reads + size-gates files before parsing).
#[cfg(test)]
pub fn parse_gpx(path: &str, activity_id: &str) -> Result<ParsedActivity, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("Failed to open GPX file: {}", e))?;
    parse_gpx_bytes(raw.as_bytes(), activity_id)
}

/// Parse GPX from in-memory bytes (e.g. decompressed from a .gz import).
pub fn parse_gpx_bytes(bytes: &[u8], activity_id: &str) -> Result<ParsedActivity, String> {
    let raw = std::str::from_utf8(bytes).map_err(|e| format!("GPX is not valid UTF-8: {}", e))?;
    let cleaned = strip_track_level_time(raw);

    let gpx_data =
        gpx::read(cleaned.as_bytes()).map_err(|e| format!("Failed to parse GPX: {}", e))?;

    let mut trackpoints: Vec<TrackPoint> = Vec::new();
    let mut start_time: Option<String> = None;
    let mut title: Option<String> = gpx_data.metadata.as_ref().and_then(|m| m.name.clone());

    for track in &gpx_data.tracks {
        if title.is_none() {
            title = track.name.clone();
        }

        for segment in &track.segments {
            for pt in &segment.points {
                let time_str = pt.time.map(|t| t.format().unwrap_or_default());

                if start_time.is_none() {
                    start_time = time_str.clone();
                }

                trackpoints.push(TrackPoint {
                    activity_id: activity_id.to_string(),
                    t: time_str,
                    lat: Some(pt.point().y()),
                    lon: Some(pt.point().x()),
                    altitude_m: pt.elevation,
                    speed_mps: pt.speed,
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
                });
            }
        }
    }

    // Try to extract HR/cadence from GPX extensions (Garmin TrackPointExtension)
    // The `gpx` crate doesn't parse extensions natively, so this is best-effort.

    Ok(ParsedActivity {
        start_time,
        // GPX `<type>` if present; otherwise infer from the track name's first
        // word (Runkeeper names tracks "Running …"/"Cycling …" with no <type>).
        // The pipeline normalizes this via SportType::from_str.
        sport_type: gpx_data.tracks.first().and_then(|t| {
            t.type_.clone().or_else(|| {
                t.name
                    .as_deref()
                    .and_then(|n| n.split_whitespace().next().map(str::to_string))
            })
        }),
        title,
        source_device: gpx_data.creator.clone(),
        trackpoints,
        session_metrics: None,
        laps: Vec::new(),
        lengths: Vec::new(),
        sets: Vec::new(),
        time_in_zones: Vec::new(),
        hrv_samples: Vec::new(),
        legs: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn sample_gpx() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="TestDevice"
     xmlns="http://www.topografix.com/GPX/1/1">
  <metadata>
    <name>Morning Run</name>
  </metadata>
  <trk>
    <name>Track 1</name>
    <type>Running</type>
    <trkseg>
      <trkpt lat="55.75" lon="37.62">
        <ele>150.0</ele>
        <time>2025-06-01T08:00:00Z</time>
      </trkpt>
      <trkpt lat="55.7501" lon="37.6201">
        <ele>155.0</ele>
        <time>2025-06-01T08:00:10Z</time>
      </trkpt>
      <trkpt lat="55.7502" lon="37.6202">
        <ele>152.0</ele>
        <time>2025-06-01T08:00:20Z</time>
      </trkpt>
    </trkseg>
  </trk>
</gpx>"#
            .to_string()
    }

    // Runkeeper-style: a <time> directly inside <trk> before <trkseg>.
    fn runkeeper_gpx() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="RunKeeper - http://www.runkeeper.com"
     xmlns="http://www.topografix.com/GPX/1/1">
<trk>
  <name><![CDATA[Running 5/20/14]]></name>
  <time>2014-05-20T04:53:35Z</time>
<trkseg>
<trkpt lat="55.928124" lon="37.853445"><ele>166.7</ele><time>2014-05-20T04:53:35Z</time></trkpt>
<trkpt lat="55.928120" lon="37.853423"><ele>166.6</ele><time>2014-05-20T04:53:36Z</time></trkpt>
</trkseg>
</trk>
</gpx>"#
            .to_string()
    }

    #[test]
    fn parse_runkeeper_track_level_time() {
        let tmp = std::env::temp_dir().join("tv_test_runkeeper.gpx");
        std::fs::write(&tmp, runkeeper_gpx()).unwrap();

        let result = parse_gpx(tmp.to_str().unwrap(), "rk").unwrap();
        assert_eq!(result.trackpoints.len(), 2);
        assert_eq!(result.trackpoints[0].lat, Some(55.928124));
        assert!(result.start_time.is_some());
        // Trackpoint times are preserved (only the track-level <time> is stripped).
        assert!(result.trackpoints[0].t.is_some());

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn strip_only_track_level_time() {
        let s = strip_track_level_time("<trk><time>X</time>\n<trkseg><trkpt><time>Y</time></trkpt></trkseg></trk>");
        assert!(!s.contains(">X</time>"), "track-level time removed");
        assert!(s.contains("<time>Y</time>"), "trackpoint time kept");
    }

    #[test]
    fn strip_handles_time_with_attributes_and_keeps_lookalikes() {
        // <time …> with attributes is stripped at track level…
        let s = strip_track_level_time(r#"<trk><time foo="1">X</time> <trkseg><trkpt><time>Y</time></trkpt></trkseg></trk>"#);
        assert!(!s.contains(">X</time>"), "attributed track-level time removed");
        assert!(s.contains("<time>Y</time>"), "trackpoint time kept");
        // …and a <timestamp> look-alike is left intact.
        let s2 = strip_track_level_time("<trk><timestamp>Z</timestamp><trkseg>");
        assert!(s2.contains("<timestamp>Z</timestamp>"), "non-time tag untouched");
        // Self-closing track-level <time/> is also stripped.
        let s3 = strip_track_level_time(r#"<trk><time value="x"/><trkseg><trkpt/></trkseg></trk>"#);
        assert!(!s3.contains("<time"), "self-closing track-level time removed");
    }

    #[test]
    fn parse_gpx_basic() {
        let tmp = std::env::temp_dir().join("tv_test_gpx.gpx");
        let mut f = std::fs::File::create(&tmp).unwrap();
        f.write_all(sample_gpx().as_bytes()).unwrap();

        let result = parse_gpx(tmp.to_str().unwrap(), "test-gpx").unwrap();

        assert_eq!(result.title, Some("Morning Run".to_string()));
        assert_eq!(result.sport_type, Some("Running".to_string()));
        assert_eq!(result.source_device, Some("TestDevice".to_string()));
        assert_eq!(result.trackpoints.len(), 3);
        assert!(result.session_metrics.is_none());

        let tp0 = &result.trackpoints[0];
        assert_eq!(tp0.activity_id, "test-gpx");
        assert_eq!(tp0.lat, Some(55.75));
        assert_eq!(tp0.lon, Some(37.62));
        assert_eq!(tp0.altitude_m, Some(150.0));
        assert!(tp0.t.is_some());

        assert!(result.start_time.is_some());

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn parse_gpx_no_metadata_title_falls_back_to_track_name() {
        let gpx = r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1">
  <trk>
    <name>My Track</name>
    <trkseg>
      <trkpt lat="55.75" lon="37.62">
        <time>2025-06-01T08:00:00Z</time>
      </trkpt>
    </trkseg>
  </trk>
</gpx>"#;

        let tmp = std::env::temp_dir().join("tv_test_gpx_notitle.gpx");
        let mut f = std::fs::File::create(&tmp).unwrap();
        f.write_all(gpx.as_bytes()).unwrap();

        let result = parse_gpx(tmp.to_str().unwrap(), "test-gpx2").unwrap();
        assert_eq!(result.title, Some("My Track".to_string()));

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn parse_gpx_empty_track() {
        let gpx = r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1">
  <trk><trkseg></trkseg></trk>
</gpx>"#;

        let tmp = std::env::temp_dir().join("tv_test_gpx_empty.gpx");
        let mut f = std::fs::File::create(&tmp).unwrap();
        f.write_all(gpx.as_bytes()).unwrap();

        let result = parse_gpx(tmp.to_str().unwrap(), "test-empty").unwrap();
        assert!(result.trackpoints.is_empty());
        assert!(result.start_time.is_none());

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn parse_gpx_invalid_file_returns_error() {
        let result = parse_gpx("/nonexistent/path.gpx", "test");
        assert!(result.is_err());
    }
}
