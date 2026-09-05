-- Garmin monitoring (ADR 0002): raw samples kept so the per-day aggregates
-- can be recomputed when the formula changes, plus one row per local day.

-- Monitor files share raw_file with activities (hash dedup, encryption,
-- backups); activity_id stays NULL for them and `kind` tells them apart.
ALTER TABLE raw_file ADD COLUMN kind TEXT NOT NULL DEFAULT 'activity'
    CHECK (kind IN ('activity', 'monitoring'));

-- Heart rate / stress / respiration / SpO2 readings, unix seconds (UTC).
-- Keyed per device so a second watch cannot overwrite the first; the
-- daily file and the sync-time file of one day overlap and the later
-- import wins (INSERT OR REPLACE).
CREATE TABLE monitoring_sample (
    device_serial TEXT    NOT NULL,
    kind          TEXT    NOT NULL CHECK (kind IN ('hr', 'stress', 'respiration', 'spo2')),
    ts            INTEGER NOT NULL,
    value         REAL    NOT NULL,
    confidence    INTEGER,
    raw_file_id   TEXT    REFERENCES raw_file(id) ON DELETE SET NULL,
    PRIMARY KEY (device_serial, kind, ts)
);
CREATE INDEX idx_monitoring_sample_kind_ts ON monitoring_sample(kind, ts);

-- Running day-so-far totals per activity type (steps, distance, active
-- time, calories). A day's value is the MAX per (day, type), never a sum,
-- and a total stamped at local midnight closes the PREVIOUS day.
CREATE TABLE monitoring_total (
    device_serial   TEXT    NOT NULL,
    activity_type   TEXT    NOT NULL,
    ts              INTEGER NOT NULL,
    steps           REAL,
    distance_m      REAL,
    active_calories REAL,
    active_time_s   REAL,
    raw_file_id     TEXT    REFERENCES raw_file(id) ON DELETE SET NULL,
    PRIMARY KEY (device_serial, activity_type, ts)
);
CREATE INDEX idx_monitoring_total_ts ON monitoring_total(ts);

-- Moderate / vigorous minutes: running totals (max per day) and, where the
-- device wrote them, one-minute increments (summed only without totals).
CREATE TABLE monitoring_active_minutes (
    device_serial  TEXT    NOT NULL,
    ts             INTEGER NOT NULL,
    moderate_total REAL,
    vigorous_total REAL,
    moderate_inc   REAL,
    vigorous_inc   REAL,
    raw_file_id    TEXT    REFERENCES raw_file(id) ON DELETE SET NULL,
    PRIMARY KEY (device_serial, ts)
);
CREATE INDEX idx_monitoring_active_minutes_ts ON monitoring_active_minutes(ts);

-- The time span each Monitor file covered. Deleting a date range must
-- drop the FILES of that range too (their hashes block a re-import), and
-- the per-row raw_file_id cannot say which: a later overlapping file
-- overwrites it. Cascades away with the raw_file row.
CREATE TABLE monitoring_raw_file (
    raw_file_id TEXT    PRIMARY KEY REFERENCES raw_file(id) ON DELETE CASCADE,
    first_ts    INTEGER NOT NULL,
    last_ts     INTEGER NOT NULL
);

-- Garmin's own resting-heart-rate estimates — reference only, never an
-- input to the recovery index (they swing 47…118 with wear).
CREATE TABLE monitoring_rhr (
    device_serial TEXT    NOT NULL,
    ts            INTEGER NOT NULL,
    current_day   INTEGER,
    seven_day     INTEGER,
    PRIMARY KEY (device_serial, ts)
);

-- One row per LOCAL calendar day. The tz columns are written at import
-- (the offset the file was read under, and whether the file's midnight
-- cut confirmed it); the aggregates are (re)computed from the tables
-- above — computed_at is NULL until they are.
CREATE TABLE monitoring_day (
    date             TEXT    PRIMARY KEY,
    tz_offset_s      INTEGER NOT NULL,
    tz_confirmed     INTEGER NOT NULL DEFAULT 0,
    night_samples    INTEGER NOT NULL DEFAULT 0,
    night_hr_min     REAL,
    night_hr_p10     REAL,
    night_hr_median  REAL,
    night_stress_avg REAL,
    day_stress_avg   REAL,
    resp_night_avg   REAL,
    spo2_night_avg   REAL,
    rhr_garmin       INTEGER,
    rhr_garmin_7d    INTEGER,
    steps            REAL,
    distance_m       REAL,
    active_calories  REAL,
    active_time_s    REAL,
    moderate_min     REAL,
    vigorous_min     REAL,
    computed_at      TEXT
);
