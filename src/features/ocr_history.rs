use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fs;
use tracing::info;

/// OCR history: store all extracted text with timestamps for later search.

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OcrEntry {
    pub timestamp: DateTime<Local>,
    pub text: String,
    pub region: Option<(u32, u32, u32, u32)>,
    pub screenshot_path: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct OcrHistory {
    pub entries: Vec<OcrEntry>,
}

const HISTORY_FILE: &str = "ocr_history.json";

impl OcrHistory {
    pub fn load() -> Self {
        fs::read_to_string(HISTORY_FILE)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(HISTORY_FILE, json);
        }
    }

    pub fn add(&mut self, text: String, region: Option<(u32, u32, u32, u32)>, screenshot: Option<String>) {
        self.entries.push(OcrEntry {
            timestamp: Local::now(), text, region, screenshot_path: screenshot,
        });
        self.save();
        info!(total = self.entries.len(), "OCR entry saved");
    }

    /// Search history by keyword.
    pub fn search(&self, query: &str) -> Vec<&OcrEntry> {
        let q = query.to_lowercase();
        self.entries.iter().filter(|e| e.text.to_lowercase().contains(&q)).collect()
    }

    pub fn recent(&self, n: usize) -> &[OcrEntry] {
        let start = self.entries.len().saturating_sub(n);
        &self.entries[start..]
    }
}
