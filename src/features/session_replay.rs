use super::Frame;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use tracing::info;

/// Session replay: record full sessions and replay them later.
///
/// Records all frames + timestamps + events to a session file.
/// Can replay at original speed or fast-forward.

#[derive(Serialize, Deserialize)]
struct SessionHeader {
    start_time: String,
    width: u32,
    height: u32,
    total_frames: u64,
    duration_secs: f64,
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
            width: 0, height: 0,
        }
    }

    pub fn start(&mut self) {
        self.frames.clear();
        self.bookmarks.clear();
        self.recording = true;
        self.start_time = std::time::Instant::now();
        info!("Session recording started");
    }

    pub fn stop(&mut self) { self.recording = false; info!("Session recording stopped"); }

    pub fn push_frame(&mut self, frame: &Frame) {
        if !self.recording { return; }
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

    pub fn bookmarks(&self) -> &[Bookmark] { &self.bookmarks }

    /// Save session to directory.
    pub fn save(&self, dir: &str) -> Result<String, String> {
        let path = format!("{}/session_{}", dir, Local::now().format("%Y%m%d_%H%M%S"));
        fs::create_dir_all(&path).map_err(|e| e.to_string())?;

        let header = SessionHeader {
            start_time: Local::now().to_rfc3339(),
            width: self.width, height: self.height,
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
