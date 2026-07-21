# Syzify

Syzify — local-first просмотр и журнал тренировок (приватная альтернатива Strava/Garmin Connect). Tauri 2 + React + TypeScript.

## Prerequisites

- [Node.js](https://nodejs.org/) (v20+; CI uses 22)
- [Rust](https://www.rust-lang.org/tools/install) (stable)
- Системные зависимости для Tauri (см. ниже)

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
- [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (предустановлен в Windows 10/11)

## Development

```bash
npm install
npm run tauri dev
```

## Build

### macOS (.dmg, .app)

Две раздельные сборки по архитектурам (меньше размер каждого DMG, чем universal):

```bash
npm run build:mac-arm     # Apple Silicon — tauri build --target aarch64-apple-darwin
npm run build:mac-intel   # Intel        — tauri build --target x86_64-apple-darwin
```

Каждая требует своего Rust-таргета (разово):

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
```

Артефакты: `src-tauri/target/<triple>/release/bundle/dmg/` (Tauri именует файлы
`…_aarch64.dmg` и `…_x64.dmg`). Сборка под текущую архитектуру без указания таргета —
`npm run tauri build` → `src-tauri/target/release/bundle/…`.

### Linux (.deb, .AppImage)

x64 — на x64-хосте по умолчанию:

```bash
npm run tauri build
```

ARM64 (aarch64) — нативно на ARM64-хосте:

```bash
rustup target add aarch64-unknown-linux-gnu
npm run build:linux-arm   # = tauri build --target aarch64-unknown-linux-gnu
```

Артефакты: `src-tauri/target/release/bundle/{deb,appimage}/` (для ARM64 — под
`src-tauri/target/aarch64-unknown-linux-gnu/release/bundle/`). Кросс-компиляция Linux
ARM с x64-хоста для Tauri болезненна (нужны ARM-версии webkit/gtk); проще собирать на
ARM64-машине — в CI для этого используется нативный ARM64-раннер.

### Windows (.msi, .exe)

x64 (Intel/AMD) — на x64-хосте по умолчанию:

```bash
npm run tauri build
```

ARM64 (Windows on ARM, нативно):

```bash
rustup target add aarch64-pc-windows-msvc
npm run build:win-arm   # = tauri build --target aarch64-pc-windows-msvc
```

Артефакты: `src-tauri/target/release/bundle/msi/` и `…/bundle/nsis/` (для ARM64 — под
`src-tauri/target/aarch64-pc-windows-msvc/release/bundle/`). x64-бинарь работает и на
Windows on ARM через встроенную эмуляцию; нативный ARM64 даёт скорость/батарею.

## CI (GitHub Actions)

Два воркфлоу в `.github/workflows/`:

- **`ci.yml`** — на каждый push/PR в `main`: type-check (`tsc`), unit-тесты (`vitest`)
  и `cargo test` (с установкой системных зависимостей Tauri на Linux).
- **`release.yml`** — на тег `v*`: матрица сборок (macOS Apple Silicon, macOS Intel,
  Linux x64, Linux ARM64, Windows x64, Windows ARM64), затем публикация GitHub Release
  с прикреплёнными установщиками (`.dmg`/`.deb`/`.AppImage`/`.msi`/`.exe`). ARM64-таргеты
  Linux и Windows собираются на нативных раннерах (`ubuntu-24.04-arm`, `windows-11-arm`) —
  бесплатны для публичных репозиториев; для приватных нужен платный план / larger runners.
  Релиз создаётся, только если собрались все 6 целей (упавший таргет — перезапусти job).

Релиз: `scripts/release.sh <X.Y.Z>` → ревью CHANGELOG → commit → `git tag vX.Y.Z`
→ `git push --tags` запускает `release.yml`.

## Tests

```bash
npm test
```

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## License

Syzify распространяется под [GNU AGPL v3](LICENSE) с дополнительным
разрешением — [Syzify Plugin Exception](LICENSE-PLUGIN-EXCEPTION.md):
плагины, взаимодействующие с приложением только через официальный Plugin API
(манифест, host-функции, ViewSpec — см.
[examples/plugins/README.md](examples/plugins/README.md)),
могут распространяться под любой лицензией на выбор автора, включая
коммерческую.

Примеры плагинов в [examples/plugins/](examples/plugins/) лицензированы под
MIT-0 — их можно свободно копировать в свой плагин под любой лицензией.

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

Проект открыт по модели «open source, but not open contribution» (как
SQLite): pull requests не принимаются, баг-репорты и идеи в issues —
приветствуются. Подробности в [CONTRIBUTING.md](CONTRIBUTING.md).
