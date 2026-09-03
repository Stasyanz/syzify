//! TEMP: dump MonitoringHrData / StressLevel / Monitoring(heart_rate) rows as
//! "file|kind|field=value|..." lines. Delete after use.

use fitparser::profile::MesgNum;

fn main() {
    for path in std::env::args().skip(1) {
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let messages = match fitparser::from_bytes(&data) {
            Ok(m) => m,
            Err(_) => continue,
        };
        for m in &messages {
            let kind = m.kind();
            if kind != MesgNum::MonitoringHrData
                && kind != MesgNum::StressLevel
                && kind != MesgNum::Monitoring
                && kind != MesgNum::MonitoringInfo
            {
                continue;
            }
            let fields: Vec<String> = m
                .fields()
                .iter()
                .map(|f| format!("{}={}", f.name(), f.value()))
                .collect();
            println!("{path}|{kind:?}|{}", fields.join("|"));
        }
    }
}
