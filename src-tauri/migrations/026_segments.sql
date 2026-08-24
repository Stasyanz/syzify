CREATE TABLE segment (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    sport TEXT NOT NULL,
    source_activity_id TEXT REFERENCES activity(id) ON DELETE SET NULL,
    source_start_idx INTEGER,
    source_end_idx INTEGER,
    distance_m REAL NOT NULL,
    elev_delta_m REAL,
    avg_grade_pct REAL,
    start_lat REAL NOT NULL,
    start_lon REAL NOT NULL,
    end_lat REAL NOT NULL,
    end_lon REAL NOT NULL,
    min_lat REAL NOT NULL,
    max_lat REAL NOT NULL,
    min_lon REAL NOT NULL,
    max_lon REAL NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_segment_lookup ON segment(sport, start_lat, start_lon);

CREATE TABLE segment_point (
    segment_id TEXT NOT NULL REFERENCES segment(id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    lat REAL NOT NULL,
    lon REAL NOT NULL,
    altitude_m REAL,
    distance_m REAL NOT NULL,
    PRIMARY KEY (segment_id, seq)
);
