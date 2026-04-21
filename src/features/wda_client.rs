use serde::Deserialize;
use std::process::Command;
use std::sync::Mutex;
use tracing::{debug, info, warn};

/// Minimal client for WebDriverAgent (WDA) — the service we use to inject
/// touch/swipe input into the iPhone. The usbmuxd `screenshotr` service is
/// strictly read-only, so sending input requires a developer-signed WDA build
/// installed on the device (see README "Macro setup" section).
///
/// The client targets WDA's HTTP API (usually on device port 8100, forwarded
/// to a host port by `iproxy` or equivalent). All requests are issued through
/// the bundled `curl.exe` to avoid adding a new HTTP dependency for a feature
/// that not every user exercises.
pub struct WdaClient {
    base_url: String,
    session: Mutex<Option<String>>,
}

#[derive(Debug, thiserror::Error)]
pub enum WdaError {
    #[error("WDA not reachable at {0} — is WebDriverAgent running and iproxy forwarded?")]
    Unreachable(String),
    #[error("WDA returned non-success: {0}")]
    BadResponse(String),
    #[error("WDA response parse failed: {0}")]
    ParseError(String),
    #[error("curl not available: {0}")]
    Curl(String),
}

#[derive(Deserialize)]
struct SessionResp {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    value: Option<SessionValue>,
}

#[derive(Deserialize)]
struct SessionValue {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
}

impl WdaClient {
    /// Construct a client against the given base URL, e.g. `http://127.0.0.1:8100`.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            session: Mutex::new(None),
        }
    }

    /// Lazily create a WDA session. Returns the session id.
    pub fn ensure_session(&self) -> Result<String, WdaError> {
        {
            let guard = self.session.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(id) = guard.as_ref() {
                return Ok(id.clone());
            }
        }

        let body = r#"{"capabilities":{"alwaysMatch":{}}}"#;
        let resp = self.post_json("/session", body)?;
        let parsed: SessionResp = serde_json::from_str(&resp)
            .map_err(|e| WdaError::ParseError(format!("{e}: {resp}")))?;
        let id = parsed
            .session_id
            .or(parsed.value.and_then(|v| v.session_id))
            .ok_or_else(|| WdaError::BadResponse(resp.clone()))?;

        info!(session = %id, "WDA session created");
        let mut guard = self.session.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(id.clone());
        Ok(id)
    }

    /// Tap at (x, y) in point coordinates (not pixels — WDA uses CSS-like points).
    pub fn tap(&self, x: u32, y: u32) -> Result<(), WdaError> {
        let session = self.ensure_session()?;
        let body = format!(r#"{{"x":{x},"y":{y}}}"#);
        self.post_json(&format!("/session/{session}/wda/tap/0"), &body)?;
        debug!(x, y, "WDA tap");
        Ok(())
    }

    /// Swipe from `(x1,y1)` to `(x2,y2)` over `duration_ms`.
    pub fn swipe(&self, x1: u32, y1: u32, x2: u32, y2: u32, duration_ms: u64) -> Result<(), WdaError> {
        let session = self.ensure_session()?;
        let seconds = duration_ms as f64 / 1000.0;
        let body = format!(
            r#"{{"fromX":{x1},"fromY":{y1},"toX":{x2},"toY":{y2},"duration":{seconds}}}"#
        );
        self.post_json(
            &format!("/session/{session}/wda/dragfromtoforduration"),
            &body,
        )?;
        debug!(x1, y1, x2, y2, duration_ms, "WDA swipe");
        Ok(())
    }

    /// Long press at (x, y) for `duration_ms`.
    pub fn long_press(&self, x: u32, y: u32, duration_ms: u64) -> Result<(), WdaError> {
        let session = self.ensure_session()?;
        let seconds = duration_ms as f64 / 1000.0;
        let body = format!(r#"{{"x":{x},"y":{y},"duration":{seconds}}}"#);
        self.post_json(&format!("/session/{session}/wda/touchAndHold"), &body)?;
        debug!(x, y, duration_ms, "WDA long press");
        Ok(())
    }

    fn post_json(&self, path: &str, body: &str) -> Result<String, WdaError> {
        let url = format!("{}{}", self.base_url, path);
        let output = Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "10",
                "-X",
                "POST",
                &url,
                "-H",
                "Content-Type: application/json",
                "-d",
                body,
            ])
            .output()
            .map_err(|e| WdaError::Curl(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(stderr = %stderr, url = %url, "WDA request failed");
            return Err(WdaError::Unreachable(url));
        }

        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// Read the WDA endpoint from `IOS_REMOTE_WDA_URL`, defaulting to the common
/// forwarded local port `http://127.0.0.1:8100`.
pub fn default_wda_client() -> WdaClient {
    let url = std::env::var("IOS_REMOTE_WDA_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8100".to_string());
    WdaClient::new(url)
}
