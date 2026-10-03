export interface Activity {
  id: string;
  /** The location point (typed, picked, or a suggestion), if any. */
  start_lat?: number | null;
  start_lon?: number | null;
  start_time: string;
  timezone_offset: number | null;
  sport_type: string;
  title: string | null;
  notes: string | null;
  distance_m: number | null;
  duration_s: number | null;
  elev_gain_m: number | null;
  elev_loss_m: number | null;
  avg_speed_mps: number | null;
  max_speed_mps: number | null;
  avg_hr: number | null;
  max_hr: number | null;
  avg_cadence: number | null;
  calories: number | null;
  avg_temperature_c: number | null;
  max_temperature_c: number | null;
  source_device: string | null;
  location_name: string | null;
  avg_power_w: number | null;
  max_power_w: number | null;
  normalized_power_w: number | null;
  total_work_kj: number | null;
  threshold_power_w: number | null;
  training_stress_score: number | null;
  intensity_factor: number | null;
  training_effect_aerobic: number | null;
  training_effect_anaerobic: number | null;
  training_load_peak: number | null;
  avg_vertical_oscillation_mm: number | null;
  avg_stance_time_ms: number | null;
  avg_stance_time_percent: number | null;
  avg_step_length_mm: number | null;
  total_strides: number | null;
  min_hr: number | null;
  moving_time_s: number | null;
  sub_sport: string | null;
  avg_respiration_rate: number | null;
  max_respiration_rate: number | null;
  hrv_rmssd: number | null;
  hrv_sdrr: number | null;
  end_lat: number | null;
  end_lon: number | null;
  avg_left_torque_effectiveness: number | null;
  avg_right_torque_effectiveness: number | null;
  avg_left_pedal_smoothness: number | null;
  avg_right_pedal_smoothness: number | null;
  avg_left_right_balance: number | null; // % of power from the right pedal
  // Cycling Dynamics (dual-sided pedals). Angles in degrees, 0° = top dead
  // center, clockwise.
  avg_left_pco_mm: number | null;
  avg_right_pco_mm: number | null;
  avg_left_power_phase_start_deg: number | null;
  avg_left_power_phase_end_deg: number | null;
  avg_left_power_phase_peak_start_deg: number | null;
  avg_left_power_phase_peak_end_deg: number | null;
  avg_right_power_phase_start_deg: number | null;
  avg_right_power_phase_end_deg: number | null;
  avg_right_power_phase_peak_start_deg: number | null;
  avg_right_power_phase_peak_end_deg: number | null;
  avg_power_seated_w: number | null;
  avg_power_standing_w: number | null;
  max_power_seated_w: number | null;
  max_power_standing_w: number | null;
  avg_cadence_seated: number | null;
  avg_cadence_standing: number | null;
  max_cadence_seated: number | null;
  max_cadence_standing: number | null;
  time_standing_s: number | null;
  stand_count: number | null;
  created_at: string;
  updated_at: string;
  /** The multisport container this activity is a merged leg of; null for
   * standalone activities and containers themselves. */
  parent_id: string | null;
}

export interface ActivitySummary {
  id: string;
  start_time: string;
  sport_type: string;
  title: string | null;
  distance_m: number | null;
  duration_s: number | null;
  elev_gain_m: number | null;
  avg_speed_mps: number | null;
  avg_hr: number | null;
  location_name: string | null;
  source_device: string | null;
  /** The gear item the activity is on (ADR 0003), if assigned. */
  gear_id: string | null;
}

export interface MultisportLeg {
  id: number | null;
  activity_id: string;
  leg_number: number;
  /** Normalized sport ("swim", "ride", "run"); "transition" for T1/T2. */
  sport_type: string;
  is_transition: boolean;
  start_time: string | null;
  total_distance_m: number | null;
  total_timer_time_s: number | null;
  total_elapsed_time_s: number | null;
  avg_speed_mps: number | null;
  avg_hr: number | null;
  max_hr: number | null;
  total_ascent_m: number | null;
  total_calories: number | null;
  /** The standalone activity this leg links to (merged case); null for
   * FIT-multisport legs and transitions. */
  source_activity_id: string | null;
}

export interface ActivityDetail {
  activity: Activity;
  trackpoints: TrackPointColumns;
  /** The gear item the activity is on (ADR 0003); null when unassigned. */
  gear_id: string | null;
  /** A multisport whole (merged container or FIT-native file): it carries
   * no gear of its own; the backend applies the same rule. */
  is_multisport: boolean;
  laps: Lap[];
  legs: MultisportLeg[];
  lengths: SwimLength[];
  sets: ExerciseSet[];
  time_in_zones: TimeInZone[];
  hrv_samples: HrvSample[];
  /** The newest earlier activities with an FTP, newest first, for the mismatch hint. */
  recent_power: PreviousPower[];
}

export interface PreviousPower {
  activity_id: string;
  start_time: string;
  threshold_power_w: number;
  source_device: string | null;
}

export interface HrvSample {
  id: number | null;
  activity_id: string;
  sample_index: number;
  rr_interval_ms: number;
}

export interface TimeInZone {
  id: number | null;
  activity_id: string;
  zone_type: string;
  zone_index: number;
  time_s: number;
  zone_high_boundary: number | null;
}

export interface ExerciseSet {
  id: number | null;
  activity_id: string;
  set_number: number;
  start_time: string | null;
  category: string | null;
  category_subtype: string | null;
  set_type: string | null;
  duration_s: number | null;
  repetitions: number | null;
  weight_kg: number | null;
  wkt_step_index: number | null;
}

export interface SwimLength {
  id: number | null;
  activity_id: string;
  length_number: number;
  start_time: string | null;
  total_elapsed_time_s: number | null;
  total_timer_time_s: number | null;
  avg_speed_mps: number | null;
  avg_swimming_cadence: number | null;
  swim_stroke: string | null;
  total_strokes: number | null;
  total_calories: number | null;
  length_type: string | null;
}

export interface Lap {
  id: number | null;
  activity_id: string;
  lap_number: number;
  start_time: string | null;
  total_elapsed_time_s: number | null;
  total_timer_time_s: number | null;
  total_distance_m: number | null;
  avg_speed_mps: number | null;
  max_speed_mps: number | null;
  avg_hr: number | null;
  max_hr: number | null;
  avg_cadence: number | null;
  max_cadence: number | null;
  total_ascent_m: number | null;
  total_descent_m: number | null;
  total_calories: number | null;
  avg_power_w: number | null;
  max_power_w: number | null;
  normalized_power_w: number | null;
  avg_vertical_oscillation_mm: number | null;
  avg_stance_time_ms: number | null;
  avg_step_length_mm: number | null;
}

export interface TrackPointColumns {
  t: (number | null)[];
  lat: (number | null)[];
  lon: (number | null)[];
  altitude_m: (number | null)[];
  speed_mps: (number | null)[];
  hr: (number | null)[];
  cadence: (number | null)[];
  power_w: (number | null)[];
  temperature_c: (number | null)[];
  vertical_oscillation_mm: (number | null)[];
  stance_time_ms: (number | null)[];
  stance_time_percent: (number | null)[];
  step_length_mm: (number | null)[];
  grade_percent: (number | null)[];
  distance_m: (number | null)[];
  left_right_balance: (number | null)[]; // % of power from the right pedal
  left_torque_effectiveness: (number | null)[];
  right_torque_effectiveness: (number | null)[];
  left_pedal_smoothness: (number | null)[];
  right_pedal_smoothness: (number | null)[];
}

/** A record this activity holds within its sport (header trophy chip). */
export interface RecordBadge {
  kind: "distance" | "elevation" | "duration" | "pace";
  all_time: boolean;
}

export interface ActivityFilters {
  /** Free-text search over title / notes / location name. */
  search?: string;
  /** Match ANY of these sports; unset/empty = all sports. */
  sport_types?: string[];
  date_from?: string;
  date_to?: string;
  distance_min?: number;
  distance_max?: number;
  duration_min?: number;
  duration_max?: number;
  elev_gain_min?: number;
  elev_gain_max?: number;
  /** Match ANY of these recording devices, as stored in `source_device`
   * (raw strings); "" means "no device". Unset/empty = all. */
  devices?: string[];
  /** Match ANY of these gear items by id; "" means "no gear". Unset/empty = all. */
  gear_ids?: string[];
  /** true = only with a GPS track, false = only without, unset = both. */
  has_gps?: boolean;
  sort_by?: string;
  sort_dir?: string;
  limit?: number;
  offset?: number;
}

export interface ActivityUpdate {
  title?: string;
  notes?: string;
  sport_type?: string;
  location_name?: string;
  start_lat?: number;
  start_lon?: number;
}

export interface LocationUpdateResult {
  geocoded: boolean;
  /** Not geocoded because geocoding is off in Settings: a choice, not a failure. */
  geocoding_off: boolean;
  location_name: string;
}

/** What correcting an activity's FTP wrote back. */
export interface FtpUpdateResult {
  threshold_power_w: number;
  intensity_factor: number;
  training_stress_score: number;
}

/** One suggestion under the Location field: the short name that gets
 * stored, a context line to tell namesakes apart, and the coordinates a
 * pick writes without a second geocoding round trip. */
export interface LocationHit {
  name: string;
  detail: string;
  lat: number;
  lon: number;
  kind: string;
}

export interface ImportResult {
  imported: number;
  skipped: number;
  failed: FailedFile[];
  /** Garmin Monitor files stored (ADR 0002) — separate from activities. */
  monitoring_files: number;
  /** Local days whose monitoring aggregates were (re)computed. */
  monitoring_days: number;
  /** Whether the batch carried a night's worth of heart rate — a file
   * closed at local midnight holds only the evening before. */
  monitoring_night: boolean;
  /** First and last of those days, "YYYY-MM-DD". */
  monitoring_range: [string, string] | null;
}

export interface ImportDatasource {
  id: string;
  name: string;
  description: string;
  extensions: string[];
}

export interface FailedFile {
  path: string;
  reason: string;
}

/** Title length cap (in-place rename + edit modal) — keeps the detail
 * header and list rows readable; Strava caps similarly. */
export const MAX_TITLE_LENGTH = 100;

/** Segment name cap — mirrors MAX_NAME_LEN in db/segments.rs (the backend
 * backstop; this one just keeps the input honest). */
export const MAX_SEGMENT_NAME_LENGTH = 200;

export interface DaySummary {
  date: string; // "YYYY-MM-DD"
  activity_count: number;
  total_distance_m: number;
  total_duration_s: number;
  total_elev_gain_m: number;
  sport_types: string[];
  activities: CalDayActivity[];
}

export interface CalDayActivity {
  id: string;
  sport_type: string;
  title: string | null;
  distance_m: number | null;
  duration_s: number | null;
}

// ── Gear (ADR 0003) ──

export type GearKind = "bike" | "shoes" | "other";

export interface Gear {
  id: string;
  kind: GearKind;
  name: string;
  brand: string | null;
  model: string | null;
  /** "YYYY-MM-DD". */
  purchased_at: string | null;
  /** Mileage before Syzify, so the odometer continues from it. */
  initial_distance_m: number;
  /** Wear warning threshold; null = none. */
  distance_limit_m: number | null;
  retired_at: string | null;
  notes: string | null;
  created_at: string;
}

/** What the Garage edits: the item's own fields plus the sports it is the
 * default for. The same shape creates and updates. */
export interface GearInput {
  kind: GearKind;
  name: string;
  brand: string | null;
  model: string | null;
  purchased_at: string | null;
  initial_distance_m: number;
  distance_limit_m: number | null;
  notes: string | null;
  default_for: SportType[];
  /** What puts an activity on this item at import, before the sport
   * default. Replaces the item's rules; a value named here moves over
   * from whichever item held it. */
  rules: GearRule[];
}

/** What the activities on an item add up to; computed on read. */
export interface GearStats {
  activities: number;
  distance_m: number;
  duration_s: number;
  elev_gain_m: number;
  last_used: string | null;
}

/** What a bulk assignment from the library would do: how many activities
 * change, and which other items they leave, by name. */
export interface GearTargets {
  eligible: number;
  moved_from: { name: string; count: number }[];
}

/** What a rule matches on: the activity profile the device recorded under
 * (FIT `sport.name`), or a paired sensor's serial number. */
export type GearRuleKind = "profile_name" | "sensor_serial";

/** "Put the activity on this item when …". A value belongs to one item. */
export interface GearRule {
  kind: GearRuleKind;
  value: string;
}

/** What the vault has seen that a rule could match, most frequent first. */
export interface RuleCandidates {
  profiles: { value: string; count: number }[];
  sensors: {
    serial: string;
    device_type: string | null;
    manufacturer: string | null;
    product: string | null;
    count: number;
  }[];
}

/** A Garage card: the item's fields, its totals, its default sports and
 * its rules. */
export interface GearItem extends Gear {
  stats: GearStats;
  default_for: SportType[];
  rules: GearRule[];
}

/** Name cap in the Garage modal (the backend refuses longer). */
export const MAX_GEAR_NAME_LENGTH = 60;

export interface WatchFolder {
  id: number;
  path: string;
}

export interface ScanResult {
  new_files: string[];
  import_result: ImportResult | null;
}

export interface CacheInfo {
  size_bytes: number;
  size_display: string;
}

// Device Detection
export interface DeviceStats {
  device_name: string;
  activity_count: number;
  last_activity: string;
}

export interface FilePreviewItem {
  path: string;
  filename: string;
  is_new: boolean;
}

export interface FolderPreview {
  folder: string;
  files: FilePreviewItem[];
}

export interface ScanPreview {
  folders: FolderPreview[];
  total_files: number;
  new_files: number;
}

// Encryption
export interface EncryptionScopes {
  activities: boolean;
  database: boolean;
  photos: boolean;
}

export interface EncryptionStatus {
  enabled: boolean;
  locked: boolean;
  scopes: EncryptionScopes;
}

// Mirrors the Rust SportType enum (models/activity.rs). Normalized activity
// types aligned with Garmin watch activity profiles.
export type SportType =
  | "run"
  | "trail_run"
  | "treadmill"
  | "virtual_run"
  | "ride"
  | "mountain_bike"
  | "indoor_ride"
  | "virtual_ride"
  | "walk"
  | "hike"
  | "mountaineering"
  | "swim"
  | "open_water"
  | "sailing"
  | "paddle"
  | "fishing"
  | "triathlon"
  | "strength"
  | "cardio"
  | "yoga"
  | "ski"
  | "ski_xc"
  | "snowboard"
  | "golf"
  | "tennis"
  | "soccer"
  | "basketball"
  | "other";

export const SPORT_LABELS: Record<SportType, string> = {
  run: "Run",
  trail_run: "Trail Run",
  treadmill: "Treadmill",
  virtual_run: "Virtual Run",
  ride: "Ride",
  mountain_bike: "Mountain Bike",
  indoor_ride: "Indoor Ride",
  virtual_ride: "Virtual Ride",
  walk: "Walk",
  hike: "Hike",
  mountaineering: "Mountaineering",
  swim: "Swim",
  open_water: "Open Water",
  sailing: "Sailing",
  paddle: "Paddling",
  fishing: "Fishing",
  triathlon: "Triathlon",
  strength: "Strength",
  cardio: "Cardio",
  yoga: "Yoga",
  ski: "Ski",
  ski_xc: "XC Ski",
  snowboard: "Snowboard",
  golf: "Golf",
  tennis: "Racquet",
  soccer: "Soccer",
  basketball: "Basketball",
  other: "Other",
};

/** All sport types in display order (for filters, pickers). */
export const SPORT_TYPES: SportType[] = Object.keys(SPORT_LABELS) as SportType[];

/** A ride that never left the room (#189): a trainer with no course, or a
 * smart trainer on a simulator. Cycling for power, gear and zones; not a
 * leg of any event. A virtual ride still carries the simulator's
 * coordinates and climbs (#190), an indoor ride carries neither. */
export function isTrainerRide(sport: string): boolean {
  return sport === "indoor_ride" || sport === "virtual_ride";
}

/** A sport whose coordinates come from a simulator, not the ground (#190,
 * #192): a virtual ride or run carries the lat/lon of its virtual world.
 * No map, no destination point, no segment from it. Mirrors
 * `has_simulated_course` in src-tauri/src/models/activity.rs. */
export function hasSimulatedCourse(sport: string): boolean {
  return sport === "virtual_ride" || sport === "virtual_run";
}

/** Sports whose recorded "elevation gain" is instrument noise — the water
 * sports, and a trainer with no course, where a watch's barometer drifts
 * in a room all the same (#189). Mirrors `elevation_is_noise` in
 * src-tauri/src/db/dashboard.rs. */
export function elevationIsNoise(sport: string): boolean {
  return isWaterSport(sport) || sport === "indoor_ride";
}

/** Water sports: recorded "elevation gain" is GPS/pressure noise from the
 * watch losing fix in the water — hidden from summaries and records
 * (mirrors `is_water` in src-tauri/src/db/dashboard.rs). */
export function isWaterSport(sport: string): boolean {
  return sport === "swim" || sport === "open_water";
}

/** Swim sports shown with swim pace (min per 100 m / 100 yd) instead of
 * speed. Same set as [isWaterSport] today, but a separate concern — that
 * one is about elevation noise, this one about the display metric. */
export function isSwimSport(sport: string): boolean {
  return sport === "swim" || sport === "open_water";
}

/** The du/triathlon discipline a sport belongs to; null = can't be an event
 * leg. Mirrors the backend merge gate in db/multisport_legs.rs. */
export function triathlonDiscipline(sport: string): "run" | "bike" | "swim" | "ski" | null {
  switch (sport) {
    case "run":
    case "trail_run":
    case "treadmill":
      return "run";
    case "ride":
    case "mountain_bike":
      return "bike";
    case "swim":
    case "open_water":
      return "swim";
    case "ski":
    case "ski_xc":
      return "ski";
    default:
      return null;
  }
}

/** Foot sports shown with PACE (min/km) instead of speed. Includes every
 * running form so it stays consistent with the backend's RUNNING_SPORTS
 * (run/trail_run/treadmill/virtual_run), which computes pace-based distance PBs — a
 * `run|walk|hike`-only check made trail_run/treadmill show speed while their
 * record card showed pace. */
export function isPaceSport(sport: string): boolean {
  return (
    sport === "run" ||
    sport === "trail_run" ||
    sport === "treadmill" ||
    sport === "virtual_run" ||
    sport === "walk" ||
    sport === "hike" ||
    sport === "mountaineering"
  );
}

// Activity map locations
export interface ActivityLocation {
  id: string;
  start_time: string;
  sport_type: string;
  title: string | null;
  distance_m: number | null;
  duration_s: number | null;
  lat: number;
  lon: number;
}

// Activity navigation
export interface AdjacentActivities {
  prev_id: string | null;
  next_id: string | null;
}

// Photos
export interface Photo {
  id: string;
  activity_id: string;
  path_in_vault: string;
  thumbnail_path: string | null;
  original_path: string | null;
  mime_type: string;
  width: number | null;
  height: number | null;
  size_bytes: number;
  hash_sha256: string;
  taken_at: string | null;
  caption: string | null;
  sort_order: number;
  created_at: string;
}

export interface AttachPhotosResult {
  attached: Photo[];
  skipped: string[];
  failed: { path: string; reason: string }[];
}

// Dashboard
export interface DashboardData {
  total_activities: number;
  total_distance_m: number;
  total_duration_s: number;
  total_elev_gain_m: number;
  avg_hr: number | null;
  week: WeekTotals;
  week_volume: VolumeBucket[];
  volume_buckets: VolumeBucket[];
  sport_distribution: SportEntry[];
  /** Last-7-days sport split (5 busiest), shares sum to 100. "By sport" donut. */
  week_sport_distribution: SportShare[];
  records_by_sport: SportRecords[];
}

export interface SportShare {
  sport_type: string;
  activities: number;
  share_pct: number;
}

export interface SportRecords {
  sport_type: string;
  activity_count: number;
  records: Records;
  /** Running sports only: best time on standard distances (longest first). */
  distance_pbs: DistancePb[];
}

export interface DistancePb {
  label: string;
  activity_id: string;
  title: string | null;
  date: string;
  duration_s: number;
  distance_m: number;
}

export interface WeekTotals {
  activities: number;
  distance_m: number;
  duration_s: number;
  elev_gain_m: number;
  avg_hr: number | null;
}

export interface VolumeBucket {
  label: string;
  start_date: string;
  distance_m: number;
  duration_s: number;
  activities: number;
  by_sport: Record<string, SportBucket>;
}

export interface SportBucket {
  distance_m: number;
  duration_s: number;
  activities: number;
}

export interface SportEntry {
  sport_type: string;
  activities: number;
  distance_m: number;
  duration_s: number;
}

export interface PersonalRecord {
  activity_id: string;
  title: string | null;
  date: string;
  value: number;
}

export interface Records {
  longest_distance: PersonalRecord | null;
  longest_duration: PersonalRecord | null;
  highest_elevation: PersonalRecord | null;
  fastest_speed: PersonalRecord | null;
  heaviest_set: PersonalRecord | null;
}

// Plugins
export interface PluginInfo {
  id: string;
  name: string;
  version: string;
  author: string | null;
  description: string | null;
  enabled: boolean;
  contributes: string[];
  permissions: string[];
  network_hosts: string[];
  signed: boolean;
  key_fingerprint: string | null;
  source: string;
  installed_at: string;
}

export interface PluginEndpoint {
  plugin_id: string;
  plugin_name: string;
  host: string;
}

export interface PluginContribution {
  plugin_id: string;
  name: string;
}

// Declarative view a plugin returns; the host renders it with safe primitives.
export interface StatItem {
  label: string;
  value: string;
}

export type ViewElement =
  | { type: "heading"; text: string }
  | { type: "text"; text: string }
  | { type: "stat"; label: string; value: string }
  | { type: "stat_grid"; stats: StatItem[] }
  | { type: "table"; headers: string[]; rows: string[][] }
  | { type: "divider" }
  | { type: "notice"; text: string; level?: "info" | "warning" | "error" | string }
  | { type: "input"; id: string; label: string; value: string; input_type: string }
  | { type: "select"; id: string; label: string; options: string[]; value: string }
  | { type: "button"; label: string; action: string }
  | { type: "map"; points: [number, number][]; label: string | null };

export interface ViewSpec {
  title: string | null;
  elements: ViewElement[];
  /** The action the host fires next on its own, right after showing this
   * view (a sync plugin's progress rounds). Honoured only by a full-page
   * host and only after a user action. */
  continue?: string | null;
}

// A user-saved route segment: an independent copy of a selected track slice.
export interface Segment {
  id: string;
  name: string;
  sport: string;
  source_activity_id: string | null;
  source_start_idx: number | null;
  source_end_idx: number | null;
  distance_m: number;
  elev_delta_m: number | null;
  avg_grade_pct: number | null;
  start_lat: number;
  start_lon: number;
  end_lat: number;
  end_lon: number;
  min_lat: number;
  max_lat: number;
  min_lon: number;
  max_lon: number;
  created_at: string;
}

// A close-match hit for the pre-save duplicate warning.
export interface SimilarSegment {
  id: string;
  name: string;
  distance_m: number;
}

// One row of the /segments page: the segment plus its effort aggregates.
export interface SegmentSummaryRow {
  id: string;
  name: string;
  sport: string;
  distance_m: number;
  avg_grade_pct: number | null;
  elev_delta_m: number | null;
  created_at: string;
  effort_count: number;
  best_elapsed_s: number | null;
  best_effort_power_w: number | null; // avg W of the fastest effort (the "Best" pass)
}

// One mean-max point: best average power held for window_s seconds.
export interface PowerCurvePoint {
  window_s: number;
  watts: number;
}

// One all-time-envelope point, attributed to the activity that set it.
export interface PowerCurveEnvelopePoint {
  window_s: number;
  watts: number;
  activity_id: string;
  title: string | null;
  start_time: string;
}

// The Power Curve panel's single fetch: this activity's curve + the envelope.
export interface PowerCurveData {
  points: PowerCurvePoint[];
  envelope: PowerCurveEnvelopePoint[];
}

// One leaderboard row of a segment: an effort with its activity context.
export interface SegmentLeaderboardRow {
  id: number;
  activity_id: string;
  activity_title: string | null;
  start_time: string;
  distance_m: number;
  elapsed_s: number | null;
  avg_power_w: number | null; // mean W over the pass; null without a meter
  rank: number | null;
}

// One segment pass inside an activity. Indices address the activity's full
// trackpoint arrays; per-effort speed/pace derive from the loaded track.
export interface SegmentEffortRow {
  id: number;
  segment_id: string;
  segment_name: string;
  start_idx: number;
  end_idx: number;
  distance_m: number;
  elapsed_s: number | null;
  avg_power_w: number | null; // mean W over the pass; null without a meter
  avg_grade_pct: number | null;
  rank: number | null;
  effort_count: number;
}

// Result of a manual update check against GitHub Releases.
export interface UpdateCheck {
  current_version: string;
  latest_version: string;
  update_available: boolean;
  release_url: string;
}

/** One night's recovery index for the calendar (#97) — the card's numbers per date. */
export interface RecoveryNight {
  date: string;
  index: number;
  band: "intervals_ok" | "easy_day" | "rest";
  hr: { night_median: number; baseline: number; delta: number; score: number };
  stress: { night_avg: number; score: number } | null;
  load: { tss_yesterday: number; ctl: number; score: number } | null;
  warning: "hr_above_baseline" | null;
}

/** Settings → Vault: how much Garmin monitoring the vault holds. */
export interface MonitoringSummary {
  days: number;
  first_date: string | null;
  last_date: string | null;
  files: number;
}

/** Outcome of delete_monitoring_range. */
export interface MonitoringDeleted {
  days: number;
  /** Files actually removed. */
  files: number;
  /** Files the OS refused to remove (their rows stay; retry later). */
  failed: number;
  error: string | null;
}
