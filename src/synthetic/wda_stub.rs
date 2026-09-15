//! Dummy WebDriverAgent HTTP server for `--synthetic` mode.
//!
//! When ios-remote runs without a real iPhone, the macros REST API still
//! invokes `WdaClient::tap()` / `swipe()` / `long_press()`, which curl-POST
//! to `IOS_REMOTE_WDA_URL` (default `http://127.0.0.1:8100`). This stub
//! responds successfully so the macros pipeline returns 200, **and** applies
//! the input to the shared [`DeviceState`] so the rendered screen actually
//! reacts (tap opens an app, swipe flips pages, the home/back gesture returns
//! home). That closes the input→display loop synthetic mode was missing.
//!
//! Logging is sanitised: only the parsed integer coordinates are emitted, never
//! the raw request body (I3 — avoids log-injection via a loopback POST).

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::post,
};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tracing::info;

use super::state::{SharedState, apply_long_press, apply_swipe, apply_tap, with_lock};

/// Shared context handed to every stub handler.
#[derive(Clone)]
struct StubState {
    device: SharedState,
    start: Arc<Instant>,
}

impl StubState {
    fn now_us(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }
}

/// Serve the dummy WDA endpoints on a pre-bound listener, applying input to the
/// shared device state. The caller is responsible for the bind so port-in-use
/// surfaces synchronously as a hard error (instead of silently leaving
/// `IOS_REMOTE_WDA_URL` pointing at a dead socket — or worse, another process
/// that happened to grab the port first).
pub fn serve(listener: TcpListener, device: SharedState, start: Arc<Instant>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let app = router(StubState { device, start });
        if let Ok(addr) = listener.local_addr() {
            info!(%addr, "[wda-stub] dummy WDA listening");
        }
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!(error = %e, "[wda-stub] server stopped");
        }
    })
}

fn router(state: StubState) -> Router {
    Router::new()
        .route("/session", post(create_session))
        .route("/session/{id}/wda/tap/0", post(handle_tap))
        .route("/session/{id}/wda/dragfromtoforduration", post(handle_drag))
        .route("/session/{id}/wda/touchAndHold", post(handle_hold))
        .route("/status", axum::routing::get(get_status))
        .with_state(state)
}

fn u32_field(body: &Value, key: &str) -> Option<u32> {
    body.get(key).and_then(Value::as_u64).map(|v| v as u32)
}

async fn create_session(Json(_body): Json<Value>) -> Json<Value> {
    let sid = "synthetic-session-0001";
    info!(session = sid, "[wda-stub] session created");
    Json(json!({
        "sessionId": sid,
        "value": { "sessionId": sid, "capabilities": {} }
    }))
}

async fn handle_tap(
    State(st): State<StubState>,
    Path(_id): Path<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    if let (Some(x), Some(y)) = (u32_field(&body, "x"), u32_field(&body, "y")) {
        info!(x, y, "[wda-stub] tap");
        let now = st.now_us();
        with_lock(&st.device, |d| apply_tap(d, x, y, now));
    }
    Json(json!({ "value": null }))
}

async fn handle_drag(
    State(st): State<StubState>,
    Path(_id): Path<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    if let (Some(x1), Some(y1), Some(x2), Some(y2)) = (
        u32_field(&body, "fromX"),
        u32_field(&body, "fromY"),
        u32_field(&body, "toX"),
        u32_field(&body, "toY"),
    ) {
        info!(
            from_x = x1,
            from_y = y1,
            to_x = x2,
            to_y = y2,
            "[wda-stub] drag"
        );
        let now = st.now_us();
        with_lock(&st.device, |d| apply_swipe(d, x1, y1, x2, y2, now));
    }
    Json(json!({ "value": null }))
}

async fn handle_hold(
    State(st): State<StubState>,
    Path(_id): Path<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    if let (Some(x), Some(y)) = (u32_field(&body, "x"), u32_field(&body, "y")) {
        info!(x, y, "[wda-stub] long press");
        let now = st.now_us();
        with_lock(&st.device, |d| apply_long_press(d, x, y, now));
    }
    Json(json!({ "value": null }))
}

async fn get_status() -> Json<Value> {
    Json(json!({ "value": { "ready": true, "message": "synthetic WDA ready" } }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::state::Screen;
    use std::net::SocketAddr;
    use std::time::Duration;

    async fn bind_random() -> (SocketAddr, JoinHandle<()>, SharedState) {
        // Bind a real loopback listener and hand it straight to serve() —
        // no drop-then-rebind dance, no race window.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind tmp");
        let addr = listener.local_addr().expect("local_addr");
        let device = crate::synthetic::state::new_shared(390, 844);
        let start = Arc::new(Instant::now());
        let handle = serve(listener, device.clone(), start);
        // Give axum::serve a moment to enter its accept loop.
        tokio::time::sleep(Duration::from_millis(50)).await;
        (addr, handle, device)
    }

    async fn http_post(url: &str, body: &str) -> (u16, String) {
        let url_s = url.to_string();
        let body_s = body.to_string();
        let out = tokio::task::spawn_blocking(move || {
            std::process::Command::new("curl")
                .args([
                    "-sS",
                    "-o",
                    "-",
                    "-w",
                    "\n%{http_code}",
                    "--max-time",
                    "5",
                    "-X",
                    "POST",
                    &url_s,
                    "-H",
                    "Content-Type: application/json",
                    "-d",
                    &body_s,
                ])
                .output()
                .expect("curl spawn")
        })
        .await
        .expect("join");
        let combined = String::from_utf8_lossy(&out.stdout).to_string();
        let (body, code_line) = combined.rsplit_once('\n').unwrap_or(("", &combined));
        (code_line.trim().parse().unwrap_or(0), body.to_string())
    }

    #[tokio::test]
    async fn session_endpoint_returns_session_id() {
        let (addr, h, _device) = bind_random().await;
        let (code, body) = http_post(
            &format!("http://{addr}/session"),
            r#"{"capabilities":{"alwaysMatch":{}}}"#,
        )
        .await;
        h.abort();
        assert_eq!(code, 200);
        assert!(body.contains("sessionId"), "got: {body}");
    }

    #[tokio::test]
    async fn tap_endpoint_returns_200() {
        let (addr, h, _device) = bind_random().await;
        let (code, body) = http_post(
            &format!("http://{addr}/session/abc/wda/tap/0"),
            r#"{"x":100,"y":200}"#,
        )
        .await;
        h.abort();
        assert_eq!(code, 200);
        assert!(body.contains("value"), "got: {body}");
    }

    #[tokio::test]
    async fn drag_endpoint_returns_200() {
        let (addr, h, _device) = bind_random().await;
        let (code, body) = http_post(
            &format!("http://{addr}/session/abc/wda/dragfromtoforduration"),
            r#"{"fromX":0,"fromY":0,"toX":100,"toY":200,"duration":0.5}"#,
        )
        .await;
        h.abort();
        assert_eq!(code, 200);
        assert!(body.contains("value"), "got: {body}");
    }

    #[tokio::test]
    async fn touch_and_hold_returns_200() {
        let (addr, h, _device) = bind_random().await;
        let (code, body) = http_post(
            &format!("http://{addr}/session/abc/wda/touchAndHold"),
            r#"{"x":50,"y":80,"duration":1.0}"#,
        )
        .await;
        h.abort();
        assert_eq!(code, 200);
        assert!(body.contains("value"), "got: {body}");
    }

    #[tokio::test]
    async fn tap_opens_app_in_shared_state() {
        let (addr, h, device) = bind_random().await;
        // Centre of the first home icon (row 0, col 0).
        let r = crate::synthetic::layout::icon_rect(390, 0, 0);
        let body = format!(r#"{{"x":{},"y":{}}}"#, r.x + r.w / 2, r.y + r.h / 2);
        let (code, _) = http_post(&format!("http://{addr}/session/s/wda/tap/0"), &body).await;
        h.abort();
        assert_eq!(code, 200);
        let screen = with_lock(&device, |d| d.screen);
        assert_eq!(screen, Screen::App { index: 0 });
    }

    #[tokio::test]
    async fn swipe_changes_home_page_via_http() {
        let (addr, h, device) = bind_random().await;
        let (code, _) = http_post(
            &format!("http://{addr}/session/s/wda/dragfromtoforduration"),
            r#"{"fromX":300,"fromY":400,"toX":80,"toY":400,"duration":0.2}"#,
        )
        .await;
        h.abort();
        assert_eq!(code, 200);
        let page = with_lock(&device, |d| d.home_page);
        assert_eq!(page, 1);
    }
}
