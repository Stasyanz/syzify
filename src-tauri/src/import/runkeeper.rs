//! Runkeeper data export (`.zip`) import data source.
//!
//! A Runkeeper export is a flat zip of per-workout `*.gpx` plus a
//! `cardioActivities.csv` listing every activity. We import the GPX files
//! through the normal pipeline (real tracks) and the CSV rows that have no GPX
//! (swimming, manual, indoor) as GPS-less activities — so nothing is lost.
//! Dedup makes re-importing safe.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use uuid::Uuid;
use zip::result::ZipError;
use zip::ZipArchive;

use crate::import::pipeline::{self, FailedFile, ImportResult};
use crate::import::runkeeper_csv;

// Bounds on a (possibly malicious) export so it can't OOM or flood the disk.
const MAX_GPX_BYTES: u64 = 64 * 1024 * 1024; // 64 MiB per GPX
const MAX_CSV_BYTES: u64 = 256 * 1024 * 1024; // 256 MiB for the CSV
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024; // 2 GiB uncompressed total
const MAX_FILES: usize = 5000;

/// The bounds an export is held to. One set in production; tests shrink
/// them to reach the edges without inflating gigabytes.
#[derive(Clone, Copy)]
struct Limits {
    gpx_bytes: u64,
    csv_bytes: u64,
    total_bytes: u64,
    files: usize,
}

const LIMITS: Limits = Limits {
    gpx_bytes: MAX_GPX_BYTES,
    csv_bytes: MAX_CSV_BYTES,
    total_bytes: MAX_TOTAL_BYTES,
    files: MAX_FILES,
};

/// A reader that tallies the bytes it hands out — what an entry actually
/// inflated, whether the read then succeeds or not.
struct Counted<'a, R> {
    inner: R,
    n: &'a mut u64,
}

impl<R: std::io::Read> std::io::Read for Counted<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let k = self.inner.read(buf)?;
        *self.n += k as u64;
        Ok(k)
    }
}

/// Remove a temp directory on drop.
struct TempDir(PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn import_zip(
    conn: &Connection,
    vault_path: &Path,
    zip_path: &str,
    encryption_key: Option<&[u8; 32]>,
) -> Result<ImportResult, String> {
    import_zip_within(conn, vault_path, zip_path, encryption_key, LIMITS)
}

fn import_zip_within(
    conn: &Connection,
    vault_path: &Path,
    zip_path: &str,
    encryption_key: Option<&[u8; 32]>,
    limits: Limits,
) -> Result<ImportResult, String> {
    let file = fs::File::open(zip_path).map_err(|e| format!("Failed to open export: {e}"))?;
    let mut zip = ZipArchive::new(file).map_err(|e| format!("Not a valid .zip export: {e}"))?;

    let tmp = std::env::temp_dir().join(format!("rk_import_{}", Uuid::new_v4()));
    fs::create_dir_all(&tmp).map_err(|e| format!("Failed to create temp dir: {e}"))?;
    let _guard = TempDir(tmp.clone());

    let mut gpx_entries: Vec<(String, String)> = Vec::new(); // (temp path, original name)
    let mut csv_path: Option<PathBuf> = None;
    let mut total_bytes: u64 = 0;
    let mut file_count: usize = 0;
    // An entry that can't be opened (an unsupported packing), read (past
    // its cap, damaged data) or written under its name is that entry's
    // failure, not the archive's (#203): the rest of the export imports.
    // The archive-wide bounds, a password, a full or unwritable temp dir
    // still refuse the whole of it.
    let mut entry_failures: Vec<FailedFile> = Vec::new();

    for i in 0..zip.len() {
        // Name first, without opening the entry: only GPX and the CSV are
        // ours, so an entry of anything else — even one that can't be
        // opened — is neither counted nor reported.
        let Some(fname) = zip.name_for_index(i).and_then(bare_entry_name) else {
            continue;
        };
        let lower = fname.to_ascii_lowercase();
        let is_gpx = lower.ends_with(".gpx");
        let is_csv = lower == "cardioactivities.csv";
        if !is_gpx && !is_csv {
            continue;
        }

        file_count += 1;
        if file_count > limits.files {
            return Err(format!("export has too many files (limit {})", limits.files));
        }

        let mut entry = match zip.by_index(i) {
            Ok(entry) => entry,
            // A password is the archive's matter: said once, with its
            // reason, instead of every file failing without one.
            Err(ZipError::UnsupportedArchive(ZipError::PASSWORD_REQUIRED)) => {
                return Err("The export is password-protected — export it again without a password".to_string());
            }
            // An entry packed in a way this reader can't open (an old
            // method — Shrink, Implode, Reduce) is that entry's failure.
            Err(e) => {
                entry_failures.push(FailedFile { reason: format!("Can't open {fname}: {e}"), path: fname });
                continue;
            }
        };
        // Only regular files: this deliberately rejects directory and symlink
        // entries (a zip symlink could otherwise be a write-outside vector).
        if !entry.is_file() {
            continue;
        }

        let cap = if is_csv { limits.csv_bytes } else { limits.gpx_bytes };
        // What the entry really inflated counts toward the archive's total
        // — a refused one too, so thousands of oversized entries can't each
        // be inflated in turn.
        let mut inflated = 0u64;
        let read = crate::util::read_capped(&mut Counted { inner: &mut entry, n: &mut inflated }, cap, &fname);
        total_bytes += inflated;
        if total_bytes > limits.total_bytes {
            return Err("export is too large (uncompressed) — refusing to import".to_string());
        }
        let bytes = match read {
            Ok(bytes) => bytes,
            Err(reason) => {
                entry_failures.push(FailedFile { path: fname, reason });
                continue;
            }
        };

        let dest = tmp.join(extracted_name(i, &fname));
        if let Err(e) = fs::write(&dest, &bytes) {
            if write_failure_is_the_entrys(e.kind()) {
                entry_failures.push(FailedFile { reason: format!("Failed to extract {fname}: {e}"), path: fname });
                continue;
            }
            return Err(format!("Failed to extract {fname}: {e}"));
        }

        if is_gpx {
            gpx_entries.push((dest.to_string_lossy().into_owned(), fname));
        } else {
            csv_path = Some(dest);
        }
    }

    gpx_entries.sort();
    let gpx_paths: Vec<String> = gpx_entries.iter().map(|(p, _)| p.clone()).collect();

    // 1) GPX first (real tracks). 2) CSV adds only the GPS-less rows (it skips
    //    rows that reference a GPX file). Dedup guards any overlap.
    let mut result = pipeline::import_files(conn, vault_path, &gpx_paths, encryption_key, |_, _, _| {});
    // A GPX the pipeline refused is named as in the export, like the
    // entries refused above — not by its soon-deleted temp path.
    for failed in &mut result.failed {
        if let Some((_, name)) = gpx_entries.iter().find(|(temp, _)| *temp == failed.path) {
            failed.path = name.clone();
        }
    }
    result.failed.splice(0..0, entry_failures);
    // Provenance: store the real export filename, not the (soon-deleted) temp path.
    for (temp, name) in &gpx_entries {
        let _ = conn.execute(
            "UPDATE raw_file SET original_path = ?1 WHERE original_path = ?2",
            rusqlite::params![name, temp],
        );
    }
    if let Some(csv) = csv_path {
        // A broken CSV must not discard the already-imported GPX — record it as a
        // failure instead of aborting the whole import.
        match runkeeper_csv::import_runkeeper_csv(conn, &csv.to_string_lossy()) {
            Ok(csv_res) => {
                result.imported += csv_res.imported;
                result.skipped += csv_res.skipped;
                result.failed.extend(csv_res.failed);
            }
            Err(e) => result.failed.push(FailedFile {
                path: "cardioActivities.csv".to_string(),
                reason: e,
            }),
        }
    }
    Ok(result)
}

/// The bare file name of a zip entry — everything after its last `/` or
/// `\`, whichever the archive's author used — so no entry name can reach
/// outside the extraction directory (`../x`, `/abs/x`, `C:\x`, a UNC
/// path). `None` for a name with nothing left (a directory, `..`, `.`).
fn bare_entry_name(name: &str) -> Option<String> {
    let last = name.rsplit(['/', '\\']).next().unwrap_or("");
    match last {
        "" | "." | ".." => None,
        s => Some(s.to_string()),
    }
}

/// Whether a failed extraction write is the entry's own matter — a name
/// the OS refuses (too long, a NUL) — or every entry's, like a full or
/// unwritable temp dir, which is said once for the whole import.
fn write_failure_is_the_entrys(kind: std::io::ErrorKind) -> bool {
    matches!(kind, std::io::ErrorKind::InvalidFilename | std::io::ErrorKind::InvalidInput)
}

/// The name an entry is extracted under: its index first, so same-named
/// entries from different folders don't overwrite each other — and no
/// entry lands under a reserved DOS device name (`CON.gpx` → `3_CON.gpx`).
fn extracted_name(index: usize, bare: &str) -> String {
    format!("{index}_{bare}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    const GPX: &str = r#"<?xml version="1.0"?>
<gpx version="1.1" creator="RunKeeper" xmlns="http://www.topografix.com/GPX/1/1">
<trk><name>Running 1/1/15</name><time>2015-01-01T08:00:00Z</time><trkseg>
<trkpt lat="55.75" lon="37.62"><ele>150</ele><time>2015-01-01T08:00:00Z</time></trkpt>
<trkpt lat="55.751" lon="37.621"><ele>151</ele><time>2015-01-01T08:01:00Z</time></trkpt>
</trkseg></trk></gpx>"#;

    const CSV: &str = "Date,Type,Route Name,Distance (km),Duration,Average Pace,Average Speed (km/h),Calories Burned,Climb (m),Average Heart Rate (bpm),Notes,GPX File\n\
2015-01-01 08:00:00,Running,,2.0,10:00,,12.0,150,5,,,2015-01-01-0800.gpx\n\
2015-01-02 07:00:00,Swimming,,1.0,30:00,,2.0,300,0,,,\n";

    #[test]
    fn imports_gpx_and_gpsless_from_zip() {
        let conn = db::test_db();
        let dir = crate::test_support::ScratchDir::new("zip_test");
        let zip_path = dir.join("export.zip");
        {
            let f = fs::File::create(&zip_path).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            let o = SimpleFileOptions::default();
            zw.start_file("2015-01-01-0800.gpx", o).unwrap();
            zw.write_all(GPX.as_bytes()).unwrap();
            zw.start_file("cardioActivities.csv", o).unwrap();
            zw.write_all(CSV.as_bytes()).unwrap();
            zw.finish().unwrap();
        }

        let r = import_zip(&conn, &dir, zip_path.to_str().unwrap(), None).unwrap();
        // 1 GPX (running) + 1 GPS-less CSV row (swimming); the CSV running row is
        // skipped because it references a GPX file.
        assert_eq!(r.imported, 2, "{r:?}");

        let count: i64 = conn.query_row("SELECT COUNT(*) FROM activity", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 2);
        let swim: i64 = conn
            .query_row("SELECT COUNT(*) FROM activity WHERE sport_type='swim'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(swim, 1);

    }

    #[test]
    fn runkeeper_in_datasource_list() {
        let sources = crate::import::datasource::list();
        assert!(sources.iter().any(|d| d.id == "runkeeper" && d.extensions == ["zip"]));
    }

    fn gpx_at(date: &str, lat: &str) -> String {
        format!(
            "<?xml version=\"1.0\"?>\n<gpx version=\"1.1\" creator=\"RunKeeper\" xmlns=\"http://www.topografix.com/GPX/1/1\">\n\
<trk><name>Running</name><time>{date}T08:00:00Z</time><trkseg>\n\
<trkpt lat=\"{lat}\" lon=\"37.62\"><ele>150</ele><time>{date}T08:00:00Z</time></trkpt>\n\
<trkpt lat=\"{lat}\" lon=\"37.63\"><ele>151</ele><time>{date}T08:05:00Z</time></trkpt>\n\
</trkseg></trk></gpx>"
        )
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        let f = fs::File::create(path).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        for (name, bytes) in entries {
            zw.start_file(*name, SimpleFileOptions::default()).unwrap();
            zw.write_all(bytes).unwrap();
        }
        zw.finish().unwrap();
    }

    /// An entry the OS won't take a file for (a name past 255 bytes, a
    /// NUL in it) is that entry's failure: reported by name, and the rest
    /// of the export imports (#203).
    #[test]
    fn an_unwritable_entry_name_fails_only_that_entry() {
        let conn = db::test_db();
        let dir = crate::test_support::ScratchDir::new("unwritable");
        let zip_path = dir.join("export.zip");
        let long = format!("{}.gpx", "x".repeat(300));
        write_zip(
            &zip_path,
            &[
                (long.as_str(), gpx_at("2015-05-05", "55.10").as_bytes()),
                ("bad\0name.gpx", gpx_at("2015-06-06", "55.20").as_bytes()),
                ("good.gpx", gpx_at("2015-07-07", "55.30").as_bytes()),
            ],
        );
        let r = import_zip(&conn, &dir, zip_path.to_str().unwrap(), None).unwrap();
        assert_eq!(r.imported, 1, "the good entry imports: {r:?}");
        let failed: Vec<&str> = r.failed.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(failed, vec![long.as_str(), "bad\0name.gpx"]);
        assert!(r.failed.iter().all(|f| f.reason.starts_with("Failed to extract")), "{r:?}");
    }

    /// Rewrite one central-directory header of a written zip: the `nth`
    /// entry's (PK\x01\x02) bytes at `offset` go through `patch`.
    fn patch_central(zip_path: &Path, nth: usize, patch: impl Fn(&mut [u8])) {
        let mut bytes = fs::read(zip_path).unwrap();
        let at = bytes
            .windows(4)
            .enumerate()
            .filter(|(_, w)| *w == b"PK\x01\x02")
            .nth(nth)
            .map(|(at, _)| at)
            .unwrap();
        patch(&mut bytes[at..at + 46]);
        fs::write(zip_path, &bytes).unwrap();
    }

    /// A GPX whose data is damaged (its checksum fails) is that entry's
    /// failure; what it actually inflated is what it spends of the budget —
    /// counted at its cap instead, two damaged files would refuse this
    /// export as too large, which it is not.
    #[test]
    fn a_damaged_entry_fails_alone_and_spends_what_it_inflated() {
        let conn = db::test_db();
        let dir = crate::test_support::ScratchDir::new("damaged");
        let zip_path = dir.join("export.zip");
        let a = gpx_at("2016-01-01", "55.10");
        let b = gpx_at("2016-02-02", "55.20");
        let c = gpx_at("2016-03-03", "55.30");
        write_zip(&zip_path, &[("a.gpx", a.as_bytes()), ("b.gpx", b.as_bytes()), ("c.gpx", c.as_bytes())]);
        for nth in [0, 1] {
            patch_central(&zip_path, nth, |h| h[16] ^= 0xff); // CRC-32
        }
        let total = (a.len() + b.len() + c.len()) as u64;
        let limits = Limits { gpx_bytes: 1 << 20, csv_bytes: 1 << 20, total_bytes: total, files: 10 };
        let r = import_zip_within(&conn, &dir, zip_path.to_str().unwrap(), None, limits).unwrap();
        assert_eq!(r.imported, 1, "{r:?}");
        let failed: Vec<&str> = r.failed.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(failed, vec!["a.gpx", "b.gpx"]);
    }

    /// A refused name is the entry's; a full disk, a permission or any
    /// other failure is the import's.
    #[test]
    fn only_a_refused_name_is_the_entrys_write_failure() {
        use std::io::ErrorKind::*;
        assert!(write_failure_is_the_entrys(InvalidFilename));
        assert!(write_failure_is_the_entrys(InvalidInput));
        for kind in [StorageFull, PermissionDenied, NotFound, ReadOnlyFilesystem, Other] {
            assert!(!write_failure_is_the_entrys(kind), "{kind:?}");
        }
    }

    /// A GPX packed with a method this reader can't open (Reduce, 7) is
    /// that entry's failure; the others import.
    #[test]
    fn a_gpx_packed_in_an_unsupported_way_fails_only_itself() {
        let conn = db::test_db();
        let dir = crate::test_support::ScratchDir::new("method");
        let zip_path = dir.join("export.zip");
        write_zip(
            &zip_path,
            &[("a.gpx", gpx_at("2016-04-04", "55.10").as_bytes()), ("b.gpx", gpx_at("2016-05-05", "55.20").as_bytes())],
        );
        patch_central(&zip_path, 1, |h| h[10..12].copy_from_slice(&7u16.to_le_bytes()));
        let r = import_zip(&conn, &dir, zip_path.to_str().unwrap(), None).unwrap();
        assert_eq!(r.imported, 1, "{r:?}");
        let failed: Vec<&str> = r.failed.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(failed, vec!["b.gpx"]);
        assert!(r.failed[0].reason.contains("not supported"), "{}", r.failed[0].reason);
    }

    /// An entry that is not ours (not a GPX, not the CSV) is never opened:
    /// one that can't be is no failure, and it counts toward no limit.
    #[test]
    fn an_unopenable_entry_of_another_kind_is_not_ours() {
        let conn = db::test_db();
        let dir = crate::test_support::ScratchDir::new("other");
        let zip_path = dir.join("export.zip");
        write_zip(&zip_path, &[("README.txt", b"hello"), ("good.gpx", gpx_at("2015-11-11", "55.30").as_bytes())]);
        patch_central(&zip_path, 0, |h| h[8] |= 1); // encrypted
        let small = Limits { gpx_bytes: 1 << 20, csv_bytes: 1 << 20, total_bytes: 1 << 22, files: 1 };
        let r = import_zip_within(&conn, &dir, zip_path.to_str().unwrap(), None, small).unwrap();
        assert_eq!(r.imported, 1, "{r:?}");
        assert!(r.failed.is_empty(), "{r:?}");
    }

    /// A password-protected export is refused as a whole, with the reason
    /// — not reported as every file failing without one.
    #[test]
    fn a_password_protected_export_is_refused_with_its_reason() {
        let conn = db::test_db();
        let dir = crate::test_support::ScratchDir::new("password");
        let zip_path = dir.join("export.zip");
        write_zip(&zip_path, &[("run.gpx", gpx_at("2015-12-12", "55.40").as_bytes())]);
        patch_central(&zip_path, 0, |h| h[8] |= 1);
        let err = import_zip(&conn, &dir, zip_path.to_str().unwrap(), None).unwrap_err();
        assert!(err.contains("password-protected"), "{err}");
    }

    /// A GPX the pipeline refuses is named as in the export, not by its
    /// temp path.
    #[test]
    fn a_gpx_the_pipeline_refuses_keeps_its_export_name() {
        let conn = db::test_db();
        let dir = crate::test_support::ScratchDir::new("junk");
        let zip_path = dir.join("export.zip");
        write_zip(&zip_path, &[("folder/junk.gpx", b"not a gpx at all")]);
        let r = import_zip(&conn, &dir, zip_path.to_str().unwrap(), None).unwrap();
        assert_eq!(r.imported, 0);
        assert_eq!(r.failed.len(), 1, "{r:?}");
        assert_eq!(r.failed[0].path, "junk.gpx");
    }

    /// One entry past its size cap is skipped and reported, the rest
    /// import; but the bytes inflated for it count toward the archive's
    /// total, so a flood of oversized entries still refuses the export
    /// instead of being inflated one after another.
    #[test]
    fn an_oversized_entry_is_skipped_but_still_spends_the_budget() {
        let conn = db::test_db();
        let dir = crate::test_support::ScratchDir::new("oversized");
        let small = Limits { gpx_bytes: 4096, csv_bytes: 4096, total_bytes: 20_000, files: 50 };
        let big = vec![b'x'; 10_000];

        let one = dir.join("one.zip");
        write_zip(&one, &[("big.gpx", &big), ("good.gpx", gpx_at("2015-08-08", "55.40").as_bytes())]);
        let r = import_zip_within(&conn, &dir, one.to_str().unwrap(), None, small).unwrap();
        assert_eq!(r.imported, 1, "{r:?}");
        assert_eq!(r.failed.len(), 1);
        assert_eq!(r.failed[0].path, "big.gpx");
        assert!(r.failed[0].reason.contains("exceeds the 4096-byte limit"), "{}", r.failed[0].reason);

        // Each refused entry inflated its cap and one byte before it was
        // refused: five are 5 × 4097, past the 20 000-byte budget — the
        // export as a whole is refused.
        let flood = dir.join("flood.zip");
        let names: Vec<String> = (0..5).map(|i| format!("big{i}.gpx")).collect();
        let entries: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), big.as_slice())).collect();
        write_zip(&flood, &entries);
        let err = import_zip_within(&conn, &dir, flood.to_str().unwrap(), None, small).unwrap_err();
        assert!(err.contains("too large"), "{err}");
    }

    #[test]
    fn duplicate_gpx_names_do_not_collapse() {
        let conn = db::test_db();
        let dir = crate::test_support::ScratchDir::new("dup");
        let zip_path = dir.join("export.zip");
        // Same base name in two folders, different content (different dates/coords).
        let a = gpx_at("2015-01-01", "55.10");
        let b = gpx_at("2015-02-02", "55.20");
        write_zip(&zip_path, &[("a/run.gpx", a.as_bytes()), ("b/run.gpx", b.as_bytes())]);

        let r = import_zip(&conn, &dir, zip_path.to_str().unwrap(), None).unwrap();
        assert_eq!(r.imported, 2, "both GPX must import, not collapse: {r:?}");
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM activity", [], |x| x.get(0)).unwrap();
        assert_eq!(count, 2);
        // N1: original_path is the real export filename, not a temp path.
        let orig: String = conn
            .query_row("SELECT DISTINCT original_path FROM raw_file", [], |x| x.get(0))
            .unwrap();
        assert_eq!(orig, "run.gpx", "provenance is the export filename");
    }

    #[test]
    fn traversal_named_entry_stays_inside_temp() {
        let conn = db::test_db();
        let dir = crate::test_support::ScratchDir::new("slip");
        let zip_path = dir.join("export.zip");
        // A traversal name beside an ordinary one: both must be flattened
        // to evil.gpx inside the extraction dir and imported. Asserted on
        // the recorded names, not on the file system (#202): an unflattened
        // name would fail here on a missing or unwritable directory only
        // by luck of the machine's permissions.
        write_zip(
            &zip_path,
            &[
                ("a/evil.gpx", gpx_at("2015-03-03", "55.40").as_bytes()),
                ("../evil.gpx", gpx_at("2015-04-04", "55.50").as_bytes()),
            ],
        );
        let r = import_zip(&conn, &dir, zip_path.to_str().unwrap(), None).unwrap();
        assert_eq!(r.imported, 2, "both imported as flattened files: {r:?}");
        let names: Vec<String> = {
            let mut st = conn.prepare("SELECT original_path FROM raw_file ORDER BY original_path").unwrap();
            st.query_map([], |x| x.get(0)).unwrap().map(|x| x.unwrap()).collect()
        };
        assert_eq!(names, vec!["evil.gpx", "evil.gpx"]);
    }

    /// Every way an entry name can point outside — parent hops, an absolute
    /// path, either slash, a drive letter, a UNC share — lands as a bare
    /// name directly inside the extraction dir (#202). The end-to-end test
    /// above checks the flattened names; it cannot see an escape on disk —
    /// the extraction dir is the import's own and gone when it returns.
    #[test]
    fn every_entry_name_lands_inside_the_extraction_dir() {
        let root = Path::new("/x/rk_import_1");
        for name in [
            "../../../evil.gpx",
            "/abs/evil.gpx",
            "a/b/evil.gpx",
            "..\\..\\evil.gpx",
            "C:\\evil.gpx",
            "\\\\host\\share\\evil.gpx",
            "evil.gpx",
        ] {
            let bare = bare_entry_name(name).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(bare, "evil.gpx", "{name}");
            let dest = root.join(extracted_name(7, &bare));
            assert_eq!(dest.parent(), Some(root), "{name} stays in the dir");
            assert_eq!(dest.file_name().unwrap(), "7_evil.gpx");
        }
        for name in ["", "dir/", "..", ".", "a/..", "a\\."] {
            assert_eq!(bare_entry_name(name), None, "{name:?} names no file");
        }
    }

    /// With the (scope-gated) vault key, the GPX raw file lands in the vault
    /// already encrypted — a datasource import must not open a plaintext
    /// window any more than the manual-import pipeline does.
    #[test]
    fn import_zip_encrypts_raw_files_with_key() {
        let conn = db::test_db();
        let dir = crate::test_support::ScratchDir::new("enc");
        let zip_path = dir.join("export.zip");
        write_zip(&zip_path, &[("run.gpx", gpx_at("2015-04-04", "55.50").as_bytes())]);

        let key = [8u8; 32];
        let r = import_zip(&conn, &dir, zip_path.to_str().unwrap(), Some(&key)).unwrap();
        assert_eq!(r.imported, 1, "{r:?}");

        // The DB row points at .enc, the ciphertext exists, no plaintext copy.
        let stored: String = conn
            .query_row("SELECT path_in_vault FROM raw_file", [], |x| x.get(0))
            .unwrap();
        assert!(stored.ends_with(".enc"), "raw file stored encrypted: {stored}");
        assert!(dir.join(&stored).exists());
        assert!(!dir.join(stored.trim_end_matches(".enc")).exists());

    }

    #[test]
    fn broken_csv_does_not_abort_gpx_import() {
        let conn = db::test_db();
        let dir = crate::test_support::ScratchDir::new("badcsv");
        let zip_path = dir.join("export.zip");
        // A valid GPX + a CSV that isn't a Runkeeper export (missing Date/Type).
        write_zip(
            &zip_path,
            &[
                ("2015-01-01-0800.gpx", gpx_at("2015-01-01", "55.30").as_bytes()),
                ("cardioActivities.csv", b"garbage,not,runkeeper\n1,2,3\n"),
            ],
        );

        let r = import_zip(&conn, &dir, zip_path.to_str().unwrap(), None).unwrap();
        assert_eq!(r.imported, 1, "GPX still imported despite the bad CSV");
        assert!(
            r.failed.iter().any(|f| f.path.contains("cardioActivities")),
            "the CSV error is reported, not silently dropped: {r:?}"
        );
    }
}
