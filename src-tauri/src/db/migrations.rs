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
        M::up(include_str!("../../migrations/030_monitoring.sql")),
        M::up(include_str!("../../migrations/031_plugin_secret.sql")),
        M::up(include_str!("../../migrations/032_drop_tags.sql")),
        M::up(include_str!("../../migrations/033_gear.sql")),
        M::up(include_str!("../../migrations/034_gear_rules.sql")),
        M::up(include_str!("../../migrations/035_gps_dropped.sql")),
    ]
}

fn migrations_count() -> usize {
    migration_list().len()
}

/// Why a vault's schema was not brought up to date.
#[derive(Debug)]
pub enum MigrateError {
    /// A newer build migrated the vault past this build's last migration:
    /// `vault` is its schema version, `app` the latest this build knows.
    /// Nothing this build does can open it — the app needs updating.
    TooNew { vault: i64, app: i64 },
    /// The migration itself failed: the library's own error.
    Failed(rusqlite_migration::Error),
}

impl From<rusqlite_migration::Error> for MigrateError {
    fn from(e: rusqlite_migration::Error) -> Self {
        MigrateError::Failed(e)
    }
}

pub fn run_migrations(conn: &mut Connection) -> Result<(), MigrateError> {
    let migrations = Migrations::new(migration_list());

    // A vault past the last migration: told apart before migrating, with
    // both versions, so the app says "update" instead of the library's
    // DatabaseTooFarAhead.
    let latest = migrations_count() as i64;
    let version = user_version(conn)?;
    if version > latest {
        return Err(MigrateError::TooNew { vault: version, app: latest });
    }

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

    Ok(migrations.to_latest(conn)?)
}

/// The vault's schema version (`PRAGMA user_version`).
fn user_version(conn: &Connection) -> Result<i64, rusqlite_migration::Error> {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| rusqlite_migration::Error::RusqliteError { query: "PRAGMA user_version".into(), err: e })
}

fn needs_version_recovery(conn: &Connection) -> Result<bool, rusqlite_migration::Error> {
    let version = user_version(conn)?;
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

    /// 032 drops the tag tables from a vault that still holds tags (#157),
    /// with foreign keys enforced; the activities they referenced survive
    /// and stay deletable afterwards.
    #[test]
    fn drop_tags_migration_clears_a_populated_vault() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        // Up to and including 031: the schema the drop runs against.
        let list = migration_list();
        let before_drop = 31;
        Migrations::new(list[..before_drop].to_vec()).to_latest(&mut conn).unwrap();
        conn.execute_batch(
            "INSERT INTO activity (id, start_time) VALUES ('a1', '2026-01-01T10:00:00+00:00');
             INSERT INTO tag (id, name) VALUES (1, 'hills');
             INSERT INTO activity_tag (activity_id, tag_id) VALUES ('a1', 1);",
        )
        .unwrap();

        run_migrations(&mut conn).unwrap();

        let leftovers: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name IN ('tag', 'activity_tag', 'idx_activity_tag_tag')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(leftovers, 0);
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, migrations_count() as i64);
        let activities: i64 = conn.query_row("SELECT count(*) FROM activity", [], |r| r.get(0)).unwrap();
        assert_eq!(activities, 1);
        conn.execute("DELETE FROM activity WHERE id = 'a1'", []).unwrap();
    }

    /// A genuinely empty DB (no schema) must NOT be treated as recovery — it
    /// really needs migrations from scratch.
    #[test]
    fn empty_db_is_not_treated_as_recovery() {
        let conn = Connection::open_in_memory().unwrap();
        assert!(!needs_version_recovery(&conn).unwrap());
    }

    /// One past the last migration is a vault a newer build migrated: refused
    /// before anything runs, with its version and this build's, the schema
    /// left as it was.
    #[test]
    fn a_vault_past_the_last_migration_is_too_new() {
        let mut conn = Connection::open_in_memory().unwrap();
        let latest = migrations_count() as i64;
        conn.pragma_update(None, "user_version", latest + 1).unwrap();
        let Err(MigrateError::TooNew { vault, app }) = run_migrations(&mut conn) else { panic!("not too new") };
        assert_eq!((vault, app), (latest + 1, latest));
        let tables: i64 = conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0)).unwrap();
        assert_eq!(tables, 0);
        assert_eq!(user_version(&conn).unwrap(), latest + 1);
    }

    /// Exactly at the last migration is up to date: nothing runs, nothing
    /// is refused, the version stays.
    #[test]
    fn a_vault_at_the_last_migration_is_up_to_date() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        let latest = migrations_count() as i64;
        assert_eq!(user_version(&conn).unwrap(), latest);
        run_migrations(&mut conn).unwrap();
        assert_eq!(user_version(&conn).unwrap(), latest);
    }

    /// What the library says is passed on: a file that is no database, a
    /// schema a migration trips over.
    #[test]
    fn a_failed_migration_carries_the_library_error() {
        let dir = crate::test_support::ScratchDir::new("migrations_garbage");
        let path = dir.join("vault.db");
        std::fs::write(&path, vec![b'x'; 4096]).unwrap();
        let mut garbage = Connection::open(&path).unwrap();
        let Err(MigrateError::Failed(e)) = run_migrations(&mut garbage) else { panic!("a garbage file migrated") };
        assert!(e.to_string().contains("PRAGMA user_version"), "{e}");

        let mut clash = Connection::open_in_memory().unwrap();
        clash.execute_batch("CREATE TABLE trackpoint (x INTEGER);").unwrap();
        let Err(MigrateError::Failed(e)) = run_migrations(&mut clash) else { panic!("a clashing schema migrated") };
        assert!(e.to_string().contains("activity_id"), "{e}");
    }
}
