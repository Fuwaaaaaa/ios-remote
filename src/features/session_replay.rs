use super::{Frame, FrameBus};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{info, warn};

/// Session replay: record full sessions and replay them later.
///
/// Records all frames + timestamps + events to a session file.
/// Can replay at original speed or fast-forward.

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SessionHeader {
    pub start_time: String,
    pub width: u32,
    pub height: u32,
    pub total_frames: u64,
    pub duration_secs: f64,
}

pub struct SessionRecorder {
    frames: Vec<(u64, Vec<u8>)>, // (timestamp_us, h264_nalu)
    bookmarks: Vec<Bookmark>,
    recording: bool,
    start_time: std::time::Instant,
    width: u32,
    height: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bookmark {
    pub timestamp_us: u64,
    pub label: String,
    pub frame_index: u64,
}

impl SessionRecorder {
    pub fn new() -> Self {
        Self {
            frames: Vec::new(),
            bookmarks: Vec::new(),
            recording: false,
            start_time: std::time::Instant::now(),
            width: 0,
            height: 0,
        }
    }

    pub fn start(&mut self) {
        self.frames.clear();
        self.bookmarks.clear();
        self.recording = true;
        self.start_time = std::time::Instant::now();
        info!("Session recording started");
    }

    pub fn stop(&mut self) {
        self.recording = false;
        info!("Session recording stopped");
    }

    pub fn push_frame(&mut self, frame: &Frame) {
        if !self.recording {
            return;
        }
        self.width = frame.width;
        self.height = frame.height;
        if let Some(ref nalu) = frame.h264_nalu {
            self.frames.push((frame.timestamp_us, nalu.clone()));
        }
    }

    pub fn add_bookmark(&mut self, label: &str) {
        let ts = self.start_time.elapsed().as_micros() as u64;
        self.bookmarks.push(Bookmark {
            timestamp_us: ts,
            label: label.to_string(),
            frame_index: self.frames.len() as u64,
        });
        info!(label, "Bookmark added");
    }

    pub fn bookmarks(&self) -> &[Bookmark] {
        &self.bookmarks
    }

    /// Save session to directory.
    pub fn save(&self, dir: &str) -> Result<String, String> {
        let path = format!("{}/session_{}", dir, Local::now().format("%Y%m%d_%H%M%S"));
        fs::create_dir_all(&path).map_err(|e| e.to_string())?;

        let header = SessionHeader {
            start_time: Local::now().to_rfc3339(),
            width: self.width,
            height: self.height,
            total_frames: self.frames.len() as u64,
            duration_secs: self.start_time.elapsed().as_secs_f64(),
        };
        let hdr_json = serde_json::to_string_pretty(&header).map_err(|e| e.to_string())?;
        fs::write(format!("{}/session.json", path), hdr_json).map_err(|e| e.to_string())?;

        // Save bookmarks
        let bm_json = serde_json::to_string_pretty(&self.bookmarks).map_err(|e| e.to_string())?;
        fs::write(format!("{}/bookmarks.json", path), bm_json).map_err(|e| e.to_string())?;

        // Save H.264 stream
        let mut h264 = Vec::new();
        for (_, nalu) in &self.frames {
            h264.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
            h264.extend_from_slice(nalu);
        }
        fs::write(format!("{}/video.h264", path), h264).map_err(|e| e.to_string())?;

        info!(path = %path, frames = self.frames.len(), "Session saved");
        Ok(path)
    }
}

/// Loads a saved session and replays its NAL units at their recorded timestamps.
///
/// The h264 byte stream is scanned once on load to build an in-memory NAL index
/// without copying the frame payloads, so seeking and restarting playback is
/// effectively free. Playback emits frames with `h264_nalu` populated but
/// `rgba` empty — consumers that render RGBA need a decoder on top.
pub struct SessionPlayer {
    pub header: SessionHeader,
    pub bookmarks: Vec<Bookmark>,
    video: Vec<u8>,
    /// Byte ranges [start..end) of each NAL unit inside `video`.
    nalu_ranges: Vec<(usize, usize)>,
}

impl SessionPlayer {
    /// Load a session from a directory containing
    /// `session.json`, `bookmarks.json`, and `video.h264`.
    pub fn load(dir: impl AsRef<Path>) -> Result<Self, String> {
        let dir = dir.as_ref();

        let header_json = fs::read_to_string(dir.join("session.json"))
            .map_err(|e| format!("read session.json: {e}"))?;
        let header: SessionHeader =
            serde_json::from_str(&header_json).map_err(|e| format!("parse session.json: {e}"))?;

        let bookmarks_path = dir.join("bookmarks.json");
        let bookmarks: Vec<Bookmark> = if bookmarks_path.exists() {
            let raw = fs::read_to_string(&bookmarks_path)
                .map_err(|e| format!("read bookmarks.json: {e}"))?;
            serde_json::from_str(&raw).map_err(|e| format!("parse bookmarks.json: {e}"))?
        } else {
            Vec::new()
        };

        let video =
            fs::read(dir.join("video.h264")).map_err(|e| format!("read video.h264: {e}"))?;
        let nalu_ranges = index_nal_units(&video);

        info!(
            path = %dir.display(),
            nalus = nalu_ranges.len(),
            bookmarks = bookmarks.len(),
            "Session loaded"
        );
        Ok(Self {
            header,
            bookmarks,
            video,
            nalu_ranges,
        })
    }

    pub fn nal_count(&self) -> usize {
        self.nalu_ranges.len()
    }

    /// Return the NAL unit bytes at `index`.
    pub fn nalu(&self, index: usize) -> Option<&[u8]> {
        self.nalu_ranges
            .get(index)
            .map(|(start, end)| &self.video[*start..*end])
    }

    /// Seek to the first NAL unit at or after `timestamp_us`. This currently
    /// returns the equivalent NAL index by proportional mapping because the
    /// NAL stream does not carry per-frame timestamps inline. Accurate seek
    /// requires a companion timestamp sidecar, which is tracked in v0.6.
    pub fn seek_proportional(&self, timestamp_us: u64) -> usize {
        if self.header.duration_secs <= 0.0 || self.nalu_ranges.is_empty() {
            return 0;
        }
        let total_us = (self.header.duration_secs * 1_000_000.0) as u64;
        let frac = (timestamp_us.min(total_us) as f64) / (total_us as f64);
        ((frac * self.nalu_ranges.len() as f64) as usize).min(self.nalu_ranges.len() - 1)
    }
}

/// Split an h264 byte stream into its NAL units by scanning for the 3- or 4-byte
/// Annex B start codes (`00 00 01` / `00 00 00 01`).
fn index_nal_units(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        if bytes[i] == 0 && bytes[i + 1] == 0 {
            if i + 4 <= bytes.len() && bytes[i + 2] == 0 && bytes[i + 3] == 1 {
                starts.push(i + 4);
                i += 4;
                continue;
            }
            if bytes[i + 2] == 1 {
                starts.push(i + 3);
                i += 3;
                continue;
            }
        }
        i += 1;
    }

    let mut ranges = Vec::with_capacity(starts.len());
    for win in starts.windows(2) {
        // End the previous NAL just before the next start code — walk back to
        // skip the trailing 00 00 (0? 1) prefix of the next unit.
        let next_start = win[1];
        let mut end = next_start.saturating_sub(3);
        if end >= 1 && bytes.get(end - 1) == Some(&0) {
            end -= 1;
        }
        ranges.push((win[0], end));
    }
    if let Some(&last) = starts.last() {
        ranges.push((last, bytes.len()));
    }
    ranges
}

/// Single-flight playback controller that decodes a loaded `SessionPlayer`
/// through an ffmpeg subprocess and republishes decoded RGBA frames on the
/// shared `FrameBus`. The display window picks them up via the same path as
/// live capture because both consumers only look at `Frame.rgba`.
///
/// Lifecycle: `load` → `play` → `pause` (or natural end) → optionally
/// `seek` + `play` again. Seeking while playing is rejected; the dashboard
/// pauses first, then seeks, then resumes.
#[derive(Clone)]
pub struct SessionPlaybackController {
    active: Arc<AtomicBool>,
    loaded: Arc<std::sync::Mutex<Option<Arc<SessionPlayer>>>>,
    position: Arc<AtomicUsize>,
    frame_bus: FrameBus,
    /// Test seam: point at a nonexistent binary to exercise the spawn-failure
    /// path without poking `$PATH`.
    ffmpeg_bin: String,
}

impl SessionPlaybackController {
    pub fn new(frame_bus: FrameBus) -> Self {
        Self {
            active: Arc::new(AtomicBool::new(false)),
            loaded: Arc::new(std::sync::Mutex::new(None)),
            position: Arc::new(AtomicUsize::new(0)),
            frame_bus,
            ffmpeg_bin: "ffmpeg".to_string(),
        }
    }

    #[must_use]
    pub fn with_ffmpeg_bin(mut self, bin: impl Into<String>) -> Self {
        self.ffmpeg_bin = bin.into();
        self
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    pub fn current_position(&self) -> usize {
        self.position.load(Ordering::SeqCst)
    }

    pub fn header(&self) -> Option<SessionHeader> {
        self.loaded
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|p| p.header.clone())
    }

    pub fn bookmarks(&self) -> Vec<Bookmark> {
        self.loaded
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|p| p.bookmarks.clone())
            .unwrap_or_default()
    }

    /// Load a session from disk. Stops any in-flight playback; the new session
    /// replaces the old one and position resets to the first NAL.
    pub fn load(&self, dir: impl AsRef<Path>) -> Result<SessionHeader, String> {
        // Stop before swapping so a stale writer task does not feed the new
        // player's NALs into a doomed ffmpeg stdin.
        self.active.store(false, Ordering::SeqCst);
        let player = SessionPlayer::load(dir)?;
        let header = player.header.clone();
        self.position.store(0, Ordering::SeqCst);
        let mut slot = self.loaded.lock().unwrap_or_else(|e| e.into_inner());
        *slot = Some(Arc::new(player));
        Ok(header)
    }

    /// Start decoding + publishing. Errors if no session is loaded or if
    /// ffmpeg fails to spawn. Already-playing is a no-op (returns Ok).
    pub fn play(&self) -> Result<(), String> {
        let player = {
            let slot = self.loaded.lock().unwrap_or_else(|e| e.into_inner());
            slot.as_ref().cloned().ok_or("no session loaded")?
        };
        if self.active.load(Ordering::SeqCst) {
            return Ok(());
        }

        let child = tokio::process::Command::new(&self.ffmpeg_bin)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "h264",
                "-i",
                "pipe:0",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgba",
                "pipe:1",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("spawn ffmpeg: {e}"))?;

        self.active.store(true, Ordering::SeqCst);
        let player_for_task = player.clone();
        let position = self.position.clone();
        let active = self.active.clone();
        let frame_bus = self.frame_bus.clone();
        tokio::spawn(async move {
            if let Err(e) =
                run_playback(player_for_task, position, active.clone(), frame_bus, child).await
            {
                warn!(error = %e, "playback task ended with error");
            }
            active.store(false, Ordering::SeqCst);
        });
        Ok(())
    }

    /// Flip the active flag off. The decode task observes it and tears down
    /// its ffmpeg child (via `Child::kill_on_drop`).
    pub fn pause(&self) {
        self.active.store(false, Ordering::SeqCst);
    }

    /// Update the playback position by proportional seek. Requires the
    /// playback to be paused; returns `Err` while playing to avoid racing the
    /// live decode task on position updates.
    pub fn seek(&self, timestamp_us: u64) -> Result<usize, String> {
        if self.active.load(Ordering::SeqCst) {
            return Err("pause before seeking".to_string());
        }
        let slot = self.loaded.lock().unwrap_or_else(|e| e.into_inner());
        let player = slot.as_ref().ok_or("no session loaded")?;
        let idx = player.seek_proportional(timestamp_us);
        self.position.store(idx, Ordering::SeqCst);
        Ok(idx)
    }
}

async fn run_playback(
    player: Arc<SessionPlayer>,
    position: Arc<AtomicUsize>,
    active: Arc<AtomicBool>,
    frame_bus: FrameBus,
    mut child: tokio::process::Child,
) -> Result<(), String> {
    let width = player.header.width;
    let height = player.header.height;
    if width == 0 || height == 0 {
        active.store(false, Ordering::SeqCst);
        return Err("session header has zero dimensions".to_string());
    }
    let frame_bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or("frame size overflow")?;
    let target_fps = if player.header.duration_secs > 0.0 && player.header.total_frames > 0 {
        (player.header.total_frames as f64 / player.header.duration_secs).clamp(1.0, 120.0)
    } else {
        30.0
    };
    let frame_interval = Duration::from_secs_f64(1.0 / target_fps);

    let mut stdin = child.stdin.take().ok_or("ffmpeg stdin unavailable")?;
    let mut stdout = child.stdout.take().ok_or("ffmpeg stdout unavailable")?;

    let writer_active = active.clone();
    let writer_player = player.clone();
    let writer_position = position.clone();
    let writer = tokio::spawn(async move {
        while writer_active.load(Ordering::SeqCst) {
            let i = writer_position.load(Ordering::SeqCst);
            let Some(nalu) = writer_player.nalu(i).map(<[u8]>::to_vec) else {
                break;
            };
            if stdin.write_all(&[0x00, 0x00, 0x00, 0x01]).await.is_err() {
                break;
            }
            if stdin.write_all(&nalu).await.is_err() {
                break;
            }
            writer_position.store(i + 1, Ordering::SeqCst);
            tokio::time::sleep(frame_interval).await;
        }
        // Close stdin so ffmpeg flushes any pending frames and exits.
        let _ = stdin.shutdown().await;
    });

    let reader_active = active.clone();
    let reader = tokio::spawn(async move {
        let mut buf = vec![0u8; frame_bytes];
        let mut decoded: u64 = 0;
        while reader_active.load(Ordering::SeqCst) {
            match stdout.read_exact(&mut buf).await {
                Ok(_) => {
                    let timestamp_us = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_micros() as u64)
                        .unwrap_or(0);
                    frame_bus.publish(Frame {
                        width,
                        height,
                        rgba: buf.clone(),
                        timestamp_us,
                        h264_nalu: None,
                    });
                    decoded += 1;
                }
                Err(_) => break,
            }
        }
        decoded
    });

    let _ = writer.await;
    let decoded = reader.await.unwrap_or(0);
    let _ = child.kill().await;
    info!(
        frames = decoded,
        "Session playback ended (pause, EOF, or decoder exit)"
    );
    Ok(())
}

/// Convenience: list sessions available in `./recordings` (or another dir).
pub fn list_sessions(dir: impl AsRef<Path>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(dir.as_ref()) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() && p.join("session.json").exists() {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_session(dir: &std::path::Path, total_frames: u64, duration_secs: f64) {
        std::fs::create_dir_all(dir).unwrap();
        let header = SessionHeader {
            start_time: "2026-04-21T00:00:00+00:00".into(),
            width: 100,
            height: 200,
            total_frames,
            duration_secs,
        };
        std::fs::write(
            dir.join("session.json"),
            serde_json::to_string(&header).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("bookmarks.json"),
            serde_json::to_string::<Vec<Bookmark>>(&vec![]).unwrap(),
        )
        .unwrap();
        // Three tiny NAL units separated by the 4-byte start code.
        let video = vec![
            0x00, 0x00, 0x00, 0x01, 0x10, 0x00, 0x00, 0x00, 0x01, 0x20, 0x21, 0x00, 0x00, 0x00,
            0x01, 0x30,
        ];
        std::fs::write(dir.join("video.h264"), video).unwrap();
    }

    #[test]
    fn indexes_annex_b_nal_units() {
        // Two NAL units: [0x00,0x00,0x00,0x01, AA, BB] [0x00,0x00,0x01, CC]
        let stream = vec![0x00, 0x00, 0x00, 0x01, 0xAA, 0xBB, 0x00, 0x00, 0x01, 0xCC];
        let ranges = index_nal_units(&stream);
        assert_eq!(ranges.len(), 2);
        assert_eq!(&stream[ranges[0].0..ranges[0].1], &[0xAA, 0xBB]);
        assert_eq!(&stream[ranges[1].0..ranges[1].1], &[0xCC]);
    }

    #[test]
    fn loader_reads_header_and_indexes_nals() {
        let dir = std::env::temp_dir().join(format!(
            "ios-remote-replay-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        write_session(&dir, 3, 3.0);

        let player = SessionPlayer::load(&dir).unwrap();
        assert_eq!(player.header.width, 100);
        assert_eq!(player.header.height, 200);
        assert_eq!(player.nal_count(), 3);
        assert_eq!(player.nalu(0), Some(&[0x10][..]));
        assert_eq!(player.nalu(1), Some(&[0x20, 0x21][..]));
        assert_eq!(player.nalu(2), Some(&[0x30][..]));
        assert_eq!(player.nalu(3), None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn seek_proportional_maps_endpoints_to_first_and_last() {
        let dir = std::env::temp_dir().join(format!(
            "ios-remote-replay-seek-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        write_session(&dir, 3, 3.0);
        let player = SessionPlayer::load(&dir).unwrap();

        assert_eq!(player.seek_proportional(0), 0);
        // Halfway through 3 seconds → NAL index 1 (of 3).
        assert_eq!(player.seek_proportional(1_500_000), 1);
        // Clamped to last index when requesting past the end.
        assert_eq!(player.seek_proportional(u64::MAX), player.nal_count() - 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    fn unique_session_dir(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("ios-remote-playback-{tag}-{nanos}"))
    }

    #[tokio::test]
    async fn controller_load_then_play_errors_when_ffmpeg_missing() {
        let dir = unique_session_dir("missing-ffmpeg");
        write_session(&dir, 3, 3.0);
        let bus = FrameBus::new();
        let ctl = SessionPlaybackController::new(bus)
            .with_ffmpeg_bin("ios-remote-ffmpeg-definitely-not-installed");

        let header = ctl.load(&dir).unwrap();
        assert_eq!(header.width, 100);

        let err = ctl.play().expect_err("spawn of bogus binary must fail");
        assert!(err.contains("spawn ffmpeg"), "unexpected error: {err}");
        assert!(
            !ctl.is_active(),
            "active flag must remain false on spawn failure"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn pause_when_idle_is_noop() {
        let bus = FrameBus::new();
        let ctl = SessionPlaybackController::new(bus);
        assert!(!ctl.is_active());
        ctl.pause();
        assert!(!ctl.is_active());
    }

    #[test]
    fn play_without_loaded_session_errors() {
        let bus = FrameBus::new();
        let ctl = SessionPlaybackController::new(bus);
        let err = ctl.play().expect_err("play without load must error");
        assert!(err.contains("no session loaded"), "unexpected: {err}");
    }

    #[test]
    fn seek_updates_position_when_paused() {
        let dir = unique_session_dir("seek");
        write_session(&dir, 3, 3.0);
        let bus = FrameBus::new();
        let ctl = SessionPlaybackController::new(bus);
        ctl.load(&dir).unwrap();

        assert_eq!(ctl.current_position(), 0);
        // Halfway through the 3-second clip maps to NAL index 1 (matches
        // seek_proportional_maps_endpoints_to_first_and_last above).
        let idx = ctl.seek(1_500_000).unwrap();
        assert_eq!(idx, 1);
        assert_eq!(ctl.current_position(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Roundtrip test: requires ffmpeg on PATH. Boots the controller against a
    /// tiny garbage NAL stream — ffmpeg will log decode errors but the spawn
    /// must succeed and `play()` must flip the active flag. Ignored by default
    /// because CI does not install ffmpeg.
    #[tokio::test]
    #[ignore]
    async fn roundtrip_play_flips_active_with_real_ffmpeg() {
        let dir = unique_session_dir("roundtrip");
        write_session(&dir, 3, 3.0);
        let bus = FrameBus::new();
        let ctl = SessionPlaybackController::new(bus);
        ctl.load(&dir).unwrap();

        ctl.play().expect("ffmpeg must be on PATH for this test");
        assert!(ctl.is_active());
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        ctl.pause();
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert!(!ctl.is_active());

        std::fs::remove_dir_all(&dir).ok();
    }
}
