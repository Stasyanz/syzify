-- Backfill average speed for activities whose source didn't record it.
-- Pace/speed in the UI is derived from avg_speed_mps; many imports (e.g.
-- Runkeeper) leave it NULL even though distance and duration are present.
UPDATE activity
SET avg_speed_mps = distance_m / duration_s
WHERE avg_speed_mps IS NULL
  AND distance_m > 0
  AND duration_s > 0;
