-- Gear rules (ADR 0003, stage 4): what a FIT file carries that names the
-- bike — the activity profile it was recorded under and the paired
-- sensors — kept per activity so a rule can match it at import and over
-- the history.
ALTER TABLE activity ADD COLUMN profile_name TEXT;

CREATE TABLE activity_sensor (
    activity_id  TEXT NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    serial       TEXT NOT NULL,
    device_type  TEXT,
    manufacturer TEXT,
    product      TEXT,
    PRIMARY KEY (activity_id, serial)
);
CREATE INDEX idx_activity_sensor_serial ON activity_sensor(serial);

-- "Put the activity on this item when its profile is ROAD" / "… when sensor
-- 3632674300 is paired". One value belongs to one item, whatever its case:
-- the match is case-insensitive too ("Road" and "ROAD" are one profile;
-- NOCASE folds ASCII only, as SQLite does).
CREATE TABLE gear_rule (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    gear_id TEXT NOT NULL REFERENCES gear(id) ON DELETE CASCADE,
    kind    TEXT NOT NULL CHECK (kind IN ('profile_name', 'sensor_serial')),
    value   TEXT NOT NULL COLLATE NOCASE,
    UNIQUE (kind, value)
);
