use super::{Frame, FrameBus};
use chrono::Local;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{info, warn};

/// Record H.264 NAL units to a raw .h264 file.
///
/// We save raw NALUs instead of muxing to MP4 in real-time for simplicity and
/// zero overhead. Convert afterwards with:
///   ffmpeg -i recording.h264 -c copy recording.mp4
///
/// Two entry points:
/// - `run(rx)`      — legacy; record until the frame channel closes.
/// - `RecordingController::{start, stop}` — preferred; allows the API / hotkey
///    layer to start and stop recordings on demand.
pub async fn run(rx: broadcast::Receiver<Arc<Frame>>) {
    let path = match create_output_path() {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, "Recording aborted");
            return;
        }
    };
    let active = Arc::new(AtomicBool::new(true));
    let _ = record_inner(rx, path, active).await;
}

/// Single-flight recording controller shared across API handlers.
#[derive(Clone)]
pub struct RecordingController {
    active: Arc<AtomicBool>,
    current_path: Arc<std::sync::Mutex<Option<PathBuf>>>,
    frame_bus: FrameBus,
}

impl RecordingController {
    pub fn new(frame_bus: FrameBus) -> Self {
        Self {
            active: Arc::new(AtomicBool::new(false)),
            current_path: Arc::new(std::sync::Mutex::new(None)),
            frame_bus,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    pub fn current_path(&self) -> Option<PathBuf> {
        self.current_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Start a new recording. Returns the output path.
    /// Errors if a recording is already in progress.
    pub fn start(&self) -> Result<PathBuf, String> {
        if self.active.swap(true, Ordering::SeqCst) {
            return Err("recording already in progress".to_string());
        }
        let path = create_output_path().map_err(|e| {
            self.active.store(false, Ordering::SeqCst);
            e
        })?;
        {
            let mut slot = self
                .current_path
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *slot = Some(path.clone());
        }
        let rx = self.frame_bus.subscribe();
        let active = self.active.clone();
        let path_for_task = path.clone();
        let current_path = self.current_path.clone();
        tokio::spawn(async move {
            let _ = record_inner(rx, path_for_task, active.clone()).await;
            // Ensure flag + path slot are cleared even on early exit.
            active.store(false, Ordering::SeqCst);
            if let Ok(mut slot) = current_path.lock() {
                *slot = None;
            }
        });
        Ok(path)
    }

    /// Signal the recorder to stop. Returns the final file path if a recording
    /// was active, `None` if nothing was running.
    pub fn stop(&self) -> Option<PathBuf> {
        let was_active = self.active.swap(false, Ordering::SeqCst);
        if !was_active {
            return None;
        }
        self.current_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }
}

fn create_output_path() -> Result<PathBuf, String> {
    let dir = "recordings";
    fs::create_dir_all(dir).map_err(|e| format!("create recordings dir: {e}"))?;
    let filename = format!("rec_{}.h264", Local::now().format("%Y%m%d_%H%M%S"));
    Ok(PathBuf::from(dir).join(filename))
}

async fn record_inner(
    mut rx: broadcast::Receiver<Arc<Frame>>,
    path: PathBuf,
    active: Arc<AtomicBool>,
) -> Result<(), String> {
    let mut file = fs::File::create(&path).map_err(|e| format!("create {path:?}: {e}"))?;
    info!(file = %path.display(), "Recording started");
    let mut frame_count: u64 = 0;

    loop {
        if !active.load(Ordering::SeqCst) {
            break;
        }
        tokio::select! {
            msg = rx.recv() => match msg {
                Ok(frame) => {
                    if let Some(ref nalu) = frame.h264_nalu {
                        let _ = file.write_all(&[0x00, 0x00, 0x00, 0x01]);
                        let _ = file.write_all(nalu);
                        frame_count += 1;
                        if frame_count % 300 == 0 {
                            info!(frames = frame_count, "Recording in progress");
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(dropped = n, "Recorder falling behind — dropped frames");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            _ = tokio::time::sleep(Duration::from_millis(200)) => {
                // Tick: re-check the active flag so stop() is observed promptly.
            }
        }
    }

    info!(frames = frame_count, file = %path.display(), "Recording stopped");
    Ok(())
}
