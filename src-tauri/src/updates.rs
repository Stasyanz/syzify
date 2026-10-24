//! Manual update check against GitHub Releases.
//!
//! Privacy invariant: the ONLY network calls happen on the user's explicit
//! clicks, and the Settings row discloses the endpoints. No background
//! polling. This module answers "is there a newer release?"; actually
//! updating is a second explicit click handled by `install_update`
//! (tauri-plugin-updater), which downloads the minisign-verified bundle
//! published next to each release.

use serde::Deserialize;

use crate::error::CommandError;
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

/// The latest release from `url` (the GitHub API; a loopback server in the
/// tests). What the user can act on is coded (#236): no network, no release
/// yet, GitHub refusing; a client that cannot be built or data that does not
/// parse stay uncoded.
fn fetch_latest(url: &str) -> Result<LatestRelease, CommandError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Syzify/1.0")
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;
    let resp = client.get(url).send().map_err(|e| {
        CommandError::coded("updates.checkFailed", format!("Update check failed: {}", e)).param("detail", e.to_string())
    })?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(CommandError::coded("updates.noReleases", "No releases published yet"));
    }
    if !resp.status().is_success() {
        return Err(CommandError::coded("updates.badStatus", format!("GitHub returned status {}", resp.status()))
            .param("status", resp.status().to_string()));
    }
    Ok(resp.json().map_err(|e| format!("Unexpected release data: {}", e))?)
}

/// Blocking check: fetch the latest release and compare against the build's
/// own version. Runs inside `spawn_blocking` from the command.
pub fn check() -> Result<UpdateCheck, CommandError> {
    let latest = fetch_latest(LATEST_RELEASE_API)?;
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

    /// Losing any of these silently would break the whole update trust
    /// model: no signature enforcement, or a manifest fetched from a host
    /// we don't control. CI must fail loudly instead.
    #[test]
    fn updater_config_keeps_its_trust_anchor() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(conf["bundle"]["createUpdaterArtifacts"], true);
        let pubkey = conf["plugins"]["updater"]["pubkey"].as_str().unwrap();
        // minisign pubkeys start with the base64 of "untrusted comment:".
        assert!(pubkey.starts_with("dW50cnVzdGVk"));
        let endpoints = conf["plugins"]["updater"]["endpoints"].as_array().unwrap();
        assert!(!endpoints.is_empty());
        for e in endpoints {
            assert!(e
                .as_str()
                .unwrap()
                .starts_with("https://github.com/Stasyanz/syzify/"));
        }
    }

    /// Privacy invariant: only the thin `install_update` command may touch
    /// the updater plugin. Granting any `updater:*` IPC permission would let
    /// webview code start network calls without the explicit Settings click.
    #[test]
    fn webview_cannot_reach_the_updater_directly() {
        let caps = include_str!("../capabilities/default.json");
        assert!(!caps.contains("updater:"));
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

    /// What GitHub answers is the user's to read in their language (#236):
    /// no release yet, a refusal by status, no network; data that does not
    /// parse stays the library's words.
    #[test]
    fn fetch_latest_says_why_in_coded_words() {
        use crate::plugins::net::test_server::{Reply, Server};
        let server = Server::start(|r| match r.path.as_str() {
            "/missing" => Reply::ok("").status(404),
            "/down" => Reply::ok("").status(503),
            "/junk" => Reply::ok("not json"),
            _ => Reply::ok(r#"{"tag_name":"v9.9.9","html_url":""}"#),
        });
        assert_eq!(fetch_latest(&server.url("/latest")).unwrap().tag_name, "v9.9.9");
        assert_eq!(
            fetch_latest(&server.url("/missing")).unwrap_err(),
            CommandError::coded("updates.noReleases", "No releases published yet")
        );
        let e = fetch_latest(&server.url("/down")).unwrap_err();
        assert_eq!(e.code, Some("updates.badStatus"));
        assert_eq!(e.params["status"].as_str(), Some("503 Service Unavailable"));
        assert_eq!(e.message, "GitHub returned status 503 Service Unavailable");
        let e = fetch_latest(&server.url("/junk")).unwrap_err();
        assert_eq!(e.code, None);
        assert!(e.message.starts_with("Unexpected release data"), "{e}");

        // A port nothing listens on: no network.
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let e = fetch_latest(&format!("http://127.0.0.1:{port}/latest")).unwrap_err();
        assert_eq!(e.code, Some("updates.checkFailed"));
        assert_eq!(e.message, format!("Update check failed: {}", e.params["detail"].as_str().unwrap()));
    }
}
