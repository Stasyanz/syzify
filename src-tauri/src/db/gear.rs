use rusqlite::{params, Connection, OptionalExtension, Result};

use crate::models::activity::ActivityFilters;
use crate::models::gear::{Gear, GearInput, GearItem, GearKind, GearStats, GearTargets, MovedFrom};

const GEAR_COLUMNS: &str = "id, kind, name, brand, model, purchased_at, initial_distance_m, \
    distance_limit_m, retired_at, notes, created_at";

fn row_to_gear(row: &rusqlite::Row) -> Result<Gear> {
    let kind: String = row.get(1)?;
    Ok(Gear {
        id: row.get(0)?,
        kind: GearKind::parse(&kind).unwrap_or(GearKind::Other),
        name: row.get(2)?,
        brand: row.get(3)?,
        model: row.get(4)?,
        purchased_at: row.get(5)?,
        initial_distance_m: row.get(6)?,
        distance_limit_m: row.get(7)?,
        retired_at: row.get(8)?,
        notes: row.get(9)?,
        created_at: row.get(10)?,
    })
}

/// Every item with its totals and defaults: the ones in use first, then
/// bikes, shoes, the rest, by name. The totals are summed over the activities carrying
/// the item — a merged triathlon's legs carry theirs, the container none
/// (stage 2 never offers an item to a container), so nothing counts twice.
pub fn list(conn: &Connection) -> Result<Vec<GearItem>> {
    let sql = format!(
        "SELECT {cols}, \
                COALESCE(s.n, 0), COALESCE(s.dist, 0), COALESCE(s.dur, 0), \
                COALESCE(s.elev, 0), s.last_used \
         FROM gear g \
         LEFT JOIN (SELECT gear_id, COUNT(*) AS n, \
                           SUM(COALESCE(distance_m, 0)) AS dist, \
                           SUM(COALESCE(duration_s, 0)) AS dur, \
                           SUM(COALESCE(elev_gain_m, 0)) AS elev, \
                           MAX(start_time) AS last_used \
                    FROM activity WHERE gear_id IS NOT NULL GROUP BY gear_id) s \
           ON s.gear_id = g.id \
         ORDER BY (g.retired_at IS NOT NULL), \
                  CASE g.kind WHEN 'bike' THEN 0 WHEN 'shoes' THEN 1 ELSE 2 END, \
                  g.name COLLATE NOCASE, g.created_at",
        cols = GEAR_COLUMNS
            .split(", ")
            .map(|c| format!("g.{c}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        let gear = row_to_gear(row)?;
        let stats = GearStats {
            activities: row.get(11)?,
            distance_m: row.get(12)?,
            duration_s: row.get(13)?,
            elev_gain_m: row.get(14)?,
            last_used: row.get(15)?,
        };
        Ok((gear, stats))
    })?;
    let mut items = Vec::new();
    for row in rows {
        let (gear, stats) = row?;
        let default_for = defaults_of(conn, &gear.id)?;
        items.push(GearItem { gear, stats, default_for });
    }
    Ok(items)
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<Gear>> {
    conn.query_row(
        &format!("SELECT {GEAR_COLUMNS} FROM gear WHERE id = ?1"),
        params![id],
        row_to_gear,
    )
    .optional()
}

fn defaults_of(conn: &Connection, gear_id: &str) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT sport_type FROM gear_default WHERE gear_id = ?1 ORDER BY sport_type")?;
    let rows = stmt.query_map(params![gear_id], |r| r.get::<_, String>(0))?;
    rows.collect()
}

/// The item an import of `sport_type` starting at `start_time` gets, if
/// one is set, in use and already bought by then — a 2019 archive must
/// not land on a bike bought in 2024 (the same rule as `assign_history`).
pub fn default_for_sport(conn: &Connection, sport_type: &str, start_time: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT d.gear_id FROM gear_default d JOIN gear g ON g.id = d.gear_id \
         WHERE d.sport_type = ?1 AND g.retired_at IS NULL \
           AND (g.purchased_at IS NULL OR ?2 >= g.purchased_at)",
        params![sport_type, start_time],
        |r| r.get(0),
    )
    .optional()
}

/// Create an item from a normalized input (see `GearInput::normalized`).
/// The row and its defaults land together or not at all: a half-made
/// item would be duplicated by the retry.
pub fn insert(conn: &Connection, input: &GearInput) -> Result<Gear> {
    let tx = conn.unchecked_transaction()?;
    let id = uuid::Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO gear (id, kind, name, brand, model, purchased_at, initial_distance_m, \
                           distance_limit_m, notes) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            input.kind.as_str(),
            input.name,
            input.brand,
            input.model,
            input.purchased_at,
            input.initial_distance_m,
            input.distance_limit_m,
            input.notes,
        ],
    )?;
    set_defaults(&tx, &id, &input.default_for)?;
    let gear = get(&tx, &id)?;
    tx.commit()?;
    Ok(gear.expect("the row just inserted"))
}

/// Replace an item's fields and defaults. Ok(false) when there is no such
/// item. A retired item keeps no defaults whatever the input says — the
/// command refuses such an input; this is the floor under it.
pub fn update(conn: &Connection, id: &str, input: &GearInput) -> Result<bool> {
    let tx = conn.unchecked_transaction()?;
    let retired = match get(&tx, id)? {
        Some(g) => g.retired_at.is_some(),
        None => return Ok(false),
    };
    tx.execute(
        "UPDATE gear SET kind = ?2, name = ?3, brand = ?4, model = ?5, purchased_at = ?6, \
                         initial_distance_m = ?7, distance_limit_m = ?8, notes = ?9 \
         WHERE id = ?1",
        params![
            id,
            input.kind.as_str(),
            input.name,
            input.brand,
            input.model,
            input.purchased_at,
            input.initial_distance_m,
            input.distance_limit_m,
            input.notes,
        ],
    )?;
    let defaults: &[String] = if retired { &[] } else { &input.default_for };
    set_defaults(&tx, id, defaults)?;
    tx.commit()?;
    Ok(true)
}

/// Whether the item exists and is retired (None = no such item).
pub fn is_retired(conn: &Connection, id: &str) -> Result<Option<bool>> {
    Ok(get(conn, id)?.map(|g| g.retired_at.is_some()))
}

/// The item an activity is on, if any (None also for no such activity).
pub fn gear_of_activity(conn: &Connection, activity_id: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT gear_id FROM activity WHERE id = ?1",
        params![activity_id],
        |r| r.get::<_, Option<String>>(0),
    )
    .optional()
    .map(|r| r.flatten())
}

/// Put an activity on an item, or take it off (None). Ok(false) when there
/// is no such activity. The FK refuses an unknown item; the command checks
/// the activity is no multisport whole first.
pub fn set_activity_gear(conn: &Connection, activity_id: &str, gear_id: Option<&str>) -> Result<bool> {
    let n = conn.execute(
        "UPDATE activity SET gear_id = ?2 WHERE id = ?1",
        params![activity_id, gear_id],
    )?;
    Ok(n > 0)
}

/// The WHERE of a library filter, minus its sort, limit and offset, over
/// the activities that can carry gear: the facets the list applies (so
/// what the user sees is what is touched), and no multisport whole.
fn filtered_targets_sql(filters: &ActivityFilters, params: &mut Vec<Box<dyn rusqlite::types::ToSql>>) -> String {
    let mut conditions: Vec<String> = Vec::new();
    let mut idx = params.len() + 1;
    super::activities::push_facet_conditions(filters, "a.", &mut conditions, params, &mut idx);
    conditions.push("NOT EXISTS (SELECT 1 FROM activity c WHERE c.parent_id = a.id)".into());
    conditions.push("NOT EXISTS (SELECT 1 FROM multisport_leg l WHERE l.activity_id = a.id)".into());
    format!("SELECT a.id FROM activity a WHERE {}", conditions.join(" AND "))
}

/// What `assign_filtered` onto `gear_id` would do, for the confirmation:
/// the same rows it would update (so "Put N on X" is the N the toast
/// will say), and the items they leave, by name.
pub fn filtered_targets(conn: &Connection, filters: &ActivityFilters, gear_id: &str) -> Result<GearTargets> {
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(gear_id.to_string())];
    let targets = filtered_targets_sql(filters, &mut params);
    let refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let eligible: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM activity WHERE id IN ({targets}) AND gear_id IS NOT ?1"),
        refs.as_slice(),
        |r| r.get(0),
    )?;
    let mut stmt = conn.prepare(&format!(
        "SELECT g.name, COUNT(*) FROM activity x JOIN gear g ON g.id = x.gear_id \
         WHERE x.id IN ({targets}) AND x.gear_id <> ?1 \
         GROUP BY g.id ORDER BY COUNT(*) DESC, g.name COLLATE NOCASE"
    ))?;
    let moved_from = stmt
        .query_map(refs.as_slice(), |r| Ok(MovedFrom { name: r.get(0)?, count: r.get(1)? }))?
        .collect::<Result<Vec<_>>>()?;
    Ok(GearTargets { eligible, moved_from })
}

/// The library's bulk action: put the item on every activity the filter
/// matches (moving the ones already on other gear — the user picked them),
/// multisport wholes left out. Returns how many were assigned.
pub fn assign_filtered(conn: &Connection, filters: &ActivityFilters, gear_id: &str) -> Result<usize> {
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(gear_id.to_string())];
    let targets = filtered_targets_sql(filters, &mut params);
    let refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    conn.execute(
        &format!("UPDATE activity SET gear_id = ?1 WHERE id IN ({targets}) AND gear_id IS NOT ?1"),
        refs.as_slice(),
    )
}

/// The Garage card's quick action: put the item on every activity of its
/// default sports that has no gear yet, from its purchase date on (all
/// time without one). Multisport wholes (merged containers, FIT-native
/// files) are skipped: their aggregate spans several sports. Returns how
/// many activities were assigned.
pub fn assign_history(conn: &Connection, gear_id: &str) -> Result<usize> {
    let since: Option<String> = conn.query_row(
        "SELECT purchased_at FROM gear WHERE id = ?1",
        params![gear_id],
        |r| r.get(0),
    )?;
    // start_time is ISO ("2026-10-01T07:55:38+03:00" from FIT, with the
    // local offset; a GPX/TCX keeps its file's form, often "…Z"); against a
    // bare date the string order is the date order, and a day's activities
    // sort after the day itself, so ">=" takes the purchase day in. The
    // date compared is the one the string carries — for a "…Z" file the
    // UTC one — so an activity in the small hours of the purchase day can
    // fall a day short; hours on one boundary day, accepted.
    conn.execute(
        "UPDATE activity SET gear_id = ?1 \
         WHERE gear_id IS NULL \
           AND sport_type IN (SELECT sport_type FROM gear_default WHERE gear_id = ?1) \
           AND (?2 IS NULL OR start_time >= ?2) \
           AND NOT EXISTS (SELECT 1 FROM activity c WHERE c.parent_id = activity.id) \
           AND NOT EXISTS (SELECT 1 FROM multisport_leg l WHERE l.activity_id = activity.id)",
        params![gear_id, since],
    )
}

/// The item's defaults become exactly `sports`; a sport named here is
/// taken over from whichever item held it (one default per sport).
fn set_defaults(conn: &Connection, gear_id: &str, sports: &[String]) -> Result<()> {
    conn.execute("DELETE FROM gear_default WHERE gear_id = ?1", params![gear_id])?;
    for sport in sports {
        conn.execute(
            "INSERT OR REPLACE INTO gear_default (sport_type, gear_id) VALUES (?1, ?2)",
            params![sport, gear_id],
        )?;
    }
    Ok(())
}

/// Retire or bring back an item. A retired item keeps its activities and
/// its mileage but stops being a default: new imports must not land on
/// shoes that are in the bin. Ok(false) when there is no such item.
pub fn set_retired(conn: &Connection, id: &str, retired: bool) -> Result<bool> {
    let tx = conn.unchecked_transaction()?;
    let n = if retired {
        tx.execute(
            "UPDATE gear SET retired_at = datetime('now') WHERE id = ?1 AND retired_at IS NULL",
            params![id],
        )?
    } else {
        tx.execute(
            "UPDATE gear SET retired_at = NULL WHERE id = ?1 AND retired_at IS NOT NULL",
            params![id],
        )?
    };
    if n == 0 {
        return Ok(get(&tx, id)?.is_some());
    }
    if retired {
        tx.execute("DELETE FROM gear_default WHERE gear_id = ?1", params![id])?;
    }
    tx.commit()?;
    Ok(true)
}

/// Delete an item. Its activities stay and are detached (the FK says SET
/// NULL; done by hand too, so the outcome never depends on the pragma).
pub fn delete(conn: &Connection, id: &str) -> Result<bool> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("UPDATE activity SET gear_id = NULL WHERE gear_id = ?1", params![id])?;
    let n = tx.execute("DELETE FROM gear WHERE id = ?1", params![id])?;
    tx.commit()?;
    Ok(n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn bike(name: &str) -> GearInput {
        GearInput {
            kind: GearKind::Bike,
            name: name.into(),
            brand: None,
            model: None,
            purchased_at: None,
            initial_distance_m: 1000.0,
            distance_limit_m: None,
            notes: None,
            default_for: vec!["ride".into()],
        }
    }

    fn activity(conn: &Connection, id: &str, start: &str, dist: f64, dur: f64, elev: Option<f64>, gear: Option<&str>) {
        conn.execute(
            "INSERT INTO activity (id, start_time, sport_type, distance_m, duration_s, elev_gain_m, gear_id) \
             VALUES (?1, ?2, 'ride', ?3, ?4, ?5, ?6)",
            params![id, start, dist, dur, elev, gear],
        )
        .unwrap();
    }

    #[test]
    fn totals_come_from_the_activities_and_defaults_follow_the_item() {
        let conn = db::test_db();
        let g = insert(&conn, &bike("Road")).unwrap();
        activity(&conn, "a1", "2026-09-01T08:00:00+03:00", 30_000.0, 3600.0, Some(200.0), Some(&g.id));
        activity(&conn, "a2", "2026-09-05T08:00:00+03:00", 20_000.0, 2400.0, None, Some(&g.id));
        activity(&conn, "a3", "2026-09-09T08:00:00+03:00", 99_000.0, 9.0, Some(9.0), None);

        let items = list(&conn).unwrap();
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.gear.name, "Road");
        assert_eq!(it.stats.activities, 2);
        assert_eq!(it.stats.distance_m, 50_000.0);
        assert_eq!(it.stats.duration_s, 6000.0);
        assert_eq!(it.stats.elev_gain_m, 200.0);
        assert_eq!(it.stats.last_used.as_deref(), Some("2026-09-05T08:00:00+03:00"));
        assert_eq!(it.default_for, vec!["ride"]);
        assert_eq!(default_for_sport(&conn, "ride", "2026-09-01T08:00:00+03:00").unwrap().as_deref(), Some(g.id.as_str()));
        assert_eq!(default_for_sport(&conn, "run", "2026-09-01T08:00:00+03:00").unwrap(), None);
        // Bought on a date: nothing earlier lands on it, the day itself does.
        let mut dated = bike("Road");
        dated.purchased_at = Some("2024-06-15".into());
        update(&conn, &g.id, &dated).unwrap();
        assert_eq!(default_for_sport(&conn, "ride", "2024-06-14T23:00:00+03:00").unwrap(), None);
        assert_eq!(default_for_sport(&conn, "ride", "2024-06-15T06:00:00+03:00").unwrap().as_deref(), Some(g.id.as_str()));
    }

    #[test]
    fn an_item_without_activities_lists_with_zero_totals() {
        let conn = db::test_db();
        insert(&conn, &bike("New")).unwrap();
        let it = &list(&conn).unwrap()[0];
        assert_eq!(it.stats, GearStats::default());
    }

    #[test]
    fn a_sport_has_one_default_and_it_moves_to_the_item_that_claims_it() {
        let conn = db::test_db();
        let a = insert(&conn, &bike("A")).unwrap();
        let b = insert(&conn, &bike("B")).unwrap();
        assert_eq!(default_for_sport(&conn, "ride", "2026-01-01").unwrap().as_deref(), Some(b.id.as_str()));
        let items = list(&conn).unwrap();
        let of = |id: &str| items.iter().find(|i| i.gear.id == id).unwrap().default_for.clone();
        assert!(of(&a.id).is_empty());
        assert_eq!(of(&b.id), vec!["ride"]);

        // Updating A to claim it takes it back; B keeps nothing.
        let mut claim = bike("A");
        claim.default_for = vec!["ride".into(), "mountain_bike".into()];
        assert!(update(&conn, &a.id, &claim).unwrap());
        assert_eq!(default_for_sport(&conn, "ride", "2026-01-01").unwrap().as_deref(), Some(a.id.as_str()));
        assert_eq!(default_for_sport(&conn, "mountain_bike", "2026-01-01").unwrap().as_deref(), Some(a.id.as_str()));
        assert!(!update(&conn, "nope", &claim).unwrap());
    }

    #[test]
    fn update_replaces_the_fields() {
        let conn = db::test_db();
        let g = insert(&conn, &bike("Old")).unwrap();
        let mut next = bike("New");
        next.kind = GearKind::Other;
        next.brand = Some("Canyon".into());
        next.purchased_at = Some("2025-03-01".into());
        next.distance_limit_m = Some(5000.0);
        next.notes = Some("n".into());
        assert!(update(&conn, &g.id, &next).unwrap());
        let got = get(&conn, &g.id).unwrap().unwrap();
        assert_eq!(got.kind, GearKind::Other);
        assert_eq!(got.name, "New");
        assert_eq!(got.brand.as_deref(), Some("Canyon"));
        assert_eq!(got.purchased_at.as_deref(), Some("2025-03-01"));
        assert_eq!(got.distance_limit_m, Some(5000.0));
        assert_eq!(got.notes.as_deref(), Some("n"));
        assert_eq!(got.created_at, g.created_at);
    }

    #[test]
    fn retiring_keeps_the_mileage_but_drops_the_defaults_and_sorts_last() {
        let conn = db::test_db();
        let old = insert(&conn, &bike("Old")).unwrap();
        activity(&conn, "a1", "2026-09-01T08:00:00+03:00", 1000.0, 60.0, None, Some(&old.id));
        let new = insert(&conn, &bike("Aaa new")).unwrap();
        // "Aaa new" claimed ride; give it back to Old to see retirement drop it.
        let mut back = bike("Old");
        back.default_for = vec!["ride".into()];
        update(&conn, &old.id, &back).unwrap();
        assert_eq!(default_for_sport(&conn, "ride", "2026-01-01").unwrap().as_deref(), Some(old.id.as_str()));

        assert!(set_retired(&conn, &old.id, true).unwrap());
        assert_eq!(default_for_sport(&conn, "ride", "2026-01-01").unwrap(), None, "a retired item is no default");
        let items = list(&conn).unwrap();
        assert_eq!(items[0].gear.id, new.id, "in-use items first");
        assert!(items[1].gear.retired_at.is_some());
        assert_eq!(items[1].stats.distance_m, 1000.0, "mileage stays");
        assert!(items[1].default_for.is_empty());

        // Editing the retired item cannot hand it a default back: the
        // active item's default stays where it is.
        update(&conn, &new.id, &back).unwrap();
        assert_eq!(default_for_sport(&conn, "ride", "2026-01-01").unwrap().as_deref(), Some(new.id.as_str()));
        let mut sneak = bike("Old renamed");
        sneak.default_for = vec!["ride".into()];
        assert!(update(&conn, &old.id, &sneak).unwrap());
        assert_eq!(default_for_sport(&conn, "ride", "2026-01-01").unwrap().as_deref(), Some(new.id.as_str()));
        assert_eq!(get(&conn, &old.id).unwrap().unwrap().name, "Old renamed");
        assert_eq!(is_retired(&conn, &old.id).unwrap(), Some(true));
        assert_eq!(is_retired(&conn, &new.id).unwrap(), Some(false));
        assert_eq!(is_retired(&conn, "nope").unwrap(), None);

        // Retiring twice is a no-op that still reports the item exists;
        // bringing it back clears the stamp.
        assert!(set_retired(&conn, &old.id, true).unwrap());
        assert!(set_retired(&conn, &old.id, false).unwrap());
        assert!(get(&conn, &old.id).unwrap().unwrap().retired_at.is_none());
        assert!(!set_retired(&conn, "nope", true).unwrap());
    }

    #[test]
    fn deleting_detaches_the_activities_and_their_defaults_go_too() {
        let conn = db::test_db();
        let g = insert(&conn, &bike("Gone")).unwrap();
        activity(&conn, "a1", "2026-09-01T08:00:00+03:00", 1000.0, 60.0, None, Some(&g.id));
        assert!(delete(&conn, &g.id).unwrap());
        assert!(!delete(&conn, &g.id).unwrap());
        let gear_of_a1: Option<String> = conn
            .query_row("SELECT gear_id FROM activity WHERE id = 'a1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(gear_of_a1, None);
        let activities: i64 = conn.query_row("SELECT count(*) FROM activity", [], |r| r.get(0)).unwrap();
        assert_eq!(activities, 1, "the activity itself stays");
        let defaults: i64 = conn.query_row("SELECT count(*) FROM gear_default", [], |r| r.get(0)).unwrap();
        assert_eq!(defaults, 0);
        assert!(list(&conn).unwrap().is_empty());
    }

    fn activity_of(conn: &Connection, id: &str, start: &str, sport: &str, parent: Option<&str>) {
        conn.execute(
            "INSERT INTO activity (id, start_time, sport_type, parent_id) VALUES (?1, ?2, ?3, ?4)",
            params![id, start, sport, parent],
        )
        .unwrap();
    }

    #[test]
    fn an_activity_goes_on_an_item_and_comes_off_again() {
        let conn = db::test_db();
        let g = insert(&conn, &bike("Road")).unwrap();
        activity_of(&conn, "a1", "2026-09-01T08:00:00+03:00", "ride", None);
        assert_eq!(gear_of_activity(&conn, "a1").unwrap(), None);
        assert!(set_activity_gear(&conn, "a1", Some(&g.id)).unwrap());
        assert_eq!(gear_of_activity(&conn, "a1").unwrap().as_deref(), Some(g.id.as_str()));
        assert_eq!(list(&conn).unwrap()[0].stats.activities, 1);
        assert!(set_activity_gear(&conn, "a1", None).unwrap());
        assert_eq!(gear_of_activity(&conn, "a1").unwrap(), None);
        assert!(!set_activity_gear(&conn, "nope", Some(&g.id)).unwrap());
        assert_eq!(gear_of_activity(&conn, "nope").unwrap(), None);
        // An unknown item is refused by the schema, not stored as a dangling id.
        assert!(set_activity_gear(&conn, "a1", Some("ghost")).is_err());
    }

    #[test]
    fn history_assignment_takes_the_default_sports_from_the_purchase_on_and_skips_multisport() {
        let conn = db::test_db();
        let mut road = bike("Road");
        road.purchased_at = Some("2024-01-01".into());
        road.default_for = vec!["ride".into(), "mountain_bike".into()];
        let g = insert(&conn, &road).unwrap();
        let other = insert(&conn, &bike("Other bike")).unwrap(); // claims nothing: Road re-claims below
        let mut reclaim = road.clone();
        reclaim.default_for = vec!["ride".into(), "mountain_bike".into()];
        update(&conn, &g.id, &reclaim).unwrap();

        activity_of(&conn, "before", "2023-12-31T23:00:00+03:00", "ride", None);
        activity_of(&conn, "on-the-day", "2024-01-01T07:00:00+03:00", "ride", None);
        activity_of(&conn, "mtb", "2025-05-05T07:00:00+03:00", "mountain_bike", None);
        activity_of(&conn, "run", "2025-05-06T07:00:00+03:00", "run", None);
        activity_of(&conn, "taken", "2025-05-07T07:00:00+03:00", "ride", None);
        set_activity_gear(&conn, "taken", Some(&other.id)).unwrap();
        // A merged container (it has a child) and a native multisport
        // (it has leg rows) are both skipped; the merged leg itself is not.
        activity_of(&conn, "container", "2025-06-01T07:00:00+03:00", "triathlon", None);
        activity_of(&conn, "leg", "2025-06-01T08:00:00+03:00", "ride", Some("container"));
        activity_of(&conn, "native", "2025-07-01T07:00:00+03:00", "ride", None);
        conn.execute(
            "INSERT INTO multisport_leg (activity_id, leg_number, sport_type) VALUES ('native', 1, 'ride')",
            [],
        )
        .unwrap();

        assert_eq!(assign_history(&conn, &g.id).unwrap(), 3);
        let on_road = |id: &str| gear_of_activity(&conn, id).unwrap().as_deref() == Some(g.id.as_str());
        assert!(on_road("on-the-day"));
        assert!(on_road("mtb"));
        assert!(on_road("leg"));
        assert!(!on_road("before"), "earlier than the purchase");
        assert!(!on_road("run"), "not a default sport");
        assert!(!on_road("container"));
        assert!(!on_road("native"));
        assert_eq!(gear_of_activity(&conn, "taken").unwrap().as_deref(), Some(other.id.as_str()), "already on an item");
        // Nothing left to take: a second run assigns none.
        assert_eq!(assign_history(&conn, &g.id).unwrap(), 0);

        // Without a purchase date the whole history qualifies.
        let mut undated = road.clone();
        undated.purchased_at = None;
        update(&conn, &g.id, &undated).unwrap();
        set_activity_gear(&conn, "before", None).unwrap();
        assert_eq!(assign_history(&conn, &g.id).unwrap(), 1);
        assert!(on_road("before"));
    }

    #[test]
    fn a_filtered_assignment_touches_what_the_filter_shows_and_no_multisport_whole() {
        let conn = db::test_db();
        let road = insert(&conn, &bike("Road")).unwrap();
        let gravel = insert(&conn, &bike("Gravel")).unwrap();
        activity_of(&conn, "jan", "2026-01-10T08:00:00+03:00", "ride", None);
        activity_of(&conn, "feb", "2026-02-10T08:00:00+03:00", "ride", None);
        activity_of(&conn, "feb-run", "2026-02-11T08:00:00+03:00", "run", None);
        activity_of(&conn, "feb-on-gravel", "2026-02-12T08:00:00+03:00", "ride", None);
        set_activity_gear(&conn, "feb-on-gravel", Some(&gravel.id)).unwrap();
        activity_of(&conn, "container", "2026-02-13T07:00:00+03:00", "triathlon", None);
        activity_of(&conn, "leg", "2026-02-13T08:00:00+03:00", "ride", Some("container"));
        activity_of(&conn, "native", "2026-02-14T07:00:00+03:00", "ride", None);
        conn.execute(
            "INSERT INTO multisport_leg (activity_id, leg_number, sport_type) VALUES ('native', 1, 'ride')",
            [],
        )
        .unwrap();

        // February rides: feb and feb-on-gravel; the container and the
        // native file are out, the leg is hidden from the library (its
        // parent_id), the run and January are off the filter.
        let feb_rides = ActivityFilters {
            sport_types: Some(vec!["ride".into()]),
            date_from: Some("2026-02-01".into()),
            limit: Some(1), // the list's page size must not cap a bulk action
            ..Default::default()
        };
        assert_eq!(
            filtered_targets(&conn, &feb_rides, &road.id).unwrap(),
            GearTargets { eligible: 2, moved_from: vec![MovedFrom { name: "Gravel".into(), count: 1 }] }
        );
        assert_eq!(assign_filtered(&conn, &feb_rides, &road.id).unwrap(), 2);
        let on_road = |id: &str| gear_of_activity(&conn, id).unwrap().as_deref() == Some(road.id.as_str());
        assert!(on_road("feb"));
        assert!(on_road("feb-on-gravel"), "moved: the user filtered it in");
        assert!(!on_road("jan"));
        assert!(!on_road("feb-run"));
        assert!(!on_road("leg"));
        assert!(!on_road("container"));
        assert!(!on_road("native"));
        // Already there: nothing to do, and the preview says so (the count
        // is of what WOULD change, not of what the filter shows).
        assert_eq!(assign_filtered(&conn, &feb_rides, &road.id).unwrap(), 0);
        assert_eq!(filtered_targets(&conn, &feb_rides, &road.id).unwrap(), GearTargets::default());
        // Onto Gravel the same two would move off Road.
        assert_eq!(
            filtered_targets(&conn, &feb_rides, &gravel.id).unwrap(),
            GearTargets { eligible: 2, moved_from: vec![MovedFrom { name: "Road".into(), count: 2 }] }
        );

        // No facets at all: the whole library that can carry gear.
        let all = ActivityFilters::default();
        assert_eq!(
            filtered_targets(&conn, &all, &gravel.id).unwrap(),
            GearTargets { eligible: 4, moved_from: vec![MovedFrom { name: "Road".into(), count: 2 }] }
        );
        // The gear facet itself composes: "no gear" rides only.
        let bare = ActivityFilters { gear_ids: Some(vec!["".into()]), ..Default::default() };
        assert_eq!(filtered_targets(&conn, &bare, &gravel.id).unwrap(), GearTargets { eligible: 2, moved_from: vec![] });
        assert_eq!(assign_filtered(&conn, &bare, &gravel.id).unwrap(), 2);
    }

    #[test]
    fn lists_bikes_then_shoes_then_the_rest_by_name_case_insensitively() {
        let conn = db::test_db();
        let mut helmet = bike("Aero helmet");
        helmet.kind = GearKind::Other;
        helmet.default_for = vec![];
        insert(&conn, &helmet).unwrap();
        let mut shoes = bike("zoom");
        shoes.kind = GearKind::Shoes;
        shoes.default_for = vec![];
        insert(&conn, &shoes).unwrap();
        insert(&conn, &bike("beta")).unwrap();
        insert(&conn, &bike("Alpha")).unwrap();
        let names: Vec<String> = list(&conn).unwrap().into_iter().map(|i| i.gear.name).collect();
        assert_eq!(names, vec!["Alpha", "beta", "zoom", "Aero helmet"]);
    }
}
