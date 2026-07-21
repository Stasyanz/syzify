# Changelog

All notable changes to Syzify are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/); versioning: [SemVer](https://semver.org/).
Pre-1.0: `minor` = new feature, `patch` = fix. Detailed history before 0.25.0 lives
in the [PRD roadmap](PRD.md#102-v11--complete).

## [Unreleased]

### Changed
- **Rebrand: Sisyfy → Syzify.** Product name, package/crate (`syzify`, `syzify_lib`),
  bundle identifier (`com.syzify.app`), default vault path (`~/Syzify`) and the signed
  plugin package format (`.syzify-ext`, example ids `com.syzify.example.*`) all renamed.
  The crypto verifier constant changed, so **pre-existing encrypted vaults and the old
  app-data directory are no longer compatible** — start fresh or restore from a backup.

### Security
- Runkeeper import hardened against a malicious export `.zip`: entries are read
  through a shared `read_capped` (reused from the package installer) with per-entry,
  total-size and file-count limits (zip-bomb/OOM guard); a broken/non-UTF8
  `cardioActivities.csv` no longer aborts the whole import (decoded lossily, BOM
  stripped, errors recorded instead of dropping the imported GPX); same-named zip
  entries can't overwrite each other; and a CSV row no longer dedups against a
  different sport at a similar distance/time (no silent data loss). The GPX
  track-level `<time>` stripper now also handles the tag with attributes; a CSV
  truncated inside a quoted field is reported instead of silently dropped; non-file
  (symlink) zip entries are rejected.

### Added
- Share image: crop the photo. Drag a crop box (move + resize from the corners) over
  the photo or pick an aspect preset (Free, 1:1, 16:9, 16:10); the exported PNG uses the
  cropped region and the overlay blocks stay in place. Non-destructive — the stored photo
  is unchanged.
- Share image crop: rotation. **Orientation** — the ±90° buttons turn the cropped output a
  quarter-turn, swapping width↔height so a landscape crop becomes a vertical (portrait)
  image; the overlay metrics/route stay upright and readable. **Straighten** — a slider
  levels a tilted horizon. The crop box itself is rotatable in the editor — drag the top knob
  to tilt the frame to any angle (Shift snaps to 15°, snaps square near 0/90/180°), or use the
  Rotate slider; the export de-rotates that tilted region to an upright image. The frame is
  kept inside the photo as it rotates (no blank corners for a fitting box), the compose preview
  shares the export code path (what you see is what you get), and a stored photo whose
  dimensions disagree with its pixels no longer distorts.
- Import data sources (the `import.datasource` extension point). A first-party
  **Runkeeper** source imports a full export `.zip` in one step: GPS workouts from the
  GPX files (real tracks) plus GPS-less activities (swimming, manual, indoor) from
  `cardioActivities.csv`. Settings → Import → "Import from Runkeeper". Dedup makes
  re-import safe.

### Fixed
- Share image: a vertical phone photo (stored with EXIF orientation) is no longer
  squashed in the export. The crop now works in the browser's natural (EXIF-applied)
  coordinate system instead of the stored width/height, so the saved PNG keeps the
  photo's true aspect; a rotated crop frame that overhangs the photo fills its corners
  opaque (matching the preview) instead of leaving them transparent.
- Share image crop: the on-photo preview is now a pixel-exact replica of the exported
  PNG. The route map and elevation profile share one layout function
  (`shareGeometry.ts`) between the preview and the canvas export, rendered in the
  preview through an SVG `viewBox` so it scales uniformly — previously each used its own
  formula (constant vs proportional inner padding, constant chip padding/​radius), which
  diverged under a crop and even collapsed the route to a negative box at a small export
  size. Overlay text and stroke floors are likewise computed from the export dimensions
  and scaled down. The crop box is fully contained in the photo, frozen against a
  mid-drag preview resize, and a non-finite / not-yet-measured crop falls back to the
  full photo (single normalized `crop` used for both preview and export) instead of
  producing an empty image.
- GPX import: accept Runkeeper exports. Tolerate a track-level `<time>` element
  (emitted inside `<trk>`, not valid per GPX 1.1) that the strict parser rejected,
  and infer the sport type from the track name ("Running"/"Cycling", …) when the
  file has no `<type>`.

## [0.26.0] - 2026-05-30

### Security
- Plugins: `.syzify-ext` packages are read with a per-entry decompression cap
  (manifest 256 KiB, wasm 32 MiB, signature 1 KiB), so a zip-bomb can't OOM the
  app at install time.
- Plugins: validate the manifest `id` (reverse-DNS, no path separators or `..`) and
  `entry` (plain filename) before they are used to build vault paths, closing
  path-traversal that allowed writing files outside the vault during install/uninstall
  (arbitrary-file-write / RCE, even unsigned). Removed the frontend-callable
  `install_plugin` command (arbitrary manifest + source); the webview now installs only
  via the file picker / signed package.
- Plugins: a signed package is now labelled **Self-signed · <fingerprint>** (neutral),
  not a green "verified" badge — the signature is integrity + key-pinning, not vetted
  authorship. Installs can no longer silently change a plugin's author key or flip it
  between unsigned and signed (id-squatting / trust hijack); switching requires
  uninstalling first.
- Plugins: host functions no longer `unwrap()` the state lock (a panic under the wasm
  runtime would abort the app — DoS); the private data store rejects non-JSON payloads
  and reads rebuild the array through serde instead of string concatenation.
- Plugins: `net:host=` values are validated as plain hostnames — a wildcard `*`, scheme,
  path, port, IP literal (e.g. `169.254.169.254` link-local) or punycode is rejected, so
  a plugin can't request (or hide) whole-internet/metadata access.
- Plugins: a `map` overlay caps points (10k) and drops out-of-range lat/lon (UI DoS);
  plugin load errors no longer surface filesystem paths to the UI; lenient version
  parsing tolerates a leading `v` / pre-release suffixes.

### Added
- Plugin/extension system — phase 2 (runtime): plugins compiled to **WASM** run in a
  memory-isolated Extism (wasmtime) sandbox in the Rust backend, calling
  capability-gated host functions (`host_query`, `host_data_*`, `host_kv_*`) that go
  through the `db/` layer; no network (default-deny). UI contributions return a
  declarative ViewSpec the host renders with safe primitives. Contribution points:
  `dashboard.widget`, `activity.detail.panel` (the `consistency-widget` example) and a
  full-page `route.planner` (the `smart-route` example). ViewSpec supports interactive
  `input`/`select`/`button` elements and a `map` polyline overlay; pressing a button
  re-invokes the plugin with `{action, values}` and re-renders, so plugins can be
  interactive tools (smart-route plans a loop from a distance input + live weather).
  Signed plugin packages: a `.syzify-ext` (zip of manifest + wasm + Ed25519 signature)
  is verified on install and shown as **Self-signed · <fingerprint>** (integrity +
  key-pinning, not vetted authorship); replacing a plugin requires the same author key
  (trust-on-first-use); a `tools/pack-plugin` helper signs and packages plugins.
  Network is default-deny, brokered per declared `net:host=` host (disclosed to the
  user); an unapproved host is blocked at the WASM boundary. Installed plugins are
  copied into the vault (included in backups, survive a vault move) and each call is
  bounded by a 5s timeout + 64 MiB memory cap + a 5 MiB cap per network response.
- Plugin/extension system — phase 1 (framework): plugin registry,
  `plugin.json` manifest + capability/permission model, install / enable / disable /
  uninstall, and a management screen at **Settings → Plugins**. Requested permissions
  and network hosts are disclosed before a plugin is enabled; new plugins install
  disabled. The public Plugin API is documented in `examples/plugins/README.md`.

### Changed
- Routed all database access through the `db/` layer — it is now the single data
  gateway (no SQL in command handlers). Added `db/settings.rs`, `db/watch_folders.rs`.
- Moved read-model DTOs into `models/`; deduplicated activity row mapping; relocated
  background geocoding to `import/`.
- Unified Tauri event naming on the `domain:event` convention
  (`backup_progress` → `backup:progress`).

### Documentation
- Plugin/extension system design notes and `docs/development-guidelines.md`
  (architecture, how to add code, what not to do).

## [0.25.0]

### Added
- Photo attachments per activity and share-image export (overlay metrics / route /
  elevation onto a photo, exported as PNG).

---

Earlier releases (0.1.0 – 0.24.0) are catalogued in the
[PRD roadmap](PRD.md#103-v12).
