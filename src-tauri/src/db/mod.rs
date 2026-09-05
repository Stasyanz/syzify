pub mod activities;
pub mod best_efforts;
pub mod dashboard;
pub mod dbcrypt;
pub mod exercise_sets;
pub mod hrv_samples;
pub mod laps;
pub mod migrations;
pub mod monitoring;
pub mod multisport_legs;
pub mod photos;
pub mod plugins;
pub mod power_curve;
pub mod raw_files;
pub mod segment_efforts;
pub mod segments;
pub mod settings;
pub mod swim_lengths;
pub mod tags;
pub mod time_in_zones;
pub mod training_load;
pub mod trackpoints;
pub mod watch_folders;

#[cfg(test)]
pub fn test_db() -> rusqlite::Connection {
    let mut conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    migrations::run_migrations(&mut conn).expect("migrations");
    conn
}
