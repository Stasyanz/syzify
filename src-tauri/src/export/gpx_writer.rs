use crate::models::activity::Activity;
use crate::models::trackpoint::TrackPoint;

/// Generate GPX XML string from an activity and its trackpoints.
/// `barometric` marks the altitude stream as coming from a barometric
/// altimeter (known from the source FIT's device_info).
pub fn activity_to_gpx(activity: &Activity, trackpoints: &[TrackPoint], barometric: bool) -> String {
    write_gpx(activity, trackpoints, &course_jumps(activity, trackpoints), barometric)
}

/// [`activity_to_gpx`] of the track outside its privacy zones (see
/// [`privacy_kept`]). A simulated course's jumps are found before the trim: a
/// looping route jumps right into the zone around its start, and the rows
/// taken out would stretch the step until it looks ridden.
pub fn activity_to_private_gpx(
    activity: &Activity,
    trackpoints: &[TrackPoint],
    radius_m: f64,
    barometric: bool,
) -> String {
    let jumps = course_jumps(activity, trackpoints);
    let kept = privacy_kept(trackpoints, radius_m);
    let shown: Vec<TrackPoint> = keep_rows(trackpoints, &kept);
    write_gpx(activity, &shown, &jumps_among_kept(trackpoints, &kept, &jumps), barometric)
}

/// Rows a simulated course jumped to; nothing for a sport on real ground.
fn course_jumps(activity: &Activity, trackpoints: &[TrackPoint]) -> Vec<usize> {
    if crate::models::activity::has_simulated_course(&activity.sport_type) {
        crate::import::gps_clean::teleported_points(trackpoints)
    } else {
        Vec::new()
    }
}

/// `jumps` (ascending rows of `trackpoints`) as rows of the kept points: each
/// lands on the first kept fix at or after it, and jumps with no fix kept
/// between them make one. With no fix kept before a jump there's no line to
/// break, and a break would open the file on an empty segment.
fn jumps_among_kept(trackpoints: &[TrackPoint], kept: &[bool], jumps: &[usize]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut pending = false;
    let mut any_fix = false;
    let mut row = 0;
    for (i, tp) in trackpoints.iter().enumerate() {
        pending |= jumps.binary_search(&i).is_ok();
        if kept[i] {
            if tp.lat.is_some() && tp.lon.is_some() {
                if pending && any_fix {
                    out.push(row);
                }
                pending = false;
                any_fix = true;
            }
            row += 1;
        }
    }
    out
}

/// `jumps`: rows of `trackpoints` a simulated course jumped to, ascending.
fn write_gpx(activity: &Activity, trackpoints: &[TrackPoint], jumps: &[usize], barometric: bool) -> String {
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

    // A new segment after every GPS gap and every jump of a simulated course,
    // where the app's map breaks the line too (#256, #261) — a reader would
    // draw one straight across it.
    let breaks = {
        let t: Vec<Option<f64>> = trackpoints
            .iter()
            .map(|p| p.t.as_deref().and_then(crate::db::trackpoints::parse_time_seconds))
            .collect();
        let lat: Vec<Option<f64>> = trackpoints.iter().map(|p| p.lat).collect();
        let lon: Vec<Option<f64>> = trackpoints.iter().map(|p| p.lon).collect();
        let mut breaks = crate::import::gps_clean::fixes_after_gaps(&t, &lat, &lon);
        breaks.extend_from_slice(jumps);
        breaks.sort_unstable();
        breaks
    };

    for (i, tp) in trackpoints.iter().enumerate() {
        let (lat, lon) = match (tp.lat, tp.lon) {
            (Some(lat), Some(lon)) => (lat, lon),
            _ => continue, // skip indoor points
        };
        if breaks.binary_search(&i).is_ok() {
            xml.push_str("    </trkseg>\n    <trkseg>\n");
        }

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

/// Strava-style privacy zone: which trackpoints stay, each row outside
/// `radius_m` of the track's first and last GPS fixes. EVERY pass through a
/// zone is hidden, not just the leading/trailing stretch — a loop that skirts
/// home mid-ride must not leak it either. Tracks without GPS fixes all stay.
fn privacy_kept(trackpoints: &[TrackPoint], radius_m: f64) -> Vec<bool> {
    let mut fixes = trackpoints.iter().filter_map(|tp| match (tp.lat, tp.lon) {
        (Some(lat), Some(lon)) => Some((lat, lon)),
        _ => None,
    });
    let Some(start) = fixes.next() else {
        return vec![true; trackpoints.len()];
    };
    let end = fixes.last().unwrap_or(start);

    let hidden = |lat: f64, lon: f64| {
        crate::import::pipeline::haversine_m(lat, lon, start.0, start.1) <= radius_m
            || crate::import::pipeline::haversine_m(lat, lon, end.0, end.1) <= radius_m
    };
    trackpoints
        .iter()
        .map(|tp| match (tp.lat, tp.lon) {
            (Some(lat), Some(lon)) => !hidden(lat, lon),
            _ => true, // coordless points carry no location to hide
        })
        .collect()
}

fn keep_rows(trackpoints: &[TrackPoint], kept: &[bool]) -> Vec<TrackPoint> {
    trackpoints.iter().zip(kept).filter(|(_, keep)| **keep).map(|(tp, _)| tp.clone()).collect()
}

/// Our internal sport slug → the Garmin-vocabulary word services recognize
/// in a GPX <type>. Strava documents its type mapping over biking / running /
/// hiking / walking / swimming and GUESSES when the string is unknown — our
/// internal "ride" landed one upload as a water sport, which hides elevation
/// entirely. A recognized word beats a precise one (trail_run exports as
/// "running"); sports with no Garmin equivalent keep our slug.
fn gpx_type(sport: &str) -> &str {
    match sport {
        "ride" | "mountain_bike" | "indoor_ride" | "virtual_ride" => "cycling",
        "run" | "trail_run" | "treadmill" | "virtual_run" => "running",
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
            ..Default::default()
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
    fn a_gps_gap_starts_a_new_segment() {
        // 10 s of fixes, 60 s without (heart rate only), 10 s of fixes.
        let tps: Vec<TrackPoint> = (0..80)
            .map(|k| {
                let fix = !(10..70).contains(&k);
                TrackPoint {
                    activity_id: "test".to_string(),
                    t: Some(format!("2025-06-01T08:{:02}:{:02}+00:00", k / 60, k % 60)),
                    lat: fix.then_some(55.0 + k as f64 * 1e-4), lon: fix.then_some(37.0),
                    altitude_m: None, speed_mps: None,
                    hr: Some(150), cadence: None, power_w: None, temperature_c: None,
                    vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None, step_length_mm: None, grade_percent: None,
                    left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
                    left_pedal_smoothness: None, right_pedal_smoothness: None,
                }
            })
            .collect();
        let gpx = activity_to_gpx(&sample_activity(), &tps, false);
        assert_eq!(gpx.matches("<trkseg>").count(), 2);
        assert_eq!(gpx.matches("</trkseg>").count(), 2);
        assert_eq!(gpx.matches("<trkpt").count(), 20);
        // The second segment opens on the first fix after the gap.
        let second = gpx.split("<trkseg>").nth(2).unwrap();
        assert!(second.trim_start().starts_with(&format!("<trkpt lat=\"{:.7}\"", 55.0 + 70.0 * 1e-4)));
    }

    /// Three fixes riding on, a jump, two more.
    fn jumping_track() -> Vec<TrackPoint> {
        let lats = [55.0, 55.0001, 55.0002, 55.5, 55.5001];
        lats.iter()
            .enumerate()
            .map(|(k, lat)| TrackPoint {
                activity_id: "test".to_string(),
                t: Some(format!("2025-06-01T08:00:{k:02}+00:00")),
                lat: Some(*lat),
                lon: Some(37.0),
                ..Default::default()
            })
            .collect()
    }

    #[test]
    fn a_jump_of_a_simulated_course_starts_a_new_segment() {
        let mut activity = sample_activity();
        activity.sport_type = "virtual_ride".to_string();
        let gpx = activity_to_gpx(&activity, &jumping_track(), false);
        assert_eq!(gpx.matches("<trkseg>").count(), 2);
        assert_eq!(gpx.matches("<trkpt").count(), 5);
        let second = gpx.split("<trkseg>").nth(2).unwrap();
        assert!(second.trim_start().starts_with("<trkpt lat=\"55.5000000\""));
    }

    /// A second a row: `jump` is where 0.5° are added, 60 s from row 40 have no fix.
    fn gapped_track(jump: usize) -> Vec<TrackPoint> {
        (0..120)
            .map(|k| {
                let fix = !(40..100).contains(&k);
                let lat = 55.0 + k as f64 * 1e-5 + if k >= jump { 0.5 } else { 0.0 };
                TrackPoint {
                    activity_id: "test".to_string(),
                    t: Some(format!("2025-06-01T08:{:02}:{:02}+00:00", k / 60, k % 60)),
                    lat: fix.then_some(lat),
                    lon: fix.then_some(37.0),
                    ..Default::default()
                }
            })
            .collect()
    }

    #[test]
    fn jumps_and_gps_gaps_break_a_track_in_either_order() {
        let mut activity = sample_activity();
        activity.sport_type = "virtual_ride".to_string();
        let segments = |jump| activity_to_gpx(&activity, &gapped_track(jump), false).matches("<trkseg>").count();
        assert_eq!(segments(10), 3, "a jump before the gap");
        assert_eq!(segments(110), 3, "a jump after the gap");
        assert_eq!(segments(100), 2, "the first fix after the gap is both");
    }

    /// `n` rows of 8 m/s on a meridian, then a jump back to the start.
    fn looping_course(n: usize, after: usize) -> Vec<TrackPoint> {
        (0..n + after)
            .map(|k| {
                let along = if k < n { k } else { k - n };
                TrackPoint {
                    activity_id: "test".to_string(),
                    t: Some(format!("2025-06-01T{:02}:{:02}:{:02}+00:00", k / 3600, k / 60 % 60, k % 60)),
                    lat: Some(55.0 + 8.0 * along as f64 / 111_195.0),
                    lon: Some(37.0),
                    ..Default::default()
                }
            })
            .collect()
    }

    #[test]
    fn a_privacy_zone_keeps_the_break_of_a_jump_into_it() {
        let mut activity = sample_activity();
        activity.sport_type = "virtual_ride".to_string();
        // 5 km out, back to the start: the landing and 125 s after it are hidden,
        // and the rows taken out make the 4 km between what's left look ridden.
        let track = looping_course(625, 300);
        let gpx = activity_to_private_gpx(&activity, &track, 1000.0, false);
        assert_eq!(gpx.matches("<trkseg>").count(), 2);
        let second = gpx.split("<trkseg>").nth(2).unwrap();
        // The first fix out of the zone around the start, on the second lap.
        let first = 55.0 + 8.0 * 126.0 / 111_195.0;
        assert!(second.trim_start().starts_with(&format!("<trkpt lat=\"{first:.7}\"")), "{second:.80}");
    }

    #[test]
    fn a_privacy_zone_leaves_a_real_track_in_one_segment() {
        let gpx = activity_to_private_gpx(&sample_activity(), &looping_course(625, 300), 1000.0, false);
        assert_eq!(gpx.matches("<trkseg>").count(), 1);
    }

    #[test]
    fn a_jump_with_nothing_kept_after_it_breaks_nothing() {
        let track = looping_course(3, 2);
        let kept = [true, true, true, false, false];
        assert!(jumps_among_kept(&track, &kept, &[3]).is_empty());
        // Two jumps with no fix kept between them are one break.
        let kept = [true, false, false, false, true];
        assert_eq!(jumps_among_kept(&track, &kept, &[1, 3]), vec![1]);
        // A row without a fix can't carry one: the fix after it does.
        let mut gapped = looping_course(3, 2);
        gapped[3].lat = None;
        gapped[3].lon = None;
        assert_eq!(jumps_among_kept(&gapped, &[true; 5], &[3]), vec![4]);
    }

    #[test]
    fn a_jump_with_nothing_kept_before_it_opens_no_empty_segment() {
        let track = looping_course(3, 2);
        let kept = [false, false, false, true, true];
        assert!(jumps_among_kept(&track, &kept, &[3]).is_empty());
        // A later jump still breaks: the first one left nothing behind it.
        assert_eq!(jumps_among_kept(&track, &kept, &[3, 4]), vec![1]);
        // Through the writer: everything before the jump lies in the zone.
        let mut activity = sample_activity();
        activity.sport_type = "virtual_ride".to_string();
        let gpx = activity_to_private_gpx(&activity, &looping_course(50, 2000), 1000.0, false);
        assert_eq!(gpx.matches("<trkseg>").count(), 1);
        assert!(gpx.split("<trkseg>").nth(1).unwrap().contains("<trkpt"));
    }

    #[test]
    fn the_same_step_on_real_ground_keeps_one_segment() {
        let gpx = activity_to_gpx(&sample_activity(), &jumping_track(), false);
        assert_eq!(gpx.matches("<trkseg>").count(), 1);
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

    fn privacy_trim(trackpoints: &[TrackPoint], radius_m: f64) -> Vec<TrackPoint> {
        keep_rows(trackpoints, &privacy_kept(trackpoints, radius_m))
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
            ("indoor_ride", "cycling"),
            ("virtual_ride", "cycling"),
            ("trail_run", "running"),
            ("virtual_run", "running"),
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
