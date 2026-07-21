use serde::{Deserialize, Serialize};

/// A plugin manifest (`plugin.json`). This is the authoring format written by
/// plugin developers, so it uses camelCase like the rest of the JS ecosystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Minimum host app version required, e.g. "0.26.0". `None` = any.
    #[serde(default)]
    pub min_app_version: Option<String>,
    /// WASM module filename relative to the manifest, e.g. "plugin.wasm".
    /// Absent for manifest-only entries (e.g. phase-1 examples without code).
    #[serde(default)]
    pub entry: Option<String>,
    /// Author's Ed25519 public key (hex). Present in signed `.syzify-ext`
    /// packages; absent for unsigned dev sideloads.
    #[serde(default)]
    pub public_key: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Contribution points the plugin hooks into, e.g. "dashboard.widget".
    #[serde(default)]
    pub contributes: Vec<String>,
    /// Capabilities requested, e.g. "read:activities", "net:host=api.example.com".
    #[serde(default)]
    pub permissions: Vec<String>,
}

impl PluginManifest {
    /// Parse and validate a manifest from raw JSON, checking required fields and
    /// host-version compatibility. `app_version` is the current app version.
    pub fn parse_and_validate(json: &str, app_version: &str) -> Result<PluginManifest, String> {
        let manifest: PluginManifest =
            serde_json::from_str(json).map_err(|e| format!("Invalid manifest JSON: {e}"))?;

        validate_id(&manifest.id)?;
        if manifest.name.trim().is_empty() {
            return Err("Manifest is missing a name".to_string());
        }
        if manifest.version.trim().is_empty() {
            return Err("Manifest is missing a version".to_string());
        }
        if let Some(entry) = &manifest.entry {
            validate_entry(entry)?;
        }
        for host in manifest.network_hosts() {
            validate_net_host(&host)?;
        }
        if let Some(min) = &manifest.min_app_version {
            if !version_at_least(app_version, min) {
                return Err(format!(
                    "Plugin requires app version {min} or newer (current: {app_version})"
                ));
            }
        }
        Ok(manifest)
    }

    /// Parsed capabilities requested by this manifest.
    pub fn parsed_permissions(&self) -> Vec<Permission> {
        self.permissions.iter().map(|p| Permission::parse(p)).collect()
    }

    /// Hostnames this plugin wants to reach over the network, for disclosure.
    pub fn network_hosts(&self) -> Vec<String> {
        self.parsed_permissions()
            .into_iter()
            .filter_map(|p| match p {
                Permission::Net { host } => Some(host),
                _ => None,
            })
            .collect()
    }
}

/// Validate a plugin id so it is safe to use as a single path segment under
/// `vault/plugins/`. Reverse-DNS style: lowercase alphanumerics plus `.`, `-`,
/// `_`, no path separators, no `..`, no absolute/NUL tricks.
pub fn validate_id(id: &str) -> Result<(), String> {
    let bytes = id.as_bytes();
    let valid = !id.is_empty()
        && id.len() <= 128
        && !id.contains("..")
        && bytes[0].is_ascii_alphanumeric()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(format!(
            "invalid plugin id {id:?}: use lowercase a-z, 0-9, '.', '-', '_' (reverse-DNS); \
             no path separators, no '..'"
        ))
    }
}

/// Validate a `net:host=` host: a plain lowercase hostname only — no wildcard
/// (`*` would mean the whole internet), no scheme, path, port or userinfo.
pub fn validate_net_host(host: &str) -> Result<(), String> {
    let labels: Vec<&str> = host.split('.').collect();
    let ok = !host.is_empty()
        && host.len() <= 253
        && host.contains('.')
        && !host.contains("..")
        && !host.starts_with('.')
        && !host.ends_with('.')
        && host
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-'))
        // The TLD must start with a letter — rejects IP literals like 127.0.0.1
        // and 169.254.169.254 (link-local metadata).
        && labels
            .last()
            .is_some_and(|tld| tld.starts_with(|c: char| c.is_ascii_alphabetic()))
        // Reject punycode/IDN (homograph risk) — keep hosts human-verifiable.
        && !labels.iter().any(|l| l.starts_with("xn--"));
    if ok {
        Ok(())
    } else {
        Err(format!(
            "invalid net host {host:?}: use a plain hostname like api.example.com \
             (no '*', scheme, path, port, IP literal, punycode or '..')"
        ))
    }
}

/// Validate that a manifest `entry` is a plain filename (a single path
/// component), so it can't escape the plugin's vault directory.
pub fn validate_entry(entry: &str) -> Result<(), String> {
    use std::path::{Component, Path};
    let mut components = Path::new(entry).components();
    let single_normal = matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
        && !entry.contains("..");
    if single_normal {
        Ok(())
    } else {
        Err(format!("invalid plugin entry {entry:?}: must be a plain filename"))
    }
}

/// A capability requested by a plugin. `Unknown` preserves forward-compat:
/// a permission this app version doesn't recognize is kept verbatim rather
/// than dropped, so it can still be shown to the user and stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Permission {
    ReadActivities,
    ReadTrackpoints,
    ReadHrv,
    ReadLaps,
    ReadDashboard,
    /// Access to the plugin's own isolated storage.
    DataOwn,
    /// Network access to a specific host.
    Net { host: String },
    Unknown(String),
}

impl Permission {
    pub fn parse(raw: &str) -> Permission {
        let raw = raw.trim();
        if let Some(host) = raw.strip_prefix("net:host=") {
            return Permission::Net { host: host.to_string() };
        }
        match raw {
            "read:activities" => Permission::ReadActivities,
            "read:trackpoints" => Permission::ReadTrackpoints,
            "read:hrv" => Permission::ReadHrv,
            "read:laps" => Permission::ReadLaps,
            "read:dashboard" => Permission::ReadDashboard,
            "data:own" => Permission::DataOwn,
            other => Permission::Unknown(other.to_string()),
        }
    }
}

/// Compare two `major.minor.patch` strings; returns true if `have >= want`.
/// Missing/garbled segments are treated as 0, so it degrades gracefully.
fn version_at_least(have: &str, want: &str) -> bool {
    parse_semver(have) >= parse_semver(want)
}

fn parse_semver(v: &str) -> (u32, u32, u32) {
    // Tolerate a leading "v" and non-numeric suffixes ("1.2.3-rc1", "1.2.x"):
    // take the leading digits of each of the first three dot/dash-separated parts.
    let mut parts = v
        .trim()
        .trim_start_matches(['v', 'V'])
        .split(['.', '-', '+'])
        .map(|s| {
            s.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u32>()
                .unwrap_or(0)
        });
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

/// A plugin record as stored in the `plugin` registry table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub enabled: bool,
    /// Installed from a signature-verified `.syzify-ext` package.
    pub signed: bool,
    pub manifest: String,
    pub source: String,
    pub installed_at: String,
    pub updated_at: String,
}

/// What the frontend sees for a registered plugin. Uses snake_case like the
/// rest of the app's IPC contract (distinct from the camelCase manifest format).
#[derive(Debug, Clone, Serialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub enabled: bool,
    pub contributes: Vec<String>,
    pub permissions: Vec<String>,
    pub network_hosts: Vec<String>,
    /// True when installed from a signature-verified `.syzify-ext` package.
    /// NOTE: this means the package was self-signed by the author key below and
    /// not tampered with — it is integrity, not vetted authorship.
    pub signed: bool,
    /// Short fingerprint (first 16 hex chars) of the author's public key, if any.
    pub key_fingerprint: Option<String>,
    pub source: String,
    pub installed_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_permissions() {
        assert_eq!(Permission::parse("read:activities"), Permission::ReadActivities);
        assert_eq!(Permission::parse("data:own"), Permission::DataOwn);
        assert_eq!(
            Permission::parse("net:host=api.open-meteo.com"),
            Permission::Net { host: "api.open-meteo.com".to_string() }
        );
    }

    #[test]
    fn unknown_permission_is_preserved() {
        assert_eq!(
            Permission::parse("read:future-thing"),
            Permission::Unknown("read:future-thing".to_string())
        );
    }

    #[test]
    fn version_comparison() {
        assert!(version_at_least("0.26.0", "0.26.0"));
        assert!(version_at_least("0.27.1", "0.26.0"));
        assert!(version_at_least("1.0.0", "0.26.0"));
        assert!(!version_at_least("0.25.0", "0.26.0"));
        assert!(!version_at_least("0.26.0", "0.26.1"));
        // Tolerates a leading 'v' and pre-release/non-numeric suffixes.
        assert!(version_at_least("v1.2.3", "1.2.3"));
        assert!(version_at_least("1.2.3-rc1", "1.2.0"));
        assert!(!version_at_least("1.2.x", "1.2.1"));
    }

    #[test]
    fn parse_rejects_missing_id() {
        let json = r#"{"id":"","name":"X","version":"1.0.0"}"#;
        assert!(PluginManifest::parse_and_validate(json, "0.25.0").is_err());
    }

    #[test]
    fn validate_id_rejects_path_traversal() {
        for bad in ["../evil", "..", "a/b", "a\\b", "/abs", "C:/x", "a\0b", ".lead", "trail.", "Up"] {
            assert!(validate_id(bad).is_err(), "{bad:?} should be rejected");
        }
        for good in ["com.acme.sleep", "a", "x1.y-2_z"] {
            assert!(validate_id(good).is_ok(), "{good:?} should be accepted");
        }
    }

    #[test]
    fn validate_net_host_rejects_wildcards_and_urls() {
        for bad in ["*", "*.example.com", "http://x.com", "x.com/path", "x.com:8080", "", "localhost", "..", ".x.com", "127.0.0.1", "169.254.169.254", "10.0.0.5", "xn--80ak6aa92e.com"] {
            assert!(validate_net_host(bad).is_err(), "{bad:?} should be rejected");
        }
        for good in ["api.open-meteo.com", "a.b.c", "x1-2.example.com"] {
            assert!(validate_net_host(good).is_ok(), "{good:?} should be accepted");
        }
    }

    #[test]
    fn parse_rejects_wildcard_net_host() {
        let json = r#"{"id":"com.x","name":"X","version":"1.0.0","permissions":["net:host=*"]}"#;
        assert!(PluginManifest::parse_and_validate(json, "0.25.0").is_err());
    }

    #[test]
    fn parse_rejects_traversal_id_and_entry() {
        let bad_id = r#"{"id":"../../etc/x","name":"X","version":"1.0.0"}"#;
        assert!(PluginManifest::parse_and_validate(bad_id, "0.25.0").is_err());

        let bad_entry =
            r#"{"id":"com.x","name":"X","version":"1.0.0","entry":"../../../etc/cron.d/x"}"#;
        assert!(PluginManifest::parse_and_validate(bad_entry, "0.25.0").is_err());

        let abs_entry = r#"{"id":"com.x","name":"X","version":"1.0.0","entry":"/etc/passwd"}"#;
        assert!(PluginManifest::parse_and_validate(abs_entry, "0.25.0").is_err());

        let ok = r#"{"id":"com.x","name":"X","version":"1.0.0","entry":"plugin.wasm"}"#;
        assert!(PluginManifest::parse_and_validate(ok, "0.25.0").is_ok());
    }

    #[test]
    fn parse_rejects_incompatible_app_version() {
        let json = r#"{"id":"com.x","name":"X","version":"1.0.0","minAppVersion":"9.9.9"}"#;
        let err = PluginManifest::parse_and_validate(json, "0.25.0").unwrap_err();
        assert!(err.contains("requires app version"));
    }

    #[test]
    fn parse_accepts_valid_manifest_and_extracts_hosts() {
        let json = r#"{
            "id":"com.acme.smart-route","name":"Smart Route","version":"1.2.0",
            "minAppVersion":"0.25.0",
            "contributes":["route.planner","map.overlay"],
            "permissions":["read:activities","net:host=api.open-meteo.com"]
        }"#;
        let m = PluginManifest::parse_and_validate(json, "0.25.0").unwrap();
        assert_eq!(m.id, "com.acme.smart-route");
        assert_eq!(m.contributes.len(), 2);
        assert_eq!(m.network_hosts(), vec!["api.open-meteo.com".to_string()]);
    }
}
