//! Synthetic device mode — drives the FrameBus / WDA / subtitles pipelines
//! without a physical iPhone. Activated by `--synthetic` on the CLI.
//!
//! The pipeline mirrors the real-device path:
//!
//! ```text
//!   IPhoneMockRenderer ──30 FPS── FrameBus ──> display / recording /
//!                                              screenshot / OCR / AI / replay
//!   WdaStub (127.0.0.1:8101)  <── macros REST  (tap / swipe / long-press)
//!   SubtitlePump ──5s tick──> Transcriber ──> /api/subtitles
//! ```
//!
//! See `notes/v0.8.0.md` for the user-facing summary.
//!
//! The 5x7 bitmap font used by the renderer lives in
//! `crate::features::stats_overlay` (also used by the subtitle renderer);
//! this module does not duplicate it.

pub mod renderer;
pub mod subtitle_pump;
pub mod wda_stub;

/// Returned to callers (main.rs / API) so the dashboard can report a stable
/// fake identity instead of "waiting".
#[derive(Debug, Clone)]
pub struct SyntheticDeviceInfo {
    pub name: String,
    pub model: String,
    pub ios_version: String,
    pub udid: String,
    pub width: u32,
    pub height: u32,
}

impl SyntheticDeviceInfo {
    pub fn default_iphone_15() -> Self {
        Self {
            name: "Synthetic iPhone".into(),
            model: "iPhone15,2".into(),
            ios_version: "17.5".into(),
            udid: "SYNTHETIC-00000000-000000000000000".into(),
            width: 390,
            height: 844,
        }
    }
}

/// Lightweight handle returned by `spawn()` so the caller can keep the tasks
/// alive for the lifetime of main. Drop = abort all spawned tasks.
pub struct SyntheticHandles {
    pub _frame_task: tokio::task::JoinHandle<()>,
    pub _subtitle_task: tokio::task::JoinHandle<()>,
    pub _wda_task: tokio::task::JoinHandle<()>,
}

/// Spawn all synthetic background tasks. Caller must keep `SyntheticHandles`
/// alive for the duration of the run. The WDA listener is bound by the
/// caller so port-in-use is a hard error reported by main before the
/// `IOS_REMOTE_WDA_URL` redirect points at a dead socket.
pub fn spawn(
    info: SyntheticDeviceInfo,
    frame_publish: std::sync::Arc<dyn Fn(renderer::SyntheticFrame) + Send + Sync>,
    subtitle_push: std::sync::Arc<dyn Fn(String) + Send + Sync>,
    wda_listener: tokio::net::TcpListener,
) -> SyntheticHandles {
    let frame_task = renderer::spawn_frame_loop(info.clone(), frame_publish);
    let subtitle_task = subtitle_pump::spawn(subtitle_push);
    let wda_task = wda_stub::serve(wda_listener);
    SyntheticHandles {
        _frame_task: frame_task,
        _subtitle_task: subtitle_task,
        _wda_task: wda_task,
    }
}
