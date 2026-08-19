# Syzify

![Vibe Coded](https://img.shields.io/badge/vibe-coded-blueviolet)

Syzify is a local-first workout viewer and journal — a private alternative to Strava/Garmin Connect. Tauri 2 + React + TypeScript.

Looking for installers? Grab them from [GitHub Releases](https://github.com/Stasyanz/syzify/releases).

## Prerequisites

- [Node.js](https://nodejs.org/) (v20+; CI uses 22)
- [Rust](https://www.rust-lang.org/tools/install) (stable)
- Tauri system dependencies (see below)

### macOS

```bash
xcode-select --install
```

### Linux (Debian/Ubuntu)

```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

### Windows

- [Microsoft Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
- [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (preinstalled on Windows 10/11)

## Development

```bash
npm install
npm run tauri dev
```

## Build

### macOS (.dmg, .app)

Two separate per-architecture builds (each DMG is smaller than a universal one):

```bash
npm run build:mac-arm     # Apple Silicon — tauri build --target aarch64-apple-darwin
npm run build:mac-intel   # Intel        — tauri build --target x86_64-apple-darwin
```

Each needs its Rust target installed (once):

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
```

Artifacts: `src-tauri/target/<triple>/release/bundle/dmg/` (Tauri names the files
`…_aarch64.dmg` and `…_x64.dmg`). A build for the current architecture without an
explicit target — `npm run tauri build` → `src-tauri/target/release/bundle/…`.

### Linux (.deb, .AppImage)

x64 — the default on an x64 host:

```bash
npm run tauri build
```

ARM64 (aarch64) — natively on an ARM64 host:

```bash
rustup target add aarch64-unknown-linux-gnu
npm run build:linux-arm   # = tauri build --target aarch64-unknown-linux-gnu
```

Artifacts: `src-tauri/target/release/bundle/{deb,appimage}/` (for ARM64 — under
`src-tauri/target/aarch64-unknown-linux-gnu/release/bundle/`). Cross-compiling Linux
ARM from an x64 host is painful with Tauri (it needs ARM builds of webkit/gtk); building
on an ARM64 machine is simpler — CI uses a native ARM64 runner for this.

### Windows (.msi, .exe)

x64 (Intel/AMD) — the default on an x64 host:

```bash
npm run tauri build
```

ARM64 (Windows on ARM, natively):

```bash
rustup target add aarch64-pc-windows-msvc
npm run build:win-arm   # = tauri build --target aarch64-pc-windows-msvc
```

Artifacts: `src-tauri/target/release/bundle/msi/` and `…/bundle/nsis/` (for ARM64 —
under `src-tauri/target/aarch64-pc-windows-msvc/release/bundle/`). The x64 binary also
runs on Windows on ARM through the built-in emulation; native ARM64 buys speed and
battery life.

## CI (GitHub Actions)

Two workflows in `.github/workflows/`:

- **`ci.yml`** — on every push/PR to `main`: type-check (`tsc`), unit tests (`vitest`)
  and `cargo test` (installing Tauri's system dependencies on Linux).
- **`release.yml`** — on a `v*` tag: a build matrix (macOS Apple Silicon, macOS Intel,
  Linux x64, Linux ARM64, Windows x64, Windows ARM64), then a GitHub Release with the
  installers attached (`.dmg`/`.deb`/`.AppImage`/`.msi`/`.exe`). The Linux and Windows
  ARM64 targets build on native runners (`ubuntu-24.04-arm`, `windows-11-arm`) — free
  for public repositories; private ones need a paid plan / larger runners. The release
  is created only when all 6 targets build (a failed target — rerun the job).

Cutting a release: `scripts/release.sh <X.Y.Z>` → review the CHANGELOG → commit →
`git tag vX.Y.Z` → `git push origin main vX.Y.Z` triggers `release.yml`. Push the
tag explicitly — `git push --tags` would also publish any stale local tags.

## Tests

```bash
npm test
```

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## License

Syzify is distributed under the [GNU AGPL v3](LICENSE) with an additional
permission — the [Syzify Plugin Exception](LICENSE-PLUGIN-EXCEPTION.md):
plugins that interact with the application only through the official Plugin API
(manifest, host functions, ViewSpec — see
[examples/plugins/README.md](examples/plugins/README.md))
may be distributed under any license of the author's choosing, including a
commercial one.

The example plugins in [examples/plugins/](examples/plugins/) are licensed
under MIT-0 — feel free to copy them into your own plugin under any license.

```
Syzify — local-first workout viewer and journal
Copyright (C) 2026 Stanislav Zainullin

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as
published by the Free Software Foundation, version 3.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public
License along with this program.  If not, see <https://www.gnu.org/licenses/>.
```

The project is open source, but not open contribution (like SQLite):
pull requests are not accepted; bug reports and ideas in issues are
welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for details.
