-- Merge separate same-day workouts into one triathlon container.
-- A leg is an ordinary activity that points at its container via parent_id;
-- containers themselves have parent_id NULL. Every listing/aggregate filters
-- parent_id IS NULL so legs don't double-count — they're reached only through
-- the container's Legs card.
ALTER TABLE activity ADD COLUMN parent_id TEXT REFERENCES activity(id) ON DELETE SET NULL;
CREATE INDEX idx_activity_parent ON activity(parent_id);

-- A leg row may link back to the standalone activity it was built from
-- (merge case); NULL for FIT-multisport legs that have no separate activity.
ALTER TABLE multisport_leg ADD COLUMN source_activity_id TEXT;
