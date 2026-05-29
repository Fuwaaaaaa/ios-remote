//! End-to-end black-box tests for `--synthetic` mode. Spawns the ios-remote
//! binary as a subprocess and exercises the REST API + dummy WDA stub.
//!
//! Each test claims its own loopback ports so they can run in parallel
//! without colliding. Subprocesses are killed when `Subprocess` drops.
//!
//! Set `IOS_REMOTE_E2E_LOGS=1` to dump each subprocess's captured stdout/stderr
//! when its `Subprocess` drops — invaluable for triaging CI failures where only
//! HTTP codes are otherwise visible (I5).

#![cfg(target_os = "windows")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::Duration;

const TEST_TOKEN: &str = "synthetic-e2e-token";
/// PALETTE[0] in renderer.rs — the colour of home icon 0 (app "A").
const ICON0_COLOR: [u8; 4] = [220, 60, 60, 255];

fn logs_enabled() -> bool {
    std::env::var("IOS_REMOTE_E2E_LOGS").is_ok()
}

struct Subprocess {
    child: Child,
    log_path: PathBuf,
}

impl Subprocess {
    /// Poll for process exit up to `secs`, returning the status if it exited.
    fn wait_for_exit(&mut self, secs: u64) -> Option<ExitStatus> {
        for _ in 0..(secs * 4) {
            match self.child.try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) => std::thread::sleep(Duration::from_millis(250)),
                Err(_) => return None,
            }
        }
        None
    }
}

impl Drop for Subprocess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if logs_enabled()
            && let Ok(contents) = fs::read_to_string(&self.log_path)
        {
            eprintln!(
                "─── subprocess log ({}) ───\n{}\n─── end log ───",
                self.log_path.display(),
                contents
            );
        }
        let _ = fs::remove_file(&self.log_path);
    }
}

/// Spawn `--synthetic` with the given ports, capturing stdio to a temp log.
fn spawn_synthetic(web_port: u16, wda_port: u16) -> Subprocess {
    spawn_opts(web_port, wda_port, None, &[])
}

/// Spawn `--synthetic` with optional working directory + extra args. Stdio is
/// routed to a per-port temp log file so failures can be triaged (I5).
fn spawn_opts(web_port: u16, wda_port: u16, cwd: Option<&Path>, extra: &[&str]) -> Subprocess {
    let bin = env!("CARGO_BIN_EXE_ios-remote");
    let log_path = std::env::temp_dir().join(format!("ios_remote_e2e_{web_port}_{wda_port}.log"));
    let out = fs::File::create(&log_path).expect("create log file");
    let err = out.try_clone().expect("clone log handle");

    let mut cmd = Command::new(bin);
    cmd.args([
        "--synthetic",
        "--web-port",
        &web_port.to_string(),
        "--synthetic-wda-port",
        &wda_port.to_string(),
        "--bind",
        "127.0.0.1",
        "--token",
        TEST_TOKEN,
    ]);
    cmd.args(extra);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let child = cmd
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .spawn()
        .expect("spawn ios-remote");
    Subprocess { child, log_path }
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

/// Open a WDA session on the dummy stub and return its id.
fn wda_open_session(wda_port: u16) -> String {
    let (code, body) = curl(&[
        "-H",
        "Content-Type: application/json",
        "-X",
        "POST",
        &format!("http://127.0.0.1:{wda_port}/session"),
        "-d",
        r#"{"capabilities":{"alwaysMatch":{}}}"#,
    ]);
    assert_eq!(code, 200, "session body={body}");
    "synthetic-session-0001".to_string()
}

fn wda_tap(wda_port: u16, session: &str, x: u32, y: u32) -> (u16, String) {
    curl(&[
        "-H",
        "Content-Type: application/json",
        "-X",
        "POST",
        &format!("http://127.0.0.1:{wda_port}/session/{session}/wda/tap/0"),
        "-d",
        &format!(r#"{{"x":{x},"y":{y}}}"#),
    ])
}

/// Poll `GET /api/synthetic/state` until its body contains `needle`. Returns the
/// last body seen (for assert messages).
fn poll_state(port: u16, needle: &str, attempts: u32) -> (bool, String) {
    let url = format!("http://127.0.0.1:{port}/api/synthetic/state");
    let mut last = String::new();
    for _ in 0..attempts {
        let (code, body) = http_get(&url);
        last = body;
        if code == 200 && last.contains(needle) {
            return (true, last);
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    (false, last)
}

// ─── Existing coverage ─────────────────────────────────────────────────────

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
    let sid = wda_open_session(38107);
    let (tap_code, tap_body) = wda_tap(38107, &sid, 195, 420);
    assert_eq!(tap_code, 200);
    assert!(tap_body.contains("value"), "got: {tap_body}");
}

// ─── I6: drag + long-press via session ──────────────────────────────────────

#[test]
fn dummy_wda_drag_returns_200_via_session() {
    let _proc = spawn_synthetic(38088, 38108);
    wait_until_ready(38088);
    let sid = wda_open_session(38108);
    let (code, body) = curl(&[
        "-H",
        "Content-Type: application/json",
        "-X",
        "POST",
        &format!("http://127.0.0.1:38108/session/{sid}/wda/dragfromtoforduration"),
        "-d",
        r#"{"fromX":300,"fromY":400,"toX":80,"toY":400,"duration":0.2}"#,
    ]);
    assert_eq!(code, 200);
    assert!(body.contains("value"), "got: {body}");
}

#[test]
fn dummy_wda_long_press_returns_200_via_session() {
    let _proc = spawn_synthetic(38089, 38109);
    wait_until_ready(38089);
    let sid = wda_open_session(38109);
    let (code, body) = curl(&[
        "-H",
        "Content-Type: application/json",
        "-X",
        "POST",
        &format!("http://127.0.0.1:38109/session/{sid}/wda/touchAndHold"),
        "-d",
        r#"{"x":78,"y":114,"duration":1.0}"#,
    ]);
    assert_eq!(code, 200);
    assert!(body.contains("value"), "got: {body}");
}

// ─── I6: --synthetic --diag short-circuits ──────────────────────────────────

#[test]
fn synthetic_with_diag_exits_without_web_server() {
    // --diag short-circuits at the top of main() before the synthetic web bind.
    let mut proc = spawn_opts(38090, 38110, None, &["--diag"]);
    let status = proc.wait_for_exit(25);
    assert!(
        status.is_some(),
        "--synthetic --diag should exit quickly, not run the server"
    );
    // The web dashboard must never have come up.
    let (code, _) = http_get("http://127.0.0.1:38090/api/status");
    assert_ne!(code, 200, "diag mode must not bind the web dashboard");
}

// ─── I6: subtitle rotation across multiple ticks ────────────────────────────

#[test]
fn subtitles_rotate_across_multiple_ticks() {
    let _proc = spawn_synthetic(38091, 38111);
    wait_until_ready(38091);
    // Pump ticks every 5s; each marker uniquely identifies one rotated line, so
    // seeing ≥2 proves rotation across ticks (not just tick #1). ~12s budget.
    let markers = [
        "WELCOME",
        "NO IPHONE",
        "RECORDING AND REPLAY",
        "HOTKEY S",
        "HOTKEY F2",
        "DASHBOARD",
    ];
    let mut seen = std::collections::HashSet::new();
    for _ in 0..26 {
        let (code, body) = http_get("http://127.0.0.1:38091/api/subtitles");
        if code == 200 {
            for m in markers {
                if body.contains(m) {
                    seen.insert(m);
                }
            }
        }
        if seen.len() >= 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    assert!(
        seen.len() >= 2,
        "expected the subtitle pump to rotate ≥2 distinct lines, saw {seen:?}"
    );
}

// ─── Port collision regression (locks in the C2 fix) ────────────────────────

#[test]
fn wda_port_collision_exits_nonzero_with_clear_error() {
    // Hold the WDA port so the synthetic stub can't bind it.
    let _hog = std::net::TcpListener::bind(("127.0.0.1", 38112u16)).expect("pre-bind wda port");
    let mut proc = spawn_opts(38092, 38112, None, &[]);
    let status = proc
        .wait_for_exit(25)
        .expect("process should exit on WDA bind failure");
    assert!(
        !status.success(),
        "expected non-zero exit when the WDA port is already in use"
    );
}

// ─── Interactivity: input → shared state → screen ───────────────────────────

#[test]
fn tap_app_icon_opens_app_via_state() {
    let _proc = spawn_synthetic(38093, 38113);
    wait_until_ready(38093);
    // Initially Home.
    let (ok, body) = poll_state(38093, "\"screen\":\"home\"", 20);
    assert!(ok, "expected initial home screen, got {body}");
    // Tap the centre of home icon 0 (grid_left(390)=48 → centre ≈ (78,114)).
    let sid = wda_open_session(38113);
    let (code, _) = wda_tap(38113, &sid, 78, 114);
    assert_eq!(code, 200);
    let (ok, body) = poll_state(38093, "\"screen\":\"app\"", 20);
    assert!(ok, "tap should open an app, state={body}");
    assert!(
        body.contains("\"letter\":\"A\""),
        "expected app A, got {body}"
    );
}

#[test]
fn home_indicator_returns_to_home() {
    let _proc = spawn_synthetic(38094, 38114);
    wait_until_ready(38094);
    let sid = wda_open_session(38114);
    // Open an app.
    wda_tap(38114, &sid, 78, 114);
    let (ok, _) = poll_state(38094, "\"screen\":\"app\"", 20);
    assert!(ok, "precondition: app open");
    // Tap the home indicator (bottom-centre ≈ (195,822)).
    wda_tap(38114, &sid, 195, 822);
    let (ok, body) = poll_state(38094, "\"screen\":\"home\"", 20);
    assert!(ok, "home indicator should return home, state={body}");
}

#[test]
fn swipe_changes_home_page() {
    let _proc = spawn_synthetic(38095, 38115);
    wait_until_ready(38095);
    let sid = wda_open_session(38115);
    let (code, _) = curl(&[
        "-H",
        "Content-Type: application/json",
        "-X",
        "POST",
        &format!("http://127.0.0.1:38115/session/{sid}/wda/dragfromtoforduration"),
        "-d",
        r#"{"fromX":320,"fromY":420,"toX":70,"toY":420,"duration":0.2}"#,
    ]);
    assert_eq!(code, 200);
    let (ok, body) = poll_state(38095, "\"page\":1", 20);
    assert!(ok, "swipe-left should advance to page 1, state={body}");
}

// ─── Macro pipeline drives the interactive device end-to-end ────────────────

fn macro_workspace(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ios_remote_e2e_ws_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("macros")).expect("create macro workspace");
    dir
}

fn write_macro(ws: &Path, name: &str, value: serde_json::Value) {
    let json = serde_json::to_string_pretty(&value).expect("serialize macro");
    fs::write(ws.join("macros").join(format!("{name}.json")), json).expect("write macro");
}

fn write_solid_png(path: &Path, w: u32, h: u32, c: [u8; 4]) {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba(c));
    img.save(path).expect("write solid template");
}

#[test]
fn macro_tap_opens_app() {
    let ws = macro_workspace("tap");
    write_macro(
        &ws,
        "open_app",
        serde_json::json!({
            "name": "open_app",
            "description": "tap home icon 0",
            "actions": [ { "type": "Tap", "x": 78, "y": 114, "delay_ms": 100 } ]
        }),
    );
    let _proc = spawn_opts(38096, 38116, Some(&ws), &[]);
    wait_until_ready(38096);
    let (code, body) = http_post(
        "http://127.0.0.1:38096/api/macros/run",
        r#"{"name":"open_app"}"#,
    );
    assert_eq!(code, 200, "macro run body={body}");
    let (ok, state) = poll_state(38096, "\"screen\":\"app\"", 30);
    assert!(ok, "macro tap should open an app, state={state}");
    let _ = fs::remove_dir_all(&ws);
}

#[test]
fn macro_repeat_multiplies_interactions() {
    let ws = macro_workspace("repeat");
    write_macro(
        &ws,
        "repeat_tap",
        serde_json::json!({
            "name": "repeat_tap",
            "description": "tap then repeat 3×",
            "actions": [
                { "type": "Tap", "x": 78, "y": 114, "delay_ms": 50 },
                { "type": "Repeat", "count": 3, "actions_back": 1 }
            ]
        }),
    );
    let _proc = spawn_opts(38097, 38117, Some(&ws), &[]);
    wait_until_ready(38097);
    let (code, _) = http_post(
        "http://127.0.0.1:38097/api/macros/run",
        r#"{"name":"repeat_tap"}"#,
    );
    assert_eq!(code, 200);
    // 1 original + 3 repeated taps = 4 interactions.
    let (ok, state) = poll_state(38097, "\"interactions\":4", 40);
    assert!(ok, "Repeat should produce 4 interactions, state={state}");
    let _ = fs::remove_dir_all(&ws);
}

#[test]
fn macro_wait_for_screen_then_tap_runs() {
    let ws = macro_workspace("wfs_ok");
    let tpl = ws.join("icon0.png");
    // Solid icon-0 colour: matches the (uniform) top-left corner of icon 0.
    write_solid_png(&tpl, 20, 20, ICON0_COLOR);
    write_macro(
        &ws,
        "wfs_ok",
        serde_json::json!({
            "name": "wfs_ok",
            "description": "wait for icon 0 then tap it",
            "actions": [
                { "type": "WaitForScreen", "template_path": tpl.to_string_lossy(),
                  "timeout_ms": 5000, "region": [50, 86, 30, 30] },
                { "type": "Tap", "x": 78, "y": 114, "delay_ms": 50 }
            ]
        }),
    );
    let _proc = spawn_opts(38098, 38118, Some(&ws), &[]);
    wait_until_ready(38098);
    std::thread::sleep(Duration::from_millis(300)); // ensure a frame is published
    let (code, _) = http_post(
        "http://127.0.0.1:38098/api/macros/run",
        r#"{"name":"wfs_ok"}"#,
    );
    assert_eq!(code, 200);
    // The template matches → the trailing Tap runs → an app opens.
    let (ok, state) = poll_state(38098, "\"screen\":\"app\"", 40);
    assert!(ok, "WaitForScreen should match then tap, state={state}");
    let _ = fs::remove_dir_all(&ws);
}

#[test]
fn macro_wait_for_screen_timeout_skips_trailing_tap() {
    let ws = macro_workspace("wfs_to");
    let tpl = ws.join("never.png");
    // Structured (half black / half white) template — NCC ≈0.71 against the
    // uniform icon region, below the 0.85 threshold, so it never matches.
    let mut img = image::RgbaImage::from_pixel(20, 20, image::Rgba([0, 0, 0, 255]));
    for y in 10..20 {
        for x in 0..20 {
            img.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
        }
    }
    img.save(&tpl).expect("write structured template");
    write_macro(
        &ws,
        "wfs_to",
        serde_json::json!({
            "name": "wfs_to",
            "description": "wait for an absent screen (times out)",
            "actions": [
                { "type": "WaitForScreen", "template_path": tpl.to_string_lossy(),
                  "timeout_ms": 800, "region": [50, 86, 30, 30] },
                { "type": "Tap", "x": 78, "y": 114, "delay_ms": 50 }
            ]
        }),
    );
    let _proc = spawn_opts(38099, 38119, Some(&ws), &[]);
    wait_until_ready(38099);
    std::thread::sleep(Duration::from_millis(300));
    let (code, _) = http_post(
        "http://127.0.0.1:38099/api/macros/run",
        r#"{"name":"wfs_to"}"#,
    );
    assert_eq!(code, 200);
    // Give the macro time to time out (800ms) and (not) run the tap.
    std::thread::sleep(Duration::from_secs(2));
    let (_, state) = http_get("http://127.0.0.1:38099/api/synthetic/state");
    assert!(
        state.contains("\"screen\":\"home\"") && state.contains("\"interactions\":0"),
        "timeout must abort the macro before the tap, state={state}"
    );
    let _ = fs::remove_dir_all(&ws);
}
