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

/// `write_location` that proves the root survives the round-trip. The marker
/// is a trimmed string: a root whose name ends in whitespace (or isn't UTF-8)
/// reads back as a DIFFERENT path, and the next boot would create a fresh
/// vault there — the user's data left aside. On mismatch the previous marker
/// (or its absence) is restored, so a refused switch changes nothing.
pub fn write_location_checked(config_dir: &Path, vault_root: &Path) -> Result<(), String> {
    let marker = config_dir.join(LOCATION_FILE);
    let previous = fs::read(&marker).ok();
    write_location(config_dir, vault_root)?;
    if read_location(config_dir).as_deref() == Some(vault_root) {
        return Ok(());
    }
    let restored = match previous {
        Some(bytes) => fs::write(&marker, bytes),
        None => fs::remove_file(&marker),
    };
    restored.map_err(|e| format!("Failed to restore vault location: {}", e))?;
    Err(format!(
        "Failed to save vault location: \"{}\" can't be stored as written",
        vault_root.display()
    ))
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

/// Resolve the root of an EXISTING vault to switch to (no data is moved):
/// the picked folder itself, or its `Syzify` subfolder — the usual miss when
/// the dialog lands one level above the vault. Anything else is "no vault
/// here", never a fallback to the relocate placement rules (those would
/// happily report a *placement* error about a folder that holds the very
/// vault the user meant to open).
pub fn resolve_existing_vault_root(dest: &Path) -> Result<PathBuf, String> {
    let dest = normalize_path(dest)?;
    for candidate in [dest.clone(), dest.join("Syzify")] {
        match fs::metadata(candidate.join("vault.db")) {
            Ok(m) if m.is_file() => return Ok(candidate),
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            // "No vault found" would be a lie on a folder we can't read —
            // and lacking Full Disk Access is why the boot screen exists.
            Err(e) => {
                return Err(format!("Couldn't check \"{}\": {}", candidate.display(), e));
            }
        }
    }
    Err(format!("No vault found in \"{}\"", dest.display()))
}

/// Resolve the root for a NEW vault. Same placement rules as relocate, but a
/// pick that already holds a vault — or sits anywhere inside one — is refused:
/// the boot path would otherwise nest a fresh vault in the old one (a real
/// `…/Syzify/Syzify` from picking the existing vault folder), where relocate
/// can never move it out and the outer vault's backups swallow it.
pub fn resolve_new_vault_root(dest: &Path) -> Result<PathBuf, String> {
    let dest = normalize_path(dest)?;
    if let Some(existing) = enclosing_vault(&dest)? {
        return Err(if existing == dest {
            format!(
                "\"{}\" already holds a vault — use \"Open another vault…\" to open it",
                dest.display()
            )
        } else {
            format!(
                "\"{}\" is inside the vault at \"{}\" — pick a folder outside it",
                dest.display(),
                existing.display()
            )
        });
    }
    resolve_target_root(&dest)
}

/// Boot-time twin of `resolve_new_vault_root`: a root that has no vault.db is
/// about to become a fresh vault, so refuse it when it lies inside another
/// vault. Catches markers written by older builds (which never checked) and a
/// vault that appeared above the marker's path later. A root whose vault.db
/// can't even be stat'ed is left alone — the open that follows reports the
/// real error (missing Full Disk Access, a gone volume) in its own words.
pub fn ensure_not_nested(root: &Path) -> Result<(), String> {
    match fs::metadata(root.join("vault.db")) {
        Ok(_) => return Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(_) => return Ok(()),
    }
    let Some(parent) = root.parent() else {
        return Ok(());
    };
    match enclosing_vault(parent)? {
        Some(outer) => Err(format!(
            "Refusing to create a vault at \"{}\": it is inside the vault at \"{}\"",
            root.display(),
            outer.display()
        )),
        None => Ok(()),
    }
}

/// Canonical form of a path that may not exist yet: the deepest existing
/// ancestor is resolved through symlinks and `..`, the rest re-appended.
/// Lexical ancestors alone would let `~/Shortcut → ~/Syzify/photos` pass the
/// enclosing-vault check and drop a vault into the real one, and would flag
/// `~/Syzify/../Other` as "inside" a vault it is not.
pub fn normalize_path(path: &Path) -> Result<PathBuf, String> {
    let existing = path
        .ancestors()
        .find(|p| p.exists())
        .ok_or_else(|| format!("\"{}\" cannot be resolved", path.display()))?;
    let canon = fs::canonicalize(existing)
        .map_err(|e| format!("Couldn't resolve \"{}\": {}", existing.display(), e))?;
    let rest = path
        .strip_prefix(existing)
        .map_err(|_| format!("\"{}\" cannot be resolved", path.display()))?;
    // `join("")` would append a trailing separator — harmless to Path
    // comparisons, but it would leak into the marker file and the UI.
    if rest.as_os_str().is_empty() {
        return Ok(canon);
    }
    Ok(canon.join(rest))
}

/// The nearest vault root at `path` or above it (a directory with a
/// `vault.db` file). `Ok(None)` means every level was checked and none holds
/// a vault; a level that can't be checked (macOS TCC on Documents, a vanished
/// volume) is an error, not a "no" — guessing "no" there is exactly how a
/// vault ends up nested inside another.
fn enclosing_vault(path: &Path) -> Result<Option<&Path>, String> {
    for p in path.ancestors() {
        match fs::metadata(p.join("vault.db")) {
            Ok(m) if m.is_file() => return Ok(Some(p)),
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(format!("Couldn't check \"{}\": {}", p.display(), e));
            }
        }
    }
    Ok(None)
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

    /// A root the marker can't hold verbatim is refused AND leaves the
    /// marker exactly as it was — the poisoned-marker path would otherwise
    /// boot a fresh vault next to the user's data.
    #[cfg(unix)]
    #[test]
    fn write_location_checked_restores_marker_on_mismatch() {
        let tmp = test_dir("marker_roundtrip");
        let cfg = tmp.join("cfg");
        let good = tmp.join("good");
        let trailing = tmp.join("trailing ");
        fs::create_dir_all(&good).unwrap();
        fs::create_dir_all(&trailing).unwrap();

        // No marker yet → refused, still no marker.
        let err = write_location_checked(&cfg, &trailing).unwrap_err();
        assert!(err.contains("can't be stored"), "{err}");
        assert_eq!(read_location(&cfg), None);

        // Existing marker → refused, previous value intact.
        write_location_checked(&cfg, &good).unwrap();
        assert!(write_location_checked(&cfg, &trailing).is_err());
        assert_eq!(read_location(&cfg), Some(good));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn normalize_path_resolves_existing_prefix_and_keeps_the_rest() {
        let tmp = test_dir("normalize");
        let canon = fs::canonicalize(&tmp).unwrap();
        assert_eq!(normalize_path(&tmp).unwrap(), canon);
        assert!(!normalize_path(&tmp).unwrap().to_string_lossy().ends_with('/'));
        assert_eq!(normalize_path(&tmp.join("a/b/c")).unwrap(), canon.join("a/b/c"));
        assert_eq!(normalize_path(Path::new("/")).unwrap(), PathBuf::from("/"));
        // `..` through an existing prefix is resolved by the filesystem.
        let sub = tmp.join("sub");
        fs::create_dir(&sub).unwrap();
        assert_eq!(normalize_path(&sub.join("../x")).unwrap(), canon.join("x"));
        let _ = fs::remove_dir_all(&tmp);
    }

    /// Opening an existing vault accepts the vault folder or its parent
    /// (the `Syzify` subfolder), and reports "no vault" for anything else —
    /// never a relocate placement error about the folder the user meant.
    #[test]
    fn existing_vault_root_accepts_vault_or_its_parent_only() {
        let tmp = test_dir("existing_root");
        let home = tmp.join("home");
        let vault = home.join("Syzify");
        make_vault(&vault);
        fs::write(home.join("other.txt"), b"x").unwrap();
        let canon_vault = fs::canonicalize(&vault).unwrap();

        assert_eq!(resolve_existing_vault_root(&vault).unwrap(), canon_vault);
        assert_eq!(resolve_existing_vault_root(&home).unwrap(), canon_vault);

        let empty = tmp.join("empty");
        fs::create_dir(&empty).unwrap();
        let err = resolve_existing_vault_root(&empty).unwrap_err();
        assert!(err.contains("No vault found"), "{err}");
        let err = resolve_existing_vault_root(&tmp.join("missing")).unwrap_err();
        assert!(err.contains("No vault found"), "{err}");
        // The refusal creates nothing.
        assert!(!empty.join("Syzify").exists());
        let _ = fs::remove_dir_all(&tmp);
    }

    /// A new vault follows the relocate placement rules, but never lands in
    /// (or inside) an existing vault — the `…/Syzify/Syzify` nesting seen
    /// live when the existing vault folder itself was picked.
    #[test]
    fn new_vault_root_refuses_existing_and_nested_vaults() {
        let tmp = test_dir("new_vault_root");
        let vault = tmp.join("vault");
        make_vault(&vault);

        // The vault folder itself: relocate rules would create vault/Syzify.
        let err = resolve_new_vault_root(&vault).unwrap_err();
        assert!(err.contains("already holds a vault"), "{err}");
        assert!(!vault.join("Syzify").exists());

        // Any folder below it, existing or not, existing-empty included.
        let inside = vault.join("photos/2026");
        let err = resolve_new_vault_root(&inside).unwrap_err();
        assert!(err.contains("inside the vault at"), "{err}");
        assert!(err.contains(&vault.display().to_string()), "{err}");
        let empty_inside = vault.join("fresh");
        fs::create_dir(&empty_inside).unwrap();
        assert!(resolve_new_vault_root(&empty_inside).is_err());
        assert!(resolve_new_vault_root(&vault.join("does/not/exist")).is_err());

        // Outside a vault the relocate rules apply unchanged (results are
        // canonical: on macOS the temp dir itself sits behind a symlink).
        let canon = fs::canonicalize(&tmp).unwrap();
        let empty = tmp.join("empty");
        fs::create_dir(&empty).unwrap();
        assert_eq!(resolve_new_vault_root(&empty).unwrap(), canon.join("empty"));
        assert_eq!(
            resolve_new_vault_root(&tmp.join("missing")).unwrap(),
            canon.join("missing")
        );
        // A sibling of the vault is not "inside" it — not even spelled via `..`.
        let busy = tmp.join("busy");
        fs::create_dir(&busy).unwrap();
        fs::write(busy.join("x.txt"), b"x").unwrap();
        assert_eq!(resolve_new_vault_root(&busy).unwrap(), canon.join("busy/Syzify"));
        assert_eq!(
            resolve_new_vault_root(&vault.join("../busy")).unwrap(),
            canon.join("busy/Syzify")
        );
        // A vault.db that is a directory is not a vault.
        let odd = tmp.join("odd");
        fs::create_dir_all(odd.join("vault.db")).unwrap();
        assert_eq!(resolve_new_vault_root(&odd).unwrap(), canon.join("odd/Syzify"));
        let _ = fs::remove_dir_all(&tmp);
    }

    /// A symlink pointing into a vault must not smuggle a new vault inside
    /// it: lexical ancestors of the link never see the vault.
    #[cfg(unix)]
    #[test]
    fn new_vault_root_sees_through_symlinks() {
        let tmp = test_dir("new_vault_symlink");
        let vault = tmp.join("vault");
        make_vault(&vault);
        let link = tmp.join("shortcut");
        std::os::unix::fs::symlink(vault.join("photos"), &link).unwrap();

        let err = resolve_new_vault_root(&link).unwrap_err();
        assert!(err.contains("inside the vault at"), "{err}");
        let err = resolve_new_vault_root(&link.join("new")).unwrap_err();
        assert!(err.contains("inside the vault at"), "{err}");
        assert!(!vault.join("photos/Syzify").exists());
        let _ = fs::remove_dir_all(&tmp);
    }

    /// An ancestor that can't be checked is a refusal, not a pass — the
    /// boot-error screen (no Full Disk Access) is where new vaults get made.
    #[cfg(unix)]
    #[test]
    fn new_vault_root_refuses_when_an_ancestor_is_unreadable() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = test_dir("new_vault_unreadable");
        let locked = tmp.join("locked");
        fs::create_dir(&locked).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        // Root (some CI containers) reads anything; the guard is moot there.
        let readable_anyway = fs::read_dir(&locked).is_ok();

        let result = resolve_new_vault_root(&locked.join("sub"));
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        if !readable_anyway {
            let err = result.unwrap_err();
            assert!(err.contains("Couldn't check"), "{err}");
        }
        assert!(!locked.join("sub").exists());
        let _ = fs::remove_dir_all(&tmp);
    }

    /// Boot-time guard: a marker pointing at a vault-less folder inside a
    /// vault must not create one there; an existing vault is left alone.
    #[test]
    fn ensure_not_nested_guards_only_fresh_roots() {
        let tmp = test_dir("ensure_not_nested");
        let vault = tmp.join("vault");
        make_vault(&vault);

        assert!(ensure_not_nested(&vault).is_ok());
        let err = ensure_not_nested(&vault.join("Syzify")).unwrap_err();
        assert!(err.contains("inside the vault at"), "{err}");
        assert!(ensure_not_nested(&tmp.join("fresh")).is_ok());
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
