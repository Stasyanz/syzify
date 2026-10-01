use rusqlite::{params, Connection, OptionalExtension, Result};

use crate::models::gear::{GearRule, GearRuleKind, ProfileCandidate, RuleCandidates, SensorCandidate};

/// An item's rules, profile names first, in the order they were saved.
pub fn rules_of(conn: &Connection, gear_id: &str) -> Result<Vec<GearRule>> {
    let mut stmt = conn.prepare(
        "SELECT kind, value FROM gear_rule WHERE gear_id = ?1 \
         ORDER BY (kind <> 'profile_name'), id",
    )?;
    let rows = stmt.query_map(params![gear_id], |r| {
        let kind: String = r.get(0)?;
        Ok(GearRule {
            kind: GearRuleKind::parse(&kind).unwrap_or(GearRuleKind::ProfileName),
            value: r.get(1)?,
        })
    })?;
    rows.collect()
}

/// The sports an item of a kind can be put on by a rule: a bike takes
/// rides, shoes take what is done on foot, anything else takes any sport.
/// A rule is automatic, so unlike the manual pick this IS enforced — the
/// HRM strap worn on every workout must not put the runs on the bike.
/// The SQL fragment reads the item as `g` and the activity's sport as the
/// given expression.
fn kind_takes_sport(sport_expr: &str) -> String {
    format!(
        "(g.kind = 'other' \
          OR (g.kind = 'bike' AND {sport_expr} IN ('ride', 'mountain_bike')) \
          OR (g.kind = 'shoes' AND {sport_expr} IN ('run', 'trail_run', 'treadmill', 'walk', 'hike', 'mountaineering')))"
    )
}

/// The item's rules become exactly `rules`: the ones it no longer has go,
/// the new ones come — a value named here is taken over from whichever
/// item held it (one value, one item) — and the ones it keeps stay as
/// they were, so their place in the order (oldest first) survives a save
/// that only touched the notes.
pub fn set_rules(conn: &Connection, gear_id: &str, rules: &[GearRule]) -> Result<()> {
    let current = rules_of(conn, gear_id)?;
    let same = |a: &GearRule, b: &GearRule| a.kind == b.kind && a.value.eq_ignore_ascii_case(&b.value);
    for gone in current.iter().filter(|c| !rules.iter().any(|r| same(r, c))) {
        conn.execute(
            "DELETE FROM gear_rule WHERE gear_id = ?1 AND kind = ?2 AND value = ?3",
            params![gear_id, gone.kind.as_str(), gone.value],
        )?;
    }
    for new in rules.iter().filter(|r| !current.iter().any(|c| same(r, c))) {
        conn.execute(
            "INSERT OR REPLACE INTO gear_rule (gear_id, kind, value) VALUES (?1, ?2, ?3)",
            params![gear_id, new.kind.as_str(), new.value],
        )?;
    }
    Ok(())
}

/// What the vault has seen that a rule could match: the profile names and
/// the sensors the files carried, most frequent first.
pub fn candidates(conn: &Connection) -> Result<RuleCandidates> {
    let mut stmt = conn.prepare(
        "SELECT profile_name, COUNT(*) FROM activity WHERE profile_name IS NOT NULL \
         GROUP BY profile_name ORDER BY COUNT(*) DESC, profile_name COLLATE NOCASE",
    )?;
    let profiles = stmt
        .query_map([], |r| Ok(ProfileCandidate { value: r.get(0)?, count: r.get(1)? }))?
        .collect::<Result<Vec<_>>>()?;
    let mut stmt = conn.prepare(
        "SELECT serial, MAX(device_type), MAX(manufacturer), MAX(product), COUNT(*) \
         FROM activity_sensor GROUP BY serial ORDER BY COUNT(*) DESC, serial",
    )?;
    let sensors = stmt
        .query_map([], |r| {
            Ok(SensorCandidate {
                serial: r.get(0)?,
                device_type: r.get(1)?,
                manufacturer: r.get(2)?,
                product: r.get(3)?,
                count: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>>>()?;
    Ok(RuleCandidates { profiles, sensors })
}

/// The item a rule puts an activity on, given what its file carried: a
/// profile rule first (one profile per bike is the Garmin habit; pedals
/// move between bikes), then a sensor rule, oldest rule first within a
/// kind. Only items whose kind takes the sport, in use and already bought
/// by `start_time`, like the sport default.
pub fn gear_for(
    conn: &Connection,
    profile_name: Option<&str>,
    serials: &[String],
    sport_type: &str,
    start_time: &str,
) -> Result<Option<String>> {
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![
        Box::new(profile_name.map(str::to_string)),
        Box::new(start_time.to_string()),
        Box::new(sport_type.to_string()),
    ];
    let placeholders: Vec<String> = serials
        .iter()
        .map(|s| {
            params.push(Box::new(s.clone()));
            format!("?{}", params.len())
        })
        .collect();
    let serial_match = if placeholders.is_empty() {
        "0".to_string()
    } else {
        format!("(r.kind = 'sensor_serial' AND r.value IN ({}))", placeholders.join(", "))
    };
    let refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let takes = kind_takes_sport("?3");
    conn.query_row(
        &format!(
            "SELECT r.gear_id FROM gear_rule r JOIN gear g ON g.id = r.gear_id \
             WHERE g.retired_at IS NULL AND (g.purchased_at IS NULL OR ?2 >= g.purchased_at) \
               AND {takes} \
               AND ((r.kind = 'profile_name' AND ?1 IS NOT NULL AND r.value = ?1 COLLATE NOCASE) OR {serial_match}) \
             ORDER BY (r.kind <> 'profile_name'), r.id LIMIT 1"
        ),
        refs.as_slice(),
        |r| r.get(0),
    )
    .optional()
}

/// What a new import goes on: a rule match first, else the sport's
/// default — both only for items in use and bought by `start_time`.
pub fn gear_for_import(
    conn: &Connection,
    profile_name: Option<&str>,
    serials: &[String],
    sport_type: &str,
    start_time: &str,
) -> Result<Option<String>> {
    match gear_for(conn, profile_name, serials, sport_type, start_time)? {
        Some(g) => Ok(Some(g)),
        None => super::gear::default_for_sport(conn, sport_type, start_time),
    }
}

/// Put the rules over the history: every activity without gear that a
/// rule matches goes on the rule's item — profile rules before sensor
/// rules, so an activity matching both lands as `gear_for` would put it;
/// multisport wholes left out, items in use and bought by then only.
/// Returns how many activities were assigned.
pub fn apply_all(conn: &Connection) -> Result<usize> {
    let tx = conn.unchecked_transaction()?;
    let mut stmt = tx.prepare(
        "SELECT r.gear_id, r.kind, r.value FROM gear_rule r JOIN gear g ON g.id = r.gear_id \
         WHERE g.retired_at IS NULL ORDER BY (r.kind <> 'profile_name'), r.id",
    )?;
    let rules = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?
        .collect::<Result<Vec<_>>>()?;
    drop(stmt);
    let takes = kind_takes_sport("activity.sport_type");
    let mut n = 0;
    for (gear_id, kind, value) in rules {
        let matcher = if kind == "profile_name" {
            "activity.profile_name = ?2 COLLATE NOCASE"
        } else {
            "activity.id IN (SELECT activity_id FROM activity_sensor WHERE serial = ?2)"
        };
        n += tx.execute(
            &format!(
                "UPDATE activity SET gear_id = ?1 \
                 WHERE gear_id IS NULL AND {matcher} \
                   AND EXISTS (SELECT 1 FROM gear g WHERE g.id = ?1 AND {takes}) \
                   AND NOT EXISTS (SELECT 1 FROM activity c WHERE c.parent_id = activity.id) \
                   AND NOT EXISTS (SELECT 1 FROM multisport_leg l WHERE l.activity_id = activity.id) \
                   AND ((SELECT purchased_at FROM gear WHERE id = ?1) IS NULL \
                        OR start_time >= (SELECT purchased_at FROM gear WHERE id = ?1))"
            ),
            params![gear_id, value],
        )?;
    }
    tx.commit()?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::models::gear::{GearInput, GearKind};
    use crate::parser::SensorInfo;

    fn bike(name: &str, purchased: Option<&str>, rules: Vec<GearRule>) -> GearInput {
        GearInput {
            kind: GearKind::Bike,
            name: name.into(),
            brand: None,
            model: None,
            purchased_at: purchased.map(str::to_string),
            initial_distance_m: 0.0,
            distance_limit_m: None,
            notes: None,
            default_for: vec![],
            rules,
        }
    }
    fn profile(v: &str) -> GearRule {
        GearRule { kind: GearRuleKind::ProfileName, value: v.into() }
    }
    fn sensor(v: &str) -> GearRule {
        GearRule { kind: GearRuleKind::SensorSerial, value: v.into() }
    }
    fn activity(conn: &Connection, id: &str, start: &str, profile: Option<&str>, serials: &[&str], parent: Option<&str>) {
        conn.execute(
            "INSERT INTO activity (id, start_time, sport_type, profile_name, parent_id) VALUES (?1, ?2, 'ride', ?3, ?4)",
            params![id, start, profile, parent],
        )
        .unwrap();
        let sensors: Vec<SensorInfo> = serials
            .iter()
            .map(|s| SensorInfo { serial: (*s).into(), device_type: None, manufacturer: None, product: None })
            .collect();
        db::activity_sensors::replace(conn, id, &sensors).unwrap();
    }
    fn gear_of(conn: &Connection, id: &str) -> Option<String> {
        db::gear::gear_of_activity(conn, id).unwrap()
    }

    #[test]
    fn rules_are_saved_with_the_item_and_a_value_moves_to_the_item_that_claims_it() {
        let conn = db::test_db();
        let road = db::gear::insert(&conn, &bike("Road", None, vec![sensor("3632674300"), profile("ROAD")])).unwrap();
        assert_eq!(rules_of(&conn, &road.id).unwrap(), vec![profile("ROAD"), sensor("3632674300")], "profiles first");
        let gravel = db::gear::insert(&conn, &bike("Gravel", None, vec![profile("ROAD")])).unwrap();
        assert_eq!(rules_of(&conn, &road.id).unwrap(), vec![sensor("3632674300")]);
        assert_eq!(rules_of(&conn, &gravel.id).unwrap(), vec![profile("ROAD")]);
        // Deleting the item takes its rules with it.
        db::gear::delete(&conn, &gravel.id).unwrap();
        let left: i64 = conn.query_row("SELECT count(*) FROM gear_rule", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 1);
    }

    #[test]
    fn candidates_are_what_the_files_carried_most_frequent_first() {
        let conn = db::test_db();
        activity(&conn, "a1", "2026-01-01T08:00:00+03:00", Some("ROAD"), &["3632674300"], None);
        activity(&conn, "a2", "2026-01-02T08:00:00+03:00", Some("ROAD"), &["3632674300", "111"], None);
        activity(&conn, "a3", "2026-01-03T08:00:00+03:00", Some("MTB"), &[], None);
        activity(&conn, "a4", "2026-01-04T08:00:00+03:00", None, &[], None);
        conn.execute(
            "UPDATE activity_sensor SET device_type = 'bike_power', manufacturer = 'favero_electronics', product = 'assioma_duo' WHERE serial = '3632674300'",
            [],
        )
        .unwrap();
        let c = candidates(&conn).unwrap();
        assert_eq!(
            c.profiles,
            vec![ProfileCandidate { value: "ROAD".into(), count: 2 }, ProfileCandidate { value: "MTB".into(), count: 1 }]
        );
        assert_eq!(c.sensors.len(), 2);
        assert_eq!(c.sensors[0].serial, "3632674300");
        assert_eq!(c.sensors[0].count, 2);
        assert_eq!(c.sensors[0].product.as_deref(), Some("assioma_duo"));
        assert_eq!(c.sensors[1].serial, "111");
    }

    #[test]
    fn a_profile_rule_beats_a_sensor_rule_and_both_honour_retirement_and_purchase() {
        let conn = db::test_db();
        let road = db::gear::insert(&conn, &bike("Road", Some("2025-01-01"), vec![profile("ROAD")])).unwrap();
        let gravel = db::gear::insert(&conn, &bike("Gravel", None, vec![sensor("3632674300")])).unwrap();
        let serials = vec!["3632674300".to_string()];
        // Both match: the profile wins. Case does not matter for a profile.
        assert_eq!(gear_for(&conn, Some("road"), &serials, "ride", "2026-01-01").unwrap().as_deref(), Some(road.id.as_str()));
        // Only the sensor matches.
        assert_eq!(gear_for(&conn, Some("MTB"), &serials, "ride", "2026-01-01").unwrap().as_deref(), Some(gravel.id.as_str()));
        assert_eq!(gear_for(&conn, None, &serials, "ride", "2026-01-01").unwrap().as_deref(), Some(gravel.id.as_str()));
        // Nothing matches.
        assert_eq!(gear_for(&conn, Some("MTB"), &[], "ride", "2026-01-01").unwrap(), None);
        assert_eq!(gear_for(&conn, None, &[], "ride", "2026-01-01").unwrap(), None);
        // Before Road was bought the profile rule does not fire; the sensor one does.
        assert_eq!(gear_for(&conn, Some("ROAD"), &serials, "ride", "2024-06-01").unwrap().as_deref(), Some(gravel.id.as_str()));
        // A retired item's rules sleep.
        db::gear::set_retired(&conn, &road.id, true).unwrap();
        assert_eq!(gear_for(&conn, Some("ROAD"), &[], "ride", "2026-01-01").unwrap(), None);
    }

    #[test]
    fn a_rule_only_fires_for_a_sport_the_item_s_kind_takes() {
        let conn = db::test_db();
        // The HRM strap is worn on every workout: a rule on it on the bike
        // must not put the runs on the bike.
        let road = db::gear::insert(&conn, &bike("Road", None, vec![sensor("3945193103"), profile("Bike")])).unwrap();
        let mut shoes = bike("Pegasus", None, vec![profile("Run")]);
        shoes.kind = GearKind::Shoes;
        let shoes = db::gear::insert(&conn, &shoes).unwrap();
        let mut strap = bike("Strap", None, vec![sensor("999")]);
        strap.kind = GearKind::Other;
        let strap = db::gear::insert(&conn, &strap).unwrap();
        let hrm = vec!["3945193103".to_string()];
        assert_eq!(gear_for(&conn, None, &hrm, "ride", "2026-01-01").unwrap().as_deref(), Some(road.id.as_str()));
        assert_eq!(gear_for(&conn, None, &hrm, "mountain_bike", "2026-01-01").unwrap().as_deref(), Some(road.id.as_str()));
        assert_eq!(gear_for(&conn, None, &hrm, "run", "2026-01-01").unwrap(), None, "a run is no ride");
        assert_eq!(gear_for(&conn, None, &hrm, "swim", "2026-01-01").unwrap(), None);
        assert_eq!(gear_for(&conn, Some("Run"), &[], "run", "2026-01-01").unwrap().as_deref(), Some(shoes.id.as_str()));
        assert_eq!(gear_for(&conn, Some("Run"), &[], "ride", "2026-01-01").unwrap(), None, "shoes take no ride");
        // "Other" takes anything.
        let other = vec!["999".to_string()];
        assert_eq!(gear_for(&conn, None, &other, "swim", "2026-01-01").unwrap().as_deref(), Some(strap.id.as_str()));
        // Over the history the same gate holds.
        activity(&conn, "ride", "2026-01-01T08:00:00+03:00", None, &["3945193103"], None);
        activity(&conn, "run", "2026-01-02T08:00:00+03:00", None, &["3945193103"], None);
        conn.execute("UPDATE activity SET sport_type = 'run' WHERE id = 'run'", []).unwrap();
        assert_eq!(apply_all(&conn).unwrap(), 1);
        assert_eq!(gear_of(&conn, "ride").as_deref(), Some(road.id.as_str()));
        assert_eq!(gear_of(&conn, "run"), None);
    }

    #[test]
    fn a_save_that_keeps_a_rule_keeps_its_place_in_the_order_and_case_is_one_value() {
        let conn = db::test_db();
        let road = db::gear::insert(&conn, &bike("Road", None, vec![sensor("111")])).unwrap();
        let gravel = db::gear::insert(&conn, &bike("Gravel", None, vec![sensor("222")])).unwrap();
        let both = vec!["111".to_string(), "222".to_string()];
        assert_eq!(gear_for(&conn, None, &both, "ride", "2026-01-01").unwrap().as_deref(), Some(road.id.as_str()), "oldest rule first");
        // Editing Road's notes re-saves its rules unchanged: still oldest.
        let mut edit = bike("Road", None, vec![sensor("111")]);
        edit.notes = Some("new saddle".into());
        db::gear::update(&conn, &road.id, &edit).unwrap();
        assert_eq!(gear_for(&conn, None, &both, "ride", "2026-01-01").unwrap().as_deref(), Some(road.id.as_str()));
        // Dropping and re-adding the rule does make it the youngest.
        db::gear::update(&conn, &road.id, &bike("Road", None, vec![])).unwrap();
        db::gear::update(&conn, &road.id, &bike("Road", None, vec![sensor("111")])).unwrap();
        assert_eq!(gear_for(&conn, None, &both, "ride", "2026-01-01").unwrap().as_deref(), Some(gravel.id.as_str()));
        // "Road" claimed by Gravel takes "ROAD" off Road: one value, any case.
        db::gear::update(&conn, &road.id, &bike("Road", None, vec![profile("ROAD")])).unwrap();
        db::gear::update(&conn, &gravel.id, &bike("Gravel", None, vec![profile("Road")])).unwrap();
        assert_eq!(rules_of(&conn, &road.id).unwrap(), vec![]);
        assert_eq!(rules_of(&conn, &gravel.id).unwrap(), vec![profile("Road")]);
        let n: i64 = conn.query_row("SELECT count(*) FROM gear_rule", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn an_import_takes_a_rule_match_before_the_sport_default() {
        let conn = db::test_db();
        let mut road = bike("Road", None, vec![profile("ROAD")]);
        road.default_for = vec!["ride".into()];
        let road = db::gear::insert(&conn, &road).unwrap();
        let mtb = db::gear::insert(&conn, &bike("MTB", None, vec![profile("MTB")])).unwrap();
        let none: Vec<String> = vec![];
        // The MTB profile overrides the ride default; no profile falls back to it.
        assert_eq!(gear_for_import(&conn, Some("MTB"), &none, "ride", "2026-01-01").unwrap().as_deref(), Some(mtb.id.as_str()));
        assert_eq!(gear_for_import(&conn, Some("Bike"), &none, "ride", "2026-01-01").unwrap().as_deref(), Some(road.id.as_str()));
        assert_eq!(gear_for_import(&conn, None, &none, "ride", "2026-01-01").unwrap().as_deref(), Some(road.id.as_str()));
        assert_eq!(gear_for_import(&conn, None, &none, "run", "2026-01-01").unwrap(), None);
    }

    #[test]
    fn applying_the_rules_over_the_history_assigns_the_unassigned_only_and_skips_multisport() {
        let conn = db::test_db();
        let road = db::gear::insert(&conn, &bike("Road", Some("2025-01-01"), vec![profile("ROAD")])).unwrap();
        let gravel = db::gear::insert(&conn, &bike("Gravel", None, vec![sensor("3632674300")])).unwrap();
        let other = db::gear::insert(&conn, &bike("Other", None, vec![])).unwrap();
        activity(&conn, "both", "2026-01-01T08:00:00+03:00", Some("ROAD"), &["3632674300"], None);
        activity(&conn, "sensor-only", "2026-01-02T08:00:00+03:00", Some("MTB"), &["3632674300"], None);
        activity(&conn, "early-road", "2024-06-01T08:00:00+03:00", Some("ROAD"), &[], None);
        activity(&conn, "taken", "2026-01-03T08:00:00+03:00", Some("ROAD"), &[], None);
        db::gear::set_activity_gear(&conn, "taken", Some(&other.id)).unwrap();
        activity(&conn, "plain", "2026-01-04T08:00:00+03:00", Some("Bike"), &[], None);
        activity(&conn, "container", "2026-01-05T07:00:00+03:00", Some("ROAD"), &[], None);
        activity(&conn, "leg", "2026-01-05T08:00:00+03:00", Some("ROAD"), &[], Some("container"));
        activity(&conn, "native", "2026-01-06T07:00:00+03:00", Some("ROAD"), &[], None);
        conn.execute("INSERT INTO multisport_leg (activity_id, leg_number, sport_type) VALUES ('native', 1, 'ride')", []).unwrap();

        assert_eq!(apply_all(&conn).unwrap(), 3);
        assert_eq!(gear_of(&conn, "both").as_deref(), Some(road.id.as_str()), "profile beats sensor");
        assert_eq!(gear_of(&conn, "sensor-only").as_deref(), Some(gravel.id.as_str()));
        assert_eq!(gear_of(&conn, "leg").as_deref(), Some(road.id.as_str()), "a merged leg carries its own");
        assert_eq!(gear_of(&conn, "early-road"), None, "before the purchase");
        assert_eq!(gear_of(&conn, "taken").as_deref(), Some(other.id.as_str()), "already on an item");
        assert_eq!(gear_of(&conn, "plain"), None);
        assert_eq!(gear_of(&conn, "container"), None);
        assert_eq!(gear_of(&conn, "native"), None);
        assert_eq!(apply_all(&conn).unwrap(), 0, "nothing left");
    }
}
