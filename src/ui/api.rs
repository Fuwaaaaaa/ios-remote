use crate::config::{AppConfig, ConnectionHistory};
use crate::features::display_state::DisplayState;
use crate::features::recording::RecordingController;
use crate::features::session_replay::{SessionPlaybackController, list_sessions};
use crate::features::{FrameBus, screenshot};
use axum::{
    Router,
    extract::{Path, State},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Shared API state.
pub struct ApiState {
    pub frame_bus: FrameBus,
    pub config: Arc<Mutex<AppConfig>>,
    pub history: Arc<Mutex<ConnectionHistory>>,
    pub stats: Arc<Mutex<StreamStats>>,
    /// Bearer token required on every /api/* request. Empty string disables auth
    /// (not recommended; used only for internal tests).
    pub api_token: String,
    /// Recording lifecycle handle — shared with the recording task spawned by
    /// `RecordingController::start()`.
    pub recorder: RecordingController,
    /// Playback lifecycle handle — shared with the decode task spawned by
    /// `SessionPlaybackController::play()`.
    pub replay: SessionPlaybackController,
    /// Public URL of the local dashboard (e.g. `http://127.0.0.1:8080`).
    /// Populated from the resolved bind address + port at startup so the
    /// `web_dashboard` command can launch a browser at the right URL even
    /// when `--web-port` or `--lan` shifts it.
    pub dashboard_url: String,
    /// Display-window state shared between the render loop and dispatch
    /// handlers (zoom, game mode, annotations, stats overlay visibility).
    /// `std::sync::Mutex` deliberately — locks are short, the display
    /// thread reads it every frame on a non-async path.
    pub display: Arc<std::sync::Mutex<DisplayState>>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct StreamStats {
    pub connected: bool,
    pub device_name: String,
    pub fps: f64,
    pub frames_received: u64,
    pub uptime_secs: u64,
    pub resolution: String,
    pub bitrate_kbps: f64,
}

/// Build the REST API router. All /api/* routes are protected by a bearer
/// token middleware derived from `state.api_token`.
pub fn router(state: Arc<ApiState>) -> Router {
    Router::new()
        // Status
        .route("/api/status", get(get_status))
        .route("/api/stats", get(get_stats))
        // Actions
        .route("/api/screenshot", post(take_screenshot))
        .route("/api/recording/start", post(start_recording))
        .route("/api/recording/stop", post(stop_recording))
        // Replay
        .route("/api/replay/sessions", get(list_replay_sessions))
        .route("/api/replay/load", post(load_replay))
        .route("/api/replay/play", post(play_replay))
        .route("/api/replay/pause", post(pause_replay))
        .route("/api/replay/seek", post(seek_replay))
        // Config
        .route("/api/config", get(get_config))
        .route("/api/config", post(update_config))
        // History
        .route("/api/history", get(get_history))
        // OCR
        .route("/api/ocr", post(run_ocr))
        // AI
        .route("/api/ai/describe", post(ai_describe))
        // Macros
        .route("/api/macros", get(list_macros))
        .route("/api/macros/run", post(run_macro))
        // Command palette dispatch
        .route("/api/command/{id}", post(run_command))
        .route("/api/commands", get(list_commands))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_bearer,
        ))
        .with_state(state)
}

/// Axum middleware enforcing `Authorization: Bearer <token>` on /api/* routes.
/// Accepts the token via `?token=<t>` as a fallback for QR-code/URL embedding,
/// but discourages it in docs. Unauthorized requests get 401.
async fn require_bearer(
    State(state): State<Arc<ApiState>>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if state.api_token.is_empty() {
        return Ok(next.run(req).await);
    }

    let header_ok = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|t| constant_time_eq(t.as_bytes(), state.api_token.as_bytes()))
        .unwrap_or(false);

    let query_ok = req
        .uri()
        .query()
        .and_then(|q| q.split('&').find_map(|p| p.strip_prefix("token=")))
        .map(|t| constant_time_eq(t.as_bytes(), state.api_token.as_bytes()))
        .unwrap_or(false);

    if header_ok || query_ok {
        Ok(next.run(req).await)
    } else {
        warn!(path = %req.uri().path(), "Unauthorized API request");
        Err(StatusCode::UNAUTHORIZED)
    }
}

/// Constant-time byte slice comparison to avoid timing attacks on the token.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ─── Handlers ────────────────────────────────────────────────────────────────

async fn get_status(State(state): State<Arc<ApiState>>) -> Json<serde_json::Value> {
    let stats = state.stats.lock().await;
    Json(serde_json::json!({
        "status": if stats.connected { "connected" } else { "waiting" },
        "device": stats.device_name,
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn get_stats(State(state): State<Arc<ApiState>>) -> Json<StreamStats> {
    let stats = state.stats.lock().await;
    Json(stats.clone())
}

async fn take_screenshot(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.frame_bus.latest_frame() {
        Some(frame) => match screenshot::save_frame(&frame) {
            Ok(path) => Ok(Json(serde_json::json!({ "path": path }))),
            Err(e) => {
                tracing::warn!(error = %e, "Screenshot API failed");
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        },
        None => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

async fn start_recording(State(state): State<Arc<ApiState>>) -> Json<serde_json::Value> {
    match state.recorder.start() {
        Ok(path) => Json(serde_json::json!({
            "status": "recording_started",
            "path": path.display().to_string(),
        })),
        Err(e) => Json(serde_json::json!({ "status": "error", "error": e })),
    }
}

async fn stop_recording(State(state): State<Arc<ApiState>>) -> Json<serde_json::Value> {
    match state.recorder.stop() {
        Some(path) => Json(serde_json::json!({
            "status": "recording_stopped",
            "path": path.display().to_string(),
        })),
        None => Json(serde_json::json!({
            "status": "idle",
            "error": "no recording in progress",
        })),
    }
}

// ─── Replay handlers ─────────────────────────────────────────────────────────

async fn list_replay_sessions() -> Json<serde_json::Value> {
    let sessions: Vec<serde_json::Value> = list_sessions("recordings")
        .into_iter()
        .filter_map(|p| {
            let header_path = p.join("session.json");
            let raw = std::fs::read_to_string(&header_path).ok()?;
            let header: crate::features::session_replay::SessionHeader =
                serde_json::from_str(&raw).ok()?;
            Some(serde_json::json!({
                "path": p.display().to_string(),
                "start_time": header.start_time,
                "width": header.width,
                "height": header.height,
                "total_frames": header.total_frames,
                "duration_secs": header.duration_secs,
            }))
        })
        .collect();
    Json(serde_json::json!({ "sessions": sessions }))
}

#[derive(Deserialize)]
struct ReplayLoadRequest {
    path: String,
}

async fn load_replay(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<ReplayLoadRequest>,
) -> Json<serde_json::Value> {
    match state.replay.load(&req.path) {
        Ok(header) => Json(serde_json::json!({
            "status": "loaded",
            "header": header,
            "bookmarks": state.replay.bookmarks(),
        })),
        Err(e) => Json(serde_json::json!({ "status": "error", "error": e })),
    }
}

async fn play_replay(State(state): State<Arc<ApiState>>) -> Json<serde_json::Value> {
    match state.replay.play() {
        Ok(()) => Json(serde_json::json!({ "status": "playing" })),
        Err(e) => Json(serde_json::json!({ "status": "error", "error": e })),
    }
}

async fn pause_replay(State(state): State<Arc<ApiState>>) -> Json<serde_json::Value> {
    state.replay.pause();
    Json(serde_json::json!({
        "status": "paused",
        "position": state.replay.current_position(),
    }))
}

#[derive(Deserialize)]
struct ReplaySeekRequest {
    ts_us: u64,
}

async fn seek_replay(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<ReplaySeekRequest>,
) -> Json<serde_json::Value> {
    match state.replay.seek(req.ts_us) {
        Ok(position) => Json(serde_json::json!({
            "status": "seeked",
            "position": position,
        })),
        Err(e) => Json(serde_json::json!({ "status": "error", "error": e })),
    }
}

async fn get_config(State(state): State<Arc<ApiState>>) -> Json<AppConfig> {
    let config = state.config.lock().await;
    Json(config.clone())
}

async fn update_config(
    State(state): State<Arc<ApiState>>,
    Json(new_config): Json<AppConfig>,
) -> Json<serde_json::Value> {
    let mut config = state.config.lock().await;
    *config = new_config.clone();
    config.save();
    info!("Config updated via API");
    Json(serde_json::json!({ "status": "updated" }))
}

async fn get_history(State(state): State<Arc<ApiState>>) -> Json<ConnectionHistory> {
    let history = state.history.lock().await;
    Json(history.clone())
}

async fn run_ocr(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.frame_bus.latest_frame() {
        Some(frame) => match crate::features::ocr::extract_text(&frame, None) {
            Ok(text) => Ok(Json(serde_json::json!({ "text": text }))),
            Err(e) => Ok(Json(serde_json::json!({ "error": e }))),
        },
        None => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

#[derive(Deserialize)]
struct AiRequest {
    prompt: Option<String>,
}

async fn ai_describe(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<AiRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.frame_bus.latest_frame() {
        Some(frame) => {
            match crate::features::ai_vision::describe_screen(&frame, req.prompt.as_deref()) {
                Ok(desc) => Ok(Json(serde_json::json!({ "description": desc }))),
                Err(e) => Ok(Json(serde_json::json!({ "error": e }))),
            }
        }
        None => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

async fn list_macros() -> Json<serde_json::Value> {
    let macros = crate::features::macros::list_macros();
    Json(serde_json::json!({ "macros": macros }))
}

#[derive(Deserialize)]
struct MacroRunRequest {
    name: String,
}

async fn run_macro(Json(req): Json<MacroRunRequest>) -> Json<serde_json::Value> {
    let path = std::path::Path::new("macros").join(format!("{}.json", req.name));
    match crate::features::macros::Macro::load(&path) {
        Ok(m) => {
            tokio::spawn(async move {
                let _ = m.execute().await;
            });
            Json(serde_json::json!({ "status": "started", "name": req.name }))
        }
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

// ─── Command palette ─────────────────────────────────────────────────────────

async fn list_commands() -> Json<serde_json::Value> {
    let cmds = crate::devtools::command_palette::all_commands();
    Json(serde_json::json!({ "commands": cmds }))
}

/// `POST /api/command/{id}` — dispatch a command palette action by id.
///
/// Status mapping:
/// - 200 OK: handler ran successfully (`{ ok: true, action, message }`)
/// - 404 Not Found: unknown action id
/// - 409 Conflict: action recognized but not dispatchable (phase B/C/D, or
///   recoverable failure like "no recording in progress")
/// - 503 Service Unavailable: no frame received yet (analysis commands)
/// - 500 Internal Server Error: handler ran but failed
async fn run_command(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Response {
    use crate::devtools::command_palette::{CommandError, execute};

    // Quit short-circuits the process — log and return 202 before exit so the
    // caller sees something. (`execute` calls `process::exit(0)` for "quit".)
    if id == "quit" {
        info!("quit requested via REST API");
        // We let execute() fire — the response below is just-in-case.
    }

    match execute(&id, &state) {
        Ok(result) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": true,
                "action": result.action,
                "message": result.message,
            })),
        )
            .into_response(),
        Err(CommandError::UnknownAction(a)) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "ok": false,
                "error": "unknown_action",
                "action": a,
            })),
        )
            .into_response(),
        Err(CommandError::NotDispatchable { action, reason }) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "ok": false,
                "error": "not_dispatchable",
                "action": action,
                "reason": reason,
            })),
        )
            .into_response(),
        Err(CommandError::NoFrame) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "ok": false,
                "error": "no_frame",
                "reason": "no frame available yet — connect a device first",
            })),
        )
            .into_response(),
        Err(CommandError::Failed { action, message }) => {
            warn!(action = %action, error = %message, "command dispatch failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "error": "handler_failed",
                    "action": action,
                    "message": message,
                })),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, ConnectionHistory};
    use crate::features::FrameBus;
    use crate::features::recording::RecordingController;
    use crate::features::session_replay::SessionPlaybackController;

    fn dummy_state() -> Arc<ApiState> {
        let bus = FrameBus::new();
        Arc::new(ApiState {
            frame_bus: bus.clone(),
            config: Arc::new(Mutex::new(AppConfig::default())),
            history: Arc::new(Mutex::new(ConnectionHistory::default())),
            stats: Arc::new(Mutex::new(StreamStats::default())),
            api_token: String::new(),
            recorder: RecordingController::new(bus.clone()),
            replay: SessionPlaybackController::new(bus),
            dashboard_url: "http://127.0.0.1:8080".into(),
            display: Arc::new(std::sync::Mutex::new(DisplayState::new())),
        })
    }

    #[tokio::test]
    async fn unknown_command_returns_404() {
        let state = dummy_state();
        let resp = run_command(State(state), Path("not_a_real_action".into())).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn not_dispatchable_command_returns_409() {
        let state = dummy_state();
        // color_pick is Phase C (needs mouse events) — still 409 Conflict.
        let resp = run_command(State(state), Path("color_pick".into())).await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn no_frame_command_returns_503() {
        let state = dummy_state();
        // 'screenshot' needs a frame; bus is empty → 503.
        let resp = run_command(State(state), Path("screenshot".into())).await;
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn record_stop_when_idle_returns_500() {
        let state = dummy_state();
        // No recording in progress → handler runs, returns Failed.
        let resp = run_command(State(state), Path("record_stop".into())).await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
