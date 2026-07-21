CREATE TABLE hrv_sample (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    activity_id TEXT NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    sample_index INTEGER NOT NULL,
    rr_interval_ms REAL NOT NULL
);
CREATE INDEX idx_hrv_sample_activity ON hrv_sample(activity_id);
