CREATE TABLE swim_length (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    activity_id TEXT NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    length_number INTEGER NOT NULL,
    start_time TEXT,
    total_elapsed_time_s REAL,
    total_timer_time_s REAL,
    avg_speed_mps REAL,
    avg_swimming_cadence REAL,
    swim_stroke TEXT,
    total_strokes INTEGER,
    total_calories REAL,
    length_type TEXT
);
CREATE INDEX idx_swim_length_activity ON swim_length(activity_id);
