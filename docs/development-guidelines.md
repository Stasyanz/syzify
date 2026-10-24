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
| **commands** | `src-tauri/src/commands/` | Thin `#[tauri::command]` handlers. Lock state, delegate, map errors to `CommandError`. No SQL, no business logic. |
| **db** | `src-tauri/src/db/` | The single gateway to SQLite. All SQL lives here. Returns `rusqlite::Result<T>`. Query-shaping/aggregation that is inseparable from SQL belongs here too. |
| **models** | `src-tauri/src/models/` | Data contracts: row structs, DTOs, read-models, enums. `serde`, snake_case. Owns every type that crosses a layer or the IPC boundary. |
| **parser** | `src-tauri/src/parser/` | `parse_<fmt>(...) -> ParsedActivity`; one normalized output type, dispatched in `import/pipeline.rs`. |
| **import** | `src-tauri/src/import/` | `AppHandle`-driven background services (watcher, volume_monitor, geocoding) and the import pipeline. |

Canonical command shape:

```rust
#[tauri::command]
pub fn get_thing(id: String, state: State<AppState>) -> Result<Thing, CommandError> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(db::things::get(&conn, &id).map_err(|e| e.to_string())?)
}
```

**Command errors (#236).** A command errs with `CommandError`
(`src-tauri/src/error.rs`), which reaches the webview as `{ code, params,
message }`. A refusal the user reads and can act on is coded:
`CommandError::coded("thing.notFound", format!("Thing not found: {id}"))
.param("id", id)` — the code a literal (`src/lib/errorCodes.test.ts` scans
for them), the English message what the user read before, the params
named, numbers left numbers. Each code has an `error.<code>` message in
`src/locales/` whose English equals the Rust text with `{name}` per param
(the same test compares them); `errorText` says it in the interface
language. A library's own words (io, SQLite, zip, HTTP), an internal step
and a developer's guard stay uncoded — a `String` turns into one by `?`
(`Ok(db::x(&conn).map_err(|e| e.to_string())?)`, the one shape for a
command's tail) and is shown as it is. Where it may live:
- a **domain function whose refusal the user reads** (vault placement,
  backup/restore, import, a model's validator) may return `CommandError`
  itself, coded at the place that knows why;
- the **db layer** returns `rusqlite::Result` — `CommandError` only in a
  validator the commands call before any SQL (`db::segments::validated_name`);
- a **command** maps the library errors (`map_err(|e| e.to_string())?`) and
  wraps a domain refusal in its code when the domain returns a typed error
  (`AssignRefusal`, `DecryptAllError`);
- `From<CommandError> for String` drops the code: never code an error that
  passes through a `String` layer on its way (a helper that returns
  `Result<_, String>`) — the code is gone before the webview sees it.
Plugins keep reading strings: the Host SDK converts at its boundary
(`String::from(command_error)` is the English message). A DTO field the
user reads (`RestoreOutcome.error`) carries a `CommandError` too and goes
through `errorText`; so does the reason the vault would not open at boot
(`AppState.vault_error`, returned by `get_vault_error`, #244) — `init_vault`
and the restore's restart demand code it; the vault error screen and the
unlock prompt tell a too-new vault by `vault.tooNew` (`lib/vaultError.ts`),
and the Full Disk Access hint goes by the vault's path, never the text. A
screen that words one refusal its own way compares
`errorCode(e)` with the code, never the text.

### Monitoring & recovery (ADR 0002)

- Garmin Monitor files go through the same import pipeline as workouts:
  `parser/monitoring` decodes, `db/monitoring::store` writes samples and
  marks the days, `MonitoringBatch::finish` recomputes each touched day once
  per batch. Never compute a day's aggregates per file.
- The recovery index is pure: `recovery::night_indices` / `nights()` take the
  stored days and the daily hrTSS (`db/training_load`) and never touch SQL or
  the clock — commands load, the module folds. Add a new consumer as another
  reader of `nights()`, not another computation.
- Frontend queries live under the `"recovery"` / `"monitoring"` prefixes so an
  import or a delete invalidates them through `invalidateActivityData`; date
  keys are `"YYYY-MM-DD"` from the backend, the frontend clock only decides
  when to refetch (`useInvalidateOnNewDay`).

### Frontend

- **Bridge:** every backend call goes through the single `api` object in
  `src/lib/tauri.ts` (`invoke<T>("snake_case_cmd", { camelCaseArgs })`). Components
  never call `invoke` directly (events excepted).
- **Data fetching:** React Query (`useQuery`/`useMutation`) everywhere.
- **UI state:** Zustand stores (`src/stores/`). No data-fetching in stores.
- **Types:** `src/lib/types.ts` mirrors the Rust models, snake_case, `| null`.
- **Interface text (#237):** every string the user reads comes from the
  dictionaries in `src/locales/` — `en.ts` is the source, keyed
  `area.screen.thing`; `ru.ts` and `it.ts` are typed by it, so a key missing
  from either fails `tsc`. A component calls `const t = useT()` (it
  re-renders on a language change); the module-level `t` is for code outside
  React, and `i18n.scan.test.ts` keeps it out of `.tsx`. `{name}` fills a
  placeholder, a plural message is an object of `Intl.PluralRules` forms
  picked by a numeric `count`. Text that is a name and reads the same in
  every language (a device model, the wordmark) is marked `translate="no"`
  on its element or `i18n-ignore` on its line — never a word. Every screen
  is translated (#235): the scan test fails on any English string in JSX,
  a shown label or a toast, in any file. Tables of names and strings a
  function returns are out of its sight — a `return "Text"` in a helper
  passes it, and only review catches that — so a table maps to message
  keys (`as const satisfies Record<…, MessageKey>`, looked up by own key
  only) and a helper in a `.tsx` takes a `translate: Translator`. A long
  operation of Settings that must outlive a remount (a language change,
  another tab, leaving the page) keeps its busy flag in
  `stores/busyStore.ts`: the language picker stays locked and a change of
  the system's languages waits while any runs. A sentence with a
  bold word or a link inside is ONE message, never two halves around it:
  `const rt = useRichT()` and `rt("app.watch.found", { count, files:
  <strong>…</strong> })` cut the message at its `{name}`s before filling,
  so each translation puts the element where its word order wants it.
  A sport's name is `sportLabel(id, fallback, t)` — the dictionaries'
  `sport.<id>`; `fallback` (shown, so a message too, e.g.
  `t("common.activity")`) is for an id the app does not know, and a
  component passes its own `t` so the name and the sentence around it
  speak one language. `SPORT_TYPES` must list every `SportType`, or `tsc`
  names the one missing.
  An activity's title is shown through `displayTitle(title, t)`
  (`lib/activityTitle.ts`, #240): the backend stores a generated title
  in English ("Morning Run", `default_activity_title`; "Triathlon" or
  "Duathlon" for a merged event), and its exact text — any part, any
  `SportType::label()` — is shown in the language from the `title.*`
  keys: each sport's `title.sport.<id>` places `{part}` where its
  language wants it, `title.gender.<id>` (m/f/n; English "-") picks the
  part's adjective («Утренний забег», «Corsa mattutina sul tapis
  roulant»). Any other title is shown as it is. Only the display
  changes: the database, GPX `<name>`, backups and plugins keep the
  English, and an editor (InlineTitle, the
  Edit dialog) starts from the shown text and writes a title only when
  the user changed it. Accepted limit: search matches the stored text,
  so «Утренний» finds no generated title, "Morning" does.
  `activityTitle.test.ts` reads `models/activity.rs` and
  `db/multisport_legs.rs`: a new part, label or event title fails it
  until the dictionaries know it (a special case for one sport inside
  the Rust function is not seen).
- **Numbers, dates and units (#232)** follow the interface language, not
  the OS region: a shown number goes through `formatNumber` /
  `formatPercent`, a date through `formatDateTime` (all in `lib/i18n.ts`,
  cached per language and options), a unit symbol through `unitLabel`
  (`lib/units.ts`) — `distanceUnit()` and the like stay the symbols logic
  compares. One grouping policy per language, message parameters
  included: English groups every thousand (`1,214 m`), Italian and
  Russian from five digits (`1214 m`, `12 345 км`); a value that rounds to
  zero has no sign.
  `toFixed` and `toLocale*` are for what is not shown (SVG paths, keys).
  Weekday and month names come from `lib/calendar.ts` (Monday first). A
  language switch remounts `<AppShell key={language}>`, so memoized chart
  configs and module-level text are written again; a module-level constant
  that shows a unit reads it in a getter. Settings locks the language
  picker while an operation of its own runs, since the remount would cut
  it short, and puts the focus back on the picker after it.

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

**A host function** (`plugins/host.rs`) → `host_fn!` + `ctx.require(&Permission::X)`
first, then go through `db/` or the import pipeline — never the filesystem
directly. Keep the host layer Tauri-free: what it needs from the app comes in
through `PluginCtx` (`Db`, `VaultAccess`), so it is testable with a plain
`AppState`. A write host function claims `vault_flight` for the call and reads
the encryption key fresh (see `host_import_file`). Cover it three times: the pure
function directly, through a real wasm fixture (the e2e ABI test in
`plugins/runtime.rs`; add the fixture to the CI "Build plugin test fixtures" step),
and — for anything that needs an `AppHandle` (the runtime, events) — on a
windowless app from `tauri::test::mock_builder` (dev feature `test`; see
`run_contribution_imports_through_the_live_app_and_emits_the_event`). Keep such
functions generic over `tauri::Runtime` so the mock runtime fits.

**A plugin's secret** (a token) → `plugins/secrets.rs` behind `data:secret`, never
`plugin_kv`: sealed with `crypto::seal` under the vault key when any scope is on
(row identity as associated data), plain with a disclosure otherwise. The
enable/disable/unlock flows in `commands/settings.rs` re-seal them; a new place
that changes the key must call `secrets::encrypt_all` / `decrypt_all` too.

**Long plugin work** (a sync) → never one long invocation: the plugin does one
step per round and answers with `"continue": "<action>"`; the host
(`PluginWidget`, `followContinue`) calls it again, one chain at a time, at most
`MAX_CONTINUE_ROUNDS` per press. Honour `continue` only on the sync page and
only after a user action; the backend validates the token (`ViewSpec::validate`). A new interactive export joins
`INTERACTIVE_EXPORTS` in `plugins/runtime.rs` for the 30 s network budget.

**Network from a plugin** → `host_http` (`plugins/net.rs`) only; never hand a
host to Extism's manifest allow-list (its HTTP has no cookie jar and does not
re-check redirects). The allow-list is the plugin's `net:host=` set and it is
checked on **every hop**, `https://` only; the cookie jar is per invocation and
never persisted. Test against the loopback `test_server` in `net.rs` (set
`NetState::plain_loopback`; the runtime never does).

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
- ❌ Frontend: no `err.message` on a caught or rejected value — a rejection
  may be a bare string (a plugin's), and a command's `CommandError` carries
  a code to translate, which `.message` skips. Use `errorText(e)`
  from `src/lib/errors.ts` (the query error type is registered as `unknown`
  in `src/queryRegister.d.ts`, so `onError: (e) => e.message` will not compile —
  but an explicit `(e: Error) =>` annotation would, so never write one;
  `src/lib/errors.scan.test.ts` fails the suite on it, and on `String(…)` or
  `${…}` of anything named `e`, `err` or `…error`). A test mocks a command's
  refusal with `commandError(code, message, params)` from
  `src/test/commandError.ts`, the shape the backend sends.
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
- **Files on disk in a Rust test:** take a `crate::test_support::ScratchDir`
  (`ScratchDir::new("tag")`), never a fixed name under `temp_dir()`. It is a
  fresh UUID-named directory removed when the guard drops, panic included,
  and it derefs to `Path`. A fixed name plus a silently ignored
  `remove_dir_all` was #132: a stale `.enc` from an interrupted run broke the
  next run's disable sweep. A temporary guard (`ScratchDir::new("x").join(..)`)
  is removed at the end of the statement — bind it to a name first.

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
- **Distribution:** pushing the `vX.Y.Z` tag runs `.github/workflows/release.yml`:
  six platform builds (mac/linux/win × x64/arm64), installers attached to a GitHub
  Release, plus the auto-updater feed (`latest.json` with embedded minisign
  signatures). The workflow runs from the **tag's** commit — fixing it means
  re-tagging — so its manifest-assembly script is guarded by
  `scripts/test_release_manifest.sh` in CI.
- **Update signing key:** release bundles are signed with a minisign key
  (`~/.tauri/syzify_updater.key` + its password file next to it; mirrored in the
  repo's Actions secrets `TAURI_SIGNING_PRIVATE_KEY` / `_PASSWORD`). The public
  key is pinned in `tauri.conf.json`. Keep an **offline backup** of both files:
  the key cannot be rotated — it is baked into every shipped binary, and losing
  it permanently ends auto-updates for existing installs.
- **Building from source without the key:** `bundle.createUpdaterArtifacts`
  makes `tauri build` refuse to run unsigned. Use `npm run tauri build -- --no-bundle`
  for a bare binary, or generate a throwaway key
  (`npm run tauri signer generate -- -w /tmp/dev.key`) and export it as
  `TAURI_SIGNING_PRIVATE_KEY` to produce full local bundles.

## 6. Conventions checklist (quick)

- [ ] New SQL only in `db/`
- [ ] New cross-layer type in `models/`
- [ ] Command registered in `lib.rs` + wrapped in `api` + typed in `types.ts`
- [ ] Events `domain:event`
- [ ] Tests added; `cargo test` + `npm test` + `tsc` green
- [ ] No new network endpoint without Settings disclosure
- [ ] Migration is new (never edit shipped ones)
