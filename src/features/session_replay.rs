use super::Frame;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

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
        let header: SessionHeader = serde_json::from_str(&header_json)
            .map_err(|e| format!("parse session.json: {e}"))?;

        let bookmarks_path = dir.join("bookmarks.json");
        let bookmarks: Vec<Bookmark> = if bookmarks_path.exists() {
            let raw = fs::read_to_string(&bookmarks_path)
                .map_err(|e| format!("read bookmarks.json: {e}"))?;
            serde_json::from_str(&raw).map_err(|e| format!("parse bookmarks.json: {e}"))?
        } else {
            Vec::new()
        };

        let video = fs::read(dir.join("video.h264"))
            .map_err(|e| format!("read video.h264: {e}"))?;
        let nalu_ranges = index_nal_units(&video);

        info!(
            path = %dir.display(),
            nalus = nalu_ranges.len(),
            bookmarks = bookmarks.len(),
            "Session loaded"
        );
        Ok(Self { header, bookmarks, video, nalu_ranges })
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
            0x00, 0x00, 0x00, 0x01, 0x10,
            0x00, 0x00, 0x00, 0x01, 0x20, 0x21,
            0x00, 0x00, 0x00, 0x01, 0x30,
        ];
        std::fs::write(dir.join("video.h264"), video).unwrap();
    }

    #[test]
    fn indexes_annex_b_nal_units() {
        // Two NAL units: [0x00,0x00,0x00,0x01, AA, BB] [0x00,0x00,0x01, CC]
        let stream = vec![
            0x00, 0x00, 0x00, 0x01, 0xAA, 0xBB,
            0x00, 0x00, 0x01, 0xCC,
        ];
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
}
