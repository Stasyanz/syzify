# Syzify plugins — manifest format (phase 1)

Plugins extend Syzify locally. A plugin is described by a `plugin.json` manifest.
This directory holds reference manifests you can sideload to try the installer.

> **Status:** Phase 1 ships the framework (registry, manifest + permission model,
> install/enable/disable/uninstall, management UI). Phase 2 adds the **runtime**:
> plugins are compiled to **WASM** and run in a memory-isolated Extism (wasmtime)
> sandbox in the Rust backend, calling capability-gated host functions, with
> default-deny network brokered by declared `net:host=` hosts. Contribution points so
> far: `dashboard.widget`, `activity.detail.panel` (the `consistency-widget` example)
> and `route.planner` (the `smart-route` example).

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
- `net:host=<hostname>` — network access to one host. **Every host is disclosed**
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

Network is **default-deny**: a plugin can reach only the hosts it declared via
`net:host=` (each shown to the user before enabling). The host wires exactly those
into the sandbox's allow-list; any other host aborts the call. See
[`smart-route/`](smart-route/) for a `route.planner` page that fetches weather.

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
(`smart-route` builds the same way; its artifact is `smart_route.wasm`.)


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
