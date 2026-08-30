# Changelog

All notable changes to Syzify are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/); versioning: [SemVer](https://semver.org/).
Pre-1.0: `minor` = new feature, `patch` = fix.

## [Unreleased]

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
