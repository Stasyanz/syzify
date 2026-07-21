ALTER TABLE trackpoint ADD COLUMN left_right_balance REAL;
ALTER TABLE trackpoint ADD COLUMN left_torque_effectiveness REAL;
ALTER TABLE trackpoint ADD COLUMN right_torque_effectiveness REAL;
ALTER TABLE trackpoint ADD COLUMN left_pedal_smoothness REAL;
ALTER TABLE trackpoint ADD COLUMN right_pedal_smoothness REAL;

ALTER TABLE activity ADD COLUMN avg_left_torque_effectiveness REAL;
ALTER TABLE activity ADD COLUMN avg_right_torque_effectiveness REAL;
ALTER TABLE activity ADD COLUMN avg_left_pedal_smoothness REAL;
ALTER TABLE activity ADD COLUMN avg_right_pedal_smoothness REAL;
ALTER TABLE activity ADD COLUMN avg_left_right_balance REAL;
