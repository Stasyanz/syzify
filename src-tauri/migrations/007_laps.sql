CREATE TABLE lap (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    activity_id TEXT NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    lap_number INTEGER NOT NULL,
    start_time TEXT,
    total_elapsed_time_s REAL,
    total_timer_time_s REAL,
    total_distance_m REAL,
    avg_speed_mps REAL,
    max_speed_mps REAL,
    avg_hr REAL,
    max_hr REAL,
    avg_cadence REAL,
    max_cadence REAL,
    total_ascent_m REAL,
    total_descent_m REAL,
    total_calories REAL
);
CREATE INDEX idx_lap_activity ON lap(activity_id);
