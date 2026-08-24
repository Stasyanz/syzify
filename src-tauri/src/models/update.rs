use serde::{Deserialize, Serialize};

/// Result of a MANUAL update check against GitHub Releases. Updating stays a
/// user action — `release_url` opens the release page in the browser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheck {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: String,
}
