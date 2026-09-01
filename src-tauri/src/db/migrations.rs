use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

/// The ordered migration set. The count of these is the up-to-date
/// `user_version`; deriving it here (rather than a hardcoded literal) keeps the
/// recovery restamp correct when a migration is added — no second source of
/// truth to forget to bump.
fn migration_list() -> Vec<M<'static>> {
    vec![
        M::up(include_str!("../../migrations/001_initial.sql")),
        M::up(include_str!("../../migrations/002_settings.sql")),
        M::up(include_str!("../../migrations/003_location_name.sql")),
        M::up(include_str!("../../migrations/004_activity_coords.sql")),
        M::up(include_str!("../../migrations/005_calories.sql")),
        M::up(include_str!("../../migrations/006_temperature.sql")),
        M::up(include_str!("../../migrations/007_laps.sql")),
        M::up(include_str!("../../migrations/008_power_metrics.sql")),
        M::up(include_str!("../../migrations/009_training_metrics.sql")),
        M::up(include_str!("../../migrations/010_running_dynamics.sql")),
        M::up(include_str!("../../migrations/011_advanced_session.sql")),
        M::up(include_str!("../../migrations/012_cycling_dynamics.sql")),
        M::up(include_str!("../../migrations/013_swim_lengths.sql")),
        M::up(include_str!("../../migrations/014_exercise_sets.sql")),
        M::up(include_str!("../../migrations/015_time_in_zones.sql")),
        M::up(include_str!("../../migrations/016_hrv_data.sql")),
        M::up(include_str!("../../migrations/017_lap_extended.sql")),
        M::up(include_str!("../../migrations/018_photos.sql")),
        M::up(include_str!("../../migrations/019_plugins.sql")),
        M::up(include_str!("../../migrations/020_plugin_signed.sql")),
        M::up(include_str!("../../migrations/021_best_efforts.sql")),
        M::up(include_str!("../../migrations/022_avg_speed_backfill.sql")),
        M::up(include_str!("../../migrations/023_multisport_legs.sql")),
        M::up(include_str!("../../migrations/024_multisport_merge.sql")),
        M::up(include_str!("../../migrations/025_cycling_dynamics_position.sql")),
        M::up(include_str!("../../migrations/026_segments.sql")),
        M::up(include_str!("../../migrations/027_segment_efforts.sql")),
        M::up(include_str!("../../migrations/028_power_curve.sql")),
        M::up(include_str!("../../migrations/029_segment_effort_power.sql")),
    ]
}

fn migrations_count() -> usize {
    migration_list().len()
}

pub fn run_migrations(conn: &mut Connection) -> Result<(), rusqlite_migration::Error> {
    let migrations = Migrations::new(migration_list());

    // Recovery net: a fully-migrated database can lose its user_version stamp
    // (e.g. an older SQLCipher encrypt that didn't carry it across). Such a DB
    // reads as version 0 and would re-run migration 1 on a schema that already
    // has every column. If the schema is clearly present (the `activity` table
    // exists) but the version is 0, restamp it to the migration count so
    // to_latest treats it as up to date instead of failing on duplicates.
    if needs_version_recovery(conn)? {
        let count = migrations_count();
        conn.pragma_update(None, "user_version", count as i64)
            .map_err(|e| rusqlite_migration::Error::RusqliteError {
                query: "PRAGMA user_version".into(),
                err: e,
            })?;
    }

    migrations.to_latest(conn)
}

fn needs_version_recovery(conn: &Connection) -> Result<bool, rusqlite_migration::Error> {
    let map_err = |e| rusqlite_migration::Error::RusqliteError {
        query: "version recovery probe".into(),
        err: e,
    };
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(map_err)?;
    if version != 0 {
        return Ok(false);
    }
    let has_activity: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='activity'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    Ok(has_activity)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fully-migrated DB that lost its user_version (e.g. old SQLCipher
    /// export) must not re-run migration 1 on an already-complete schema.
    #[test]
    fn recovers_lost_user_version_without_reapplying() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        let full: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(full, migrations_count() as i64);

        // Simulate the lost stamp: schema intact, version reset to 0.
        conn.pragma_update(None, "user_version", 0).unwrap();
        assert!(needs_version_recovery(&conn).unwrap());

        // Re-running must succeed (recovery restamps) rather than erroring on a
        // duplicate column, and end up back at the latest version.
        run_migrations(&mut conn).unwrap();
        let after: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(after, migrations_count() as i64);
    }

    /// A genuinely empty DB (no schema) must NOT be treated as recovery — it
    /// really needs migrations from scratch.
    #[test]
    fn empty_db_is_not_treated_as_recovery() {
        let conn = Connection::open_in_memory().unwrap();
        assert!(!needs_version_recovery(&conn).unwrap());
    }
}
