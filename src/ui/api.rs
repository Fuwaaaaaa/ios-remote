use crate::config::{AppConfig, ConnectionHistory};
use crate::features::audio_transcription::Transcriber;
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
    /// Live connection state + measured frame statistics.
    pub stats: crate::ui::stats::StatsHub,
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
    /// Live transcriber. `Some` when WASAPI / mic capture is active (built
    /// with `--features audio_capture` and a working device). Subtitles and
    /// the audio source state are read/written through this handle.
    pub transcriber: Option<Arc<std::sync::Mutex<Transcriber>>>,
    /// Shared interactive synthetic-device state. `Some` only in `--synthetic`
    /// mode; exposed read-only via `GET /api/synthetic/state` so tests (and
    /// curious users) can observe the current screen / page after input.
    pub synthetic_state: Option<crate::synthetic::state::SharedState>,
    /// Outcome of the most recent `POST /api/macros/run`, surfaced as
    /// `last_run` on `GET /api/macros`.
    pub macro_runs: crate::features::macros::MacroRunTracker,
}

/// JSON error body with a matching HTTP status, so clients can branch on the
/// status code instead of sniffing a 200 body for an `error` key.
fn json_error(status: StatusCode, message: impl Into<String>) -> Response {
    (
        status,
        Json(serde_json::json!({ "status": "error", "error": message.into() })),
    )
        .into_response()
}

fn no_frame() -> Response {
    json_error(
        StatusCode::SERVICE_UNAVAILABLE,
        "no frame available yet — connect a device first",
    )
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
        // Audio capture / subtitles
        .route("/api/audio/status", get(get_audio_status))
        .route("/api/subtitles", get(get_subtitles))
        // Synthetic-mode introspection (read-only; 503 in real-device mode)
        .route("/api/synthetic/state", get(get_synthetic_state))
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
    let stats = state.stats.snapshot();
    Json(serde_json::json!({
        "status": if stats.connected { "connected" } else { "waiting" },
        "device": stats.device_name,
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn get_stats(State(state): State<Arc<ApiState>>) -> Json<StreamStats> {
    Json(state.stats.snapshot())
}

async fn take_screenshot(State(state): State<Arc<ApiState>>) -> Response {
    let Some(frame) = state.frame_bus.latest_frame() else {
        return no_frame();
    };
    match tokio::task::spawn_blocking(move || screenshot::save_frame(&frame)).await {
        Ok(Ok(path)) => Json(serde_json::json!({ "path": path })).into_response(),
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "Screenshot API failed");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, e)
        }
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn start_recording(State(state): State<Arc<ApiState>>) -> Response {
    match state.recorder.start() {
        Ok(path) => Json(serde_json::json!({
            "status": "recording_started",
            "path": path.display().to_string(),
        }))
        .into_response(),
        Err(e) => json_error(StatusCode::CONFLICT, e),
    }
}

async fn stop_recording(State(state): State<Arc<ApiState>>) -> Response {
    match state.recorder.stop() {
        Some(path) => Json(serde_json::json!({
            "status": "recording_stopped",
            "path": path.display().to_string(),
        }))
        .into_response(),
        None => json_error(StatusCode::CONFLICT, "no recording in progress"),
    }
}

// ─── Replay handlers ─────────────────────────────────────────────────────────

async fn list_replay_sessions(State(state): State<Arc<ApiState>>) -> Json<serde_json::Value> {
    let sessions: Vec<serde_json::Value> = list_sessions(state.recorder.output_dir())
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
) -> Response {
    match state.replay.load(&req.path) {
        Ok(header) => Json(serde_json::json!({
            "status": "loaded",
            "header": header,
            "bookmarks": state.replay.bookmarks(),
        }))
        .into_response(),
        Err(e) => json_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn play_replay(State(state): State<Arc<ApiState>>) -> Response {
    match state.replay.play() {
        Ok(()) => Json(serde_json::json!({ "status": "playing" })).into_response(),
        Err(e) => json_error(StatusCode::CONFLICT, e),
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
) -> Response {
    match state.replay.seek(req.ts_us) {
        Ok(position) => Json(serde_json::json!({
            "status": "seeked",
            "position": position,
        }))
        .into_response(),
        Err(e) => json_error(StatusCode::CONFLICT, e),
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
    *config = new_config.into_persistent();
    config.save();
    info!("Config updated via API");
    // Settings are read at startup; say so instead of implying a live change.
    Json(serde_json::json!({
        "status": "updated",
        "restart_required": true,
    }))
}

async fn get_history(State(state): State<Arc<ApiState>>) -> Json<ConnectionHistory> {
    let history = state.history.lock().await;
    Json(history.clone())
}

async fn run_ocr(State(state): State<Arc<ApiState>>) -> Response {
    let Some(frame) = state.frame_bus.latest_frame() else {
        return no_frame();
    };
    // tesseract is a subprocess — keep it off the async workers.
    match tokio::task::spawn_blocking(move || crate::features::ocr::extract_text(&frame, None))
        .await
    {
        Ok(Ok(text)) => Json(serde_json::json!({ "text": text })).into_response(),
        Ok(Err(e)) => json_error(StatusCode::INTERNAL_SERVER_ERROR, e),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(Deserialize, Default)]
struct AiRequest {
    prompt: Option<String>,
}

async fn ai_describe(State(state): State<Arc<ApiState>>, body: axum::body::Bytes) -> Response {
    // The body is optional: `{}` / empty both mean "use the default prompt".
    let req: AiRequest = if body.iter().all(u8::is_ascii_whitespace) {
        AiRequest::default()
    } else {
        match serde_json::from_slice(&body) {
            Ok(r) => r,
            Err(e) => return json_error(StatusCode::BAD_REQUEST, format!("invalid JSON: {e}")),
        }
    };
    if !crate::features::ai_vision::api_key_configured() {
        return json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "ANTHROPIC_API_KEY not set. Set it to use AI screen understanding.",
        );
    }
    let Some(frame) = state.frame_bus.latest_frame() else {
        return no_frame();
    };
    let result = tokio::task::spawn_blocking(move || {
        crate::features::ai_vision::describe_screen(&frame, req.prompt.as_deref())
    })
    .await;
    match result {
        Ok(Ok(desc)) => Json(serde_json::json!({ "description": desc })).into_response(),
        Ok(Err(e)) => json_error(StatusCode::BAD_GATEWAY, e),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn list_macros(State(state): State<Arc<ApiState>>) -> Json<serde_json::Value> {
    let macros = crate::features::macros::list_macros();
    Json(serde_json::json!({
        "macros": macros,
        "last_run": state.macro_runs.last(),
    }))
}

#[derive(Deserialize)]
struct MacroRunRequest {
    name: String,
}

async fn run_macro(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<MacroRunRequest>,
) -> Response {
    use crate::features::macros::{Macro, is_valid_macro_name};

    if !is_valid_macro_name(&req.name) {
        return json_error(StatusCode::BAD_REQUEST, "invalid macro name");
    }
    let path = std::path::Path::new("macros").join(format!("{}.json", req.name));
    if !path.is_file() {
        return json_error(
            StatusCode::NOT_FOUND,
            format!("macro '{}' not found under ./macros", req.name),
        );
    }
    let m = match Macro::load(&path) {
        Ok(m) => m,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, format!("invalid macro: {e}")),
    };

    // Fail fast when the macro needs touch input but WebDriverAgent is not
    // reachable — otherwise the caller gets "started" for a run that dies on
    // its first tap. Wait/Screenshot-only macros skip the check.
    if m.requires_input() {
        let probe = tokio::task::spawn_blocking(|| {
            crate::features::wda_client::default_wda_client()
                .ensure_session()
                .map(|_| ())
        })
        .await;
        match probe {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return json_error(StatusCode::SERVICE_UNAVAILABLE, e.to_string()),
            Err(e) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        }
    }

    if !state.macro_runs.try_begin(&req.name) {
        return json_error(StatusCode::CONFLICT, "another macro is still running");
    }

    // Fire-and-forget: the run's outcome lands in `macro_runs` and is exposed
    // as `last_run` on `GET /api/macros`.
    //
    // Run on a dedicated OS thread with its own current-thread runtime.
    // `WdaClient` issues a *blocking* `curl`; in synthetic mode the WDA stub
    // lives on the main runtime, so running the macro there would block a
    // worker and could starve the very stub it's calling (loopback request →
    // no response → timeout). A separate runtime keeps the blocking I/O off
    // the main workers.
    let frame_bus = state.frame_bus.clone();
    let tracker = state.macro_runs.clone();
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                warn!(error = %e, "macro runtime build failed");
                tracker.finish(&Err(format!("macro runtime build failed: {e}")));
                return;
            }
        };
        let result = rt.block_on(async move {
            let client = crate::features::wda_client::default_wda_client();
            m.execute_full(&client, &frame_bus).await
        });
        if let Err(e) = &result {
            warn!(error = %e, "macro execution failed");
        }
        tracker.finish(&result);
    });
    Json(serde_json::json!({ "status": "started", "name": req.name })).into_response()
}

/// `GET /api/synthetic/state` — read-only view of the interactive synthetic
/// device. Returns `503` in real-device mode (no synthetic state). This is the
/// deterministic observable e2e tests use to assert that input changed the
/// screen (far more robust than diffing frames whose clock ticks every frame).
async fn get_synthetic_state(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use crate::synthetic::state::Screen;
    let Some(shared) = &state.synthetic_state else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let snap = {
        let guard = shared.lock().unwrap_or_else(|e| e.into_inner());
        guard.snapshot()
    };
    let (screen, app) = match snap.screen {
        Screen::Home => ("home", serde_json::Value::Null),
        Screen::App { index } => (
            "app",
            serde_json::json!({
                "index": index,
                "letter": crate::synthetic::layout::app_letter(index).to_string(),
            }),
        ),
    };
    Ok(Json(serde_json::json!({
        "screen": screen,
        "app": app,
        "page": snap.home_page,
        "app_scroll": snap.app_scroll,
        "interactions": snap.interactions,
    })))
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
async fn run_command(State(state): State<Arc<ApiState>>, Path(id): Path<String>) -> Response {
    use crate::devtools::command_palette::{CommandError, execute};

    // `execute("quit")` exits synchronously, which would drop this connection
    // before any response is written. Answer 202 first, exit shortly after.
    if id == "quit" {
        info!("quit requested via REST API");
        tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            std::process::exit(0);
        });
        return (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({
                "ok": true,
                "action": "quit",
                "message": "shutting down",
            })),
        )
            .into_response();
    }

    // Handlers shell out (tesseract, curl, ffmpeg launch) — run them on the
    // blocking pool so a slow command can't stall the async workers.
    let outcome = {
        let state = state.clone();
        let id = id.clone();
        tokio::task::spawn_blocking(move || execute(&id, &state)).await
    };
    let outcome = match outcome {
        Ok(r) => r,
        Err(e) => {
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    };

    match outcome {
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

/// `GET /api/audio/status` — returns whether audio capture is live and the
/// recent subtitle window. The `enabled` flag distinguishes "feature flag
/// off / no device" (Some(false) plus empty subtitles) from "device is
/// streaming but silent" (Some(true), empty subtitles).
async fn get_audio_status(State(state): State<Arc<ApiState>>) -> Json<serde_json::Value> {
    let Some(t) = &state.transcriber else {
        return Json(serde_json::json!({
            "enabled": false,
            "reason": "audio_capture feature disabled or no device",
        }));
    };
    let t = t.lock().unwrap_or_else(|p| p.into_inner());
    let now_ms = t.now_ms();
    let active: Vec<_> = t
        .active_subtitles(now_ms)
        .iter()
        .map(|s| {
            serde_json::json!({
                "text": s.text,
                "start_ms": s.start_ms,
                "duration_ms": s.duration_ms,
            })
        })
        .collect();
    Json(serde_json::json!({
        "enabled": true,
        "now_ms": now_ms,
        "active_subtitles": active,
    }))
}

/// `GET /api/subtitles` — full subtitle history kept by the Transcriber
/// (capped at `Transcriber::max_subtitles`).
async fn get_subtitles(State(state): State<Arc<ApiState>>) -> Json<serde_json::Value> {
    let Some(t) = &state.transcriber else {
        return Json(serde_json::json!({ "subtitles": [] }));
    };
    let t = t.lock().unwrap_or_else(|p| p.into_inner());
    let items: Vec<_> = t
        .subtitles
        .iter()
        .map(|s| {
            serde_json::json!({
                "text": s.text,
                "start_ms": s.start_ms,
                "duration_ms": s.duration_ms,
            })
        })
        .collect();
    Json(serde_json::json!({ "subtitles": items }))
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
            stats: crate::ui::stats::StatsHub::new(None),
            api_token: String::new(),
            recorder: RecordingController::new(bus.clone()),
            replay: SessionPlaybackController::new(bus),
            dashboard_url: "http://127.0.0.1:8080".into(),
            display: Arc::new(std::sync::Mutex::new(DisplayState::new())),
            transcriber: None,
            synthetic_state: None,
            macro_runs: Default::default(),
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
        // ruler still 409 — Phase C only wired color_pick so far.
        let resp = run_command(State(state), Path("ruler".into())).await;
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
    async fn analysis_endpoints_return_503_json_without_a_frame() {
        let state = dummy_state();
        let resp = run_ocr(State(state.clone())).await;
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        let resp = take_screenshot(State(state)).await;
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn recording_stop_when_idle_is_409_not_200() {
        let resp = stop_recording(State(dummy_state())).await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn macro_run_rejects_traversal_and_missing_names() {
        let state = dummy_state();
        let resp = run_macro(
            State(state.clone()),
            Json(MacroRunRequest {
                name: "../../etc/passwd".into(),
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let resp = run_macro(
            State(state),
            Json(MacroRunRequest {
                name: "definitely_not_a_saved_macro_zzz".into(),
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn ai_describe_accepts_empty_body() {
        let state = dummy_state();
        let resp = ai_describe(State(state), axum::body::Bytes::new()).await;
        // Never 415/422 for an empty body: either the key is missing (503),
        // or — with a key in the environment — there is no frame yet (503).
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
