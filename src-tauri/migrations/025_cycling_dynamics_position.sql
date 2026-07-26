-- Cycling Dynamics (Garmin dual-sided pedals): seated/standing split,
-- platform center offset, power phase angles. Session-level only —
-- per-trackpoint dynamics stay unparsed until something charts them.
ALTER TABLE activity ADD COLUMN avg_left_pco_mm REAL;
ALTER TABLE activity ADD COLUMN avg_right_pco_mm REAL;
ALTER TABLE activity ADD COLUMN avg_left_power_phase_start_deg REAL;
ALTER TABLE activity ADD COLUMN avg_left_power_phase_end_deg REAL;
ALTER TABLE activity ADD COLUMN avg_left_power_phase_peak_start_deg REAL;
ALTER TABLE activity ADD COLUMN avg_left_power_phase_peak_end_deg REAL;
ALTER TABLE activity ADD COLUMN avg_right_power_phase_start_deg REAL;
ALTER TABLE activity ADD COLUMN avg_right_power_phase_end_deg REAL;
ALTER TABLE activity ADD COLUMN avg_right_power_phase_peak_start_deg REAL;
ALTER TABLE activity ADD COLUMN avg_right_power_phase_peak_end_deg REAL;
ALTER TABLE activity ADD COLUMN avg_power_seated_w REAL;
ALTER TABLE activity ADD COLUMN avg_power_standing_w REAL;
ALTER TABLE activity ADD COLUMN max_power_seated_w REAL;
ALTER TABLE activity ADD COLUMN max_power_standing_w REAL;
ALTER TABLE activity ADD COLUMN avg_cadence_seated REAL;
ALTER TABLE activity ADD COLUMN avg_cadence_standing REAL;
ALTER TABLE activity ADD COLUMN max_cadence_seated REAL;
ALTER TABLE activity ADD COLUMN max_cadence_standing REAL;
ALTER TABLE activity ADD COLUMN time_standing_s REAL;
ALTER TABLE activity ADD COLUMN stand_count INTEGER;