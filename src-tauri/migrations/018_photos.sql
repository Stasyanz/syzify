CREATE TABLE IF NOT EXISTS photo (
    id              TEXT PRIMARY KEY,
    activity_id     TEXT NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    path_in_vault   TEXT NOT NULL,
    thumbnail_path  TEXT,
    original_path   TEXT,
    mime_type       TEXT NOT NULL,
    width           INTEGER,
    height          INTEGER,
    size_bytes      INTEGER NOT NULL,
    hash_sha256     TEXT NOT NULL,
    taken_at        TEXT,
    caption         TEXT,
    sort_order      INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_photo_activity ON photo(activity_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_photo_activity_hash ON photo(activity_id, hash_sha256);
