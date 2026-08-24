export interface Activity {
  id: string;
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
  tags: string[];
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
  tags: string[];
  laps: Lap[];
  legs: MultisportLeg[];
  lengths: SwimLength[];
  sets: ExerciseSet[];
  time_in_zones: TimeInZone[];
  hrv_samples: HrvSample[];
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
  tag_ids?: number[];
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
  location_name: string;
}

export interface ImportResult {
  imported: number;
  skipped: number;
  failed: FailedFile[];
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

export interface Tag {
  id: number;
  name: string;
}

/** Max tags shown per activity (extras collapse into a "+N" chip) and the
 * most that can be assigned to a single activity while editing. */
export const MAX_TAGS_PER_ACTIVITY = 3;

/** Title length cap (in-place rename + edit modal) — keeps the detail
 * header and list rows readable; Strava caps similarly. */
export const MAX_TITLE_LENGTH = 100;

export interface DaySummary {
  date: string; // "YYYY-MM-DD"
  activity_count: number;
  total_distance_m: number;
  total_duration_s: number;
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

export interface SuggestedPath {
  label: string;
  path: string;
  exists: boolean;
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
  | "ride"
  | "mountain_bike"
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
  ride: "Ride",
  mountain_bike: "Mountain Bike",
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
 * (run/trail_run/treadmill), which computes pace-based distance PBs — a
 * `run|walk|hike`-only check made trail_run/treadmill show speed while their
 * record card showed pace. */
export function isPaceSport(sport: string): boolean {
  return (
    sport === "run" ||
    sport === "trail_run" ||
    sport === "treadmill" ||
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
  | { type: "input"; id: string; label: string; value: string; input_type: string }
  | { type: "select"; id: string; label: string; options: string[]; value: string }
  | { type: "button"; label: string; action: string }
  | { type: "map"; points: [number, number][]; label: string | null };

export interface ViewSpec {
  title: string | null;
  elements: ViewElement[];
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
  avg_grade_pct: number | null;
  rank: number | null;
  effort_count: number;
}
