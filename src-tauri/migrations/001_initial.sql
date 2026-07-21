CREATE TABLE IF NOT EXISTS vault (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    root_path   TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    app_version TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS activity (
    id               TEXT PRIMARY KEY,
    start_time       TEXT NOT NULL,
    timezone_offset  INTEGER,
    sport_type       TEXT NOT NULL DEFAULT 'other',
    title            TEXT,
    notes            TEXT,
    distance_m       REAL,
    duration_s       REAL,
    elev_gain_m      REAL,
    elev_loss_m      REAL,
    avg_speed_mps    REAL,
    max_speed_mps    REAL,
    avg_hr           REAL,
    max_hr           REAL,
    avg_cadence      REAL,
    source_device    TEXT,
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS trackpoint (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    activity_id TEXT NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    t           TEXT,
    lat         REAL,
    lon         REAL,
    altitude_m  REAL,
    speed_mps   REAL,
    hr          INTEGER,
    cadence     INTEGER,
    power_w     INTEGER
);

CREATE TABLE IF NOT EXISTS raw_file (
    id             TEXT PRIMARY KEY,
    activity_id    TEXT REFERENCES activity(id) ON DELETE SET NULL,
    path_in_vault  TEXT NOT NULL,
    original_path  TEXT,
    format         TEXT NOT NULL,
    hash_sha256    TEXT NOT NULL,
    imported_at    TEXT NOT NULL DEFAULT (datetime('now')),
    parse_status   TEXT NOT NULL DEFAULT 'ok',
    failure_reason TEXT
);

CREATE TABLE IF NOT EXISTS tag (
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS activity_tag (
    activity_id TEXT NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    tag_id      INTEGER NOT NULL REFERENCES tag(id) ON DELETE CASCADE,
    PRIMARY KEY (activity_id, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_activity_start_time ON activity(start_time);
CREATE INDEX IF NOT EXISTS idx_activity_sport_type ON activity(sport_type);
CREATE INDEX IF NOT EXISTS idx_activity_distance   ON activity(distance_m);
CREATE INDEX IF NOT EXISTS idx_raw_file_hash       ON raw_file(hash_sha256);
CREATE INDEX IF NOT EXISTS idx_trackpoint_activity ON trackpoint(activity_id);
CREATE INDEX IF NOT EXISTS idx_activity_tag_tag    ON activity_tag(tag_id);
