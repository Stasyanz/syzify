use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use zip::write::FileOptions;
use zip::ZipArchive;
use zip::ZipWriter;

type ZipOptions = FileOptions<'static, ()>;

/// Create a vault backup as a zip archive.
/// Includes: vault.db + vault.lock + raw/ + photos/ + plugins/ directory contents.
///
/// The archive is written to a temporary `<dest>.part` file and only renamed
/// to `dest_path` after the central directory has been written and flushed.
/// This guarantees a partial/interrupted backup never leaves a corrupt file
/// at `dest_path`.
#[cfg(test)] // convenience wrapper reading the live vault.db; the app snapshots first
pub fn create_backup(vault_path: &Path, dest_path: &Path) -> Result<(), String> {
    create_backup_with_progress(vault_path, dest_path, vault_path, &|_, _| {})
}

/// Like [`create_backup`], but reports progress as `(processed_bytes, total_bytes)`.
/// The callback is throttled (~1% steps) and always fires once at 100% on success.
///
/// `db_source` is the directory whose `vault.db` (and non-empty `vault.db-wal`)
/// go into the archive. The live database runs in WAL mode, so copying
/// `vault.db` straight out of the vault while the connection is open would
/// miss every transaction still sitting in the WAL — or worse, race a
/// checkpoint into a torn, unopenable copy. The command therefore checkpoints
/// and copies the DB into a snapshot directory under the connection lock and
/// passes that here (see `backup_vault_core`); tests without a live connection
/// pass the vault itself.
pub fn create_backup_with_progress(
    vault_path: &Path,
    dest_path: &Path,
    db_source: &Path,
    progress: &dyn Fn(u64, u64),
) -> Result<(), String> {
    let tmp_path = part_path(dest_path);

    // Ensure no stale .part from a previous failed run.
    let _ = fs::remove_file(&tmp_path);

    // Run the actual work in a closure so we can always clean up the .part
    // file if anything fails before the final rename.
    let result = write_backup(vault_path, db_source, &tmp_path, progress);

    match result {
        Ok(()) => {
            fs::rename(&tmp_path, dest_path).map_err(|e| {
                let _ = fs::remove_file(&tmp_path);
                format!("Failed to finalize backup file: {}", e)
            })?;
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp_path);
            Err(e)
        }
    }
}

/// Throttled progress reporter measured in bytes.
struct Progress<'a> {
    processed: u64,
    last_emit: u64,
    total: u64,
    step: u64,
    cb: &'a dyn Fn(u64, u64),
}

impl<'a> Progress<'a> {
    fn new(total: u64, cb: &'a dyn Fn(u64, u64)) -> Self {
        // Emit at most ~100 times, but no more often than every 256 KiB.
        let step = (total / 100).max(256 * 1024);
        Progress {
            processed: 0,
            last_emit: 0,
            total,
            step,
            cb,
        }
    }

    fn add(&mut self, n: u64) {
        self.processed += n;
        if self.processed - self.last_emit >= self.step {
            self.last_emit = self.processed;
            (self.cb)(self.processed, self.total);
        }
    }
}

/// Write the full zip archive to `tmp_path`, flushing to disk before returning.
fn write_backup(
    vault_path: &Path,
    db_source: &Path,
    tmp_path: &Path,
    progress: &dyn Fn(u64, u64),
) -> Result<(), String> {
    let total = total_backup_bytes(vault_path, db_source);
    let mut prog = Progress::new(total, progress);

    let zip_file =
        File::create(tmp_path).map_err(|e| format!("Failed to create backup file: {}", e))?;
    let writer = BufWriter::new(zip_file);
    let mut zip = ZipWriter::new(writer);
    let options: ZipOptions = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(true);

    // 1. Add vault.db (from the checkpointed snapshot when the app calls us)
    let db_path = db_source.join("vault.db");
    if db_path.exists() {
        add_file_to_zip(&mut zip, &db_path, "vault.db", options, &mut prog)?;
    }
    // A LOCKED vault can't checkpoint (the live connection is a placeholder),
    // so a leftover WAL may hold committed transactions vault.db alone lacks.
    // Carry it; SQLite replays it on the first open after restore.
    let wal_path = db_source.join("vault.db-wal");
    if wal_path.is_file() && fs::metadata(&wal_path).map(|m| m.len() > 0).unwrap_or(false) {
        add_file_to_zip(&mut zip, &wal_path, "vault.db-wal", options, &mut prog)?;
    }

    // 2. Add vault.lock if it exists (encryption metadata)
    let lock_path = vault_path.join("vault.lock");
    if lock_path.exists() {
        add_file_to_zip(&mut zip, &lock_path, "vault.lock", options, &mut prog)?;
    }

    // 3. Add raw/ + photos/ files
    let raw_dir = vault_path.join("raw");
    if raw_dir.is_dir() {
        add_directory_to_zip(&mut zip, &raw_dir, "raw", options, &mut prog)?;
    }
    let photos_dir = vault_path.join("photos");
    if photos_dir.is_dir() {
        add_directory_to_zip(&mut zip, &photos_dir, "photos", options, &mut prog)?;
    }

    // 4. Add installed plugins (manifest + wasm) so they survive backup/restore.
    let plugins_dir = vault_path.join("plugins");
    if plugins_dir.is_dir() {
        add_directory_to_zip(&mut zip, &plugins_dir, "plugins", options, &mut prog)?;
    }

    let writer = zip
        .finish()
        .map_err(|e| format!("Failed to finalize zip: {}", e))?;
    // Flush the BufWriter and the underlying file so every byte (including the
    // central directory + EOCD) is on disk before the caller renames it.
    let file = writer
        .into_inner()
        .map_err(|e| format!("Failed to flush backup buffer: {}", e))?;
    file.sync_all()
        .map_err(|e| format!("Failed to flush backup to disk: {}", e))?;

    progress(total, total);
    Ok(())
}

/// Sum of bytes that will be read into the archive (used for progress totals).
fn total_backup_bytes(vault_path: &Path, db_source: &Path) -> u64 {
    let mut total = 0;
    for name in ["vault.db", "vault.db-wal"] {
        if let Ok(m) = fs::metadata(db_source.join(name)) {
            total += m.len();
        }
    }
    if let Ok(m) = fs::metadata(vault_path.join("vault.lock")) {
        total += m.len();
    }
    total += dir_size(&vault_path.join("raw"));
    total += dir_size(&vault_path.join("photos"));
    total += dir_size(&vault_path.join("plugins"));
    total
}

fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            // Don't follow symlinks (mirrors add_directory_to_zip): a link
            // cycle would recurse forever, and a link out of the vault would
            // count bytes the archive won't contain.
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                total += dir_size(&entry.path());
            } else if ft.is_file() {
                if let Ok(m) = entry.metadata() {
                    total += m.len();
                }
            }
        }
    }
    total
}

/// Append a single file to the zip, streaming its contents (no full in-memory copy).
fn add_file_to_zip(
    zip: &mut ZipWriter<BufWriter<File>>,
    src: &Path,
    zip_name: &str,
    options: ZipOptions,
    prog: &mut Progress,
) -> Result<(), String> {
    zip.start_file(zip_name, options)
        .map_err(|e| format!("Failed to add {} to zip: {}", zip_name, e))?;
    let mut f =
        File::open(src).map_err(|e| format!("Failed to open {}: {}", zip_name, e))?;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|e| format!("Failed to read {}: {}", zip_name, e))?;
        if n == 0 {
            break;
        }
        zip.write_all(&buf[..n])
            .map_err(|e| format!("Failed to write {}: {}", zip_name, e))?;
        prog.add(n as u64);
    }
    Ok(())
}

/// Build the temporary path used during writing (`<dest>.part`).
fn part_path(dest_path: &Path) -> PathBuf {
    let mut name = dest_path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".part");
    dest_path.with_file_name(name)
}

/// Declared-total compression ratio above which an archive is treated as a
/// zip bomb. Real vault data sits far below it (FIT/JPEG/SQLCipher ≈ 1:1,
/// GPX/SQLite ≈ 3–20:1); only crafted repeating data deflates past 100:1.
const MAX_DECLARED_TOTAL_RATIO: u64 = 100;
/// No ratio check below this declared total: small archives can legitimately
/// compress extremely well (a fresh, mostly-empty vault.db is near-zeros),
/// and an extraction this size can't fill a disk anyway.
const RATIO_CHECK_FLOOR_BYTES: u64 = 256 * 1024 * 1024;

/// The zip-bomb gate for restore: every import path caps what it reads, and
/// restore must too — extraction runs past the point of no return (live DB
/// closed, vault quarantined), where running the disk full strands the
/// session on a partial vault. Split out for testing with small limits.
fn ensure_backup_size(
    total_declared: u64,
    archive_bytes: u64,
    floor: u64,
    max_ratio: u64,
) -> Result<(), String> {
    if total_declared > floor && total_declared / archive_bytes.max(1) > max_ratio {
        return Err(format!(
            "Backup archive declares {} MiB of data from a {} MiB file — refusing to restore a suspicious archive",
            total_declared / (1024 * 1024),
            archive_bytes / (1024 * 1024),
        ));
    }
    Ok(())
}

/// Pre-flight check of a backup archive, WITHOUT extracting anything —
/// callers validate before closing the live DB connection so a bad archive
/// fails while the app is still fully functional. "Opens as a zip" is not
/// enough: any zip (a Runkeeper export, a photo album) passes that, and past
/// the point of no return it would kill the live DB, quarantine the vault and
/// boot an empty one. So every entry name must be extraction-safe, the
/// declared sizes must not be a zip bomb, AND the archive must actually be a
/// vault backup (carry vault.db at its root).
pub fn validate_backup(backup_path: &Path) -> Result<(), String> {
    validate_backup_with_limits(backup_path, RATIO_CHECK_FLOOR_BYTES, MAX_DECLARED_TOTAL_RATIO)
}

fn validate_backup_with_limits(
    backup_path: &Path,
    floor: u64,
    max_ratio: u64,
) -> Result<(), String> {
    let zip_file =
        File::open(backup_path).map_err(|e| format!("Failed to open backup file: {}", e))?;
    let archive_bytes = zip_file
        .metadata()
        .map_err(|e| format!("Failed to read backup file: {}", e))?
        .len();
    let mut archive =
        ZipArchive::new(zip_file).map_err(|e| format!("Invalid backup archive: {}", e))?;

    let mut has_db = false;
    let mut total_declared: u64 = 0;
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read zip entry: {}", e))?;
        let Some(rel) = entry.enclosed_name() else {
            return Err(format!("Unsafe path in backup archive: {}", entry.name()));
        };
        has_db = has_db || rel == Path::new("vault.db");
        total_declared = total_declared.saturating_add(entry.size());
    }
    if !has_db {
        return Err(
            "This zip is not a Syzify vault backup (no vault.db inside) — nothing was changed"
                .to_string(),
        );
    }
    ensure_backup_size(total_declared, archive_bytes, floor, max_ratio)
}

/// Everything a restore replaces — the current vault's data and its encryption
/// state. `vault.lock` moves WITH the ciphertext it unlocks (INV-1: the lock is
/// the only copy of the salt; separating them makes `.enc` files permanently
/// undecryptable). `.backup-snapshot` is a crashed backup's plaintext DB copy —
/// it belongs to the OLD vault, so it must not stay behind either. `tiles/`
/// (re-downloadable cache) stays put.
const QUARANTINE_ENTRIES: [&str; 9] = [
    "vault.db",
    "vault.db-wal",
    "vault.db-shm",
    "vault.db.migrating",
    "vault.lock",
    ".backup-snapshot",
    "raw",
    "photos",
    "plugins",
];

/// Move the current vault's data aside into a fresh `pre-restore-<timestamp>/`
/// directory (same-volume renames — no copying) before a restore extracts over
/// it. This is what keeps restore inside the encryption failure model:
///   - no stale `vault.lock` survives a plaintext backup, so boot can't settle
///     LOCKED over a plaintext DB and fail every unlock forever;
///   - the current salt leaves TOGETHER with the `.enc` files it decrypts
///     (INV-1), instead of being overwritten by the archive's different lock
///     while newer ciphertext stays behind, stranded;
///   - old raw/photos never mix with the archive's, so the restored DB and the
///     restored files agree.
/// Returns the quarantine dir, or None when there was nothing to move.
/// On a mid-move failure every completed rename is undone (best-effort) so the
/// caller restarts onto the original, intact vault.
pub fn quarantine_current_vault(vault_path: &Path) -> Result<Option<PathBuf>, String> {
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let mut quarantine = vault_path.join(format!("{}{}", PRE_RESTORE_PREFIX, stamp));
    // Two restores within one second: suffix instead of mixing into the first.
    let mut n = 1;
    while quarantine.exists() {
        n += 1;
        quarantine = vault_path.join(format!("{}{}-{}", PRE_RESTORE_PREFIX, stamp, n));
    }

    let mut moved: Vec<&str> = Vec::new();
    for name in QUARANTINE_ENTRIES {
        let src = vault_path.join(name);
        // symlink_metadata: move a symlink itself rather than following it.
        if fs::symlink_metadata(&src).is_err() {
            continue;
        }
        if moved.is_empty() {
            fs::create_dir_all(&quarantine)
                .map_err(|e| format!("Failed to create pre-restore dir: {}", e))?;
        }
        if let Err(e) = fs::rename(&src, quarantine.join(name)) {
            // Roll the completed renames back so the vault is whole again.
            for done in &moved {
                let _ = fs::rename(quarantine.join(done), vault_path.join(done));
            }
            let _ = fs::remove_dir(&quarantine);
            return Err(format!("Failed to move {} aside for restore: {}", name, e));
        }
        moved.push(name);
    }

    if moved.is_empty() {
        return Ok(None);
    }
    Ok(Some(quarantine))
}

/// Restore a vault from a backup zip archive by extracting into `vault_path`.
///
/// Callers MUST move the current vault contents aside first (see
/// [`quarantine_current_vault`]) — extracting over live data would leave a
/// stale `vault.lock`/`.enc` mix that breaks the encryption invariants.
pub fn restore_backup(vault_path: &Path, backup_path: &Path) -> Result<(), String> {
    let zip_file =
        File::open(backup_path).map_err(|e| format!("Failed to open backup file: {}", e))?;
    let mut archive =
        ZipArchive::new(zip_file).map_err(|e| format!("Invalid backup archive: {}", e))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read zip entry: {}", e))?;

        let entry_name = entry.name().to_string();

        // Security (zip-slip): enclosed_name() rejects absolute paths and any
        // `..` traversal. A plain `contains("..")` check is insufficient because
        // Path::join with an absolute entry silently discards the vault base.
        let rel = entry
            .enclosed_name()
            .ok_or_else(|| format!("Unsafe path in backup archive: {}", entry_name))?;

        let out_path = vault_path.join(&rel);
        // Defense in depth: the resolved path must stay inside the vault.
        if !out_path.starts_with(vault_path) {
            return Err(format!("Unsafe path in backup archive: {}", entry_name));
        }

        if entry.is_dir() {
            fs::create_dir_all(&out_path)
                .map_err(|e| format!("Failed to create dir {}: {}", entry_name, e))?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create parent dir: {}", e))?;
            }
            // Stream straight to disk: buffering the whole entry in memory
            // would let a forged central directory (or just a multi-GB
            // vault.db) exhaust RAM — the recurring `read_to_end`-on-zip bug.
            let declared = entry.size();
            let mut out = File::create(&out_path)
                .map_err(|e| format!("Failed to create {}: {}", entry_name, e))?;
            copy_entry_capped(&mut entry, &mut out, declared, &entry_name)?;
        }
    }

    Ok(())
}

/// Stream a zip entry to `out`, refusing to write past its declared size.
/// validate_backup already vetted the declared totals, but the header sizes
/// are attacker-controlled too — a deflate stream that keeps producing past
/// its own declaration is exactly the bomb the pre-flight can't see.
/// `take(declared + 1)` mirrors util::read_capped: bounded even when lied to.
fn copy_entry_capped(
    entry: &mut impl Read,
    out: &mut impl Write,
    declared: u64,
    entry_name: &str,
) -> Result<(), String> {
    let written = std::io::copy(&mut entry.take(declared.saturating_add(1)), out)
        .map_err(|e| format!("Failed to write {}: {}", entry_name, e))?;
    if written > declared {
        return Err(format!(
            "Zip entry {} produced more data than its header declares — refusing a forged archive",
            entry_name
        ));
    }
    Ok(())
}

/// Name prefix of the quarantine dirs created by [`quarantine_current_vault`].
pub(crate) const PRE_RESTORE_PREFIX: &str = "pre-restore-";

/// The `pre-restore-*` quarantine dirs currently inside the vault, by name.
/// Each holds a complete copy of a replaced vault that no encryption scope
/// manages — enable_encryption refuses while any exist, because a "fully
/// encrypted" vault carrying a plaintext copy of itself isn't encrypted at
/// all (the `.backup-snapshot` lesson, but for data the user chose to keep).
pub(crate) fn list_pre_restore_dirs(vault_path: &Path) -> Vec<String> {
    let mut dirs = Vec::new();
    if let Ok(entries) = fs::read_dir(vault_path) {
        for entry in entries.flatten() {
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let name = entry.file_name().to_string_lossy().to_string();
            if is_dir && name.starts_with(PRE_RESTORE_PREFIX) {
                dirs.push(name);
            }
        }
    }
    dirs.sort();
    dirs
}

fn add_directory_to_zip(
    zip: &mut ZipWriter<BufWriter<File>>,
    dir_path: &Path,
    prefix: &str,
    options: ZipOptions,
    prog: &mut Progress,
) -> Result<(), String> {
    let entries =
        fs::read_dir(dir_path).map_err(|e| format!("Failed to read dir {}: {}", prefix, e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Dir entry error: {}", e))?;
        let ft = entry
            .file_type()
            .map_err(|e| format!("Dir entry error: {}", e))?;
        // Skip symlinks rather than follow them: a link cycle would recurse
        // forever, and an out-of-vault target doesn't belong in the archive
        // (relocate refuses symlinked vaults for the same reason).
        if ft.is_symlink() {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let zip_path = format!("{}/{}", prefix, name);

        if ft.is_dir() {
            add_directory_to_zip(zip, &path, &zip_path, options, prog)?;
        } else {
            add_file_to_zip(zip, &path, &zip_path, options, prog)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn backup_and_restore_roundtrip() {
        let tmp = std::env::temp_dir().join("tv_backup_test");
        let vault = tmp.join("vault");
        let restored = tmp.join("restored");
        let backup_file = tmp.join("backup.zip");

        // Cleanup
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(vault.join("raw")).unwrap();
        fs::create_dir_all(vault.join("photos/thumbs")).unwrap();
        fs::create_dir_all(&restored).unwrap();

        // Create fake vault content
        fs::write(vault.join("vault.db"), b"fake database content").unwrap();
        fs::write(vault.join("raw/file1.fit"), b"fit data 1").unwrap();
        fs::write(vault.join("raw/file2.gpx"), b"gpx data 2").unwrap();
        fs::write(vault.join("photos/abc.jpg"), b"jpeg bytes").unwrap();
        fs::write(vault.join("photos/thumbs/abc.jpg"), b"thumb bytes").unwrap();

        // Backup
        create_backup(&vault, &backup_file).unwrap();
        assert!(backup_file.exists());

        // Restore
        restore_backup(&restored, &backup_file).unwrap();

        // Verify
        assert_eq!(
            fs::read_to_string(restored.join("vault.db")).unwrap(),
            "fake database content"
        );
        assert_eq!(
            fs::read_to_string(restored.join("raw/file1.fit")).unwrap(),
            "fit data 1"
        );
        assert_eq!(
            fs::read_to_string(restored.join("raw/file2.gpx")).unwrap(),
            "gpx data 2"
        );
        // Photos (incl. thumbnails) survive the roundtrip — a backup that
        // restores a DB full of photo rows without the files is not a backup.
        assert_eq!(
            fs::read_to_string(restored.join("photos/abc.jpg")).unwrap(),
            "jpeg bytes"
        );
        assert_eq!(
            fs::read_to_string(restored.join("photos/thumbs/abc.jpg")).unwrap(),
            "thumb bytes"
        );

        // Cleanup
        let _ = fs::remove_dir_all(&tmp);
    }

    /// The stale-lock lockout and INV-1 scenarios: restoring a PLAINTEXT
    /// backup over an ENCRYPTED vault must not leave the old vault.lock (or
    /// any old file) behind — everything moves to the quarantine dir, the old
    /// salt staying together with the `.enc` files it decrypts.
    #[test]
    fn quarantine_moves_lock_with_its_ciphertext() {
        let tmp = std::env::temp_dir().join(format!("tv_quarantine_{}", std::process::id()));
        let vault = tmp.join("vault");
        let backup_file = tmp.join("backup.zip");
        let _ = fs::remove_dir_all(&tmp);

        // A plaintext backup (no vault.lock).
        let src = tmp.join("src");
        make_vault(&src);
        create_backup(&src, &backup_file).unwrap();

        // The current vault is encrypted: lock + .enc ciphertext newer than
        // the backup.
        fs::create_dir_all(vault.join("raw")).unwrap();
        fs::create_dir_all(vault.join("photos")).unwrap();
        fs::write(vault.join("vault.db"), b"sqlcipher gibberish").unwrap();
        fs::write(vault.join("vault.lock"), b"{\"salt\":\"the only copy\"}").unwrap();
        fs::write(vault.join("raw/newer.gpx.enc"), b"ciphertext").unwrap();
        fs::write(vault.join("photos/pic.jpg.enc"), b"ciphertext").unwrap();
        // A crashed backup's plaintext DB copy belongs to the OLD vault too.
        fs::create_dir_all(vault.join(".backup-snapshot")).unwrap();
        fs::write(vault.join(".backup-snapshot/vault.db"), b"plaintext copy").unwrap();

        let quarantine = quarantine_current_vault(&vault).unwrap().expect("dir");
        restore_backup(&vault, &backup_file).unwrap();

        // No stale lock: boot must see the restored (plaintext) state only —
        // a leftover lock would make every unlock fail forever.
        assert!(!vault.join("vault.lock").exists());
        // No old/new mixing: the restored raw/ is purely the archive's.
        assert!(!vault.join("raw/newer.gpx.enc").exists());
        assert_eq!(
            fs::read_to_string(vault.join("vault.db")).unwrap(),
            "fake database content"
        );

        // INV-1: the old salt lives in quarantine TOGETHER with its ciphertext.
        assert_eq!(
            fs::read_to_string(quarantine.join("vault.lock")).unwrap(),
            "{\"salt\":\"the only copy\"}"
        );
        assert!(quarantine.join("raw/newer.gpx.enc").exists());
        assert!(quarantine.join("photos/pic.jpg.enc").exists());
        assert!(quarantine.join("vault.db").exists());
        assert!(
            quarantine.join(".backup-snapshot/vault.db").exists(),
            "a stale snapshot must not stay behind in the restored vault"
        );
        assert!(!vault.join(".backup-snapshot").exists());

        let _ = fs::remove_dir_all(&tmp);
    }

    /// Symlinks in the vault are skipped, not followed: a link cycle used to
    /// recurse add_directory_to_zip/dir_size forever (stack overflow
    /// mid-backup), and a link out of the vault pulled foreign data in.
    #[cfg(unix)]
    #[test]
    fn backup_skips_symlinks_instead_of_following_them() {
        use std::os::unix::fs::symlink;

        let tmp = std::env::temp_dir().join(format!("tv_backup_symlink_{}", std::process::id()));
        let vault = tmp.join("vault");
        let backup_file = tmp.join("backup.zip");
        let _ = fs::remove_dir_all(&tmp);
        make_vault(&vault);

        // A cycle (raw/loop → raw) and an out-of-vault link.
        let outside = tmp.join("outside.txt");
        fs::write(&outside, b"not vault data").unwrap();
        symlink(vault.join("raw"), vault.join("raw/loop")).unwrap();
        symlink(&outside, vault.join("raw/leak.txt")).unwrap();

        create_backup(&vault, &backup_file).unwrap();

        let f = File::open(&backup_file).unwrap();
        let mut archive = ZipArchive::new(f).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.contains(&"raw/file1.fit".to_string()));
        assert!(
            !names.iter().any(|n| n.contains("loop") || n.contains("leak")),
            "symlinked entries must not be archived: {:?}",
            names
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    /// dir_size must mirror the archive walk: symlinks contribute nothing.
    /// Before the fix a cycle recursed forever and an out-of-vault link
    /// inflated the progress total past what the archive would contain.
    #[cfg(unix)]
    #[test]
    fn dir_size_ignores_symlinks() {
        use std::os::unix::fs::symlink;

        let tmp = std::env::temp_dir().join(format!("tv_dirsize_{}", std::process::id()));
        let raw = tmp.join("raw");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&raw).unwrap();
        fs::write(raw.join("a.fit"), vec![1u8; 100]).unwrap();

        let plain = dir_size(&raw);
        assert_eq!(plain, 100);

        let outside = tmp.join("huge.bin");
        fs::write(&outside, vec![0u8; 10_000]).unwrap();
        symlink(&outside, raw.join("huge.bin")).unwrap();
        symlink(&raw, raw.join("loop")).unwrap();

        assert_eq!(dir_size(&raw), plain, "symlinks must not add bytes or recurse");

        let _ = fs::remove_dir_all(&tmp);
    }

    /// A fresh/empty vault has nothing to preserve: no quarantine dir appears.
    #[test]
    fn quarantine_on_empty_vault_is_none() {
        let tmp = std::env::temp_dir().join(format!("tv_quarantine_empty_{}", std::process::id()));
        let vault = tmp.join("vault");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&vault).unwrap();

        assert_eq!(quarantine_current_vault(&vault).unwrap(), None);
        // Nothing created either.
        assert_eq!(fs::read_dir(&vault).unwrap().count(), 0);

        let _ = fs::remove_dir_all(&tmp);
    }

    fn make_vault(dir: &Path) {
        fs::create_dir_all(dir.join("raw")).unwrap();
        fs::write(dir.join("vault.db"), b"fake database content").unwrap();
        fs::write(dir.join("raw/file1.fit"), b"fit data 1").unwrap();
    }

    #[test]
    fn backup_produces_valid_archive() {
        let tmp = std::env::temp_dir().join("tv_backup_valid");
        let vault = tmp.join("vault");
        let backup_file = tmp.join("backup.zip");
        let _ = fs::remove_dir_all(&tmp);
        make_vault(&vault);

        create_backup(&vault, &backup_file).unwrap();

        // Opening with ZipArchive succeeds only if the central directory + EOCD
        // were written. This is the exact failure mode of the original bug.
        let f = File::open(&backup_file).unwrap();
        let mut archive = ZipArchive::new(f).expect("archive must have a valid central directory");
        assert!(archive.len() >= 2);
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.contains(&"vault.db".to_string()));
        assert!(names.contains(&"raw/file1.fit".to_string()));

        // No leftover .part next to the final file.
        assert!(!part_path(&backup_file).exists());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn progress_reaches_total() {
        use std::cell::Cell;

        let tmp = std::env::temp_dir().join("tv_backup_progress");
        let vault = tmp.join("vault");
        let backup_file = tmp.join("backup.zip");
        let _ = fs::remove_dir_all(&tmp);
        make_vault(&vault);
        // Add more raw bytes so total > 0 and the callback fires.
        fs::write(vault.join("raw/big.fit"), vec![7u8; 1_000_000]).unwrap();

        let last = Cell::new((0u64, 0u64));
        let calls = Cell::new(0u32);
        let cb = |processed: u64, total: u64| {
            last.set((processed, total));
            calls.set(calls.get() + 1);
        };

        create_backup_with_progress(&vault, &backup_file, &vault, &cb).unwrap();

        let (processed, total) = last.get();
        assert!(total > 0, "total bytes must be computed");
        assert_eq!(processed, total, "final progress must reach 100%");
        assert!(calls.get() >= 1, "callback must fire at least once");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn restore_rejects_zip_slip_paths() {
        use std::io::Write as _;
        use zip::write::FileOptions;

        let tmp = std::env::temp_dir().join(format!("tv_zipslip_{}", std::process::id()));
        let vault = tmp.join("vault");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&vault).unwrap();

        // Craft a malicious archive containing an absolute path. A naive
        // `contains("..")` filter would not catch this, and Path::join with an
        // absolute path would write outside the vault.
        let escape_target = tmp.join("pwned.txt");
        let evil_path = escape_target.to_str().unwrap().to_string();

        let backup = tmp.join("evil.zip");
        {
            let f = File::create(&backup).unwrap();
            let mut zip = ZipWriter::new(f);
            let opts: FileOptions<'_, ()> = FileOptions::default();
            zip.start_file(&evil_path, opts).unwrap();
            zip.write_all(b"owned").unwrap();
            zip.finish().unwrap();
        }

        let result = restore_backup(&vault, &backup);
        assert!(result.is_err(), "restore must reject unsafe paths");
        assert!(
            !escape_target.exists(),
            "file must NOT be written outside the vault"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn failed_backup_leaves_no_dest_or_part() {
        let tmp = std::env::temp_dir().join("tv_backup_fail");
        let vault = tmp.join("vault");
        let _ = fs::remove_dir_all(&tmp);
        make_vault(&vault);

        // Destination inside a non-existent directory -> writing the .part fails.
        let dest = tmp.join("missing_dir").join("backup.zip");
        let result = create_backup(&vault, &dest);

        assert!(result.is_err());
        assert!(!dest.exists(), "failed backup must not leave a dest file");
        assert!(
            !part_path(&dest).exists(),
            "failed backup must not leave a .part file"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    /// validate_backup accepts a real archive and rejects a bad/missing one —
    /// this is the pre-flight the restore command runs BEFORE closing the live
    /// DB connection, so a bad archive fails while the app is still working.
    #[test]
    fn validate_backup_accepts_valid_rejects_garbage() {
        let tmp = std::env::temp_dir().join("tv_validate_backup");
        let vault = tmp.join("vault");
        let good = tmp.join("good.zip");
        let bad = tmp.join("bad.zip");
        let _ = fs::remove_dir_all(&tmp);
        make_vault(&vault);
        create_backup(&vault, &good).unwrap();
        fs::write(&bad, b"not a zip at all").unwrap();

        assert!(validate_backup(&good).is_ok());
        assert!(validate_backup(&bad).is_err());
        assert!(validate_backup(&tmp.join("missing.zip")).is_err());

        let _ = fs::remove_dir_all(&tmp);
    }

    /// "Opens as a zip" is not enough: restoring a random zip (a Runkeeper
    /// export, a photo album) must fail at pre-flight — past the point of no
    /// return it would kill the live DB and boot an empty vault. And unsafe
    /// entry names must be rejected BEFORE extraction, not during it.
    #[test]
    fn validate_rejects_non_vault_and_zip_slip_archives() {
        use std::io::Write as _;
        use zip::write::FileOptions;

        let tmp = std::env::temp_dir().join(format!("tv_validate_deep_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let opts: FileOptions<'_, ()> = FileOptions::default();

        // A perfectly valid zip that is not a vault backup (no vault.db).
        let not_a_vault = tmp.join("runkeeper.zip");
        {
            let mut zip = ZipWriter::new(File::create(&not_a_vault).unwrap());
            zip.start_file("cardioActivities.csv", opts).unwrap();
            zip.write_all(b"Date,Type\n").unwrap();
            zip.finish().unwrap();
        }
        let err = validate_backup(&not_a_vault).unwrap_err();
        assert!(err.contains("not a Syzify vault backup"), "got: {}", err);

        // vault.db present but alongside an absolute-path entry: pre-flight
        // must catch it (extraction-time catching is only defense in depth).
        let slippery = tmp.join("slippery.zip");
        {
            let mut zip = ZipWriter::new(File::create(&slippery).unwrap());
            zip.start_file("vault.db", opts).unwrap();
            zip.write_all(b"db").unwrap();
            zip.start_file(tmp.join("pwned.txt").to_str().unwrap(), opts).unwrap();
            zip.write_all(b"owned").unwrap();
            zip.finish().unwrap();
        }
        let err = validate_backup(&slippery).unwrap_err();
        assert!(err.contains("Unsafe path"), "got: {}", err);

        let _ = fs::remove_dir_all(&tmp);
    }

    /// The restore zip-bomb gate: a huge declared total from a small archive
    /// is rejected (extraction would run the disk full PAST the point of no
    /// return), while ratios below the cap and totals below the floor pass —
    /// a fresh near-empty vault.db legitimately deflates extremely well.
    #[test]
    fn ensure_backup_size_rejects_bombs_only() {
        // Below the floor: any ratio is fine.
        assert!(ensure_backup_size(1000, 1, 1024, 100).is_ok());
        // Above the floor, sane ratio: fine.
        assert!(ensure_backup_size(2048, 100, 1024, 100).is_ok());
        // Above the floor, bomb ratio: rejected (and no div-by-zero on an
        // empty file).
        assert!(ensure_backup_size(2048, 10, 1024, 100).is_err());
        assert!(ensure_backup_size(2048, 0, 1024, 100).is_err());
    }

    /// End-to-end through the pre-flight: an archive that carries vault.db
    /// (so the has_db check passes) plus a highly-compressible bomb entry
    /// must fail validation with small test limits.
    #[test]
    fn validate_rejects_a_zip_bomb_archive() {
        use std::io::Write as _;
        use zip::write::FileOptions;

        let tmp = std::env::temp_dir().join(format!("tv_validate_bomb_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let bomb = tmp.join("bomb.zip");
        {
            let mut zip = ZipWriter::new(File::create(&bomb).unwrap());
            let opts: FileOptions<'_, ()> = FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("vault.db", opts).unwrap();
            zip.write_all(b"db").unwrap();
            // 4 MiB of zeros deflate to a few KiB — a bomb-grade ratio.
            zip.start_file("raw/bomb.gpx", opts).unwrap();
            let zeros = vec![0u8; 64 * 1024];
            for _ in 0..64 {
                zip.write_all(&zeros).unwrap();
            }
            zip.finish().unwrap();
        }

        let err =
            validate_backup_with_limits(&bomb, 1024 * 1024, MAX_DECLARED_TOTAL_RATIO).unwrap_err();
        assert!(err.contains("refusing to restore"), "got: {}", err);
        // The same archive passes with the real floor (4 MiB is harmless).
        assert!(validate_backup(&bomb).is_ok());

        let _ = fs::remove_dir_all(&tmp);
    }

    /// Header sizes are attacker-controlled: an entry whose stream produces
    /// more bytes than it declares must stop AT the declaration, not run the
    /// disk full during extraction.
    #[test]
    fn copy_entry_capped_stops_at_the_declared_size() {
        let data = [7u8; 100];

        let mut out = Vec::new();
        copy_entry_capped(&mut &data[..], &mut out, 100, "ok.bin").unwrap();
        assert_eq!(out.len(), 100);

        let mut out = Vec::new();
        let err = copy_entry_capped(&mut &data[..], &mut out, 99, "forged.bin").unwrap_err();
        assert!(err.contains("forged"), "got: {}", err);
    }

    /// list_pre_restore_dirs sees exactly the quarantine dirs — not files
    /// with the prefix, not other vault entries.
    #[test]
    fn pre_restore_dirs_are_listed() {
        let tmp = std::env::temp_dir().join(format!("tv_prerestore_list_{}", std::process::id()));
        let vault = tmp.join("vault");
        let _ = fs::remove_dir_all(&tmp);
        make_vault(&vault);

        assert!(list_pre_restore_dirs(&vault).is_empty());

        fs::create_dir_all(vault.join("pre-restore-20260708-120000")).unwrap();
        fs::create_dir_all(vault.join("pre-restore-20260708-120000-2")).unwrap();
        fs::write(vault.join("pre-restore-notes.txt"), b"just a file").unwrap();

        assert_eq!(
            list_pre_restore_dirs(&vault),
            vec![
                "pre-restore-20260708-120000".to_string(),
                "pre-restore-20260708-120000-2".to_string()
            ]
        );

        // And the dirs quarantine itself creates are found by the listing.
        fs::write(vault.join("vault.db"), b"db").unwrap();
        let q = quarantine_current_vault(&vault).unwrap().expect("dir");
        assert!(list_pre_restore_dirs(&vault)
            .contains(&q.file_name().unwrap().to_string_lossy().to_string()));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn stale_part_is_replaced_by_successful_backup() {
        let tmp = std::env::temp_dir().join("tv_backup_stale");
        let vault = tmp.join("vault");
        let backup_file = tmp.join("backup.zip");
        let _ = fs::remove_dir_all(&tmp);
        make_vault(&vault);

        // Simulate a corrupt leftover from a previously interrupted run.
        fs::create_dir_all(&tmp).unwrap();
        fs::write(part_path(&backup_file), b"garbage from a crashed run").unwrap();

        create_backup(&vault, &backup_file).unwrap();

        assert!(!part_path(&backup_file).exists());
        let f = File::open(&backup_file).unwrap();
        ZipArchive::new(f).expect("archive must be valid after replacing stale .part");

        let _ = fs::remove_dir_all(&tmp);
    }
}
