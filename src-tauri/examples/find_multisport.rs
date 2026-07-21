//! Dev utility: scan a directory of FIT files and list the multisport ones
//! (triathlon/duathlon/…) — the files whose sessions resolve to "multisport".
//! Not part of the app. Usage:
//!   cargo run --release --example find_multisport -- <dir>

use std::io::Read;

fn main() {
    let dir = std::env::args().nth(1).expect("usage: find_multisport <dir>");
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    collect(std::path::Path::new(&dir), &mut files);
    files.sort();
    eprintln!("scanning {} files…", files.len());

    let mut hits = 0usize;
    let mut failed = 0usize;
    for (i, path) in files.iter().enumerate() {
        if i > 0 && i % 500 == 0 {
            eprintln!("…{i}/{} ({hits} found)", files.len());
        }
        let Ok(raw) = std::fs::read(path) else {
            failed += 1;
            continue;
        };
        // .gz support: the importer accepts gzipped FIT too.
        let bytes = if path.extension().is_some_and(|e| e == "gz") {
            let mut out = Vec::new();
            let mut dec = flate2::read::GzDecoder::new(&raw[..]);
            if dec.read_to_end(&mut out).is_err() {
                failed += 1;
                continue;
            }
            out
        } else {
            raw
        };
        let Ok(parsed) = syzify_lib::parser::fit::parse_fit_bytes(&bytes, "scan") else {
            failed += 1;
            continue;
        };
        if parsed.sport_type.as_deref() == Some("multisport") {
            hits += 1;
            let date = parsed.start_time.as_deref().unwrap_or("????");
            let legs: Vec<String> = parsed
                .legs
                .iter()
                .map(|l| {
                    if l.is_transition {
                        "T".to_string()
                    } else {
                        format!(
                            "{} {:.1}km",
                            l.sport_type,
                            l.total_distance_m.unwrap_or(0.0) / 1000.0
                        )
                    }
                })
                .collect();
            println!("{}\n  start: {}\n  legs:  {}\n", path.display(), date, legs.join(" → "));
        }
    }
    eprintln!("done: {hits} multisport file(s), {failed} unreadable/non-FIT skipped");
}

fn collect(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("fit") || e.eq_ignore_ascii_case("gz"))
        {
            out.push(path);
        }
    }
}
