//! Manual update check against GitHub Releases.
//!
//! Privacy invariant: the ONLY network call happens on the user's explicit
//! click, and the Settings row discloses the endpoint. No background polling,
//! no auto-updater — "updating" means opening the release page in the
//! browser and installing by hand.

use serde::Deserialize;

use crate::models::update::UpdateCheck;

const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/Stasyanz/syzify/releases/latest";
const RELEASES_PAGE: &str = "https://github.com/Stasyanz/syzify/releases";

/// "v0.1.2" / "0.1.2" → (0, 1, 2). Strictly three numeric parts — anything
/// else (pre-release tags, garbage) is None so the comparison can refuse
/// rather than guess.
pub fn parse_semver(s: &str) -> Option<(u64, u64, u64)> {
    let mut parts = s.trim().trim_start_matches('v').split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// Numeric tuple ordering — 0.10.0 beats 0.9.9. Unparseable input on either
/// side means "not newer": never nag the user off the back of broken data.
pub fn is_newer(latest: &str, current: &str) -> bool {
    matches!(
        (parse_semver(latest), parse_semver(current)),
        (Some(l), Some(c)) if l > c
    )
}

#[derive(Debug, Deserialize)]
pub struct LatestRelease {
    pub tag_name: String,
    pub html_url: String,
}

/// Pure comparison step, separated from the HTTP fetch for tests.
pub fn evaluate(current: &str, latest: &LatestRelease) -> UpdateCheck {
    UpdateCheck {
        current_version: current.to_string(),
        latest_version: latest.tag_name.trim_start_matches('v').to_string(),
        update_available: is_newer(&latest.tag_name, current),
        release_url: if latest.html_url.is_empty() {
            RELEASES_PAGE.to_string()
        } else {
            latest.html_url.clone()
        },
    }
}

fn fetch_latest() -> Result<LatestRelease, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Syzify/1.0")
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;
    let resp = client
        .get(LATEST_RELEASE_API)
        .send()
        .map_err(|e| format!("Update check failed: {}", e))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("No releases published yet".into());
    }
    if !resp.status().is_success() {
        return Err(format!("GitHub returned status {}", resp.status()));
    }
    resp.json()
        .map_err(|e| format!("Unexpected release data: {}", e))
}

/// Blocking check: fetch the latest release and compare against the build's
/// own version. Runs inside `spawn_blocking` from the command.
pub fn check() -> Result<UpdateCheck, String> {
    let latest = fetch_latest()?;
    Ok(evaluate(env!("CARGO_PKG_VERSION"), &latest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_v_prefixed_versions() {
        assert_eq!(parse_semver("0.1.2"), Some((0, 1, 2)));
        assert_eq!(parse_semver("v10.20.30"), Some((10, 20, 30)));
        assert_eq!(parse_semver(" v0.1.1 "), Some((0, 1, 1)));
        assert_eq!(parse_semver("0.1"), None);
        assert_eq!(parse_semver("0.1.2.3"), None);
        assert_eq!(parse_semver("0.1.2-beta"), None);
        assert_eq!(parse_semver("latest"), None);
    }

    #[test]
    fn newer_compares_numerically_not_lexically() {
        assert!(is_newer("v0.2.0", "0.1.1"));
        assert!(is_newer("0.10.0", "0.9.9")); // lexical would say older
        assert!(!is_newer("0.1.1", "0.1.1"));
        // A dev build ahead of the last release must not be nagged.
        assert!(!is_newer("0.1.1", "0.2.0"));
        // Broken data on either side → "not newer", never a false alarm.
        assert!(!is_newer("banana", "0.1.1"));
        assert!(!is_newer("0.2.0", "banana"));
    }

    #[test]
    fn evaluate_builds_the_answer_and_falls_back_to_the_releases_page() {
        let latest = LatestRelease {
            tag_name: "v0.2.0".into(),
            html_url: "https://github.com/Stasyanz/syzify/releases/tag/v0.2.0".into(),
        };
        let r = evaluate("0.1.1", &latest);
        assert!(r.update_available);
        assert_eq!(r.latest_version, "0.2.0");
        assert_eq!(r.current_version, "0.1.1");
        assert!(r.release_url.ends_with("v0.2.0"));

        let bare = LatestRelease { tag_name: "v0.1.1".into(), html_url: String::new() };
        let r = evaluate("0.1.1", &bare);
        assert!(!r.update_available);
        assert_eq!(r.release_url, "https://github.com/Stasyanz/syzify/releases");
    }
}
