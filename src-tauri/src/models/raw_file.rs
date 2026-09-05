use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FileFormat {
    Gpx,
    Fit,
    Tcx,
}

impl FileFormat {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "gpx" => Some(FileFormat::Gpx),
            "fit" => Some(FileFormat::Fit),
            "tcx" => Some(FileFormat::Tcx),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            FileFormat::Gpx => "gpx",
            FileFormat::Fit => "fit",
            FileFormat::Tcx => "tcx",
        }
    }
}

/// What a stored raw file is: a workout, or an all-day Garmin Monitor
/// file (ADR 0002) — the latter has no activity.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RawFileKind {
    Activity,
    Monitoring,
}

impl RawFileKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            RawFileKind::Activity => "activity",
            RawFileKind::Monitoring => "monitoring",
        }
    }
}

/// Failed imports are rejected outright (nothing is stored), so `Ok` is the
/// only status ever written; the DB column keeps accepting other values in
/// case a failed-import quarantine is ever built.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ParseStatus {
    Ok,
}

impl ParseStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ParseStatus::Ok => "ok",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_format_from_extension() {
        assert_eq!(FileFormat::from_extension("gpx"), Some(FileFormat::Gpx));
        assert_eq!(FileFormat::from_extension("GPX"), Some(FileFormat::Gpx));
        assert_eq!(FileFormat::from_extension("fit"), Some(FileFormat::Fit));
        assert_eq!(FileFormat::from_extension("FIT"), Some(FileFormat::Fit));
        assert_eq!(FileFormat::from_extension("tcx"), Some(FileFormat::Tcx));
        assert_eq!(FileFormat::from_extension("TCX"), Some(FileFormat::Tcx));
        assert_eq!(FileFormat::from_extension("csv"), None);
        assert_eq!(FileFormat::from_extension(""), None);
    }

    #[test]
    fn file_format_roundtrip() {
        for fmt in [FileFormat::Gpx, FileFormat::Fit, FileFormat::Tcx] {
            assert_eq!(FileFormat::from_extension(fmt.as_str()), Some(fmt));
        }
    }

    #[test]
    fn parse_status_as_str() {
        assert_eq!(ParseStatus::Ok.as_str(), "ok");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawFile {
    pub id: String,
    pub activity_id: Option<String>,
    pub path_in_vault: String,
    pub original_path: Option<String>,
    pub format: String,
    pub hash_sha256: String,
    pub imported_at: String,
    pub parse_status: String,
    pub failure_reason: Option<String>,
}
