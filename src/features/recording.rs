use super::{Frame, FrameBus};
use chrono::Local;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
///   layer to start and stop recordings on demand.
pub async fn run(rx: broadcast::Receiver<Arc<Frame>>) {
    let path = match create_output_path_in(std::path::Path::new("recordings")) {
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
    /// Directory for new recordings. Defaults to `recordings/`; tests override
    /// via `with_output_dir` so they can use per-run temp dirs.
    output_dir: PathBuf,
}

impl RecordingController {
    pub fn new(frame_bus: FrameBus) -> Self {
        Self {
            active: Arc::new(AtomicBool::new(false)),
            current_path: Arc::new(std::sync::Mutex::new(None)),
            frame_bus,
            output_dir: PathBuf::from("recordings"),
        }
    }

    #[must_use]
    pub fn with_output_dir(mut self, dir: PathBuf) -> Self {
        self.output_dir = dir;
        self
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
        let path = create_output_path_in(&self.output_dir).inspect_err(|_e| {
            self.active.store(false, Ordering::SeqCst);
        })?;
        {
            let mut slot = self.current_path.lock().unwrap_or_else(|e| e.into_inner());
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

fn create_output_path_in(dir: &std::path::Path) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|e| format!("create recordings dir: {e}"))?;
    // Microsecond suffix guarantees unique names when multiple recordings are
    // kicked off inside the same wall-clock second (e.g. in the test suite).
    let filename = format!("rec_{}.h264", Local::now().format("%Y%m%d_%H%M%S_%6f"));
    Ok(dir.join(filename))
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
                        if frame_count.is_multiple_of(300) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::{Frame, FrameBus};

    fn make_frame(n: u8) -> Frame {
        Frame {
            width: 1,
            height: 1,
            rgba: vec![n, n, n, 255],
            timestamp_us: u64::from(n),
            h264_nalu: Some(vec![0x65, n]), // fake NAL payload
        }
    }

    /// Unique per-test temp directory to avoid CWD / filename races between
    /// tests running in parallel.
    fn unique_tmpdir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ios-remote-rec-{tag}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn controller_start_and_stop_writes_file() {
        let dir = unique_tmpdir("start-stop");
        let bus = FrameBus::new();
        let ctl = RecordingController::new(bus.clone()).with_output_dir(dir.clone());
        assert!(!ctl.is_active());

        let path = ctl.start().unwrap();
        assert!(ctl.is_active());

        // Let the recording task settle, then publish a couple of frames.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        bus.publish(make_frame(1));
        bus.publish(make_frame(2));
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;

        let stopped = ctl.stop();
        assert_eq!(stopped.as_ref(), Some(&path));
        // Give the task a beat to flush and return.
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        assert!(!ctl.is_active());

        let bytes = std::fs::read(&path).unwrap();
        // Two frames × (4-byte start code + 2-byte NAL) = 12 bytes minimum.
        assert!(
            bytes.len() >= 12,
            "expected recorded file to contain NAL units, got {}",
            bytes.len()
        );
        // Start-code prefix appears at offset 0.
        assert_eq!(&bytes[..4], &[0x00, 0x00, 0x00, 0x01]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn double_start_errors() {
        let dir = unique_tmpdir("double");
        let bus = FrameBus::new();
        let ctl = RecordingController::new(bus).with_output_dir(dir.clone());
        let _path = ctl.start().unwrap();
        let second = ctl.start();
        assert!(second.is_err(), "concurrent start should be rejected");
        ctl.stop();
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn stop_when_idle_returns_none() {
        let bus = FrameBus::new();
        let ctl = RecordingController::new(bus);
        assert!(ctl.stop().is_none());
    }
}
