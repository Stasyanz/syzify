CREATE TABLE exercise_set (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    activity_id TEXT NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    set_number INTEGER NOT NULL,
    start_time TEXT,
    category TEXT,
    category_subtype TEXT,
    set_type TEXT,
    duration_s REAL,
    repetitions INTEGER,
    weight_kg REAL,
    wkt_step_index INTEGER
);
CREATE INDEX idx_exercise_set_activity ON exercise_set(activity_id);
