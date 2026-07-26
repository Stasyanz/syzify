//! Scratch dev utility: per-message-kind field inventory of a FIT file —
//! every field name with its unit, non-invalid count and a sample value.
//! Usage: cargo run --release --example fit_field_inventory -- <file.fit>

use std::collections::BTreeMap;

fn main() {
    for path in std::env::args().skip(1) {
        println!("════ {path}");
        let data = std::fs::read(&path).expect("read file");
        let messages = fitparser::from_bytes(&data).expect("parse FIT");

        // kind -> field name -> (unit, count, sample)
        let mut inv: BTreeMap<String, BTreeMap<String, (String, usize, String)>> = BTreeMap::new();
        let mut kind_counts: BTreeMap<String, usize> = BTreeMap::new();

        for m in &messages {
            let kind = format!("{:?}", m.kind());
            *kind_counts.entry(kind.clone()).or_default() += 1;
            let fields = inv.entry(kind).or_default();
            for f in m.fields() {
                let e = fields.entry(f.name().to_string()).or_insert_with(|| {
                    (f.units().to_string(), 0, format!("{}", f.value()))
                });
                e.1 += 1;
            }
        }

        for (kind, fields) in &inv {
            println!("\n── {kind} × {}", kind_counts[kind]);
            for (name, (unit, count, sample)) in fields {
                let unit = if unit.is_empty() { "-" } else { unit };
                let sample: String = sample.chars().take(60).collect();
                println!("  {name:<38} [{unit:<8}] × {count:<6} e.g. {sample}");
            }
        }

        // Focus: raw Value variants of Record.left_right_balance (the enum-vs-
        // number question) and every TimeInZone message's reference_mesg.
        let mut variants: BTreeMap<String, usize> = BTreeMap::new();
        for m in &messages {
            if format!("{:?}", m.kind()) == "Record" {
                for f in m.fields() {
                    if f.name() == "left_right_balance" {
                        *variants.entry(format!("{:?}", f.value())).or_default() += 1;
                    }
                }
            }
        }
        println!("\n── Record.left_right_balance raw variants (top 15):");
        let mut v: Vec<_> = variants.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        for (val, n) in v.into_iter().take(15) {
            println!("  {val:<30} × {n}");
        }

        println!("\n── TimeInZone messages:");
        for m in &messages {
            if format!("{:?}", m.kind()) == "TimeInZone" {
                let get = |name: &str| {
                    m.fields()
                        .iter()
                        .find(|f| f.name() == name)
                        .map(|f| format!("{}", f.value()))
                        .unwrap_or_else(|| "—".into())
                };
                println!(
                    "  reference_mesg={} reference_index={} time_in_hr_zone={}",
                    get("reference_mesg"),
                    get("reference_index"),
                    get("time_in_hr_zone").chars().take(80).collect::<String>(),
                );
            }
        }
    }
}