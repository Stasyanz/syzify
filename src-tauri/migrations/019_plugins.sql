-- Plugin registry: one row per installed plugin. The manifest is stored
-- verbatim as the source of truth; a few fields are duplicated as columns for
-- cheap listing without parsing JSON.
CREATE TABLE plugin (
    id           TEXT PRIMARY KEY,            -- reverse-DNS id, e.g. com.acme.sleep
    name         TEXT NOT NULL,
    version      TEXT NOT NULL,
    author       TEXT,
    description  TEXT,
    enabled      INTEGER NOT NULL DEFAULT 0,  -- 0 = disabled, 1 = enabled
    manifest     TEXT NOT NULL,               -- raw plugin.json
    source       TEXT NOT NULL,               -- "sideload", "builtin", install path, ...
    installed_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Structured, plugin-owned data (sleep records, planned routes, derived metrics).
-- `kind` is a plugin-defined namespace; `activity_id`/`key` are optional links.
CREATE TABLE plugin_data (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    plugin_id   TEXT NOT NULL,
    kind        TEXT NOT NULL,
    activity_id TEXT,
    key         TEXT,
    json        TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (plugin_id)   REFERENCES plugin(id)   ON DELETE CASCADE,
    FOREIGN KEY (activity_id) REFERENCES activity(id) ON DELETE CASCADE
);

CREATE INDEX idx_plugin_data_plugin_kind ON plugin_data(plugin_id, kind);
CREATE INDEX idx_plugin_data_activity ON plugin_data(activity_id);

-- Plugin-scoped key/value store (settings, sync cursors, small state).
CREATE TABLE plugin_kv (
    plugin_id TEXT NOT NULL,
    key       TEXT NOT NULL,
    value     TEXT NOT NULL,
    PRIMARY KEY (plugin_id, key),
    FOREIGN KEY (plugin_id) REFERENCES plugin(id) ON DELETE CASCADE
);
