CREATE TABLE power_curve (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    activity_id TEXT NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    window_s INTEGER NOT NULL,
    watts REAL NOT NULL,
    UNIQUE(activity_id, window_s)
);
CREATE INDEX idx_power_curve_window ON power_curve(window_s, watts, activity_id);
