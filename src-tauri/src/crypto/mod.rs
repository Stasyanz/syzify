use std::fs;
use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use sha2::Sha256;

const PBKDF2_ITERATIONS: u32 = 600_000;
const VERIFIER_PLAINTEXT: &[u8] = b"syzify-verifier-v1";

/// Which parts of the vault a given lock encrypts. Independent toggles so the
/// user can protect e.g. the GPS database without also encrypting raw imports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EncryptionScopes {
    /// Raw imported workout files under `raw/` (the original feature).
    pub activities: bool,
    /// The SQLite database (`vault.db`) — encrypted transparently via SQLCipher.
    pub database: bool,
    /// Photo attachments and thumbnails under `photos/`.
    pub photos: bool,
}

impl EncryptionScopes {
    /// Historical default for a vault.lock written before scopes existed:
    /// activities-only, matching the original behavior.
    pub fn activities_only() -> Self {
        Self { activities: true, database: false, photos: false }
    }

    pub fn any(&self) -> bool {
        self.activities || self.database || self.photos
    }
}

impl Default for EncryptionScopes {
    fn default() -> Self {
        Self::activities_only()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VaultLock {
    pub salt: String,           // hex-encoded 32-byte salt
    pub verifier: String,       // hex-encoded encrypted verifier
    pub nonce: String,          // hex-encoded 12-byte nonce for verifier
    pub created_at: String,     // ISO 8601 timestamp
    /// Missing in locks written before multi-scope support → activities-only.
    #[serde(default)]
    pub scopes: EncryptionScopes,
}

pub fn derive_key(password: &str, salt: &[u8; 32]) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, PBKDF2_ITERATIONS, &mut key);
    key
}

pub fn generate_salt() -> [u8; 32] {
    let mut salt = [0u8; 32];
    OsRng.fill_bytes(&mut salt);
    salt
}

pub fn create_verifier(key: &[u8; 32]) -> Result<(Vec<u8>, [u8; 12]), String> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| format!("Failed to create cipher: {}", e))?;
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, VERIFIER_PLAINTEXT)
        .map_err(|e| format!("Failed to encrypt verifier: {}", e))?;
    Ok((ciphertext, nonce_bytes))
}

pub fn verify_password(key: &[u8; 32], verifier: &[u8], nonce: &[u8; 12]) -> bool {
    let cipher = match Aes256Gcm::new_from_slice(key) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let nonce = Nonce::from_slice(nonce);
    match cipher.decrypt(nonce, verifier) {
        Ok(plaintext) => plaintext == VERIFIER_PLAINTEXT,
        Err(_) => false,
    }
}

/// Encrypt a file in-place: reads file, writes [12-byte nonce][ciphertext+tag],
/// renames to .enc extension. Returns new path relative to vault.
pub fn encrypt_file(key: &[u8; 32], path: &Path) -> Result<PathBuf, String> {
    let plaintext = fs::read(path)
        .map_err(|e| format!("Failed to read file for encryption: {}", e))?;

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| format!("Failed to create cipher: {}", e))?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_ref())
        .map_err(|e| format!("Failed to encrypt file: {}", e))?;

    // Write: [12-byte nonce][ciphertext+GCM tag]
    let mut output = Vec::with_capacity(12 + ciphertext.len());
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);

    let enc_path = PathBuf::from(format!("{}.enc", path.display()));
    fs::write(&enc_path, &output)
        .map_err(|e| format!("Failed to write encrypted file: {}", e))?;

    // Remove original
    fs::remove_file(path)
        .map_err(|e| format!("Failed to remove original file: {}", e))?;

    Ok(enc_path)
}

/// Decrypt a .enc file: reads [12-byte nonce][ciphertext], writes plaintext,
/// removes .enc file. Returns new path (without .enc).
pub fn decrypt_file(key: &[u8; 32], path: &Path) -> Result<PathBuf, String> {
    let data = fs::read(path)
        .map_err(|e| format!("Failed to read encrypted file: {}", e))?;

    if data.len() < 12 {
        return Err("Encrypted file too short".to_string());
    }

    let (nonce_bytes, ciphertext) = data.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| format!("Failed to create cipher: {}", e))?;

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "Decryption failed — wrong password or corrupted file".to_string())?;

    // Remove .enc extension
    let path_str = path.to_string_lossy();
    let dec_path = match path_str.strip_suffix(".enc") {
        Some(stem) => PathBuf::from(stem),
        None => PathBuf::from(format!("{}.dec", path_str)),
    };

    fs::write(&dec_path, &plaintext)
        .map_err(|e| format!("Failed to write decrypted file: {}", e))?;

    fs::remove_file(path)
        .map_err(|e| format!("Failed to remove encrypted file: {}", e))?;

    Ok(dec_path)
}

/// Decrypt a file to memory (for GPX export without writing to disk)
pub fn decrypt_file_to_memory(key: &[u8; 32], path: &Path) -> Result<Vec<u8>, String> {
    let data = fs::read(path)
        .map_err(|e| format!("Failed to read encrypted file: {}", e))?;

    if data.len() < 12 {
        return Err("Encrypted file too short".to_string());
    }

    let (nonce_bytes, ciphertext) = data.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| format!("Failed to create cipher: {}", e))?;

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "Decryption failed — wrong password or corrupted file".to_string())
}

/// Raw files under vault/raw, sorted by name for a deterministic bulk order.
fn sorted_raw_files(vault_path: &Path) -> Result<Vec<PathBuf>, String> {
    let raw_dir = vault_path.join("raw");
    if !raw_dir.is_dir() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(&raw_dir)
        .map_err(|e| format!("Failed to read raw dir: {}", e))?;
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    Ok(files)
}

fn vault_rel(name_of: &Path) -> String {
    format!(
        "raw/{}",
        name_of
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    )
}

/// Encrypt every plaintext raw file. `on_file(old, new)` runs right after each
/// file is rewritten — the caller pairs the fs change with its DB row update,
/// so a mid-run failure leaves at most zero drift (everything done so far is
/// already consistent). Fail-fast; already-encrypted files are skipped, which
/// also makes reruns resume where a failed run stopped.
pub fn encrypt_all_raw_files(
    key: &[u8; 32],
    vault_path: &Path,
    on_file: &mut dyn FnMut(&str, &str) -> Result<(), String>,
) -> Result<usize, String> {
    let mut count = 0;
    for path in sorted_raw_files(vault_path)? {
        if path.to_string_lossy().ends_with(".enc") {
            continue;
        }
        let old_vault_path = vault_rel(&path);
        let new_path = encrypt_file(key, &path)?;
        on_file(&old_vault_path, &vault_rel(&new_path))?;
        count += 1;
    }
    Ok(count)
}

/// Decrypt every .enc raw file; same per-file pairing contract as
/// [`encrypt_all_raw_files`].
pub fn decrypt_all_raw_files(
    key: &[u8; 32],
    vault_path: &Path,
    on_file: &mut dyn FnMut(&str, &str) -> Result<(), String>,
) -> Result<usize, String> {
    let mut count = 0;
    for path in sorted_raw_files(vault_path)? {
        if !path.to_string_lossy().ends_with(".enc") {
            continue;
        }
        let old_vault_path = vault_rel(&path);
        let new_path = decrypt_file(key, &path)?;
        on_file(&old_vault_path, &vault_rel(&new_path))?;
        count += 1;
    }
    Ok(count)
}

/// Files under a vault subtree, sorted, returned as (absolute, vault-relative)
/// pairs. Recurses (photos are nested one dir per activity).
fn sorted_tree_files(vault_path: &Path, subdir: &str) -> Result<Vec<(PathBuf, String)>, String> {
    let root = vault_path.join(subdir);
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    collect_files(&root, vault_path, &mut out)?;
    out.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(out)
}

fn collect_files(
    dir: &Path,
    vault_path: &Path,
    out: &mut Vec<(PathBuf, String)>,
) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("Failed to read {}: {}", dir.display(), e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, vault_path, out)?;
        } else if path.is_file() {
            let rel = path
                .strip_prefix(vault_path)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            out.push((path, rel));
        }
    }
    Ok(())
}

/// Encrypt every plaintext photo file (recursively). Same per-file pairing
/// contract as [`encrypt_all_raw_files`]: `on_file(old_rel, new_rel)` runs
/// right after each rename so the caller can update the matching DB column.
pub fn encrypt_all_photos(
    key: &[u8; 32],
    vault_path: &Path,
    on_file: &mut dyn FnMut(&str, &str) -> Result<(), String>,
) -> Result<usize, String> {
    let mut count = 0;
    for (abs, rel) in sorted_tree_files(vault_path, "photos")? {
        if rel.ends_with(".enc") {
            continue;
        }
        let new_abs = encrypt_file(key, &abs)?;
        on_file(&rel, &format!("{}.enc", rel))?;
        debug_assert!(new_abs.to_string_lossy().ends_with(".enc"));
        count += 1;
    }
    Ok(count)
}

/// Decrypt every `.enc` photo file (recursively).
pub fn decrypt_all_photos(
    key: &[u8; 32],
    vault_path: &Path,
    on_file: &mut dyn FnMut(&str, &str) -> Result<(), String>,
) -> Result<usize, String> {
    let mut count = 0;
    for (abs, rel) in sorted_tree_files(vault_path, "photos")? {
        if !rel.ends_with(".enc") {
            continue;
        }
        decrypt_file(key, &abs)?;
        let new_rel = rel.strip_suffix(".enc").unwrap_or(&rel).to_string();
        on_file(&rel, &new_rel)?;
        count += 1;
    }
    Ok(count)
}

/// Self-healing for crash drift: for every DB path whose file is missing but
/// whose sibling under the other extension (`x` ↔ `x.enc`) exists, return the
/// (old, new) fix the caller should apply to the DB. Read-only on disk.
pub fn reconcile_paths(vault_path: &Path, db_paths: &[String]) -> Vec<(String, String)> {
    let mut fixes = Vec::new();
    for db_path in db_paths {
        if vault_path.join(db_path).exists() {
            continue;
        }
        let sibling = match db_path.strip_suffix(".enc") {
            Some(stem) => stem.to_string(),
            None => format!("{}.enc", db_path),
        };
        if vault_path.join(&sibling).exists() {
            fixes.push((db_path.clone(), sibling));
        }
    }
    fixes
}

pub fn read_vault_lock(vault_path: &Path) -> Result<Option<VaultLock>, String> {
    let lock_path = vault_path.join("vault.lock");
    if !lock_path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(&lock_path)
        .map_err(|e| format!("Failed to read vault.lock: {}", e))?;
    let lock: VaultLock = serde_json::from_str(&data)
        .map_err(|e| format!("Failed to parse vault.lock: {}", e))?;
    Ok(Some(lock))
}

pub fn write_vault_lock(vault_path: &Path, lock: &VaultLock) -> Result<(), String> {
    let lock_path = vault_path.join("vault.lock");
    let data = serde_json::to_string_pretty(lock)
        .map_err(|e| format!("Failed to serialize vault.lock: {}", e))?;
    fs::write(&lock_path, data)
        .map_err(|e| format!("Failed to write vault.lock: {}", e))?;
    Ok(())
}

pub fn remove_vault_lock(vault_path: &Path) -> Result<(), String> {
    let lock_path = vault_path.join("vault.lock");
    if lock_path.exists() {
        fs::remove_file(&lock_path)
            .map_err(|e| format!("Failed to remove vault.lock: {}", e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn derive_key_consistent() {
        let salt = [1u8; 32];
        let k1 = derive_key("password123", &salt);
        let k2 = derive_key("password123", &salt);
        assert_eq!(k1, k2);
    }

    #[test]
    fn derive_key_different_passwords() {
        let salt = [1u8; 32];
        let k1 = derive_key("password1", &salt);
        let k2 = derive_key("password2", &salt);
        assert_ne!(k1, k2);
    }

    #[test]
    fn derive_key_different_salts() {
        let salt1 = [1u8; 32];
        let salt2 = [2u8; 32];
        let k1 = derive_key("password", &salt1);
        let k2 = derive_key("password", &salt2);
        assert_ne!(k1, k2);
    }

    #[test]
    fn verifier_correct_password() {
        let salt = generate_salt();
        let key = derive_key("correct-password", &salt);
        let (verifier, nonce) = create_verifier(&key).unwrap();
        assert!(verify_password(&key, &verifier, &nonce));
    }

    #[test]
    fn verifier_wrong_password() {
        let salt = generate_salt();
        let key = derive_key("correct-password", &salt);
        let (verifier, nonce) = create_verifier(&key).unwrap();

        let wrong_key = derive_key("wrong-password", &salt);
        assert!(!verify_password(&wrong_key, &verifier, &nonce));
    }

    #[test]
    fn encrypt_decrypt_file_roundtrip() {
        let tmp = std::env::temp_dir().join("tv_crypto_test_roundtrip");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let original = b"Hello, this is a test GPX file content!";
        let file_path = tmp.join("test.gpx");
        fs::write(&file_path, original).unwrap();

        let key = derive_key("test-password", &[42u8; 32]);

        // Encrypt
        let enc_path = encrypt_file(&key, &file_path).unwrap();
        assert!(enc_path.to_string_lossy().ends_with(".gpx.enc"));
        assert!(!file_path.exists()); // original removed
        assert!(enc_path.exists());

        // Encrypted content should differ from original
        let enc_data = fs::read(&enc_path).unwrap();
        assert_ne!(&enc_data[12..], original); // skip nonce

        // Decrypt
        let dec_path = decrypt_file(&key, &enc_path).unwrap();
        assert!(dec_path.to_string_lossy().ends_with(".gpx"));
        assert!(!enc_path.exists()); // enc file removed

        let restored = fs::read(&dec_path).unwrap();
        assert_eq!(restored, original);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn decrypt_file_to_memory_works() {
        let tmp = std::env::temp_dir().join("tv_crypto_test_memory");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let original = b"GPX file data for memory test";
        let file_path = tmp.join("mem_test.gpx");
        fs::write(&file_path, original).unwrap();

        let key = derive_key("mem-test-pw", &[99u8; 32]);
        let enc_path = encrypt_file(&key, &file_path).unwrap();

        let result = decrypt_file_to_memory(&key, &enc_path).unwrap();
        assert_eq!(result, original);

        // File still exists (not removed)
        assert!(enc_path.exists());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn encrypt_all_skips_already_encrypted() {
        let tmp = std::env::temp_dir().join("tv_crypto_test_bulk");
        let _ = fs::remove_dir_all(&tmp);
        let raw_dir = tmp.join("raw");
        fs::create_dir_all(&raw_dir).unwrap();

        // Create test files
        fs::write(raw_dir.join("file1.gpx"), b"gpx1").unwrap();
        fs::write(raw_dir.join("file2.fit"), b"fit2").unwrap();
        fs::write(raw_dir.join("file3.gpx.enc"), b"already encrypted").unwrap();

        let key = derive_key("bulk-test", &[7u8; 32]);
        let mut changes: Vec<(String, String)> = Vec::new();
        let count = encrypt_all_raw_files(&key, &tmp, &mut |old, new| {
            changes.push((old.to_string(), new.to_string()));
            Ok(())
        })
        .unwrap();

        assert_eq!(count, 2);
        assert_eq!(changes.len(), 2);
        // Deterministic (sorted) order.
        assert_eq!(changes[0].0, "raw/file1.gpx");
        assert_eq!(changes[1].0, "raw/file2.fit");
        // .enc file should be skipped
        assert!(raw_dir.join("file3.gpx.enc").exists());
        // original files should be encrypted
        assert!(raw_dir.join("file1.gpx.enc").exists());
        assert!(raw_dir.join("file2.fit.enc").exists());
        assert!(!raw_dir.join("file1.gpx").exists());
        assert!(!raw_dir.join("file2.fit").exists());

        let _ = fs::remove_dir_all(&tmp);
    }

    /// GCM is authenticated encryption: flipping any ciphertext byte must fail
    /// decryption outright, never yield garbage plaintext.
    #[test]
    fn tampered_ciphertext_fails() {
        let tmp = std::env::temp_dir().join("tv_crypto_test_tamper");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let file_path = tmp.join("t.gpx");
        fs::write(&file_path, b"top secret track").unwrap();
        let key = derive_key("pw", &[3u8; 32]);
        let enc_path = encrypt_file(&key, &file_path).unwrap();

        // Flip one bit in the last ciphertext byte (inside the GCM tag zone).
        let mut data = fs::read(&enc_path).unwrap();
        let last = data.len() - 1;
        data[last] ^= 0x01;
        fs::write(&enc_path, &data).unwrap();

        assert!(decrypt_file(&key, &enc_path).is_err());
        assert!(decrypt_file_to_memory(&key, &enc_path).is_err());
        // Failure must be non-destructive: .enc stays, no plaintext appears.
        assert!(enc_path.exists());
        assert!(!file_path.exists());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn tampered_nonce_fails() {
        let tmp = std::env::temp_dir().join("tv_crypto_test_tamper_nonce");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let file_path = tmp.join("t.gpx");
        fs::write(&file_path, b"top secret track").unwrap();
        let key = derive_key("pw", &[4u8; 32]);
        let enc_path = encrypt_file(&key, &file_path).unwrap();

        let mut data = fs::read(&enc_path).unwrap();
        data[0] ^= 0x01; // first nonce byte
        fs::write(&enc_path, &data).unwrap();

        assert!(decrypt_file(&key, &enc_path).is_err());
        assert!(decrypt_file_to_memory(&key, &enc_path).is_err());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn wrong_key_fails() {
        let tmp = std::env::temp_dir().join("tv_crypto_test_wrong_key");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let file_path = tmp.join("t.gpx");
        fs::write(&file_path, b"secret").unwrap();
        let key_a = derive_key("password-a", &[5u8; 32]);
        let key_b = derive_key("password-b", &[5u8; 32]);
        let enc_path = encrypt_file(&key_a, &file_path).unwrap();

        assert!(decrypt_file(&key_b, &enc_path).is_err());
        assert!(decrypt_file_to_memory(&key_b, &enc_path).is_err());
        assert!(enc_path.exists());
        assert!(!file_path.exists());
        // The right key still works after the failed attempts.
        assert_eq!(decrypt_file_to_memory(&key_a, &enc_path).unwrap(), b"secret");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn truncated_file_fails() {
        let tmp = std::env::temp_dir().join("tv_crypto_test_truncated");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let enc_path = tmp.join("short.gpx.enc");
        fs::write(&enc_path, b"12345").unwrap(); // shorter than a nonce

        let key = derive_key("pw", &[6u8; 32]);
        assert!(decrypt_file(&key, &enc_path).unwrap_err().contains("too short"));
        assert!(decrypt_file_to_memory(&key, &enc_path)
            .unwrap_err()
            .contains("too short"));

        let _ = fs::remove_dir_all(&tmp);
    }

    /// A vault.lock written before scopes existed (no `scopes` field) must
    /// still load, defaulting to activities-only — no silent data exposure.
    #[test]
    fn vault_lock_without_scopes_defaults_to_activities() {
        let tmp = std::env::temp_dir().join("tv_crypto_test_legacy_lock");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let legacy = r#"{"salt":"aa","verifier":"bb","nonce":"cc","created_at":"2025-01-01T00:00:00Z"}"#;
        fs::write(tmp.join("vault.lock"), legacy).unwrap();

        let loaded = read_vault_lock(&tmp).unwrap().unwrap();
        assert_eq!(loaded.scopes, EncryptionScopes::activities_only());
        assert!(loaded.scopes.activities);
        assert!(!loaded.scopes.database);
        assert!(!loaded.scopes.photos);

        let _ = fs::remove_dir_all(&tmp);
    }

    /// Mid-run failure during bulk encryption: files processed before the
    /// failure are fully paired with their callback (≙ DB update); the failing
    /// file and everything after stay untouched plaintext.
    #[test]
    fn bulk_encrypt_partial_failure_is_consistent() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = std::env::temp_dir().join("tv_crypto_test_partial_enc");
        let _ = fs::remove_dir_all(&tmp);
        let raw_dir = tmp.join("raw");
        fs::create_dir_all(&raw_dir).unwrap();

        fs::write(raw_dir.join("a.gpx"), b"first").unwrap();
        fs::write(raw_dir.join("b.gpx"), b"second").unwrap();
        // Sorted order guarantees `a` is processed first; an unreadable `b`
        // then fails the run.
        fs::set_permissions(raw_dir.join("b.gpx"), fs::Permissions::from_mode(0o000)).unwrap();

        let key = derive_key("partial", &[8u8; 32]);
        let mut changes: Vec<(String, String)> = Vec::new();
        let result = encrypt_all_raw_files(&key, &tmp, &mut |old, new| {
            changes.push((old.to_string(), new.to_string()));
            Ok(())
        });

        assert!(result.is_err());
        // `a` is encrypted AND reported; `b` is untouched and unreported.
        assert_eq!(changes, vec![("raw/a.gpx".into(), "raw/a.gpx.enc".into())]);
        assert!(raw_dir.join("a.gpx.enc").exists());
        assert!(!raw_dir.join("a.gpx").exists());
        assert!(raw_dir.join("b.gpx").exists());

        // Recovery: fix the file and rerun — resumes with the remaining one.
        fs::set_permissions(raw_dir.join("b.gpx"), fs::Permissions::from_mode(0o644)).unwrap();
        changes.clear();
        let count = encrypt_all_raw_files(&key, &tmp, &mut |old, new| {
            changes.push((old.to_string(), new.to_string()));
            Ok(())
        })
        .unwrap();
        assert_eq!(count, 1);
        assert_eq!(changes, vec![("raw/b.gpx".into(), "raw/b.gpx.enc".into())]);

        let _ = fs::remove_dir_all(&tmp);
    }

    /// Mid-run failure during bulk decryption (one corrupted .enc): the valid
    /// file before it is decrypted + reported, the corrupt one is left as-is.
    #[test]
    fn bulk_decrypt_partial_failure_is_consistent() {
        let tmp = std::env::temp_dir().join("tv_crypto_test_partial_dec");
        let _ = fs::remove_dir_all(&tmp);
        let raw_dir = tmp.join("raw");
        fs::create_dir_all(&raw_dir).unwrap();

        let key = derive_key("partial-dec", &[9u8; 32]);
        fs::write(raw_dir.join("a.gpx"), b"first").unwrap();
        encrypt_file(&key, &raw_dir.join("a.gpx")).unwrap();
        fs::write(raw_dir.join("b.gpx.enc"), b"12345").unwrap(); // corrupt: too short

        let mut changes: Vec<(String, String)> = Vec::new();
        let result = decrypt_all_raw_files(&key, &tmp, &mut |old, new| {
            changes.push((old.to_string(), new.to_string()));
            Ok(())
        });

        assert!(result.is_err());
        assert_eq!(changes, vec![("raw/a.gpx.enc".into(), "raw/a.gpx".into())]);
        assert_eq!(fs::read(raw_dir.join("a.gpx")).unwrap(), b"first");
        assert!(raw_dir.join("b.gpx.enc").exists());

        let _ = fs::remove_dir_all(&tmp);
    }

    /// Crash-drift repair: DB paths pointing at the "other" extension of a
    /// file that exists on disk get a fix; healthy and missing rows don't.
    #[test]
    fn reconcile_paths_fixes_extension_drift() {
        let tmp = std::env::temp_dir().join("tv_crypto_test_reconcile");
        let _ = fs::remove_dir_all(&tmp);
        let raw_dir = tmp.join("raw");
        fs::create_dir_all(&raw_dir).unwrap();

        fs::write(raw_dir.join("drifted.gpx.enc"), b"enc").unwrap(); // DB says plain
        fs::write(raw_dir.join("restored.gpx"), b"plain").unwrap(); // DB says .enc
        fs::write(raw_dir.join("healthy.gpx"), b"ok").unwrap(); // DB matches

        let db_paths = vec![
            "raw/drifted.gpx".to_string(),
            "raw/restored.gpx.enc".to_string(),
            "raw/healthy.gpx".to_string(),
            "raw/gone.gpx".to_string(), // missing entirely — nothing to fix
        ];
        let mut fixes = reconcile_paths(&tmp, &db_paths);
        fixes.sort();
        assert_eq!(
            fixes,
            vec![
                ("raw/drifted.gpx".to_string(), "raw/drifted.gpx.enc".to_string()),
                ("raw/restored.gpx.enc".to_string(), "raw/restored.gpx".to_string()),
            ]
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn vault_lock_roundtrip() {
        let tmp = std::env::temp_dir().join("tv_crypto_test_lock");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let lock = VaultLock {
            salt: "aabbccdd".to_string(),
            verifier: "eeff0011".to_string(),
            nonce: "112233".to_string(),
            created_at: "2025-06-01T00:00:00Z".to_string(),
            scopes: EncryptionScopes::activities_only(),
        };

        write_vault_lock(&tmp, &lock).unwrap();
        let loaded = read_vault_lock(&tmp).unwrap().unwrap();
        assert_eq!(loaded.salt, "aabbccdd");
        assert_eq!(loaded.verifier, "eeff0011");
        assert_eq!(loaded.scopes, EncryptionScopes::activities_only());

        remove_vault_lock(&tmp).unwrap();
        assert!(read_vault_lock(&tmp).unwrap().is_none());

        let _ = fs::remove_dir_all(&tmp);
    }
}
