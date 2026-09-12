# Changelog

All notable changes to Syzify are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/); versioning: [SemVer](https://semver.org/).
Pre-1.0: `minor` = new feature, `patch` = fix.

## [Unreleased]

### Added
- Edit Activity suggests locations while you type (#131): after a short
  pause and three characters, up to five matches with a context line to
  tell namesakes apart; arrow keys and Enter, or a click, pick one, and the
  pick saves its coordinates without a second lookup. A street or a
  housing complex is stored with its town ("Cebeci 7, Mahmutlar"), a town
  by its name alone; an empty answer says "No matches". Text saved without
  picking is filed under the locality of its best match with its parent
  ("Mahmutlar cebeci 6" → "Mahmutlar, Alanya"), where it used to take the
  match's own name ("Cebeci Towers"). Only with geocoding
  enabled in Settings — off, the field stays plain text as before; with no
  network or the service down, a warning says so once.
- The first, full-width chart card under the map can be stretched by its
  bottom-right grip, like the map, up to half again its height (#128); the
  height is remembered and belongs to the slot, so reordering charts keeps
  it.
- The elevation profile fills the area under a climb with the grade's
  color (#125): from 2 % up, the steeper the warmer, in the line's own
  palette and changing where the line does; flats and descents keep the
  altitude tints. Over a filled segment the tooltip shows the segment's
  average grade, one number for the whole band, and the band wears the
  color of that same number.

### Fixed
- A stop with a drifting barometer no longer reads as a 90 % grade on the
  elevation profile (#125): grades beyond 40 % are gaps, and the line and
  the fill paint the grade that owns most of the surrounding 300 m of
  road — a rough descent is no longer a barcode of colors, and a short
  pitch still counts as one.

### Changed
- The elevation line's first climb color now starts at 2 % (was 4 %), the
  same threshold as the new fill, that first step is a lighter gold, and a
  new amber step at 5 % separates a gentle run-in from a real drag (#125)
  — every imported activity's profile picks this up.

## [0.6.0] - 2026-09-09

### Added
- Time in Zones card on the activity page (#101), next to Cycling Dynamics:
  the device's heart-rate and power zones as Garmin Connect shows them —
  Z1–Z5 / Z1–Z7 with their ranges, time and share of the timer, a Heart
  rate / Power switch when a ride carries both. Time below zone 1 and above
  the maximum is left out, so the shares may add up to less than 100 %.
- Plugins can import files into the vault (#103): a new `import:files`
  permission and `host_import_file` host function run the app's own import
  pipeline on a file the plugin holds — deduplicated by hash, encrypted like
  a dropped file, the touched monitoring days recomputed once when the
  plugin's invocation ends (#106) — and the app's views refresh after it.
  The building block of sync plugins; `examples/plugins/paste-import`
  is the smallest one.
- Plugins reach the network through the app (#108): a `host_http` host
  function that follows redirects itself and checks every hop — not only
  the first request — against the hosts the plugin declared, keeps a
  cookie jar for the length of one call, and hands back the final
  response's headers as pairs (repeated ones survive), so a plugin can log
  in to a service the way a browser does. The
  built-in sandbox HTTP no longer gets any host; `examples/plugins/net-probe`
  shows the call, `smart-route` uses it.
- Plugins can keep secrets (#110): a `data:secret` permission and
  `host_secret_set` / `host_secret_get` host functions for the tokens a
  sync plugin holds. They are encrypted under the vault key whenever any
  part of the vault is encrypted, follow the key when encryption is turned
  on or off, and the Plugins screen says "stores secrets" on such a plugin
  — with a note while the vault is not encrypted, because then they are
  stored in the clear.
- A place to run a sync (#112): plugins can contribute a `sync.source` page,
  opened from Settings → Plugins → Sync, and a view may ask the host to call
  the plugin again right away (`continue`) — so a sync runs as rounds of
  ordinary invocations, one activity or one day each, with a Stop button
  and progress between them. `examples/plugins/sync-demo` shows the shape.
- Plugins can hand over a zip (#114): `host_import_file` expands a `.zip`
  on the host — every entry imported like a file of its own, deduplicated
  by hash, bounded like `.gz` — the shape Garmin's activity and wellness
  downloads come in.
- The Plugins card is back on the Settings screen (#116): sideload a
  plugin, enable it, and start a sync from its card.
- Plugin views can ask for a password (#117): an `input` of type
  `password` is masked.
- Plugin views can show a notice (#118): a callout with an info,
  warning or error level, so a refused sign-in or a stopped sync stands
  out from the rest of the page.

### Fixed
- Buttons in a plugin view no longer sit glued together: adjacent ones
  form a row with a gap (#120).

## [0.5.0] - 2026-09-06

### Added
- Garmin monitoring: drop the watch's Monitor files (the all-day
  `MYMD….FIT` files next to the activities) to import heart rate, stress,
  respiration, SpO2, steps and active minutes; the import summary reports
  the days covered and whether the newest day's night came along — a file
  the watch closed at midnight holds only the evening before, so the
  summary says "no night for Sep 6 yet, sync the watch after waking".
- Recovery index (#77): every night with a full heart-rate record gets a
  0–100 score from the night's heart rate against its 90-day baseline, the
  night's stress and yesterday's training load against the chronic load;
  three bands — Intervals OK (80+), Easy day (60–79), Rest (below 60). The
  dashboard calendar wears the band on the day's cell border, and the
  day's popup opens with the night's numbers above the workouts. Needs
  three recorded nights before the first score.
- Settings → Vault: "Monitoring data" shows what the watch's files cover
  and deletes a date range — the readings, the days and the Monitor files
  nothing else needs, whose hashes are freed so they can be imported again.
- Import: dropping a folder imports the workout and monitoring files in it
  (up to three levels deep, symlinks skipped).
- Settings: "Open another…" next to the vault location switches to a
  different existing vault without moving anything — the current vault stays
  on disk. The old "Change" button is now "Move…" and does what it always did.
- Activity charts: every metric chart (heart rate, power, cadence, speed or
  pace) draws its average as a dashed reference line with an `avg …` label,
  the same number the summary tiles show.

### Changed
- Settings → Encryption: the "Raw files" scope is labelled "Raw files
  (activities & monitoring)" — Monitor files are encrypted with the
  activities' files.

### Fixed
- Zone-colored bars (heart rate, power) painted every bar one zone too cool
  when the FIT file carried zone boundaries: Garmin's top bucket ceiling is
  a sentinel, not a boundary. Colors now follow the device's zone index —
  Z2 easy is gold, Z5 maximum is dark red, power Z3 tempo is teal.
- Calendars and the dashboard roll over to the new day at midnight on their
  own: the "today" mark moves, a calendar showing the current month follows
  a month change, and the This Week tiles and 7-day volume chart refetch
  for the new day (they used to show yesterday until something re-rendered).
- "Create new vault…" on the boot-error screen no longer creates a vault
  inside an existing one (picking the existing vault folder itself used to
  nest a fresh `Syzify` vault in it); it explains and asks for another folder.

## [0.4.0] - 2026-09-03

### Added
- Dashboard: the This Month stats also show total training hours.
- Segments: the page header sticks to the top while the list scrolls, and a
  search field filters segments by name.
- Dashboard: arrow keys page the training calendar's month, same as in the
  Library's calendar view.
- Segments: power where a ride carried a power meter — each effort shows its
  average watts (segment leaderboard and the activity's Segments panel), and
  the segment list gets a Power column with the best effort-average.
  Existing efforts are filled in once on first launch.

### Fixed
- The Segments page heading no longer sits detached below the title bar.
- The "Can't open your vault" screen now recognizes a vault last used by a
  newer version of Syzify: instead of a raw migration error it says what
  happened and offers to check for and install the update right there.

## [0.3.0] - 2026-08-30

### Added
- Power curve: activities with power data get a mean-max chart (best average
  power from 1 second to 1 hour, log time axis) drawn over the all-time best
  of the same kind of sport — running power never shadows cycling records.
  Hovering shows which activity holds the record for a window; clicking it
  opens that activity. Curves are computed on import, and existing
  activities are filled in once on first launch.
- Dashboard: the "This month" stats next to the training calendar now include
  total elevation gain for the shown month (hidden when there was no
  climbing).

### Changed
- Share image: the 16:9 crop preset now matches the photo's orientation —
  vertical photos get a vertical frame right away, no manual rotation needed.

## [0.2.0] - 2026-08-24

### Added
- Segments: drag-select a section of the elevation chart, right-click it and
  save it as a named segment. Every past and future workout over the same
  route is timed against it automatically. The activity page gets a Segments
  panel — click an effort to highlight it on the map and the chart — and the
  new Segments page in the sidebar lists all of them with rename, delete and
  a per-segment leaderboard.
- Elevation chart: drag-select a range to see its distance, elapsed time and
  average grade, with the matching part of the route highlighted on the map.
  The elevation line itself is now colored by grade, and the tooltip shows
  the grade under the cursor.
- Updates: Settings → General can check for a newer release and install it in
  one click — the update is downloaded from GitHub, cryptographically
  verified and applied with an automatic restart. Checking stays strictly
  manual: the app never phones home on its own.
- First run: an empty library now shows a short overlay pointing at workout
  import.

### Fixed
- The speed chart palette was upside down — top speed now reads green, not
  red.
- Drag-selecting on the elevation chart no longer zooms the synced charts
  below it.
- Double-clicking a chart clears the selection badge and the map highlight
  together.
- The segments table no longer shifts its columns when a leaderboard expands.

## [0.1.1] - 2026-08-21

### Added
- iPhone photos: HEIC/HEIF files can be attached to activities. On macOS they
  are converted to JPEG with the system tools (no extra dependencies), so the
  vault stays viewable on every platform; on Windows/Linux HEIC files are
  reported per-file as unsupported.

### Changed
- Adding photos by drag-and-drop now works anywhere on an activity page — no
  need to aim for the gallery box. Outside an activity page the window keeps
  importing workout files, and the drop overlay now says which of the two it
  will do.

### Fixed
- Photo previews ignored the EXIF orientation tag: vertical phone photos
  showed sideways thumbnails and distorted share crops. New photos are stored
  with the rotation applied, and existing thumbnails are regenerated once on
  first launch.
- Windows: map tiles and photos now load. Custom-protocol URLs (`tile://`,
  `photo://`) were built in the macOS-only form; WebView2 serves such
  protocols as `http://<proto>.localhost/…`, and the CSP additionally
  blocked those origins for images. macOS and Linux were unaffected.

## [0.1.0] - 2026-08-19

Initial public release.

- **Local-first training vault** — FIT and GPX import (plus Runkeeper archive
  import); everything lives on your machine, no account, no cloud.
- **Activity view** — route map, elevation/speed/heart-rate/power/cadence
  charts, training-zone breakdowns, cycling dynamics, laps, photos, in-place
  renaming.
- **Multisport** — triathlon activities with per-leg maps and charts and
  computed transitions.
- **Library** — list, calendar and map views with filtering; share-image
  export with cropping and privacy options.
- **Optional vault encryption** — separate scopes for activities, database
  and photos (SQLCipher).
- **Offline-friendly maps** — tile cache; online geocoding is opt-in and off
  by default.
- **Plugin system** — signed plugin packages running in a WebAssembly
  sandbox; the Syzify Plugin Exception lets authors license their plugins
  freely, including commercially.
- **Feedback** — GitHub Issues or email, right from the app.
