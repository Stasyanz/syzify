CREATE TABLE time_in_zone (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    activity_id TEXT NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    zone_type TEXT NOT NULL,
    zone_index INTEGER NOT NULL,
    time_s REAL NOT NULL,
    zone_high_boundary REAL
);
CREATE INDEX idx_time_in_zone_activity ON time_in_zone(activity_id);
