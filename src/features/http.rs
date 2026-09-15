//! Tiny HTTP client on top of the system `curl.exe` (bundled with Windows 10
//! 1803+), so features that talk to web APIs don't pull in an HTTP stack.
//!
//! Every call goes through `curl -K -`: the request is described in a config
//! file fed over stdin, which keeps two things off the process command line
//! (readable by other local users via `tasklist /v`, WMI, Process Explorer):
//! - secrets such as `x-api-key` / `Authorization` headers, and
//! - large bodies — Windows caps a command line at 32,767 characters, which
//!   a base64-encoded screenshot blows through. Bodies go to a temp file that
//!   is deleted right after the call.
//!
//! The HTTP status code is always captured, so callers can tell a 401/429/5xx
//! from success instead of parsing an error body as if it were the answer.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct HttpRequest<'a> {
    pub method: &'a str,
    pub url: &'a str,
    /// Full header lines, e.g. `"content-type: application/json"`.
    pub headers: Vec<String>,
    /// Raw request body (sent with `--data-binary`).
    pub body: Option<&'a [u8]>,
    /// multipart form fields in curl `-F` syntax, e.g. `"file=@C:/a.wav"`.
    pub form: Vec<String>,
    pub timeout_secs: u64,
}

impl<'a> HttpRequest<'a> {
    pub fn new(method: &'a str, url: &'a str) -> Self {
        Self {
            method,
            url,
            headers: Vec::new(),
            body: None,
            form: Vec::new(),
            timeout_secs: 30,
        }
    }

    #[must_use]
    pub fn header(mut self, line: impl Into<String>) -> Self {
        self.headers.push(line.into());
        self
    }

    #[must_use]
    pub fn json_body(mut self, body: &'a [u8]) -> Self {
        self.headers
            .push("content-type: application/json".to_string());
        self.body = Some(body);
        self
    }

    #[must_use]
    pub fn form_field(mut self, field: impl Into<String>) -> Self {
        self.form.push(field.into());
        self
    }

    #[must_use]
    pub fn timeout_secs(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }
}

#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Escape a value for a double-quoted `curl -K` config string. Per `curl(1)`,
/// backslash and double quote are the specials; newlines must not appear
/// literally (they would end the line), so they are escaped too.
pub fn escape_config_value(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

fn build_config(req: &HttpRequest<'_>, body_path: Option<&str>) -> String {
    let mut config = String::new();
    config.push_str("silent\nshow-error\n");
    config.push_str(&format!(
        "request = \"{}\"\n",
        escape_config_value(req.method)
    ));
    config.push_str(&format!("url = \"{}\"\n", escape_config_value(req.url)));
    config.push_str(&format!("max-time = {}\n", req.timeout_secs.max(1)));
    for h in &req.headers {
        config.push_str(&format!("header = \"{}\"\n", escape_config_value(h)));
    }
    for f in &req.form {
        config.push_str(&format!("form = \"{}\"\n", escape_config_value(f)));
    }
    if let Some(path) = body_path {
        config.push_str(&format!(
            "data-binary = \"@{}\"\n",
            escape_config_value(path)
        ));
    }
    // Status code on its own final line; split off in `parse_output`.
    config.push_str("write-out = \"\\n%{http_code}\"\n");
    config
}

fn parse_output(stdout: &[u8]) -> Result<HttpResponse, String> {
    let text = String::from_utf8_lossy(stdout);
    let (body, code) = text
        .rsplit_once('\n')
        .ok_or_else(|| "curl produced no status line".to_string())?;
    let status = code
        .trim()
        .parse::<u16>()
        .map_err(|_| format!("curl produced an invalid status line: {code:?}"))?;
    Ok(HttpResponse {
        status,
        body: body.to_string(),
    })
}

/// Unique temp file for a request body; removed when dropped.
struct TempBody(std::path::PathBuf);

impl TempBody {
    fn write(bytes: &[u8]) -> Result<Self, String> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("ios_remote_http_{}_{n}.body", std::process::id()));
        std::fs::write(&path, bytes).map_err(|e| format!("write request body: {e}"))?;
        Ok(Self(path))
    }
}

impl Drop for TempBody {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Perform the request. `Err` means the request never completed (curl
/// missing, DNS / connection failure, timeout); any HTTP status — including
/// 4xx/5xx — is `Ok` with `status` set.
pub fn send(req: &HttpRequest<'_>) -> Result<HttpResponse, String> {
    let temp = req.body.map(TempBody::write).transpose()?;
    // curl's config parser treats backslashes as escapes; forward slashes
    // work fine for Windows paths.
    let body_path = temp
        .as_ref()
        .map(|t| t.0.display().to_string().replace('\\', "/"));
    let config = build_config(req, body_path.as_deref());

    let mut child = Command::new("curl")
        .args(["-K", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl not available: {e}"))?;
    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| "curl stdin not available".to_string())?;
        stdin
            .write_all(config.as_bytes())
            .map_err(|e| format!("curl stdin write failed: {e}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("curl wait failed: {e}"))?;
    drop(temp);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "request to {} failed (curl exit {}): {}",
            req.url,
            output.status.code().unwrap_or(-1),
            stderr.trim()
        ));
    }
    parse_output(&output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_keeps_secrets_quoted_and_escaped() {
        let req = HttpRequest::new("POST", "https://api.example/v1")
            .header("x-api-key: se\"cr\\et")
            .timeout_secs(5);
        let cfg = build_config(&req, Some("C:/tmp/body"));
        assert!(cfg.contains("header = \"x-api-key: se\\\"cr\\\\et\"\n"));
        assert!(cfg.contains("data-binary = \"@C:/tmp/body\"\n"));
        assert!(cfg.contains("max-time = 5\n"));
        assert!(cfg.ends_with("write-out = \"\\n%{http_code}\"\n"));
    }

    #[test]
    fn escape_handles_every_special() {
        assert_eq!(escape_config_value("plain"), "plain");
        assert_eq!(escape_config_value("a\\b"), "a\\\\b");
        assert_eq!(escape_config_value("a\"b"), "a\\\"b");
        assert_eq!(escape_config_value("a\r\nb"), "a\\r\\nb");
    }

    #[test]
    fn newline_in_a_value_cannot_inject_a_directive() {
        let req = HttpRequest::new("GET", "https://a").header("x: 1\nurl = \"https://evil\"");
        let cfg = build_config(&req, None);
        assert_eq!(cfg.matches("\nurl = ").count(), 1, "config:\n{cfg}");
    }

    #[test]
    fn status_line_is_split_from_body() {
        let r = parse_output(b"{\"ok\":true}\n200").unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.body, "{\"ok\":true}");
        assert!(r.is_success());
        let r = parse_output(b"line1\nline2\n401").unwrap();
        assert_eq!(r.status, 401);
        assert_eq!(r.body, "line1\nline2");
        assert!(!r.is_success());
        assert!(parse_output(b"").is_err());
    }

    #[test]
    fn unreachable_host_is_an_error_not_a_status() {
        // Port 1 on loopback refuses immediately.
        let req = HttpRequest::new("GET", "http://127.0.0.1:1/").timeout_secs(3);
        assert!(send(&req).is_err());
    }
}
