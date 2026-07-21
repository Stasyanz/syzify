use crate::models::activity::Activity;
use crate::models::trackpoint::TrackPoint;

/// Generate GPX XML string from an activity and its trackpoints.
/// `barometric` marks the altitude stream as coming from a barometric
/// altimeter (known from the source FIT's device_info).
pub fn activity_to_gpx(activity: &Activity, trackpoints: &[TrackPoint], barometric: bool) -> String {
    let mut xml = String::with_capacity(trackpoints.len() * 200);

    // The recording device goes into `creator`: services like Strava use it to
    // decide whether the <ele> series is barometric and can be trusted — an
    // unrecognized creator gets DEM "elevation correction" instead of the
    // device's altimeter data. Honest to claim: the samples ARE that device's.
    // "with barometer" is Strava's documented escape hatch that forces the
    // file's elevation even for creators missing from its device database.
    let device = activity.source_device.as_deref().unwrap_or("Syzify");
    let creator = if barometric {
        format!("{device} with barometer")
    } else {
        device.to_string()
    };
    xml.push_str(&format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="{}"
     xmlns="http://www.topografix.com/GPX/1/1"
     xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
     xsi:schemaLocation="http://www.topografix.com/GPX/1/1 http://www.topografix.com/GPX/1/1/gpx.xsd">
  <metadata>
"#,
        escape_xml(&creator)
    ));

    if let Some(ref title) = activity.title {
        xml.push_str(&format!("    <name>{}</name>\n", escape_xml(title)));
    }
    // Times and sport come from parsed third-party files — escape them like
    // the title, or a crafted import produces broken/injected XML on export.
    xml.push_str(&format!("    <time>{}</time>\n", escape_xml(&activity.start_time)));
    xml.push_str("  </metadata>\n");

    xml.push_str("  <trk>\n");
    if let Some(ref title) = activity.title {
        xml.push_str(&format!("    <name>{}</name>\n", escape_xml(title)));
    }
    xml.push_str(&format!(
        "    <type>{}</type>\n",
        escape_xml(gpx_type(&activity.sport_type))
    ));
    xml.push_str("    <trkseg>\n");

    for tp in trackpoints {
        let (lat, lon) = match (tp.lat, tp.lon) {
            (Some(lat), Some(lon)) => (lat, lon),
            _ => continue, // skip indoor points
        };

        xml.push_str(&format!(
            "      <trkpt lat=\"{:.7}\" lon=\"{:.7}\">\n",
            lat, lon
        ));

        if let Some(alt) = tp.altitude_m {
            xml.push_str(&format!("        <ele>{:.1}</ele>\n", alt));
        }
        if let Some(ref t) = tp.t {
            xml.push_str(&format!("        <time>{}</time>\n", escape_xml(t)));
        }

        // Extensions (HR, cadence, power)
        let has_ext = tp.hr.is_some() || tp.cadence.is_some() || tp.power_w.is_some();
        if has_ext {
            xml.push_str("        <extensions>\n");
            xml.push_str("          <gpxtpx:TrackPointExtension xmlns:gpxtpx=\"http://www.garmin.com/xmlschemas/TrackPointExtension/v1\">\n");
            if let Some(hr) = tp.hr {
                xml.push_str(&format!("            <gpxtpx:hr>{}</gpxtpx:hr>\n", hr));
            }
            if let Some(cad) = tp.cadence {
                xml.push_str(&format!("            <gpxtpx:cad>{}</gpxtpx:cad>\n", cad));
            }
            xml.push_str("          </gpxtpx:TrackPointExtension>\n");
            if let Some(power) = tp.power_w {
                xml.push_str(&format!("          <power>{}</power>\n", power));
            }
            xml.push_str("        </extensions>\n");
        }

        xml.push_str("      </trkpt>\n");
    }

    xml.push_str("    </trkseg>\n");
    xml.push_str("  </trk>\n");
    xml.push_str("</gpx>\n");

    xml
}

/// Strava-style privacy zone: drop every trackpoint within `radius_m` of the
/// track's first and last GPS fixes. EVERY pass through a zone is hidden, not
/// just the leading/trailing stretch — a loop that skirts home mid-ride must
/// not leak it either. Tracks without GPS fixes come back unchanged.
pub fn privacy_trim(trackpoints: &[TrackPoint], radius_m: f64) -> Vec<TrackPoint> {
    let mut fixes = trackpoints.iter().filter_map(|tp| match (tp.lat, tp.lon) {
        (Some(lat), Some(lon)) => Some((lat, lon)),
        _ => None,
    });
    let Some(start) = fixes.next() else {
        return trackpoints.to_vec();
    };
    let end = fixes.last().unwrap_or(start);

    let hidden = |lat: f64, lon: f64| {
        crate::import::pipeline::haversine_m(lat, lon, start.0, start.1) <= radius_m
            || crate::import::pipeline::haversine_m(lat, lon, end.0, end.1) <= radius_m
    };
    trackpoints
        .iter()
        .filter(|tp| match (tp.lat, tp.lon) {
            (Some(lat), Some(lon)) => !hidden(lat, lon),
            _ => true, // coordless points carry no location to hide
        })
        .cloned()
        .collect()
}

/// Our internal sport slug → the Garmin-vocabulary word services recognize
/// in a GPX <type>. Strava documents its type mapping over biking / running /
/// hiking / walking / swimming and GUESSES when the string is unknown — our
/// internal "ride" landed one upload as a water sport, which hides elevation
/// entirely. A recognized word beats a precise one (trail_run exports as
/// "running"); sports with no Garmin equivalent keep our slug.
fn gpx_type(sport: &str) -> &str {
    match sport {
        "ride" | "mountain_bike" => "cycling",
        "run" | "trail_run" | "treadmill" => "running",
        "swim" | "open_water" => "swimming",
        "hike" | "mountaineering" => "hiking",
        "walk" => "walking",
        other => other,
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_activity() -> Activity {
        Activity {
            id: "test".to_string(),
            start_time: "2025-06-01T08:00:00+00:00".to_string(),
            timezone_offset: None,
            sport_type: "run".to_string(),
            title: Some("Morning Run".to_string()),
            notes: None,
            distance_m: Some(5000.0),
            duration_s: Some(1800.0),
            elev_gain_m: None, elev_loss_m: None,
            avg_speed_mps: None, max_speed_mps: None,
            avg_hr: None, max_hr: None, avg_cadence: None,
            calories: None,
            avg_temperature_c: None, max_temperature_c: None,
            source_device: None, location_name: None,
            start_lat: None, start_lon: None,
            avg_power_w: None, max_power_w: None, normalized_power_w: None,
            total_work_kj: None, threshold_power_w: None,
            training_stress_score: None, intensity_factor: None,
            training_effect_aerobic: None, training_effect_anaerobic: None, training_load_peak: None,
            avg_vertical_oscillation_mm: None, avg_stance_time_ms: None, avg_stance_time_percent: None,
            avg_step_length_mm: None, total_strides: None,
            min_hr: None, moving_time_s: None, sub_sport: None,
            avg_respiration_rate: None, max_respiration_rate: None,
            hrv_rmssd: None, hrv_sdrr: None, end_lat: None, end_lon: None,
            avg_left_torque_effectiveness: None, avg_right_torque_effectiveness: None,
            avg_left_pedal_smoothness: None, avg_right_pedal_smoothness: None,
            avg_left_right_balance: None,
            created_at: String::new(), updated_at: String::new(), parent_id: None,
        }
    }

    #[test]
    fn generates_valid_gpx_structure() {
        let tps = vec![
            TrackPoint {
                activity_id: "test".to_string(),
                t: Some("2025-06-01T08:00:00+00:00".to_string()),
                lat: Some(55.75), lon: Some(37.62),
                altitude_m: Some(150.0), speed_mps: Some(3.0),
                hr: Some(140), cadence: Some(85), power_w: None, temperature_c: None,
                vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None, step_length_mm: None, grade_percent: None,
                left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
                left_pedal_smoothness: None, right_pedal_smoothness: None,
            },
            TrackPoint {
                activity_id: "test".to_string(),
                t: Some("2025-06-01T08:00:10+00:00".to_string()),
                lat: Some(55.7501), lon: Some(37.6201),
                altitude_m: Some(151.0), speed_mps: None,
                hr: None, cadence: None, power_w: None, temperature_c: None,
                vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None, step_length_mm: None, grade_percent: None,
                left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
                left_pedal_smoothness: None, right_pedal_smoothness: None,
            },
        ];

        let gpx = activity_to_gpx(&sample_activity(), &tps, false);

        assert!(gpx.contains("<?xml version"));
        assert!(gpx.contains("<gpx version=\"1.1\""));
        assert!(gpx.contains("<name>Morning Run</name>"));
        assert!(gpx.contains("<type>running</type>"));
        assert!(gpx.contains("lat=\"55.7500000\""));
        assert!(gpx.contains("<ele>150.0</ele>"));
        assert!(gpx.contains("<gpxtpx:hr>140</gpxtpx:hr>"));
        assert!(gpx.contains("<gpxtpx:cad>85</gpxtpx:cad>"));
        assert!(gpx.contains("</gpx>"));
    }

    #[test]
    fn skips_indoor_points() {
        let tps = vec![
            TrackPoint {
                activity_id: "test".to_string(),
                t: Some("2025-06-01T08:00:00+00:00".to_string()),
                lat: None, lon: None, // indoor
                altitude_m: None, speed_mps: None,
                hr: Some(150), cadence: None, power_w: None, temperature_c: None,
                vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None, step_length_mm: None, grade_percent: None,
                left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
                left_pedal_smoothness: None, right_pedal_smoothness: None,
            },
        ];

        let gpx = activity_to_gpx(&sample_activity(), &tps, false);
        assert!(!gpx.contains("<trkpt"));
    }

    #[test]
    fn escapes_xml_entities() {
        let mut a = sample_activity();
        a.title = Some("Run <fast> & \"hard\"".to_string());
        let gpx = activity_to_gpx(&a, &[], false);
        assert!(gpx.contains("Run &lt;fast&gt; &amp; &quot;hard&quot;"));
    }

    /// Sport and timestamps come from parsed third-party files — a crafted
    /// import must not be able to inject markup into the exported XML.
    #[test]
    fn escapes_parsed_fields_not_just_the_title() {
        let mut a = sample_activity();
        a.sport_type = "run</type><evil>".to_string();
        a.start_time = "2025-06-01T08:00:00+00:00\"><evil/>".to_string();
        let tps = vec![TrackPoint {
            activity_id: "test".to_string(),
            t: Some("<script>".to_string()),
            lat: Some(55.75), lon: Some(37.62),
            altitude_m: None, speed_mps: None,
            hr: None, cadence: None, power_w: None, temperature_c: None,
            vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None, step_length_mm: None, grade_percent: None,
            left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
            left_pedal_smoothness: None, right_pedal_smoothness: None,
        }];
        let gpx = activity_to_gpx(&a, &tps, false);
        assert!(!gpx.contains("<evil"), "raw markup must not survive: {}", gpx);
        assert!(!gpx.contains("<script>"));
        assert!(gpx.contains("&lt;evil&gt;"));
    }

    #[test]
    fn includes_power_in_extensions() {
        let tps = vec![TrackPoint {
            activity_id: "test".to_string(),
            t: None,
            lat: Some(55.75),
            lon: Some(37.62),
            altitude_m: None,
            speed_mps: None,
            hr: None,
            cadence: None,
            power_w: Some(250),
            temperature_c: None,
            vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None, step_length_mm: None, grade_percent: None,
            left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
            left_pedal_smoothness: None, right_pedal_smoothness: None,
        }];

        let gpx = activity_to_gpx(&sample_activity(), &tps, false);
        assert!(gpx.contains("<power>250</power>"));
        assert!(gpx.contains("<extensions>"));
    }

    fn gps_tp(lat: f64, lon: f64) -> TrackPoint {
        TrackPoint {
            activity_id: "test".to_string(),
            t: None,
            lat: Some(lat), lon: Some(lon),
            altitude_m: None, speed_mps: None,
            hr: None, cadence: None, power_w: None, temperature_c: None,
            vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None, step_length_mm: None, grade_percent: None,
            left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
            left_pedal_smoothness: None, right_pedal_smoothness: None,
        }
    }

    /// The privacy zone hides EVERY pass through the start/finish circles —
    /// including a mid-ride pass near home — and keeps the rest. ~0.00135° of
    /// latitude is ~150 m, inside a 200 m radius; 0.01° is ~1.1 km, outside.
    #[test]
    fn privacy_trim_hides_start_finish_and_mid_passes() {
        let home = (55.75, 37.62);
        let tps = vec![
            gps_tp(home.0, home.1),               // start — hidden
            gps_tp(home.0 + 0.00135, home.1),     // ~150 m out — hidden
            gps_tp(home.0 + 0.01, home.1),        // ~1.1 km — kept
            gps_tp(home.0 + 0.001, home.1),       // mid-ride pass near home — hidden
            gps_tp(home.0 + 0.02, home.1),        // far leg — kept
            gps_tp(home.0, home.1),               // finish — hidden
        ];
        let out = privacy_trim(&tps, 200.0);
        let lats: Vec<f64> = out.iter().filter_map(|tp| tp.lat).collect();
        assert_eq!(lats, vec![home.0 + 0.01, home.0 + 0.02]);
    }

    /// Point-to-point track: start and finish anchor two DIFFERENT zones,
    /// both trimmed.
    #[test]
    fn privacy_trim_uses_separate_start_and_finish_anchors() {
        let tps = vec![
            gps_tp(55.75, 37.62),          // start — hidden
            gps_tp(55.76, 37.62),          // middle — kept (~1.1 km from both)
            gps_tp(55.77, 37.62),          // finish — hidden
        ];
        let out = privacy_trim(&tps, 200.0);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].lat, Some(55.76));
    }

    /// No GPS fixes (indoor) — nothing to hide, the track comes back whole;
    /// coordless points among GPS ones carry no location and survive.
    #[test]
    fn privacy_trim_keeps_coordless_points() {
        let mut indoor = gps_tp(0.0, 0.0);
        indoor.lat = None;
        indoor.lon = None;
        indoor.hr = Some(150);
        assert_eq!(privacy_trim(&[indoor.clone()], 200.0).len(), 1);

        let tps = vec![gps_tp(55.75, 37.62), indoor, gps_tp(55.76, 37.62)];
        // Radius so small only the exact anchors fall inside it.
        let out = privacy_trim(&tps, 1.0);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].hr, Some(150));
    }

    /// The GPX <type> must be a word services recognize (Strava guesses on
    /// unknown strings — one upload of internal "ride" became a water sport);
    /// sports without a Garmin word keep our slug.
    #[test]
    fn sport_type_exports_as_recognized_garmin_word() {
        let cases = [
            ("ride", "cycling"),
            ("mountain_bike", "cycling"),
            ("trail_run", "running"),
            ("open_water", "swimming"),
            ("hike", "hiking"),
            ("walk", "walking"),
            ("yoga", "yoga"),
        ];
        for (sport, expected) in cases {
            let mut a = sample_activity();
            a.sport_type = sport.to_string();
            let gpx = activity_to_gpx(&a, &[], false);
            assert!(
                gpx.contains(&format!("<type>{}</type>", expected)),
                "{} should export as {}",
                sport,
                expected
            );
        }
    }

    /// Strava trusts <ele> as barometric only for a recognized recording
    /// device in `creator` — the stored source device must end up there,
    /// with the app name as the fallback for device-less imports.
    #[test]
    fn creator_is_source_device_with_app_fallback() {
        let mut a = sample_activity();
        a.source_device = Some("Garmin Edge 840".to_string());
        assert!(activity_to_gpx(&a, &[], false).contains(r#"creator="Garmin Edge 840""#));

        a.source_device = None;
        assert!(activity_to_gpx(&a, &[], false).contains(r#"creator="Syzify""#));
    }

    /// A barometric altitude stream advertises itself via Strava's documented
    /// "with barometer" creator suffix — with or without a known device.
    #[test]
    fn barometric_flag_appends_with_barometer() {
        let mut a = sample_activity();
        a.source_device = Some("Garmin fenix6x".to_string());
        assert!(
            activity_to_gpx(&a, &[], true).contains(r#"creator="Garmin fenix6x with barometer""#)
        );

        a.source_device = None;
        assert!(activity_to_gpx(&a, &[], true).contains(r#"creator="Syzify with barometer""#));
    }

    /// The device name is parsed from third-party files — a crafted one must
    /// not break out of the creator attribute.
    #[test]
    fn escapes_crafted_device_name() {
        let mut a = sample_activity();
        a.source_device = Some(r#""><evil/>"#.to_string());
        let gpx = activity_to_gpx(&a, &[], false);
        assert!(!gpx.contains("<evil"), "raw markup must not survive: {}", gpx);
        assert!(gpx.contains(r#"creator="&quot;&gt;&lt;evil/&gt;""#));
    }

    #[test]
    fn no_title_omits_name_elements() {
        let mut a = sample_activity();
        a.title = None;
        let gpx = activity_to_gpx(&a, &[], false);
        assert!(!gpx.contains("<name>"));
    }
}
