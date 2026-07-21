use std::path::Path;

use tauri::{AppHandle, Emitter, Manager};

use crate::import::datasource;
use crate::import::pipeline::{self, FailedFile, ImportOutcome, ImportResult};
use crate::state::AppState;

/// List the available first-party import data sources (the `import.datasource`
/// contribution point), e.g. Runkeeper.
#[tauri::command]
pub fn get_import_datasources() -> Vec<datasource::DatasourceInfo> {
    datasource::list()
}

/// Refuse imports while the vault is LOCKED (a vault.lock exists but the key
/// isn't in memory yet). The UnlockModal normally blocks the UI, but the
/// command itself must not rely on that: an import accepted while locked
/// would write its raw file in PLAINTEXT into an encrypted vault (no key to
/// encrypt with) until the next unlock's resume pass happens to heal it.
fn ensure_vault_unlocked(state: &AppState) -> Result<(), String> {
    if crate::crypto::read_vault_lock(&state.vault_path)?.is_none() {
        return Ok(()); // plaintext vault
    }
    let has_key = state
        .encryption_key
        .lock()
        .map_err(|e| e.to_string())?
        .is_some();
    if has_key {
        Ok(())
    } else {
        Err("The vault is locked — unlock it before importing".to_string())
    }
}

/// Run an import data source against a user-picked file (e.g. a Runkeeper .zip).
#[tauri::command]
pub fn run_import_datasource(
    id: String,
    path: String,
    app: AppHandle,
) -> Result<ImportResult, String> {
    let result = {
        let state = app.state::<AppState>();
        ensure_vault_unlocked(&state)?;
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        // Key only when the `activities` scope is on — see AppState::encryption_key_for.
        let key = state.encryption_key_for(|s| s.activities)?;
        datasource::run(&conn, &state.vault_path, &id, &path, key.as_ref())?
    };
    if result.imported > 0 {
        let _ = app.emit("activities:updated", ());
    }
    Ok(result)
}

#[tauri::command]
pub async fn import_files(
    paths: Vec<String>,
    app: AppHandle,
) -> Result<ImportResult, String> {
    ensure_vault_unlocked(&app.state::<AppState>())?;
    let total = paths.len();
    let mut result = ImportResult {
        imported: 0,
        skipped: 0,
        failed: Vec::new(),
    };

    for (i, path_str) in paths.iter().enumerate() {
        let filename = Path::new(path_str)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path_str)
            .to_string();

        let _ = app.emit(
            "import:progress",
            serde_json::json!({
                "current": i + 1,
                "total": total,
                "filename": filename,
            }),
        );

        // Yield to let the event reach the frontend before processing
        tokio::task::yield_now().await;

        let app_clone = app.clone();
        let path = path_str.clone();

        let outcome = tokio::task::spawn_blocking(move || {
            let state = app_clone.state::<AppState>();
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            // Key only when the `activities` scope is on — encrypting a raw
            // file outside its scope would strand it as ciphertext after
            // disable (see AppState::encryption_key_for).
            let key = state.encryption_key_for(|s| s.activities)?;
            pipeline::import_single_file(&conn, &state.vault_path, &path, key.as_ref())
        })
        .await
        .map_err(|e| format!("Task join error: {}", e))?;

        match outcome {
            Ok(ImportOutcome::Imported) => result.imported += 1,
            Ok(ImportOutcome::Skipped) => result.skipped += 1,
            Err(reason) => {
                result.failed.push(FailedFile {
                    path: path_str.clone(),
                    reason,
                });
            }
        }
    }

    // Kick off background geocoding for newly imported activities
    if result.imported > 0 {
        let geo_handle = app.clone();
        std::thread::spawn(move || {
            crate::import::geocoding::run_background_geocoding(&geo_handle);
        });
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn test_state(vault: &Path, key: Option<[u8; 32]>) -> AppState {
        AppState {
            db: Arc::new(Mutex::new(crate::db::test_db())),
            vault_path: vault.to_path_buf(),
            encryption_key: Mutex::new(key),
            watcher_handle: Mutex::new(None),
            db_locked: Mutex::new(false),
            vault_error: Mutex::new(None),
            services_started: Mutex::new(false),
            geocoding_flight: crate::state::SingleFlight::default(),
            vault_flight: crate::state::SingleFlight::default(),
        }
    }

    /// A locked vault (lock on disk, no key in memory) must refuse imports:
    /// accepting one would write the raw file in plaintext into an encrypted
    /// vault. Plaintext and unlocked vaults import normally.
    #[test]
    fn imports_are_refused_while_the_vault_is_locked() {
        let vault = std::env::temp_dir()
            .join(format!("syz_imp_gate_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&vault).unwrap();

        // Plaintext vault (no lock): fine.
        assert!(ensure_vault_unlocked(&test_state(&vault, None)).is_ok());

        // LOCKED: lock present, key absent → refused.
        let lock = crate::crypto::VaultLock {
            salt: String::new(),
            verifier: String::new(),
            nonce: String::new(),
            created_at: String::new(),
            scopes: crate::crypto::EncryptionScopes {
                activities: true,
                database: false,
                photos: false,
            },
        };
        crate::crypto::write_vault_lock(&vault, &lock).unwrap();
        let err = ensure_vault_unlocked(&test_state(&vault, None)).unwrap_err();
        assert!(err.contains("locked"), "got: {}", err);

        // UNLOCKED: lock present, key held → fine.
        assert!(ensure_vault_unlocked(&test_state(&vault, Some([7u8; 32]))).is_ok());

        let _ = std::fs::remove_dir_all(&vault);
    }
}
