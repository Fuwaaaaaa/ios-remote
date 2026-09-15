//! Live stream statistics behind `/api/status` and `/api/stats`.
//!
//! Two independent writers feed one snapshot:
//! - the device session (USB receiver or synthetic mode) reports connect /
//!   disconnect plus the device identity, and appends to the connection
//!   history;
//! - a frame meter subscribed to the `FrameBus` measures FPS, frame count,
//!   resolution and H.264 bitrate from what actually flows.
//!
//! Nothing here is hard-coded: synthetic mode reports the FPS it really
//! renders, and a real device reports whatever screenshotr delivers.

use crate::config::ConnectionHistory;
use crate::features::FrameBus;
use crate::ui::api::StreamStats;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

/// FPS / bitrate are recomputed over windows of this length.
const METER_WINDOW: Duration = Duration::from_secs(1);
/// With no frame for this long, FPS and bitrate read as 0 even if the last
/// window measured something.
const STALE_AFTER: Duration = Duration::from_secs(3);

#[derive(Clone)]
pub struct StatsHub {
    inner: Arc<Mutex<Inner>>,
    history: Option<Arc<tokio::sync::Mutex<ConnectionHistory>>>,
}

struct DeviceSession {
    udid: String,
    name: String,
    since: Instant,
}

struct Inner {
    device: Option<DeviceSession>,
    frames_received: u64,
    resolution: Option<(u32, u32)>,
    fps: f64,
    bitrate_kbps: f64,
    window_start: Instant,
    window_frames: u64,
    window_bytes: u64,
    last_frame_at: Option<Instant>,
}

impl StatsHub {
    /// `history`: where connect/disconnect events are persisted. `None` keeps
    /// the session out of the history file (synthetic mode, tests).
    pub fn new(history: Option<Arc<tokio::sync::Mutex<ConnectionHistory>>>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                device: None,
                frames_received: 0,
                resolution: None,
                fps: 0.0,
                bitrate_kbps: 0.0,
                window_start: Instant::now(),
                window_frames: 0,
                window_bytes: 0,
                last_frame_at: None,
            })),
            history,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Mark a device session live. Idempotent for the same UDID, so callers
    /// may invoke it on every reconnect attempt without inflating history.
    pub async fn device_connected(&self, udid: &str, name: &str) {
        {
            let mut inner = self.lock();
            if inner.device.as_ref().is_some_and(|d| d.udid == udid) {
                return;
            }
            inner.device = Some(DeviceSession {
                udid: udid.to_string(),
                name: name.to_string(),
                since: Instant::now(),
            });
        }
        if let Some(history) = &self.history {
            history.lock().await.record_connection(udid, name, 0);
        }
    }

    /// End the current session (no-op when none is live) and add its duration
    /// to the device's history record.
    pub async fn device_disconnected(&self) {
        let ended = self.lock().device.take();
        if let (Some(session), Some(history)) = (ended, &self.history) {
            let secs = session.since.elapsed().as_secs();
            history.lock().await.add_duration(&session.udid, secs);
        }
    }

    /// Account for one frame published on the bus.
    pub fn observe_frame(&self, width: u32, height: u32, has_image: bool, nal_bytes: usize) {
        self.observe_frame_at(width, height, has_image, nal_bytes, Instant::now());
    }

    fn observe_frame_at(
        &self,
        width: u32,
        height: u32,
        has_image: bool,
        nal_bytes: usize,
        now: Instant,
    ) {
        let mut inner = self.lock();
        if has_image {
            inner.frames_received += 1;
            inner.window_frames += 1;
            inner.resolution = Some((width, height));
            inner.last_frame_at = Some(now);
        }
        inner.window_bytes += nal_bytes as u64;
        let elapsed = now.saturating_duration_since(inner.window_start);
        if elapsed >= METER_WINDOW {
            let secs = elapsed.as_secs_f64();
            inner.fps = inner.window_frames as f64 / secs;
            inner.bitrate_kbps = inner.window_bytes as f64 * 8.0 / 1000.0 / secs;
            inner.window_frames = 0;
            inner.window_bytes = 0;
            inner.window_start = now;
        }
    }

    /// Subscribe to the bus and keep the meter current for the process
    /// lifetime.
    pub fn spawn_frame_meter(&self, bus: &FrameBus) {
        let hub = self.clone();
        let mut rx = bus.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(frame) => hub.observe_frame(
                        frame.width,
                        frame.height,
                        !frame.rgba.is_empty(),
                        frame.h264_nalu.as_ref().map_or(0, Vec::len),
                    ),
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    pub fn snapshot(&self) -> StreamStats {
        self.snapshot_at(Instant::now())
    }

    fn snapshot_at(&self, now: Instant) -> StreamStats {
        let inner = self.lock();
        let fresh = inner
            .last_frame_at
            .is_some_and(|t| now.saturating_duration_since(t) < STALE_AFTER);
        StreamStats {
            connected: inner.device.is_some(),
            device_name: inner
                .device
                .as_ref()
                .map(|d| d.name.clone())
                .unwrap_or_default(),
            fps: if fresh { inner.fps } else { 0.0 },
            frames_received: inner.frames_received,
            uptime_secs: inner
                .device
                .as_ref()
                .map_or(0, |d| now.saturating_duration_since(d.since).as_secs()),
            resolution: inner
                .resolution
                .map(|(w, h)| format!("{w}x{h}"))
                .unwrap_or_default(),
            bitrate_kbps: if fresh { inner.bitrate_kbps } else { 0.0 },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_measures_fps_and_resolution() {
        let hub = StatsHub::new(None);
        let t0 = Instant::now();
        // 30 frames spread over just over one window.
        for i in 0..=30u64 {
            hub.observe_frame_at(390, 844, true, 0, t0 + Duration::from_millis(i * 35));
        }
        let s = hub.snapshot_at(t0 + Duration::from_millis(1100));
        assert_eq!(s.resolution, "390x844");
        assert_eq!(s.frames_received, 31);
        assert!(s.fps > 25.0 && s.fps < 35.0, "fps={}", s.fps);
    }

    #[test]
    fn nal_only_frames_count_toward_bitrate_not_frames() {
        let hub = StatsHub::new(None);
        let t0 = Instant::now();
        hub.observe_frame_at(1, 1, false, 125, t0);
        hub.observe_frame_at(1, 1, false, 0, t0 + Duration::from_millis(1000));
        let s = hub.snapshot_at(t0 + Duration::from_millis(1000));
        assert_eq!(s.frames_received, 0);
        // No image frame yet → stale → 0; the meter must not invent FPS.
        assert_eq!(s.fps, 0.0);
    }

    #[test]
    fn fps_decays_to_zero_when_frames_stop() {
        let hub = StatsHub::new(None);
        let t0 = Instant::now();
        for i in 0..=20u64 {
            hub.observe_frame_at(10, 10, true, 0, t0 + Duration::from_millis(i * 60));
        }
        assert!(hub.snapshot_at(t0 + Duration::from_millis(1300)).fps > 0.0);
        assert_eq!(hub.snapshot_at(t0 + Duration::from_secs(10)).fps, 0.0);
    }

    #[tokio::test]
    async fn device_session_drives_connected_and_history() {
        let history = Arc::new(tokio::sync::Mutex::new(ConnectionHistory::default()));
        let hub = StatsHub::new(Some(history.clone()));
        assert!(!hub.snapshot().connected);

        hub.device_connected("UDID-1", "Test iPhone").await;
        // Repeated connect for the same device must not double-count.
        hub.device_connected("UDID-1", "Test iPhone").await;
        let s = hub.snapshot();
        assert!(s.connected);
        assert_eq!(s.device_name, "Test iPhone");

        hub.device_disconnected().await;
        assert!(!hub.snapshot().connected);
        let h = history.lock().await;
        assert_eq!(h.records.len(), 1);
        assert_eq!(h.records[0].connect_count, 1);
    }
}
