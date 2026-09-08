# Syzify plugins — manifest format (phase 1)

Plugins extend Syzify locally. A plugin is described by a `plugin.json` manifest.
This directory holds reference manifests you can sideload to try the installer.

> **Status:** Phase 1 ships the framework (registry, manifest + permission model,
> install/enable/disable/uninstall, management UI). Phase 2 adds the **runtime**:
> plugins are compiled to **WASM** and run in a memory-isolated Extism (wasmtime)
> sandbox in the Rust backend, calling capability-gated host functions, with
> default-deny network brokered by the host (`host_http`, declared `net:host=` hosts
> checked on every redirect hop, a cookie jar per call — the `net-probe` example).
> Contribution points so far: `dashboard.widget`, `activity.detail.panel` (the
> `consistency-widget` example) and `route.planner` (the `smart-route` example).
> Plugins can also import files into the vault through the app's own pipeline
> (`import:files`, the `paste-import` example) — the building block of a sync plugin.

## Install one

Settings → **Plugins** → **Install plugin** → pick either:
- a signed **`.syzify-ext`** package — its Ed25519 signature is verified (integrity) and
  it shows a neutral **Self-signed · <fingerprint>** badge. The signature proves the
  package wasn't tampered with and pins the author key; it is **not** vetted authorship.
  Replacing it requires the same author key (trust-on-first-use) — uninstall first to
  switch authors;
- a bare **`plugin.json`** — an **Unsigned** dev sideload (no integrity check).

Switching an installed plugin between unsigned and signed, or to a different author key,
is refused — uninstall the existing one first.

A freshly installed plugin is **disabled** — review its requested access first, then enable it.

## Packaging & signing

Use the bundled tool to produce a `.syzify-ext` from a plugin directory
(`plugin.json` + its wasm). Compiled binaries are **not committed** — build
the example's `plugin.wasm` first (see the build steps below):

```sh
cargo run --manifest-path tools/pack-plugin/Cargo.toml -- examples/plugins/smart-route
```

It generates a signing key on first run (`signing-key.hex` — **keep it secret, never
commit**), embeds the matching `publicKey` into the packaged manifest, signs
`sha256(manifest) ++ sha256(wasm)`, and writes `<id>.syzify-ext`. Reuse the same key on
later releases so upgrades pass the same-author check.

## Manifest fields

| Field | Required | Notes |
|---|---|---|
| `id` | yes | Reverse-DNS identifier, e.g. `com.acme.sleep`. Unique; reinstalling upgrades in place. |
| `name` | yes | Display name. |
| `version` | yes | `major.minor.patch`. |
| `entry` | no | WASM module filename next to the manifest (e.g. `plugin.wasm`). Required to run code; omit for manifest-only entries. |
| `publicKey` | auto | Author's Ed25519 public key (hex). Injected by the packaging tool; do not set by hand. |
| `minAppVersion` | no | Minimum Syzify version; install is rejected below it. |
| `author` | no | |
| `description` | no | Shown in the plugin list. |
| `contributes` | no | Contribution points the plugin hooks into. |
| `permissions` | no | Capabilities requested (see below). |

The manifest uses **camelCase** (familiar to JS authors); the app's own IPC stays snake_case.

## Contribution points

- `activity.detail.panel` — a panel on the activity detail page
- `dashboard.widget` — a card on the dashboard
- `import.datasource` — a new data source / parser (e.g. sleep)
- `route.planner` — a standalone planning page
- `map.overlay` — a layer over the existing map
- `activity.derived_metric` — compute & store extra metrics
- `settings.section`, `command`, `menu.item`

## Permissions

Capability-gated; the user grants them by enabling the plugin.

- `read:activities`, `read:trackpoints`, `read:hrv`, `read:laps`, `read:dashboard` — read-only data access
- `data:own` — the plugin's own isolated storage (`plugin_data` / `plugin_kv`)
- `data:secret` — the plugin's secrets (a sync plugin's tokens) through
  `host_secret_set` / `host_secret_get`: sealed under the vault key whenever any
  scope of vault encryption is on, stored in the clear otherwise — and the Plugins
  screen says so on the plugin's card ("stores secrets", plus a note while the
  vault is not encrypted). Never put a token in `plugin_kv`
- `import:files` — import files (FIT/GPX/TCX, optionally gzipped) into the vault through
  the app's own import pipeline — one file per `host_import_file` call, ≤ 32 MiB, deduplicated
  by hash, encrypted under the vault's `activities` scope like a dropped file. Note the
  dedup answer (`skipped`) tells the plugin whether an identical file, or an activity with
  the same start/sport/distance/duration, is already in the vault — a narrow read the
  permission implies without `read:activities`
- `net:host=<hostname>` — network access to one host through `host_http`, on the
  first request and on every redirect hop. **Every host is disclosed**
  on the Plugins screen and counts as a network endpoint (privacy policy, PRD §16.2).

Unknown permission strings are preserved verbatim (forward-compatibility) and shown
to the user rather than silently dropped.

## Writing a WASM plugin (phase 2)

A plugin is a WASM module that **exports one function per contribution point**
(dots → underscores: `dashboard.widget` → `dashboard_widget`). The export receives a
context JSON string and returns a **ViewSpec** JSON the host renders with safe
primitives — no raw HTML:

- display: `heading`, `text`, `stat`, `stat_grid`, `table`, `divider`, `map` (a polyline of `[lat,lon]` points)
- interactive: `input`, `select`, `button`

**Action loop:** when the user presses a `button`, the host re-invokes the export with
`{ "action": <button action>, "values": { <input id>: <value> }, …context }` and swaps
in the returned ViewSpec. So an export is just `context → ViewSpec`, called repeatedly.

Host functions (call only what your permissions allow):

| Host function | Needs | Purpose |
|---|---|---|
| `host_query` | `read:activities` / `read:dashboard` | `{"kind":"activities"\|"activity"\|"dashboard", …}` → JSON (`activity` takes an `id`) |
| `host_data_get` / `host_data_set` | `data:own` | the plugin's private structured store |
| `host_kv_get` / `host_kv_set` | `data:own` | the plugin's private key/value store |
| `host_secret_set` / `host_secret_get` | `data:secret` | the plugin's secrets: `host_secret_set({"key", "value"})` stores one (an empty value deletes it — a sign-out is one call per token; key ≤ 128 chars, value ≤ 64 KiB, at most 32 per plugin; an unknown field in the request fails the call), `host_secret_get(key)` reads it back, an empty string when there is none. Sealed under the vault key when vault encryption is on (any scope), plain otherwise; a locked vault fails the call with the `vault locked` prefix |
| `host_http(request, body)` + `host_http_meta()` | `net:host=` | one HTTP request: `request` is `{"url", "method"?, "headers"?: [[name, value], …], "max_redirects"?}` (GET by default; `Host`, `Content-Length`, `Transfer-Encoding`, `Connection`, `Expect`, `Upgrade`, `TE` are the host's; redirects followed up to `max_redirects`, 10 by default and at most — `0` hands a 3xx back as it is, `Location` and all), `body` the request body bytes (empty for GET/HEAD, ≤ 1 MiB) → the final response's body bytes (≤ 5 MiB); `host_http_meta()` right after → `{"status", "url", "headers": [[name, value], …], "hops": [{"status", "url", "location"}, …]}` of that response — lowercase names, one pair per header, so five `Set-Cookie` are five pairs; `hops` are the redirects taken on the way. The host holds a cookie jar for the invocation; see **Network** below. A refused hop, a bad request, a spent budget or a transport failure fails the call |
| `host_import_file(name, bytes)` | `import:files` | run the app's import pipeline on one file → the drop import's `ImportResult` JSON: `{imported, skipped, failed: [{path, reason}], monitoring_files, monitoring_days, monitoring_range, monitoring_night}` — per call only the first four are filled: the Monitor days an invocation touches are recomputed once, when it ends, so `monitoring_days` / `monitoring_range` / `monitoring_night` come back as 0 / null / false — a plugin cannot tell yet whether the night it synced is complete (no monitoring query in the Host SDK today). `name` is a bare file name (letters, digits, `.`, `-`, `_`, ≤ 128 bytes; its extension — `.fit`/`.gpx`/`.tcx`, optionally `.gz` — decides the format). A file the pipeline refuses is a `failed` entry; a bad name, an oversized file (> 32 MiB), a locked vault or a vault operation in flight fail the call |

`host_import_file` is how a **sync plugin** lands what it fetched: one call per file
(each stays inside the sandbox's 5 s budget), the same bytes the watch writes, so a
file already imported over USB is skipped by hash. Every import refreshes the app's
views, even when the call fails afterwards. Two failures are transient and worth a
retry later; their messages start with `vault busy` (a backup, restore, relocation or
encryption toggle is running) and `vault locked`. Anything else is the plugin's own
mistake. Import behind a `button` action, never in the initial render: the host
re-renders widgets on its own (a return to the window, a cache refresh), and an
import there would run again each time. See [`paste-import/`](paste-import/) for the
smallest version.

**Network** is `host_http` and nothing else — the PDK's own `http::request`
(Extism's built-in HTTP) is given no allowed host and refuses everything. It is
**default-deny**: a plugin reaches only the hosts it declared via `net:host=`
(each shown to the user before enabling), over `https://` on its default port
only, and the rule is applied **to every redirect hop** as well as the first request — a declared
host that redirects to an undeclared one fails the call, and nothing is sent
there. `Authorization` and `Cookie` headers the plugin set are dropped when a
hop changes host. Only the final response comes back; the redirects taken are
listed in the meta's `hops` (status, URL, `Location`), and `max_redirects: 0`
stops the chain at the first response so a plugin can read a `Location` itself.
A cookie jar lives for **one invocation** (one contribution
call, in memory, never written anywhere): every hop's `Set-Cookie` lands in it
— name, value, `Domain` (must cover the responding host), `Path`, `Secure`,
`Max-Age=0` or an `Expires` in the past deletes; `HttpOnly`, `SameSite` are
ignored; there is no public-suffix list, so a `Domain=co.uk` cookie would be
shared between two declared hosts under it; at most 100 cookies of ≤ 4 KiB —
and its cookies ride on every later request of the same invocation, before any
`Cookie` header the plugin set. On a cross-host hop `Authorization` and the
plugin's `Cookie` are dropped, but a 307/308 body is replayed to the new host
as a browser does. The allow-list is one of **names**: where the bytes go is
decided by DNS and the system proxy (honoured, like the app's own requests),
and the certificate must match the name. So a login flow of
several requests just works inside one action; it must **finish inside one**,
because the next action starts with an empty jar. A session that has to survive
between actions goes through `data:own` explicitly (a `Cookie` header on the
next request — discouraged; keep tokens, not cookies, when the service offers
them). A `route.planner` action of a plugin holding `net:host=` gets a 30 s
invocation budget — the one place the user pressed a button and waits; widgets
and panels keep 5 s, as they render on their own and every invocation queues
behind the previous one. One request may use all of what is left of it. See
[`net-probe/`](net-probe/) for the shape of a call and
[`smart-route/`](smart-route/) for a `route.planner` page that fetches weather.

**Secrets.** A login flow ends with tokens; keep them with `host_secret_set`,
never in `plugin_kv`. The host seals them under the vault key when encryption is
on — any scope, so an `activities`-only vault still carries only ciphertext in
`vault.db` and in backups — and follows the key when encryption is turned on or
off. In a plaintext vault they are stored in the clear, and the user is told so
on the plugin's card. Cookies from `host_http` are never persisted; store the
tokens a service hands out, not the session that produced them. Secrets follow
the trust of the build: a signed upgrade from the same key keeps them, an
unsigned reinstall (anyone can sideload under the same id) or an upgrade whose
manifest no longer asks for `data:secret` drops them.

`route.planner` contributions are opened full-page from **Settings → Plugins → Open**.

See [`consistency-widget/`](consistency-widget/) for a complete Rust example.
Compiled `plugin.wasm` binaries are **not committed** (CI only checks that the
examples build) — build the one you want to try, placing the wasm next to its
manifest where the `entry` field expects it:

```sh
cd examples/plugins/consistency-widget
cargo build --release --target wasm32-unknown-unknown   # rustup target add wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/consistency_widget.wasm plugin.wasm
```

Then sideload its `plugin.json`, enable it, and open the Dashboard.
(`smart-route`, `paste-import` and `net-probe` build the same way; their
artifacts are `smart_route.wasm`, `paste_import.wasm` and `net_probe.wasm`.)


## Licensing: Interface Material

Syzify is licensed under AGPL-3.0 with the
[Syzify Plugin Exception](../../LICENSE-PLUGIN-EXCEPTION.md): plugins that
interact with Syzify only through the official Plugin API may be distributed
under any license of the author's choosing, including commercial ones.

For the purposes of that exception, the following are identified as
**Interface Material** — you may use, adapt, and include them in your plugin
and distribute them under your plugin's own license (keeping the copyright
notices they contain):

- the `plugin.json` manifest format and its schema;
- the ViewSpec rendering schema;
- the host-function interface (Host SDK) definitions —

all as documented in this README (the *Manifest fields*, *Writing a WASM
plugin* and *Host functions* sections above).

The **example plugins in this directory are licensed under MIT-0**
(see [`LICENSE`](LICENSE)) — copy them into your own plugin freely, under any
license, with no attribution required. They are the intended starting point;
starting from AGPL-licensed app code instead would pull your plugin out of
the exception's safe harbor.

Everything else in this repository remains under AGPL-3.0 + the exception
unless a file states otherwise.
