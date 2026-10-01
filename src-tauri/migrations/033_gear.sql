-- Gear (ADR 0003): the bikes, shoes and other items an activity is done
-- on. Mileage is computed from the activities on read — no stored totals.
CREATE TABLE gear (
    id                 TEXT PRIMARY KEY,
    kind               TEXT NOT NULL CHECK (kind IN ('bike', 'shoes', 'other')),
    name               TEXT NOT NULL,
    brand              TEXT,
    model              TEXT,
    purchased_at       TEXT,
    -- Mileage before Syzify, so the odometer can continue from it.
    initial_distance_m REAL NOT NULL DEFAULT 0,
    -- Wear warning threshold; NULL = none.
    distance_limit_m   REAL,
    retired_at         TEXT,
    notes              TEXT,
    created_at         TEXT NOT NULL DEFAULT (datetime('now'))
);

-- One item per activity. Deleting the item detaches its activities.
ALTER TABLE activity ADD COLUMN gear_id TEXT REFERENCES gear(id) ON DELETE SET NULL;
CREATE INDEX idx_activity_gear ON activity(gear_id);

-- The item a new import of a sport gets. One per sport, so one pair of
-- shoes can be the default for run and hike alike.
CREATE TABLE gear_default (
    sport_type TEXT PRIMARY KEY,
    gear_id    TEXT NOT NULL REFERENCES gear(id) ON DELETE CASCADE
);
