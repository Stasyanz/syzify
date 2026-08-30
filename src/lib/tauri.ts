import { invoke } from "@tauri-apps/api/core";
import type {
  ActivitySummary,
  ActivityDetail,
  ActivityFilters,
  ActivityUpdate,
  ActivityLocation,
  AdjacentActivities,
  RecordBadge,
  ImportResult,
  ImportDatasource,
  Tag,
  DaySummary,
  DashboardData,
  CacheInfo,
  WatchFolder,
  ScanResult,
  DeviceStats,
  ScanPreview,
  SuggestedPath,
  EncryptionStatus,
  UpdateCheck,
  EncryptionScopes,
  LocationUpdateResult,
  Photo,
  AttachPhotosResult,
  PowerCurveData,
  Segment,
  SegmentEffortRow,
  SegmentLeaderboardRow,
  SegmentSummaryRow,
  SimilarSegment,
  PluginInfo,
  PluginEndpoint,
  PluginContribution,
  ViewSpec,
} from "./types";

/**
 * True when running inside the Tauri webview. Tauri 2 exposes
 * `__TAURI_INTERNALS__` (not `__TAURI__`, which only exists with
 * withGlobalTauri) — use this so environment-gated features like the caching
 * tile:// protocol actually activate in the app.
 */
export function isTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

export const api = {
  importFiles: (paths: string[]) =>
    invoke<ImportResult>("import_files", { paths }),

  getImportDatasources: () =>
    invoke<ImportDatasource[]>("get_import_datasources"),

  runImportDatasource: (id: string, path: string) =>
    invoke<ImportResult>("run_import_datasource", { id, path }),

  getActivities: (filters: ActivityFilters) =>
    invoke<ActivitySummary[]>("get_activities", { filters }),

  getActivityDetail: (id: string) =>
    invoke<ActivityDetail>("get_activity_detail", { id }),

  getAdjacentActivities: (id: string) =>
    invoke<AdjacentActivities>("get_adjacent_activities", { id }),

  getActivityRecordBadges: (id: string) =>
    invoke<RecordBadge[]>("get_activity_record_badges", { id }),

  /** Mean-max power curve of the activity + the all-time envelope. */
  getPowerCurve: (id: string) =>
    invoke<PowerCurveData>("get_power_curve", { id }),

  updateActivity: (id: string, updates: ActivityUpdate) =>
    invoke<void>("update_activity", { id, updates }),

  getUsedSportTypes: () => invoke<string[]>("get_used_sport_types"),

  /** [firstYear, lastYear] across all activities, or null for an empty library. */
  getActivityYearRange: () => invoke<[number, number] | null>("get_activity_year_range"),

  deleteActivity: (id: string) =>
    invoke<void>("delete_activity", { id }),

  /** Combine several same-day activities into one triathlon; returns its id. */
  mergeIntoTriathlon: (activityIds: string[]) =>
    invoke<string>("merge_into_triathlon", { activityIds }),

  /** Reverse a merge: free the legs and delete the container. */
  unmergeTriathlon: (id: string) =>
    invoke<void>("unmerge_triathlon", { id }),

  searchActivities: (query: string) =>
    invoke<ActivitySummary[]>("search_activities", { query }),


  updateActivityLocation: (id: string, locationText: string) =>
    invoke<LocationUpdateResult>("update_activity_location", { id, locationText }),

  setActivityLocationPoint: (id: string, lat: number, lon: number) =>
    invoke<LocationUpdateResult>("set_activity_location_point", { id, lat, lon }),

  getActivityLocations: (filters?: ActivityFilters) =>
    invoke<ActivityLocation[]>("get_activity_locations", { filters: filters ?? null }),

  getTags: () => invoke<Tag[]>("get_tags"),

  createTag: (name: string) => invoke<Tag>("create_tag", { name }),

  setActivityTags: (activityId: string, tagIds: number[]) =>
    invoke<void>("set_activity_tags", { activityId, tagIds }),

  getCalendarData: (year: number, month: number, filters?: ActivityFilters) =>
    invoke<DaySummary[]>("get_calendar_data", { year, month, filters: filters ?? null }),

  startGeocoding: () => invoke<void>("start_geocoding"),

  exportActivityGpx: (id: string, destPath: string, privacyRadiusM?: number) =>
    invoke<void>("export_activity_gpx", { id, destPath, privacyRadiusM }),

  backupVault: (destPath: string) =>
    invoke<void>("backup_vault", { destPath }),

  restoreVault: (backupPath: string) =>
    invoke<{ restored: boolean; error: string | null; preserved_at: string | null }>(
      "restore_vault",
      { backupPath }
    ),

  getTileCacheInfo: () =>
    invoke<CacheInfo>("get_tile_cache_info"),

  clearTileCache: () =>
    invoke<void>("clear_tile_cache"),

  getWatchFolders: () =>
    invoke<WatchFolder[]>("get_watch_folders"),

  addWatchFolder: (path: string) =>
    invoke<WatchFolder>("add_watch_folder", { path }),

  removeWatchFolder: (id: number) =>
    invoke<void>("remove_watch_folder", { id }),

  scanWatchFolders: () =>
    invoke<ScanResult>("scan_watch_folders"),

  getVaultPath: () =>
    invoke<string>("get_vault_path"),

  /** A boot-time vault error (protected folder / no access), or null. */
  getVaultError: () =>
    invoke<string | null>("get_vault_error"),

  /** Move the vault to a new folder; resolves to the new root. */
  relocateVault: (destPath: string) =>
    invoke<string>("relocate_vault", { destPath }),

  /** Point the app at another vault root without moving data (boot-error
   * screen only); caller restarts the app afterwards. */
  switchVault: (destPath: string, expectExisting: boolean) =>
    invoke<string>("switch_vault", { destPath, expectExisting }),

  restartApp: () =>
    invoke<void>("restart_app"),

  checkForUpdates: () =>
    invoke<UpdateCheck>("check_for_updates"),

  /** Download + install the signed update bundle; the app restarts itself,
   * so on success this promise never resolves. */
  installUpdate: () =>
    invoke<void>("install_update"),

  // Device Detection
  getDetectedDevices: () =>
    invoke<DeviceStats[]>("get_detected_devices"),

  previewWatchFolders: () =>
    invoke<ScanPreview>("preview_watch_folders"),

  getSuggestedWatchPaths: () =>
    invoke<SuggestedPath[]>("get_suggested_watch_paths"),

  // Settings (key-value)
  getSetting: (key: string) =>
    invoke<string | null>("get_setting", { key }),

  setSetting: (key: string, value: string) =>
    invoke<void>("set_setting", { key, value }),

  // Legal texts bundled as resources (LICENSE / plugin exception / notices)
  getLegalText: (doc: "license" | "exception" | "notices") =>
    invoke<string>("get_legal_text", { doc }),

  // Encryption
  getEncryptionStatus: () =>
    invoke<EncryptionStatus>("get_encryption_status"),

  unlockVault: (password: string) =>
    invoke<void>("unlock_vault", { password }),

  enableEncryption: (password: string, scopes: EncryptionScopes) =>
    invoke<void>("enable_encryption", { password, scopes }),

  disableEncryption: (password: string) =>
    invoke<void>("disable_encryption", { password }),

  // Watcher
  restartWatcher: () =>
    invoke<void>("restart_watcher"),

  // Dashboard
  getDashboardData: () => invoke<DashboardData>("get_dashboard_data"),

  // Photos
  attachPhotos: (activityId: string, paths: string[]) =>
    invoke<AttachPhotosResult>("attach_photos", { activityId, paths }),

  getPhotos: (activityId: string) =>
    invoke<Photo[]>("get_photos", { activityId }),

  deletePhoto: (photoId: string) =>
    invoke<void>("delete_photo", { photoId }),

  updatePhotoCaption: (photoId: string, caption: string | null) =>
    invoke<void>("update_photo_caption", { photoId, caption }),

  reorderPhotos: (photoIds: string[]) =>
    invoke<void>("reorder_photos", { photoIds }),

  saveShareImage: (destPath: string, pngBase64: string) =>
    invoke<void>("save_share_image", { destPath, pngBase64 }),

  getPhotoDataUrl: (photoId: string, size: "thumb" | "full" = "full") =>
    invoke<string>("get_photo_data_url", { photoId, size }),

  // Segments
  checkSimilarSegments: (activityId: string, startIdx: number, endIdx: number) =>
    invoke<SimilarSegment[]>("check_similar_segments", { activityId, startIdx, endIdx }),

  saveSegment: (activityId: string, startIdx: number, endIdx: number, name: string) =>
    invoke<Segment>("save_segment", { activityId, startIdx, endIdx, name }),

  getActivitySegmentEfforts: (activityId: string) =>
    invoke<SegmentEffortRow[]>("get_activity_segment_efforts", { activityId }),

  listSegments: () => invoke<SegmentSummaryRow[]>("list_segments"),

  renameSegment: (id: string, name: string) =>
    invoke<void>("rename_segment", { id, name }),

  deleteSegment: (id: string) => invoke<void>("delete_segment", { id }),

  getSegmentEfforts: (id: string) =>
    invoke<SegmentLeaderboardRow[]>("get_segment_efforts", { id }),

  // Plugins
  getPlugins: () => invoke<PluginInfo[]>("get_plugins"),

  installPluginFromFile: (path: string) =>
    invoke<PluginInfo>("install_plugin_from_file", { path }),

  installPluginFromPackage: (path: string) =>
    invoke<PluginInfo>("install_plugin_from_package", { path }),

  setPluginEnabled: (id: string, enabled: boolean) =>
    invoke<void>("set_plugin_enabled", { id, enabled }),

  uninstallPlugin: (id: string) =>
    invoke<void>("uninstall_plugin", { id }),

  getPluginNetworkEndpoints: () =>
    invoke<PluginEndpoint[]>("get_plugin_network_endpoints"),

  getPluginContributions: (point: string) =>
    invoke<PluginContribution[]>("get_plugin_contributions", { point }),

  renderPluginView: (pluginId: string, point: string, context: string) =>
    invoke<ViewSpec>("render_plugin_view", { pluginId, point, context }),
};
