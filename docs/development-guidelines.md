# Development guidelines — Syzify

How code is structured here and how to extend it so it stays **idiomatic,
extensible, and testable**. Read this before adding code. The plugin system's
public API is documented in
[`examples/plugins/README.md`](../examples/plugins/README.md).

Stack: Tauri 2 + Rust backend (`src-tauri/`), React 19 + TypeScript frontend
(`src/`), SQLite (rusqlite + rusqlite_migration). Local-first, privacy-first.

---

## 1. Architecture

### Backend — strict layering `commands → db → models`

| Layer | Path | Responsibility |
|---|---|---|
| **commands** | `src-tauri/src/commands/` | Thin `#[tauri::command]` handlers. Lock state, delegate, map errors to `String`. No SQL, no business logic. |
| **db** | `src-tauri/src/db/` | The single gateway to SQLite. All SQL lives here. Returns `rusqlite::Result<T>`. Query-shaping/aggregation that is inseparable from SQL belongs here too. |
| **models** | `src-tauri/src/models/` | Data contracts: row structs, DTOs, read-models, enums. `serde`, snake_case. Owns every type that crosses a layer or the IPC boundary. |
| **parser** | `src-tauri/src/parser/` | `parse_<fmt>(...) -> ParsedActivity`; one normalized output type, dispatched in `import/pipeline.rs`. |
| **import** | `src-tauri/src/import/` | `AppHandle`-driven background services (watcher, volume_monitor, geocoding) and the import pipeline. |

Canonical command shape:

```rust
#[tauri::command]
pub fn get_thing(id: String, state: State<AppState>) -> Result<Thing, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::things::get(&conn, &id).map_err(|e| e.to_string())
}
```

### Frontend

- **Bridge:** every backend call goes through the single `api` object in
  `src/lib/tauri.ts` (`invoke<T>("snake_case_cmd", { camelCaseArgs })`). Components
  never call `invoke` directly (events excepted).
- **Data fetching:** React Query (`useQuery`/`useMutation`) everywhere.
- **UI state:** Zustand stores (`src/stores/`). No data-fetching in stores.
- **Types:** `src/lib/types.ts` mirrors the Rust models, snake_case, `| null`.

### Boundary naming

snake_case **everywhere** on the app's own IPC (commands, models, `types.ts`).
The one deliberate exception is **external authoring formats** consumed by third
parties (e.g. the plugin manifest), where camelCase via `#[serde(rename_all)]` is
idiomatic for the author — kept separate from the app's internal contracts.

---

## 2. How to add code (recipes)

**A new Tauri command**
1. Add the handler in the right `commands/*.rs` (delegate to `db/`).
2. Register it in `lib.rs` `invoke_handler![…]`.
3. Add the wrapper to `api` in `src/lib/tauri.ts`.
4. Add request/response types to `src/lib/types.ts`.

**A new DB-backed entity**
1. `migrations/NNN_name.sql` (next sequential number).
2. Register it in `db/migrations.rs`.
3. Model in `models/<entity>.rs`, registered in `models/mod.rs`.
4. `db/<entity>.rs` with the queries, registered in `db/mod.rs`.
5. Unit tests using `db::test_db()` (in-memory, migrated).

**A new table that an existing command touches** → still give it a `db/<table>.rs`
module. Commands must not grow SQL.

**Reused row→struct mapping** → extract a `row_to_x(&Row) -> Result<X>` helper and
a column-list `const` as the single source of truth (see `db/activities.rs`).

**A cross-layer DTO / read-model** → define it in `models/`, not in `db/`.

**A Tauri event** → name it `domain:event` (colon-namespaced, e.g.
`import:progress`, `activities:updated`).

**A plugin capability / contribution point** → see the Plugin API docs
(`examples/plugins/README.md`); extend the
`Permission` enum and the `contributes` handling, keep `Unknown` for forward-compat.

---

## 3. What NOT to do

**Layering**
- ❌ No SQL in `commands/` (or anywhere outside `db/`). Route it through `db/`.
- ❌ Don't define DTOs/read-models in `db/`; they go in `models/`.
- ❌ Don't "fix" db-layer aggregation by pushing SQL up into `commands/` — that
  just reintroduces the leak. SQL-coupled logic stays in `db/`.

**Migrations**
- ❌ Never edit an already-shipped migration. Add a new one.
- Match the existing style of the latest migrations (no gratuitous `IF NOT EXISTS`).

**Frontend**
- ❌ No direct `invoke` in components — go through `api`.
- ❌ No data-fetching inside Zustand stores — that's React Query's job.
- ❌ No camelCase on the app's own IPC types.

**General**
- ❌ Don't add a dependency for something small/standard; prefer the std/existing crates.
- ❌ No `unwrap()`/`expect()` on fallible paths in production code; propagate errors.
- ❌ Don't break a published contract (IPC shapes, Host SDK, manifest) without versioning.

**Privacy invariants (non-negotiable, PRD §16)**
- ❌ No telemetry, no automatic uploads, no account requirement.
- ❌ No network access without disclosing the endpoint in Settings.
- Plugins: installed **disabled**; capabilities granted explicitly; every
  `net:host=` shown to the user; unknown permissions preserved, never silently dropped.

---

## 4. Testing

- Run after every feature: `cargo test --manifest-path src-tauri/Cargo.toml`,
  `npm test`, `npx tsc --noEmit`.
- **db/models:** unit tests against `db::test_db()`.
- **Pure logic:** keep it pure and test it directly (e.g. permission parsing,
  semver compat, `plugin_to_info` degradation). Prefer extracting a pure helper
  over leaving logic untested inside a `State`-bound command.
- **Frontend pure utils:** vitest (see `src/lib/*.test.ts`).
- New code without coverage → add tests. If a command is too `State`-bound to test,
  push its real logic into a tested pure helper.
- `#[cfg(test)] mod tests { … }` goes at the **bottom** of the file.

---

## 5. Releases & changelog

- **Versioning:** SemVer. Pre-1.0: `minor` = new feature, `patch` = fix. The version
  lives in three files (`package.json`, `src-tauri/Cargo.toml`,
  `src-tauri/tauri.conf.json`) — never edit them by hand; the release script keeps
  them in sync.
- **Commits drive the changelog:** Conventional Commits (`feat:`, `fix:`, `refactor:`,
  `perf:`, `docs:`, `chore:`, with optional scope like `feat(plugins):`). Mapping →
  `feat`→Added, `fix`→Fixed, `refactor`→Changed, `perf`→Performance, `docs`→Documentation;
  `chore`/`ci`/`test`/merges are skipped. Config: [`cliff.toml`](../cliff.toml).
- **`CHANGELOG.md`** follows Keep a Changelog with a top `[Unreleased]` section.
- **Release flow (hybrid):**
  1. `npm run release -- X.Y.Z` — bumps the three files + `Cargo.lock`, and drafts the
     changelog (auto via `git-cliff` if installed, otherwise stamps `[Unreleased]` as the
     new version). It does **not** commit.
  2. Review and polish `CHANGELOG.md` prose.
  3. `npm run gen:notices` — regenerates `THIRD-PARTY-NOTICES.md` from the current
     dependency tree (bundled into the app via `tauri.conf.json` → `bundle.resources`).
  4. `git commit -m "chore(release): vX.Y.Z"` then `git tag vX.Y.Z`.
- **Distribution:** local builds for now (`npm run tauri build`). A CI workflow
  (tauri-action → GitHub Releases for mac/win/linux) is deferred until there is a remote.

## 6. Conventions checklist (quick)

- [ ] New SQL only in `db/`
- [ ] New cross-layer type in `models/`
- [ ] Command registered in `lib.rs` + wrapped in `api` + typed in `types.ts`
- [ ] Events `domain:event`
- [ ] Tests added; `cargo test` + `npm test` + `tsc` green
- [ ] No new network endpoint without Settings disclosure
- [ ] Migration is new (never edit shipped ones)
