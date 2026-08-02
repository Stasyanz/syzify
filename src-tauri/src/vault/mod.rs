//! Vault location: where the vault lives and how it moves.
//!
//! The vault root is stored in a small marker file in the app config dir
//! (outside the vault — it must survive the vault moving). No marker file
//! means the default `~/Syzify`. Relocation moves the whole vault directory
//! to a user-picked destination: a same-volume `rename` when possible, else
//! a copy with byte progress; the marker file is only rewritten once the
//! files are safely at the new root.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Marker file (app config dir) holding the absolute vault root path.
const LOCATION_FILE: &str = "vault-location";

/// Read the configured vault root, if any.
pub fn read_location(config_dir: &Path) -> Option<PathBuf> {
    let raw = fs::read_to_string(config_dir.join(LOCATION_FILE)).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = PathBuf::from(trimmed);
    path.is_absolute().then_some(path)
}

/// Persist the vault root for the next launch.
pub fn write_location(config_dir: &Path, vault_root: &Path) -> Result<(), String> {
    fs::create_dir_all(config_dir)
        .map_err(|e| format!("Failed to create config dir: {}", e))?;
    fs::write(
        config_dir.join(LOCATION_FILE),
        vault_root.to_string_lossy().as_bytes(),
    )
    .map_err(|e| format!("Failed to save vault location: {}", e))
}

/// Where the vault ends up for a user-picked destination directory: an empty
/// (or missing) pick becomes the vault root itself; a non-empty pick gets a
/// `Syzify` subfolder, which in turn must be empty or missing.
pub fn resolve_target_root(dest: &Path) -> Result<PathBuf, String> {
    if is_missing_or_empty(dest)? {
        return Ok(dest.to_path_buf());
    }
    let sub = dest.join("Syzify");
    if is_missing_or_empty(&sub)? {
        return Ok(sub);
    }
    Err(format!(
        "\"{}\" already exists and is not empty",
        sub.display()
    ))
}

/// Resolve the vault root for SWITCHING (no data is moved, unlike relocate):
/// a folder that already contains a vault.db is opened as-is; anything else
/// follows the relocate rules (empty/missing dir, or its `Syzify` subfolder),
/// and the boot path creates a fresh vault there.
pub fn resolve_switch_root(dest: &Path) -> Result<PathBuf, String> {
    if dest.join("vault.db").is_file() {
        return Ok(dest.to_path_buf());
    }
    resolve_target_root(dest)
}

fn is_missing_or_empty(path: &Path) -> Result<bool, String> {
    if !path.exists() {
        return Ok(true);
    }
    if !path.is_dir() {
        return Err(format!("\"{}\" is not a folder", path.display()));
    }
    let mut entries = fs::read_dir(path)
        .map_err(|e| format!("Failed to read \"{}\": {}", path.display(), e))?;
    Ok(entries.next().is_none())
}

/// Move the vault from `src` to the target root resolved from `dest_pick`,
/// then persist the new location. Returns the new vault root.
///
/// Ordering is crash-safe: the source is only deleted after the copy fully
/// succeeded AND the marker file points at the new root. A failed copy
/// removes the partial target and leaves the source untouched; a failed
/// marker write rolls the move back.
pub fn relocate(
    src: &Path,
    dest_pick: &Path,
    config_dir: &Path,
    progress: &dyn Fn(u64, u64),
) -> Result<PathBuf, String> {
    let target = resolve_target_root(dest_pick)?;

    if target == src {
        return Err("The vault is already at this location".into());
    }
    if target.starts_with(src) {
        return Err("Cannot move the vault into itself".into());
    }
    if src.starts_with(&target) {
        return Err("The destination is inside the current vault".into());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create destination: {}", e))?;
    }
    // resolve_target_root allows an existing-but-empty dir; rename needs the
    // target itself to not exist.
    if target.exists() {
        fs::remove_dir(&target)
            .map_err(|e| format!("Failed to prepare destination: {}", e))?;
    }

    // Symlinks make the move ambiguous: the same-volume rename would carry
    // them along as links, while the cross-volume copy can't preserve them
    // portably — the result would silently depend on which path ran. A vault
    // never creates symlinks itself, so refuse a foreign one up front.
    if let Some(link) = find_symlink(src) {
        return Err(format!(
            "The vault contains a symbolic link (\"{}\"). Remove or replace it with the real file, then try again",
            link.display()
        ));
    }

    let total = dir_size(src);

    // Fast path: same volume.
    if fs::rename(src, &target).is_ok() {
        if let Err(e) = write_location(config_dir, &target) {
            // Roll back so the marker and the files never disagree.
            let _ = fs::rename(&target, src);
            return Err(e);
        }
        progress(total, total);
        return Ok(target);
    }

    // Cross-volume: copy, persist the marker, then delete the source.
    let mut processed = 0u64;
    if let Err(e) = copy_dir_recursive(src, &target, total, &mut processed, progress) {
        let _ = fs::remove_dir_all(&target);
        return Err(e);
    }
    if let Err(e) = write_location(config_dir, &target) {
        let _ = fs::remove_dir_all(&target);
        return Err(e);
    }
    // Best effort: the vault is already safe (and authoritative) at the new
    // root; leftovers at the old path are cleaned up but never fatal.
    let _ = fs::remove_dir_all(src);
    Ok(target)
}

fn copy_dir_recursive(
    src: &Path,
    dst: &Path,
    total: u64,
    processed: &mut u64,
    progress: &dyn Fn(u64, u64),
) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| copy_err(dst, &e))?;
    let entries = fs::read_dir(src).map_err(|e| copy_err(src, &e))?;
    for entry in entries {
        let entry = entry.map_err(|e| copy_err(src, &e))?;
        // file_type() does NOT follow symlinks (unlike Path::is_dir).
        // `relocate` already rejects vaults containing symlinks; this skip is
        // defense in depth so a link appearing mid-copy can't send the
        // recursion into a loop.
        let ft = entry.file_type().map_err(|e| copy_err(&entry.path(), &e))?;
        if ft.is_symlink() {
            continue;
        }
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_dir_recursive(&from, &to, total, processed, progress)?;
        } else {
            let bytes = fs::copy(&from, &to).map_err(|e| copy_err(&from, &e))?;
            *processed += bytes;
            // `total` is a best-effort snapshot taken before the copy; never
            // report past it (files may grow or appear in between).
            progress((*processed).min(total), total);
        }
    }
    Ok(())
}

fn copy_err(path: &Path, e: &io::Error) -> String {
    format!("Failed to copy \"{}\": {}", path.display(), e)
}

/// First symlink anywhere under `dir` (without following links), or None.
/// Unreadable entries are ignored — the copy itself will surface real errors.
fn find_symlink(dir: &Path) -> Option<PathBuf> {
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_symlink() {
            return Some(entry.path());
        }
        if ft.is_dir() {
            if let Some(found) = find_symlink(&entry.path()) {
                return Some(found);
            }
        }
    }
    None
}

fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            // Skip symlinks (don't follow) to match copy_dir_recursive and
            // avoid loops / double-counting.
            match entry.file_type() {
                Ok(ft) if ft.is_symlink() => continue,
                Ok(ft) if ft.is_dir() => total += dir_size(&entry.path()),
                Ok(_) => {
                    if let Ok(m) = entry.metadata() {
                        total += m.len();
                    }
                }
                // Classification failed: still count the bytes if metadata
                // works, so a copy that succeeds on this entry (it hard-fails
                // on the same error) can't outrun the reported total.
                Err(_) => total += entry.metadata().map(|m| m.len()).unwrap_or(0),
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fresh scratch dir per test (mirrors vault_backup's test setup — the
    /// project deliberately has no tempdir dev-dependency).
    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("syz_vault_{}", name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_vault(root: &Path) {
        fs::create_dir_all(root.join("raw")).unwrap();
        fs::create_dir_all(root.join("photos/2026")).unwrap();
        fs::write(root.join("vault.db"), b"db-bytes").unwrap();
        fs::write(root.join("raw/a.gpx"), b"<gpx/>").unwrap();
        fs::write(root.join("photos/2026/p.jpg"), b"jpeg").unwrap();
    }

    fn assert_vault_contents(root: &Path) {
        assert_eq!(fs::read(root.join("vault.db")).unwrap(), b"db-bytes");
        assert_eq!(fs::read(root.join("raw/a.gpx")).unwrap(), b"<gpx/>");
        assert_eq!(fs::read(root.join("photos/2026/p.jpg")).unwrap(), b"jpeg");
    }

    #[test]
    fn location_roundtrip_and_defaults() {
        let tmp = test_dir("location");
        let cfg = tmp.join("cfg");
        assert_eq!(read_location(&cfg), None);
        write_location(&cfg, Path::new("/some/where")).unwrap();
        assert_eq!(read_location(&cfg), Some(PathBuf::from("/some/where")));
        // Blank or relative contents are ignored.
        fs::write(cfg.join(LOCATION_FILE), "  \n").unwrap();
        assert_eq!(read_location(&cfg), None);
        fs::write(cfg.join(LOCATION_FILE), "relative/path").unwrap();
        assert_eq!(read_location(&cfg), None);
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn target_root_uses_empty_dir_or_syzify_subfolder() {
        let tmp = test_dir("target_root");
        let empty = tmp.join("empty");
        fs::create_dir(&empty).unwrap();
        assert_eq!(resolve_target_root(&empty).unwrap(), empty);

        let missing = tmp.join("missing");
        assert_eq!(resolve_target_root(&missing).unwrap(), missing);

        let busy = tmp.join("busy");
        fs::create_dir(&busy).unwrap();
        fs::write(busy.join("x.txt"), b"x").unwrap();
        assert_eq!(resolve_target_root(&busy).unwrap(), busy.join("Syzify"));

        fs::create_dir(busy.join("Syzify")).unwrap();
        fs::write(busy.join("Syzify/y.txt"), b"y").unwrap();
        assert!(resolve_target_root(&busy).is_err());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn switch_root_prefers_existing_vault_over_relocate_rules() {
        let tmp = test_dir("switch_root");
        // A folder holding a vault.db is used as-is, even though it is
        // non-empty (relocate rules would divert to a Syzify subfolder).
        let existing = tmp.join("existing");
        fs::create_dir(&existing).unwrap();
        fs::write(existing.join("vault.db"), b"db").unwrap();
        assert_eq!(resolve_switch_root(&existing).unwrap(), existing);

        // No vault.db → relocate rules: empty dir itself, busy dir's subfolder.
        let empty = tmp.join("empty");
        fs::create_dir(&empty).unwrap();
        assert_eq!(resolve_switch_root(&empty).unwrap(), empty);

        let busy = tmp.join("busy");
        fs::create_dir(&busy).unwrap();
        fs::write(busy.join("x.txt"), b"x").unwrap();
        assert_eq!(resolve_switch_root(&busy).unwrap(), busy.join("Syzify"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn relocate_moves_vault_and_writes_marker() {
        let tmp = test_dir("relocate");
        let src = tmp.join("vault");
        let dest = tmp.join("new-home");
        let cfg = tmp.join("cfg");
        make_vault(&src);

        let target = relocate(&src, &dest, &cfg, &|_, _| {}).unwrap();
        assert_eq!(target, dest);
        assert!(!src.exists());
        assert_vault_contents(&target);
        assert_eq!(read_location(&cfg), Some(target));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn relocate_rejects_nested_destinations() {
        let tmp = test_dir("nested");
        let src = tmp.join("vault");
        let cfg = tmp.join("cfg");
        make_vault(&src);

        assert!(relocate(&src, &src.join("photos"), &cfg, &|_, _| {}).is_err());
        assert!(relocate(&src, &src, &cfg, &|_, _| {}).is_err());
        // Untouched on failure.
        assert_vault_contents(&src);
        assert_eq!(read_location(&cfg), None);
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn copy_path_reports_progress_and_preserves_tree() {
        let tmp = test_dir("copy");
        let src = tmp.join("vault");
        let dst = tmp.join("copy");
        make_vault(&src);

        let total = dir_size(&src);
        assert!(total > 0);
        let seen = std::cell::RefCell::new(Vec::new());
        let mut processed = 0u64;
        copy_dir_recursive(&src, &dst, total, &mut processed, &|p, t| {
            seen.borrow_mut().push((p, t));
        })
        .unwrap();
        assert_eq!(processed, total);
        assert_eq!(seen.borrow().last().copied(), Some((total, total)));
        assert_vault_contents(&dst);
        let _ = fs::remove_dir_all(&tmp);
    }

    /// Progress must never overshoot the reported total, even when the size
    /// snapshot went stale (files appeared/grew between the scan and the copy).
    #[test]
    fn copy_progress_never_exceeds_total() {
        let tmp = test_dir("progress_clamp");
        let src = tmp.join("vault");
        let dst = tmp.join("copy");
        make_vault(&src);

        // Simulate a stale snapshot: pretend the vault is 1 byte.
        let stale_total = 1u64;
        let mut processed = 0u64;
        let seen = std::cell::RefCell::new(Vec::new());
        copy_dir_recursive(&src, &dst, stale_total, &mut processed, &|p, t| {
            seen.borrow_mut().push((p, t));
        })
        .unwrap();
        assert!(processed > stale_total);
        assert!(seen.borrow().iter().all(|&(p, t)| p <= t));
        assert_vault_contents(&dst);
        let _ = fs::remove_dir_all(&tmp);
    }

    /// Relocation refuses a vault containing a symlink: the rename path would
    /// keep it while the copy path can't — better to fail up front than to
    /// have the outcome depend on the destination volume.
    #[cfg(unix)]
    #[test]
    fn relocate_rejects_vault_with_symlink() {
        let tmp = test_dir("symlink_reject");
        let src = tmp.join("vault");
        let cfg = tmp.join("cfg");
        make_vault(&src);
        fs::write(tmp.join("outside.txt"), b"outside").unwrap();
        std::os::unix::fs::symlink(tmp.join("outside.txt"), src.join("raw/link.gpx")).unwrap();

        let err = relocate(&src, &tmp.join("new-home"), &cfg, &|_, _| {}).unwrap_err();
        assert!(err.contains("symbolic link"), "unexpected error: {}", err);
        assert!(err.contains("link.gpx"), "should name the link: {}", err);
        // Source untouched, marker not written, nothing copied.
        assert_vault_contents(&src);
        assert_eq!(read_location(&cfg), None);
        assert!(!tmp.join("new-home").exists());
        let _ = fs::remove_dir_all(&tmp);
    }

    /// A symlink looping back into the source must not send the copy into
    /// infinite recursion — symlinks are skipped, not followed.
    #[cfg(unix)]
    #[test]
    fn copy_skips_symlinks_no_infinite_loop() {
        let tmp = test_dir("symlink");
        let src = tmp.join("vault");
        let dst = tmp.join("copy");
        make_vault(&src);
        // A directory symlink pointing back at the vault root.
        std::os::unix::fs::symlink(&src, src.join("loop")).unwrap();

        let total = dir_size(&src);
        let mut processed = 0u64;
        // Terminates (no stack overflow) and copies the real files only.
        copy_dir_recursive(&src, &dst, total, &mut processed, &|_, _| {}).unwrap();
        assert_vault_contents(&dst);
        assert!(!dst.join("loop").exists());
        let _ = fs::remove_dir_all(&tmp);
    }
}
