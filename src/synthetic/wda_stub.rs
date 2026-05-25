//! Dummy WebDriverAgent HTTP server for `--synthetic` mode.
//!
//! When ios-remote runs without a real iPhone, the macros REST API still
//! invokes `WdaClient::tap()` / `swipe()` / `long_press()`, which curl-POST
//! to `IOS_REMOTE_WDA_URL` (default `http://127.0.0.1:8100`). This stub
//! responds successfully so the entire macros pipeline returns 200 instead
//! of "WDA not reachable". Inputs are logged at `info!` for visibility.

use axum::{Json, Router, extract::Path, routing::post};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tracing::info;

/// Serve the dummy WDA endpoints on a pre-bound listener. The caller is
/// responsible for the bind so port-in-use surfaces synchronously as a
/// hard error (instead of silently leaving `IOS_REMOTE_WDA_URL` pointing
/// at a dead socket — or worse, another process that happened to grab the
/// port first).
pub fn serve(listener: TcpListener) -> JoinHandle<()> {
    tokio::spawn(async move {
        let app = router();
        if let Ok(addr) = listener.local_addr() {
            info!(%addr, "[wda-stub] dummy WDA listening");
        }
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!(error = %e, "[wda-stub] server stopped");
        }
    })
}

fn router() -> Router {
    Router::new()
        .route("/session", post(create_session))
        .route("/session/{id}/wda/tap/0", post(handle_tap))
        .route("/session/{id}/wda/dragfromtoforduration", post(handle_drag))
        .route("/session/{id}/wda/touchAndHold", post(handle_hold))
        .route("/status", axum::routing::get(get_status))
}

async fn create_session(Json(_body): Json<Value>) -> Json<Value> {
    let sid = "synthetic-session-0001";
    info!(session = sid, "[wda-stub] session created");
    Json(json!({
        "sessionId": sid,
        "value": { "sessionId": sid, "capabilities": {} }
    }))
}

async fn handle_tap(Path(_id): Path<String>, Json(body): Json<Value>) -> Json<Value> {
    let x = body.get("x").and_then(Value::as_i64).unwrap_or(-1);
    let y = body.get("y").and_then(Value::as_i64).unwrap_or(-1);
    info!(x, y, "[wda-stub] tap");
    Json(json!({ "value": null }))
}

async fn handle_drag(Path(_id): Path<String>, Json(body): Json<Value>) -> Json<Value> {
    info!(?body, "[wda-stub] drag");
    Json(json!({ "value": null }))
}

async fn handle_hold(Path(_id): Path<String>, Json(body): Json<Value>) -> Json<Value> {
    info!(?body, "[wda-stub] long press");
    Json(json!({ "value": null }))
}

async fn get_status() -> Json<Value> {
    Json(json!({ "value": { "ready": true, "message": "synthetic WDA ready" } }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::time::Duration;

    async fn bind_random() -> (SocketAddr, JoinHandle<()>) {
        // Bind a real loopback listener and hand it straight to serve() —
        // no drop-then-rebind dance, no race window.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind tmp");
        let addr = listener.local_addr().expect("local_addr");
        let handle = serve(listener);
        // Give axum::serve a moment to enter its accept loop.
        tokio::time::sleep(Duration::from_millis(50)).await;
        (addr, handle)
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
        let (addr, h) = bind_random().await;
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
        let (addr, h) = bind_random().await;
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
        let (addr, h) = bind_random().await;
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
        let (addr, h) = bind_random().await;
        let (code, body) = http_post(
            &format!("http://{addr}/session/abc/wda/touchAndHold"),
            r#"{"x":50,"y":80,"duration":1.0}"#,
        )
        .await;
        h.abort();
        assert_eq!(code, 200);
        assert!(body.contains("value"), "got: {body}");
    }
}
