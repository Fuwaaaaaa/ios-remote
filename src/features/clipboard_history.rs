use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use tracing::info;

/// Clipboard history: keep a log of all clipboard operations.

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClipboardEntry {
    pub timestamp: DateTime<Local>,
    pub content_type: ClipContent,
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClipContent {
    Text(String),
    Image { width: u32, height: u32, path: String },
    Url(String),
}

#[derive(Default, Serialize, Deserialize)]
pub struct ClipboardHistory {
    entries: Vec<ClipboardEntry>,
    max_entries: usize,
}

impl ClipboardHistory {
    pub fn new(max: usize) -> Self { Self { entries: Vec::new(), max_entries: max } }

    pub fn add_text(&mut self, text: String, source: &str) {
        self.push(ClipboardEntry {
            timestamp: Local::now(),
            content_type: if text.starts_with("http") { ClipContent::Url(text) } else { ClipContent::Text(text) },
            source: source.to_string(),
        });
    }

    pub fn add_image(&mut self, w: u32, h: u32, path: String, source: &str) {
        self.push(ClipboardEntry {
            timestamp: Local::now(),
            content_type: ClipContent::Image { width: w, height: h, path },
            source: source.to_string(),
        });
    }

    fn push(&mut self, entry: ClipboardEntry) {
        self.entries.push(entry);
        if self.entries.len() > self.max_entries { self.entries.remove(0); }
        info!(total = self.entries.len(), "Clipboard history updated");
    }

    pub fn recent(&self, n: usize) -> &[ClipboardEntry] {
        let start = self.entries.len().saturating_sub(n);
        &self.entries[start..]
    }

    pub fn search(&self, query: &str) -> Vec<&ClipboardEntry> {
        let q = query.to_lowercase();
        self.entries.iter().filter(|e| match &e.content_type {
            ClipContent::Text(t) | ClipContent::Url(t) => t.to_lowercase().contains(&q),
            ClipContent::Image { path, .. } => path.to_lowercase().contains(&q),
        }).collect()
    }

    pub fn save(&self, path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }
}
