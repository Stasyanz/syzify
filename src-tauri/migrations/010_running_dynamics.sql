ALTER TABLE trackpoint ADD COLUMN vertical_oscillation_mm REAL;
ALTER TABLE trackpoint ADD COLUMN stance_time_ms REAL;
ALTER TABLE trackpoint ADD COLUMN stance_time_percent REAL;
ALTER TABLE trackpoint ADD COLUMN step_length_mm REAL;
ALTER TABLE trackpoint ADD COLUMN grade_percent REAL;

ALTER TABLE activity ADD COLUMN avg_vertical_oscillation_mm REAL;
ALTER TABLE activity ADD COLUMN avg_stance_time_ms REAL;
ALTER TABLE activity ADD COLUMN avg_stance_time_percent REAL;
ALTER TABLE activity ADD COLUMN avg_step_length_mm REAL;
ALTER TABLE activity ADD COLUMN total_strides INTEGER;
