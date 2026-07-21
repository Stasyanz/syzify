CREATE TABLE best_effort (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    activity_id TEXT NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    distance_m REAL NOT NULL,
    duration_s REAL NOT NULL
);
CREATE INDEX idx_best_effort_activity ON best_effort(activity_id);
CREATE INDEX idx_best_effort_distance ON best_effort(distance_m);
