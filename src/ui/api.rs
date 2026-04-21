use crate::config::{AppConfig, ConnectionHistory};
use crate::features::recording::RecordingController;
use crate::features::{screenshot, FrameBus};
use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{Json, Response},
    routing::{get, post},
    Router,
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
        .route_layer(middleware::from_fn_with_state(state.clone(), require_bearer))
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
        .and_then(|q| {
            q.split('&')
                .find_map(|p| p.strip_prefix("token="))
        })
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

async fn take_screenshot(State(state): State<Arc<ApiState>>) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.frame_bus.latest_frame() {
        Some(frame) => {
            match screenshot::save_frame(&frame) {
                Ok(path) => Ok(Json(serde_json::json!({ "path": path }))),
                Err(e) => {
                    tracing::warn!(error = %e, "Screenshot API failed");
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
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

async fn run_ocr(State(state): State<Arc<ApiState>>) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.frame_bus.latest_frame() {
        Some(frame) => {
            match crate::features::ocr::extract_text(&frame, None) {
                Ok(text) => Ok(Json(serde_json::json!({ "text": text }))),
                Err(e) => Ok(Json(serde_json::json!({ "error": e }))),
            }
        }
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
            tokio::spawn(async move { let _ = m.execute().await; });
            Json(serde_json::json!({ "status": "started", "name": req.name }))
        }
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}
