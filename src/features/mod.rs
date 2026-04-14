pub mod ai_vision;
pub mod frame_analysis;
pub mod macros;
pub mod multi_device;
pub mod notification_capture;
pub mod ocr;
pub mod recording;
pub mod screenshot;
pub mod streaming;
pub mod touch_overlay;
pub mod vr_overlay;

use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

/// A decoded video frame in RGBA format.
#[derive(Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub timestamp_us: u64,
    pub h264_nalu: Option<Vec<u8>>,
}

/// Broadcast bus for distributing frames to all consumers.
#[derive(Clone)]
pub struct FrameBus {
    sender: broadcast::Sender<Arc<Frame>>,
    latest: Arc<Mutex<Option<Arc<Frame>>>>,
}

impl FrameBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(8);
        Self {
            sender,
            latest: Arc::new(Mutex::new(None)),
        }
    }

    pub fn publish(&self, frame: Frame) {
        let frame = Arc::new(frame);
        {
            let mut latest = self.latest.lock().unwrap();
            *latest = Some(frame.clone());
        }
        let _ = self.sender.send(frame);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Frame>> {
        self.sender.subscribe()
    }

    pub fn latest_frame(&self) -> Option<Arc<Frame>> {
        self.latest.lock().unwrap().clone()
    }
}
