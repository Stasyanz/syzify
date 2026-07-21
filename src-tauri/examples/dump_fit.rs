//! Dev utility: dump a FIT file's structure — message counts, every session's
//! sport/timing/key fields, and multisport-relevant messages. For comparing
//! watch originals against platform re-exports. Usage:
//!   cargo run --release --example dump_fit -- <file.fit> [file2.fit ...]

use fitparser::profile::MesgNum;
use std::collections::BTreeMap;

fn main() {
    for path in std::env::args().skip(1) {
        println!("\n════ {path}");
        let data = std::fs::read(&path).expect("read file");
        let messages = fitparser::from_bytes(&data).expect("parse FIT");

        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for m in &messages {
            *counts.entry(format!("{:?}", m.kind())).or_default() += 1;
        }
        println!("messages: {}", messages.len());
        for (k, n) in &counts {
            println!("  {k:<24} × {n}");
        }

        // Per-session detail.
        let mut si = 0;
        for m in &messages {
            if m.kind() != MesgNum::Session {
                continue;
            }
            si += 1;
            let get = |name: &str| {
                m.fields()
                    .iter()
                    .find(|f| f.name() == name)
                    .map(|f| format!("{}", f.value()))
                    .unwrap_or_else(|| "—".into())
            };
            println!(
                "  session {si}: sport={} sub={} start={} dist={} timer={}s elapsed={}s",
                get("sport"),
                get("sub_sport"),
                get("start_time"),
                get("total_distance"),
                get("total_timer_time"),
                get("total_elapsed_time"),
            );
        }

        // FileId + activity-level info (device, type).
        for m in &messages {
            if m.kind() == MesgNum::FileId || m.kind() == MesgNum::Activity {
                let fields: Vec<String> = m
                    .fields()
                    .iter()
                    .map(|f| format!("{}={}", f.name(), f.value()))
                    .collect();
                println!("  {:?}: {}", m.kind(), fields.join(" "));
            }
        }
    }
}
