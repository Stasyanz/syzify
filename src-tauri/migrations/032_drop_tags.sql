-- Activity tags are gone from the app (#157): nothing reads or writes these
-- tables any more. Dropped in dependency order (activity_tag references tag);
-- the index on activity_tag goes with its table.
DROP TABLE activity_tag;
DROP TABLE tag;
