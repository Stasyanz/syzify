# Changelog

All notable changes to Syzify are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/); versioning: [SemVer](https://semver.org/).
Pre-1.0: `minor` = new feature, `patch` = fix.

## [Unreleased]

### Fixed
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
