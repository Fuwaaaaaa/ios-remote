//! End-to-end black-box tests for `--synthetic` mode. Spawns the ios-remote
//! binary as a subprocess and exercises the REST API + dummy WDA stub.
//!
//! Each test claims its own loopback ports so they can run in parallel
//! without colliding. Subprocesses are killed when `Subprocess` drops.

#![cfg(target_os = "windows")]

use std::process::{Child, Command, Stdio};
use std::time::Duration;

const TEST_TOKEN: &str = "synthetic-e2e-token";

struct Subprocess {
    child: Child,
}

impl Drop for Subprocess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn_synthetic(web_port: u16, wda_port: u16) -> Subprocess {
    let bin = env!("CARGO_BIN_EXE_ios-remote");
    let child = Command::new(bin)
        .args([
            "--synthetic",
            "--web-port",
            &web_port.to_string(),
            "--synthetic-wda-port",
            &wda_port.to_string(),
            "--bind",
            "127.0.0.1",
            "--token",
            TEST_TOKEN,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn ios-remote");
    Subprocess { child }
}

fn curl(args: &[&str]) -> (u16, String) {
    let mut full = vec!["-sS", "-o", "-", "-w", "\n%{http_code}", "--max-time", "5"];
    full.extend_from_slice(args);
    let out = Command::new("curl")
        .args(&full)
        .output()
        .expect("curl spawn");
    let combined = String::from_utf8_lossy(&out.stdout).to_string();
    let (body, code) = combined.rsplit_once('\n').unwrap_or(("", &combined));
    (code.trim().parse().unwrap_or(0), body.to_string())
}

fn http_get(url: &str) -> (u16, String) {
    curl(&[
        "-H",
        concat!("Authorization: Bearer ", "synthetic-e2e-token"),
        url,
    ])
}

fn http_post(url: &str, body: &str) -> (u16, String) {
    curl(&[
        "-H",
        concat!("Authorization: Bearer ", "synthetic-e2e-token"),
        "-H",
        "Content-Type: application/json",
        "-X",
        "POST",
        url,
        "-d",
        body,
    ])
}

fn wait_until_ready(port: u16) {
    // 30s total: enough for cold-start on a fresh `cargo test --test
    // synthetic_e2e` invocation where the OS hasn't paged in the binary yet.
    for _ in 0..120 {
        let (code, _) = http_get(&format!("http://127.0.0.1:{port}/api/status"));
        if code == 200 {
            return;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    panic!("API never became ready on port {port} within 30s");
}

#[test]
fn status_reports_synthetic_device_connected() {
    let _proc = spawn_synthetic(38080, 38101);
    wait_until_ready(38080);
    let (code, body) = http_get("http://127.0.0.1:38080/api/status");
    assert_eq!(code, 200, "body={body}");
    assert!(
        body.contains("connected"),
        "expected status=connected, body={body}"
    );
    assert!(
        body.contains("Synthetic"),
        "expected Synthetic device name, body={body}"
    );
}

#[test]
fn stats_report_synthetic_resolution_and_fps() {
    let _proc = spawn_synthetic(38082, 38102);
    wait_until_ready(38082);
    let (code, body) = http_get("http://127.0.0.1:38082/api/stats");
    assert_eq!(code, 200);
    assert!(body.contains("\"resolution\":\"390x844\""), "got: {body}");
    assert!(body.contains("\"fps\":30"), "got: {body}");
    assert!(body.contains("\"connected\":true"), "got: {body}");
}

#[test]
fn screenshot_returns_a_path_when_frames_flow() {
    let _proc = spawn_synthetic(38083, 38103);
    wait_until_ready(38083);
    // Wait one full frame interval to ensure FrameBus has data.
    std::thread::sleep(Duration::from_millis(300));
    let (code, body) = http_post("http://127.0.0.1:38083/api/screenshot", "{}");
    assert_eq!(code, 200, "screenshot should succeed, body={body}");
    assert!(
        body.contains("path") && body.contains(".png"),
        "got: {body}"
    );
}

#[test]
fn recording_lifecycle_returns_success() {
    let _proc = spawn_synthetic(38084, 38104);
    wait_until_ready(38084);
    std::thread::sleep(Duration::from_millis(300));
    let (start_code, start_body) = http_post("http://127.0.0.1:38084/api/recording/start", "{}");
    assert_eq!(start_code, 200, "start body={start_body}");
    std::thread::sleep(Duration::from_secs(1));
    let (stop_code, stop_body) = http_post("http://127.0.0.1:38084/api/recording/stop", "{}");
    assert_eq!(stop_code, 200, "stop body={stop_body}");
    // The H.264 file is empty without ffmpeg, but the controller lifecycle
    // succeeds regardless and the file path is reported. That's what we
    // want from synthetic mode — the API never bubbles up missing-device errors.
}

#[test]
fn subtitles_populate_within_first_second() {
    let _proc = spawn_synthetic(38085, 38105);
    wait_until_ready(38085);
    // The subtitle pump's first tick fires immediately on spawn; give it
    // a bit of headroom for the cargo-spawned process to reach the await.
    std::thread::sleep(Duration::from_millis(700));
    let (code, body) = http_get("http://127.0.0.1:38085/api/subtitles");
    assert_eq!(code, 200);
    assert!(
        body.contains("WELCOME") || body.contains("IPHONE") || body.contains("DASHBOARD"),
        "expected at least one synthetic subtitle line, got: {body}"
    );
}

#[test]
fn dummy_wda_status_returns_ready() {
    let _proc = spawn_synthetic(38086, 38106);
    wait_until_ready(38086);
    // The WDA stub doesn't need auth — it's the macros-side surface.
    let (code, body) = curl(&["http://127.0.0.1:38106/status"]);
    assert_eq!(code, 200);
    assert!(body.contains("\"ready\":true"), "got: {body}");
    assert!(body.contains("synthetic"), "got: {body}");
}

#[test]
fn dummy_wda_tap_returns_200_via_session() {
    let _proc = spawn_synthetic(38087, 38107);
    wait_until_ready(38087);

    // Create a session like the real WDA flow does.
    let (sess_code, sess_body) = curl(&[
        "-H",
        "Content-Type: application/json",
        "-X",
        "POST",
        "http://127.0.0.1:38107/session",
        "-d",
        r#"{"capabilities":{"alwaysMatch":{}}}"#,
    ]);
    assert_eq!(sess_code, 200);
    assert!(sess_body.contains("sessionId"), "got: {sess_body}");

    let (tap_code, tap_body) = curl(&[
        "-H",
        "Content-Type: application/json",
        "-X",
        "POST",
        "http://127.0.0.1:38107/session/abc123/wda/tap/0",
        "-d",
        r#"{"x":195,"y":420}"#,
    ]);
    assert_eq!(tap_code, 200);
    assert!(tap_body.contains("value"), "got: {tap_body}");
}
