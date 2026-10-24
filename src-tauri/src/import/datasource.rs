//! Import data sources — the `import.datasource` contribution point.
//!
//! A data source turns a third-party export into Syzify activities. These are
//! first-party (trusted, native) sources surfaced in the import UI; untrusted
//! WASM data sources could be added later behind a capability.

use rusqlite::Connection;
use std::path::Path;

use crate::error::CommandError;
use crate::import::pipeline::ImportResult;

/// Metadata shown in the import UI (snake_case for the IPC boundary).
#[derive(Debug, Clone, serde::Serialize)]
pub struct DatasourceInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    /// File extensions the user picker should accept.
    pub extensions: Vec<String>,
}

/// All available import data sources.
pub fn list() -> Vec<DatasourceInfo> {
    vec![DatasourceInfo {
        id: "runkeeper".to_string(),
        name: "Runkeeper".to_string(),
        description: "Import a Runkeeper data export (.zip): GPS workouts (GPX) and \
                      GPS-less activities (swimming, manual) from cardioActivities.csv."
            .to_string(),
        extensions: vec!["zip".to_string()],
    }]
}

/// Run the data source `id` against a user-picked file. `encryption_key` is
/// the vault key already gated by the `activities` scope (see
/// `AppState::encryption_key_for`); raw files are stored encrypted when set.
pub fn run(
    conn: &Connection,
    vault_path: &Path,
    id: &str,
    path: &str,
    encryption_key: Option<&[u8; 32]>,
) -> Result<ImportResult, CommandError> {
    match id {
        "runkeeper" => super::runkeeper::import_zip(conn, vault_path, path, encryption_key),
        // Uncoded: the UI offers only the sources `list` names.
        other => Err(format!("unknown import source: {other}").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_source_is_an_uncoded_error_and_a_known_one_passes_its_code_on() {
        let conn = crate::db::test_db();
        let dir = crate::test_support::ScratchDir::new("datasource");
        let e = run(&conn, &dir, "strava", "x.zip", None).unwrap_err();
        assert_eq!(e, CommandError::from("unknown import source: strava"));
        let e = run(&conn, &dir, "runkeeper", dir.join("missing.zip").to_str().unwrap(), None).unwrap_err();
        assert_eq!(e.code, Some("import.openFailed"));
    }
}
