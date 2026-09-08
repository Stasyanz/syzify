-- Secrets a plugin keeps (a sync plugin's tokens), apart from plugin_kv:
-- the value is AES-256-GCM ciphertext under the vault key whenever vault
-- encryption is on (any scope), plaintext otherwise — `encrypted` says
-- which. Cascades with the plugin like its other rows.
CREATE TABLE plugin_secret (
    plugin_id TEXT NOT NULL,
    key       TEXT NOT NULL,
    value     BLOB NOT NULL,
    encrypted INTEGER NOT NULL DEFAULT 0 CHECK (encrypted IN (0, 1)),
    PRIMARY KEY (plugin_id, key),
    FOREIGN KEY (plugin_id) REFERENCES plugin(id) ON DELETE CASCADE
);
