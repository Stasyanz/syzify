CREATE TABLE segment_effort (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    segment_id TEXT NOT NULL REFERENCES segment(id) ON DELETE CASCADE,
    activity_id TEXT NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    start_idx INTEGER NOT NULL,
    end_idx INTEGER NOT NULL,
    start_time_epoch_s REAL,
    elapsed_s REAL,
    distance_m REAL NOT NULL,
    UNIQUE(segment_id, activity_id, start_idx)
);
CREATE INDEX idx_segment_effort_activity ON segment_effort(activity_id);
CREATE INDEX idx_segment_effort_rank ON segment_effort(segment_id, elapsed_s);
