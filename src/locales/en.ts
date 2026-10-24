/**
 * The English messages — the source the other languages are typed by
 * (`Messages` in ./types.ts): a key missing from ru.ts or it.ts, or one
 * they have and this file does not, fails `tsc`.
 *
 * Keys are `area.screen.thing`. `{name}` is a placeholder; a message with
 * forms per number is an object of `Intl.PluralRules` categories with
 * `other` required, picked by the `count` parameter.
 */
export const en = {
  "settings.general.theme": "Theme",
  "settings.general.themeHint": "Warm light, warm dark, or follow the system",
  "settings.general.themeLight": "Light",
  "settings.general.themeDark": "Dark",
  "settings.general.themeSystem": "System",
  "settings.general.language": "Language",
  "settings.general.languageHint": "The language of the interface, its dates and numbers",
  "settings.general.languageSystem": "System",
  "settings.general.units": "Units",
  "settings.general.unitsHint": "Distance, pace and elevation",
  "settings.general.unitsMetric": "Metric · km",
  "settings.general.unitsImperial": "Imperial · mi",
  "settings.general.pageSize": "Activities per page",
  "settings.general.pageSizeHint": "Number of activities loaded at once in the list view",
  "settings.general.geocoding": "Automatic location names",
  "settings.general.geocodingHint":
    "Look up a city name for each imported activity via nominatim.openstreetmap.org — the activity's start coordinates are sent to that OpenStreetMap service. When off, nothing is sent; searching a location by name when editing an activity uses the same service.",
  "settings.general.import": "Import Activities",
  "settings.general.importHint": "Drop GPX/FIT/TCX files anywhere in the app to import",
  "settings.general.sourceImport": "{name} import",
  "settings.general.sourceImportButton": "Import",
  "settings.general.sourceImported": "{name} — imported {imported}, skipped {skipped}",
  "settings.general.sourceImportedFailed": "{name} — imported {imported}, skipped {skipped}, failed {failed}",
  "settings.general.sourceImportFailed": "{name} import failed: {error}",
  "settings.general.tileCache": "Map tile cache",
  "settings.general.tileCacheHint": "Cached map tiles on disk: {size}",
  "settings.general.clearCache": "Clear Cache",
  "settings.general.clearingCache": "Clearing…",
  "settings.general.cacheCleared": "Tile cache cleared",
  // Unit symbols after a number. Italian keeps the international ones.
  "units.km": "km",
  "units.mi": "mi",
  "units.m": "m",
  "units.ft": "ft",
  "units.yd": "yd",
  "units.kmh": "km/h",
  "units.mph": "mph",
  "units.kg": "kg",
  "units.lb": "lb",
  "units.bpm": "bpm",
  "units.w": "W",
  "units.rpm": "rpm",
  "units.spm": "spm",
  // A swim's cadence: strokes, not steps — the same "spm" in English.
  "units.strokesPerMin": "spm",
  "units.kcal": "kcal",
  "units.h": "h",
  "units.min": "min",
  "units.mm": "mm",
  // A swim pace is per 100 of a distance unit: "1:40 /100m".
  "format.per100": "/100{unit}",
  "format.pace": "Pace",
  "format.speed": "Speed",
  "format.durationHM": "{h}h {m}m",
  "format.durationH": "{h}h",
  "format.durationM": "{m}m",
  // The power curve's windows: "45s", "1m30".
  "powerCurve.windowS": "{s}s",
  "powerCurve.windowMS": "{m}m{s}",
  "powerCurve.allTimeBest": "all-time best",
  "powerCurve.best": "best {power} ({label})",
} as const;
