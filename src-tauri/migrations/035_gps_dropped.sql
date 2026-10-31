-- What the GPS cleaning (#256) took off a track, so a later version of the
-- filter can put it back and judge again: the position and the speed it
-- dropped from each trackpoint (NULL = that value was not touched), and the
-- activity's distance and speeds before the cleaned track replaced them.
CREATE TABLE gps_dropped (
    trackpoint_id INTEGER PRIMARY KEY REFERENCES trackpoint(id) ON DELETE CASCADE,
    activity_id   TEXT NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    lat           REAL,
    lon           REAL,
    speed_mps     REAL
);
CREATE INDEX idx_gps_dropped_activity ON gps_dropped(activity_id);

CREATE TABLE gps_replaced_figures (
    activity_id   TEXT PRIMARY KEY REFERENCES activity(id) ON DELETE CASCADE,
    distance_m    REAL,
    avg_speed_mps REAL,
    max_speed_mps REAL
);
