pub mod frame_analysis;
pub mod notification_capture;
pub mod recording;
pub mod screenshot;
pub mod streaming;

use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

/// A decoded video frame in RGBA format.
#[derive(Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// Packed RGBA pixels (width * height * 4 bytes).
    pub rgba: Vec<u8>,
    /// Timestamp in microseconds from stream start.
    pub timestamp_us: u64,
    /// H.264 NAL unit data (for recording without re-encode).
    pub h264_nalu: Option<Vec<u8>>,
}

/// Broadcast bus for distributing frames to all consumers.
///
/// The bus uses tokio::broadcast so multiple receivers (display, recorder,
/// screenshot, OBS, RTMP, AI) each get every frame independently.
/// Slow consumers drop frames automatically (lagged).
#[derive(Clone)]
pub struct FrameBus {
    sender: broadcast::Sender<Arc<Frame>>,
    /// Latest frame for snapshot access (screenshot, OCR).
    latest: Arc<Mutex<Option<Arc<Frame>>>>,
}

impl FrameBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(8); // buffer 8 frames
        Self {
            sender,
            latest: Arc::new(Mutex::new(None)),
        }
    }

    /// Publish a frame to all subscribers.
    pub fn publish(&self, frame: Frame) {
        let frame = Arc::new(frame);
        {
            let mut latest = self.latest.lock().unwrap();
            *latest = Some(frame.clone());
        }
        let _ = self.sender.send(frame); // ok if no receivers
    }

    /// Subscribe to receive all future frames.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Frame>> {
        self.sender.subscribe()
    }

    /// Get the most recent frame (for screenshot/OCR).
    pub fn latest_frame(&self) -> Option<Arc<Frame>> {
        self.latest.lock().unwrap().clone()
    }
}
