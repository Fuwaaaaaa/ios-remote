use serde::Deserialize;
use tracing::info;

const GITHUB_REPO: &str = "Fuwaaaaaa/ios-remote";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    body: Option<String>,
}

/// Check GitHub Releases for a newer version.
pub fn check_for_update() -> Result<Option<UpdateInfo>, String> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );

    use crate::features::http::{HttpRequest, send};

    let response = send(
        &HttpRequest::new("GET", &url)
            .header("User-Agent: ios-remote")
            .header("Accept: application/vnd.github+json")
            .timeout_secs(15),
    )?;
    match response.status {
        200 => {}
        404 => return Err("no published GitHub release found".to_string()),
        403 | 429 => return Err("GitHub API rate limit reached — try again later".to_string()),
        s => return Err(format!("GitHub API returned HTTP {s}")),
    }

    let release: GithubRelease =
        serde_json::from_str(&response.body).map_err(|e| format!("JSON parse error: {e}"))?;

    let remote_ver = release.tag_name.trim_start_matches('v');

    if version_newer(remote_ver, CURRENT_VERSION) {
        info!(
            current = CURRENT_VERSION,
            latest = remote_ver,
            "Update available"
        );
        Ok(Some(UpdateInfo {
            current: CURRENT_VERSION.to_string(),
            latest: remote_ver.to_string(),
            url: release.html_url,
            changelog: release.body.unwrap_or_default(),
        }))
    } else {
        info!(version = CURRENT_VERSION, "Already up to date");
        Ok(None)
    }
}

#[derive(Debug)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub url: String,
    pub changelog: String,
}

/// Simple semver comparison (a > b?).
fn version_newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> { v.split('.').filter_map(|s| s.parse().ok()).collect() };
    let va = parse(a);
    let vb = parse(b);
    for i in 0..va.len().max(vb.len()) {
        let a = va.get(i).copied().unwrap_or(0);
        let b = vb.get(i).copied().unwrap_or(0);
        if a > b {
            return true;
        }
        if a < b {
            return false;
        }
    }
    false
}
