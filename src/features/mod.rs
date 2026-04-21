pub mod ai_vision;
pub mod annotation;
pub mod app_detector;
pub mod audio_transcription;
pub mod audio_viz;
pub mod auto_connect;
pub mod battery_saver;
pub mod benchmark;
pub mod clipboard_history;
pub mod clipboard_sync;
pub mod custom_cursor;
pub mod color_picker;
pub mod design_overlay;
pub mod device_frame;
pub mod display;
pub mod drag_drop;
pub mod frame_analysis;
pub mod game_mode;
pub mod gestures;
pub mod gif_capture;
pub mod heatmap;
pub mod i18n;
pub mod keyboard_input;
pub mod macros;
pub mod wda_client;
pub mod mouse_gesture;
pub mod multi_device;
pub mod notification_capture;
pub mod notification_rules;
pub mod ocr;
pub mod ocr_history;
pub mod pdf_export;
pub mod presentation;
pub mod privacy_mode;
pub mod qr_generator;
pub mod qr_scanner;
pub mod recording;
pub mod ruler;
pub mod screen_diff;
pub mod screen_rotation;
pub mod screensaver;
pub mod screenshot;
pub mod session_replay;
pub mod sharing;
pub mod smart_crop;
pub mod scheduler;
pub mod smart_recording;
pub mod sound_notify;
pub mod stats_export;
pub mod stats_overlay;
pub mod stream_deck;
pub mod streaming;
pub mod template_match;
pub mod themes;
pub mod timelapse;
pub mod touch_overlay;
pub mod translation;
pub mod tts;
pub mod video_filter;
pub mod voice_command;
pub mod vr_overlay;
pub mod watermark;
pub mod webhook;
pub mod imgur_share;
pub mod zoom;

use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub timestamp_us: u64,
    pub h264_nalu: Option<Vec<u8>>,
}

#[derive(Clone)]
pub struct FrameBus {
    sender: broadcast::Sender<Arc<Frame>>,
    latest: Arc<Mutex<Option<Arc<Frame>>>>,
}

impl FrameBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(8);
        Self { sender, latest: Arc::new(Mutex::new(None)) }
    }
    pub fn publish(&self, frame: Frame) {
        let frame = Arc::new(frame);
        {
            // Poisoned locks recover by ignoring the poison — our state is a simple
            // Arc swap and the previous holder panicking cannot leave it inconsistent.
            let mut l = self.latest.lock().unwrap_or_else(|e| e.into_inner());
            *l = Some(frame.clone());
        }
        let _ = self.sender.send(frame);
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Frame>> { self.sender.subscribe() }
    pub fn latest_frame(&self) -> Option<Arc<Frame>> {
        self.latest
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}
