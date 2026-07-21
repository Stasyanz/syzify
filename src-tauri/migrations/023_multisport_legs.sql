CREATE TABLE multisport_leg (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    activity_id TEXT NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    leg_number INTEGER NOT NULL,
    sport_type TEXT NOT NULL,
    is_transition INTEGER NOT NULL DEFAULT 0,
    start_time TEXT,
    total_distance_m REAL,
    total_timer_time_s REAL,
    total_elapsed_time_s REAL,
    avg_speed_mps REAL,
    avg_hr REAL,
    max_hr REAL,
    total_ascent_m REAL,
    total_calories REAL
);
CREATE INDEX idx_multisport_leg_activity ON multisport_leg(activity_id);
