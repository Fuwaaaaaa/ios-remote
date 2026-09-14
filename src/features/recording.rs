use super::session_replay::{Bookmark, SessionHeader};
use super::{Frame, FrameBus};
use chrono::Local;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tracing::{info, warn};

/// Record the live H.264 feed as a replayable session directory:
///
/// ```text
/// <output_dir>/session_YYYYMMDD_HHMMSS_ffffff/
///   video.h264      raw Annex-B NAL units (convert: ffmpeg -i video.h264 -c copy out.mp4)
///   session.json    SessionHeader (resolution, NAL count, duration)
///   bookmarks.json  Vec<Bookmark>
/// ```
///
/// This is exactly the layout `SessionPlayer::load` / `list_sessions` read,
/// so every recording shows up in the dashboard's Replay card.
///
/// Two entry points:
/// - `run(rx)`      — legacy; record until the frame channel closes.
/// - `RecordingController::{start, stop}` — preferred; allows the API / hotkey
///   layer to start and stop recordings on demand.
pub async fn run(rx: broadcast::Receiver<Arc<Frame>>) {
    let dir = match create_session_dir_in(Path::new("recordings")) {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, "Recording aborted");
            return;
        }
    };
    let session = Arc::new(ActiveSession::new(dir));
    let _ = record_session(rx, session, None).await;
}

/// Why `RecordingController::start` refused to start.
#[derive(Debug, thiserror::Error)]
pub enum RecordingError {
    #[error("recording already in progress")]
    AlreadyRecording,
    /// Recording stores the H.264 stream produced by the ffmpeg encoder;
    /// without it the file would silently stay empty.
    #[error("ffmpeg not found on PATH — recording needs ffmpeg to encode H.264")]
    EncoderUnavailable,
    #[error("{0}")]
    Io(String),
}

impl RecordingError {
    pub fn status_code(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::AlreadyRecording => StatusCode::CONFLICT,
            Self::EncoderUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// State of one in-flight recording. Each recording owns its stop flag, so a
/// `stop()` immediately followed by `start()` can never resurrect the old
/// writer task (the previous shared flag flipped back to `true`).
struct ActiveSession {
    dir: PathBuf,
    stop: AtomicBool,
    bookmarks: Mutex<Vec<Bookmark>>,
    nal_count: AtomicU64,
    started: Instant,
}

impl ActiveSession {
    fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            stop: AtomicBool::new(false),
            bookmarks: Mutex::new(Vec::new()),
            nal_count: AtomicU64::new(0),
            started: Instant::now(),
        }
    }
}

type EncoderProbe = Arc<dyn Fn() -> bool + Send + Sync>;

/// Single-flight recording controller shared across API handlers.
#[derive(Clone)]
pub struct RecordingController {
    current: Arc<Mutex<Option<Arc<ActiveSession>>>>,
    frame_bus: FrameBus,
    /// Directory for new recordings. Defaults to `recordings/`; configured by
    /// `[recording] output_dir`, and tests use per-run temp dirs.
    output_dir: PathBuf,
    /// `[recording] max_duration_secs`; `None` = unlimited.
    max_duration: Option<Duration>,
    /// Whether an H.264 encoder is available. Test seam; production probes
    /// for ffmpeg.
    encoder_probe: EncoderProbe,
}

impl RecordingController {
    pub fn new(frame_bus: FrameBus) -> Self {
        Self {
            current: Arc::new(Mutex::new(None)),
            frame_bus,
            output_dir: PathBuf::from("recordings"),
            max_duration: None,
            encoder_probe: Arc::new(super::h264_encoder::ffmpeg_available),
        }
    }

    #[must_use]
    pub fn with_output_dir(mut self, dir: PathBuf) -> Self {
        self.output_dir = dir;
        self
    }

    /// Stop recordings automatically after `secs` seconds (0 = unlimited).
    #[must_use]
    pub fn with_max_duration_secs(mut self, secs: u64) -> Self {
        self.max_duration = (secs > 0).then(|| Duration::from_secs(secs));
        self
    }

    #[must_use]
    pub fn with_encoder_probe(mut self, probe: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        self.encoder_probe = Arc::new(probe);
        self
    }

    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Arc<ActiveSession>>> {
        self.current.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn is_active(&self) -> bool {
        self.lock().is_some()
    }

    /// Session directory of the recording in progress, if any.
    pub fn current_path(&self) -> Option<PathBuf> {
        self.lock().as_ref().map(|s| s.dir.clone())
    }

    /// Start a new recording. Returns the session directory.
    pub fn start(&self) -> Result<PathBuf, RecordingError> {
        let mut slot = self.lock();
        if slot.is_some() {
            return Err(RecordingError::AlreadyRecording);
        }
        if !(self.encoder_probe)() {
            return Err(RecordingError::EncoderUnavailable);
        }
        let dir = create_session_dir_in(&self.output_dir).map_err(RecordingError::Io)?;
        let session = Arc::new(ActiveSession::new(dir.clone()));
        *slot = Some(session.clone());
        drop(slot);

        let rx = self.frame_bus.subscribe();
        let current = self.current.clone();
        let max_duration = self.max_duration;
        tokio::spawn(async move {
            if let Err(e) = record_session(rx, session.clone(), max_duration).await {
                warn!(error = %e, "Recording failed");
            }
            // Clear the slot only if it still holds *this* session — a newer
            // recording may already have replaced it.
            let mut slot = current.lock().unwrap_or_else(|e| e.into_inner());
            if slot.as_ref().is_some_and(|s| Arc::ptr_eq(s, &session)) {
                *slot = None;
            }
        });
        Ok(dir)
    }

    /// Signal the recorder to stop. Returns the session directory if a
    /// recording was active, `None` if nothing was running. The metadata files
    /// are finalized by the writer task within ~200ms.
    pub fn stop(&self) -> Option<PathBuf> {
        let session = self.lock().take()?;
        session.stop.store(true, Ordering::SeqCst);
        Some(session.dir.clone())
    }

    /// Add a named bookmark at the current position of the active recording.
    pub fn add_bookmark(&self, label: &str) -> Result<Bookmark, String> {
        let slot = self.lock();
        let session = slot.as_ref().ok_or("no recording in progress")?;
        let bookmark = Bookmark {
            timestamp_us: session.started.elapsed().as_micros() as u64,
            label: label.to_string(),
            frame_index: session.nal_count.load(Ordering::SeqCst),
        };
        session
            .bookmarks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(bookmark.clone());
        Ok(bookmark)
    }
}

fn create_session_dir_in(dir: &Path) -> Result<PathBuf, String> {
    // Microsecond suffix guarantees unique names when multiple recordings are
    // kicked off inside the same wall-clock second (e.g. in the test suite).
    let session_dir = dir.join(format!(
        "session_{}",
        Local::now().format("%Y%m%d_%H%M%S_%6f")
    ));
    fs::create_dir_all(&session_dir).map_err(|e| format!("create {session_dir:?}: {e}"))?;
    Ok(session_dir)
}

/// Write `session.json` + `bookmarks.json` for the current progress. Called
/// at start, periodically, and at the end so a crash still leaves a loadable
/// session.
fn write_metadata(session: &ActiveSession, start_time: &str, dims: (u32, u32)) {
    let header = SessionHeader {
        start_time: start_time.to_string(),
        width: dims.0,
        height: dims.1,
        total_frames: session.nal_count.load(Ordering::SeqCst),
        duration_secs: session.started.elapsed().as_secs_f64(),
    };
    let bookmarks = session
        .bookmarks
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let write = |name: &str, json: Result<String, serde_json::Error>| {
        if let Ok(json) = json
            && let Err(e) = fs::write(session.dir.join(name), json)
        {
            warn!(error = %e, file = name, "Failed to write session metadata");
        }
    };
    write("session.json", serde_json::to_string_pretty(&header));
    write("bookmarks.json", serde_json::to_string_pretty(&bookmarks));
}

async fn record_session(
    mut rx: broadcast::Receiver<Arc<Frame>>,
    session: Arc<ActiveSession>,
    max_duration: Option<Duration>,
) -> Result<(), String> {
    let video_path = session.dir.join("video.h264");
    let mut file =
        fs::File::create(&video_path).map_err(|e| format!("create {video_path:?}: {e}"))?;
    let start_time = Local::now().to_rfc3339();
    let mut dims = (0u32, 0u32);
    write_metadata(&session, &start_time, dims);
    info!(dir = %session.dir.display(), "Recording started");
    let mut last_flush = Instant::now();

    loop {
        if session.stop.load(Ordering::SeqCst) {
            break;
        }
        if max_duration.is_some_and(|max| session.started.elapsed() >= max) {
            info!(dir = %session.dir.display(), "Recording reached max_duration_secs — stopping");
            break;
        }
        tokio::select! {
            msg = rx.recv() => match msg {
                Ok(frame) => {
                    if let Some(ref nalu) = frame.h264_nalu {
                        let wrote = file
                            .write_all(&[0x00, 0x00, 0x00, 0x01])
                            .and_then(|()| file.write_all(nalu));
                        if let Err(e) = wrote {
                            write_metadata(&session, &start_time, dims);
                            return Err(format!("write {video_path:?}: {e}"));
                        }
                        dims = (frame.width, frame.height);
                        let n = session.nal_count.fetch_add(1, Ordering::SeqCst) + 1;
                        if n.is_multiple_of(300) {
                            info!(nal_units = n, "Recording in progress");
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(dropped = n, "Recorder falling behind — dropped frames");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            _ = tokio::time::sleep(Duration::from_millis(200)) => {
                // Tick: re-check the stop flag so stop() is observed promptly.
            }
        }
        if last_flush.elapsed() >= Duration::from_secs(5) {
            write_metadata(&session, &start_time, dims);
            last_flush = Instant::now();
        }
    }

    let _ = file.flush();
    write_metadata(&session, &start_time, dims);
    info!(
        nal_units = session.nal_count.load(Ordering::SeqCst),
        dir = %session.dir.display(),
        "Recording stopped"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::session_replay::{SessionPlayer, list_sessions};
    use crate::features::{Frame, FrameBus};

    fn make_frame(n: u8) -> Frame {
        Frame {
            width: 64,
            height: 48,
            rgba: Vec::new(),
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

    fn controller(bus: FrameBus, dir: &Path) -> RecordingController {
        RecordingController::new(bus)
            .with_output_dir(dir.to_path_buf())
            .with_encoder_probe(|| true)
    }

    #[tokio::test]
    async fn recording_produces_a_replayable_session() {
        let dir = unique_tmpdir("session");
        let bus = FrameBus::new();
        let ctl = controller(bus.clone(), &dir);
        assert!(!ctl.is_active());

        let session_dir = ctl.start().unwrap();
        assert!(ctl.is_active());
        tokio::time::sleep(Duration::from_millis(50)).await;
        bus.publish(make_frame(1));
        ctl.add_bookmark("login screen").unwrap();
        bus.publish(make_frame(2));
        tokio::time::sleep(Duration::from_millis(250)).await;

        assert_eq!(ctl.stop().as_ref(), Some(&session_dir));
        assert!(!ctl.is_active());
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Shows up in the Replay listing and loads with the right metadata.
        assert_eq!(list_sessions(&dir), vec![session_dir.clone()]);
        let player = SessionPlayer::load(&session_dir).unwrap();
        assert_eq!(player.header.width, 64);
        assert_eq!(player.header.height, 48);
        assert_eq!(player.header.total_frames, 2);
        assert_eq!(player.nal_count(), 2);
        assert_eq!(player.nalu(0), Some(&[0x65, 1][..]));
        assert_eq!(player.bookmarks.len(), 1);
        assert_eq!(player.bookmarks[0].label, "login screen");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn double_start_errors() {
        let dir = unique_tmpdir("double");
        let ctl = controller(FrameBus::new(), &dir);
        let _path = ctl.start().unwrap();
        assert!(matches!(ctl.start(), Err(RecordingError::AlreadyRecording)));
        ctl.stop();
        tokio::time::sleep(Duration::from_millis(250)).await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn stop_then_immediate_start_does_not_revive_old_writer() {
        let dir = unique_tmpdir("restart");
        let bus = FrameBus::new();
        let ctl = controller(bus.clone(), &dir);
        let first = ctl.start().unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        ctl.stop();
        // Same instant: the first writer has not observed its stop flag yet.
        tokio::time::sleep(Duration::from_millis(2)).await;
        let second = ctl.start().unwrap();
        assert_ne!(first, second);
        tokio::time::sleep(Duration::from_millis(300)).await;

        bus.publish(make_frame(7));
        tokio::time::sleep(Duration::from_millis(300)).await;
        ctl.stop();
        tokio::time::sleep(Duration::from_millis(300)).await;

        let first_len = std::fs::metadata(first.join("video.h264")).unwrap().len();
        let second_len = std::fs::metadata(second.join("video.h264")).unwrap().len();
        assert_eq!(first_len, 0, "stopped recording must not keep writing");
        assert!(second_len > 0, "new recording receives the frame");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn max_duration_stops_and_frees_the_slot() {
        let dir = unique_tmpdir("maxdur");
        let ctl = RecordingController::new(FrameBus::new())
            .with_output_dir(dir.clone())
            .with_encoder_probe(|| true)
            .with_max_duration_secs(1);
        ctl.start().unwrap();
        tokio::time::sleep(Duration::from_millis(1500)).await;
        assert!(
            !ctl.is_active(),
            "max_duration_secs should end the recording"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn start_without_encoder_is_refused() {
        let dir = unique_tmpdir("noffmpeg");
        let ctl = RecordingController::new(FrameBus::new())
            .with_output_dir(dir.clone())
            .with_encoder_probe(|| false);
        assert!(matches!(
            ctl.start(),
            Err(RecordingError::EncoderUnavailable)
        ));
        assert!(!ctl.is_active());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn stop_when_idle_returns_none() {
        let ctl = RecordingController::new(FrameBus::new());
        assert!(ctl.stop().is_none());
        assert!(ctl.add_bookmark("x").is_err());
    }
}
