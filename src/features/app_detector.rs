use super::Frame;
use serde::Serialize;
use std::collections::HashMap;
use chrono::{DateTime, Local};
use tracing::info;

/// App detector: identify which iPhone app is running by screen analysis.
///
/// Maintains a database of app signatures (status bar color, UI patterns)
/// and tracks usage time per app.

#[derive(Debug, Clone, Serialize)]
pub struct AppSession {
    pub app_name: String,
    pub started: DateTime<Local>,
    pub duration_secs: u64,
}

pub struct AppTracker {
    current_app: String,
    app_started: std::time::Instant,
    usage: HashMap<String, u64>,
    sessions: Vec<AppSession>,
}

impl AppTracker {
    pub fn new() -> Self {
        Self {
            current_app: "Unknown".to_string(),
            app_started: std::time::Instant::now(),
            usage: HashMap::new(),
            sessions: Vec::new(),
        }
    }

    /// Analyze a frame and detect which app is likely running.
    pub fn analyze(&mut self, frame: &Frame) {
        let detected = detect_app(frame);

        if detected != self.current_app {
            // Record previous app session
            let duration = self.app_started.elapsed().as_secs();
            *self.usage.entry(self.current_app.clone()).or_insert(0) += duration;
            self.sessions.push(AppSession {
                app_name: self.current_app.clone(),
                started: Local::now() - chrono::Duration::seconds(duration as i64),
                duration_secs: duration,
            });

            info!(from = %self.current_app, to = %detected, "App switch detected");
            self.current_app = detected;
            self.app_started = std::time::Instant::now();
        }
    }

    /// Get usage stats sorted by time (descending).
    pub fn usage_stats(&self) -> Vec<(String, u64)> {
        let mut stats: Vec<_> = self.usage.iter().map(|(k, v)| (k.clone(), *v)).collect();
        stats.sort_by(|a, b| b.1.cmp(&a.1));
        stats
    }

    pub fn current(&self) -> &str { &self.current_app }
    pub fn sessions(&self) -> &[AppSession] { &self.sessions }
}

/// Simple app detection based on screen content heuristics.
fn detect_app(frame: &Frame) -> String {
    if frame.rgba.is_empty() || frame.width == 0 { return "Unknown".to_string(); }

    // Sample the status bar area (top 44px) for color
    let status_color = sample_region_avg(&frame.rgba, frame.width, 0, 0, frame.width, 44.min(frame.height));

    // Sample the bottom nav bar (bottom 83px)
    let nav_y = frame.height.saturating_sub(83);
    let nav_color = sample_region_avg(&frame.rgba, frame.width, 0, nav_y, frame.width, 83.min(frame.height));

    // Heuristic detection based on dominant colors
    match (status_color, nav_color) {
        ((r, _, _), _) if r > 200 && status_color.1 < 80 && status_color.2 < 80 => "Phone (Red)".to_string(),
        (_, (_, g, _)) if g > 200 && nav_color.0 < 80 => "Messages".to_string(),
        ((r, g, b), _) if r < 30 && g < 30 && b < 30 => "Dark Mode App".to_string(),
        ((r, g, b), _) if r > 240 && g > 240 && b > 240 => "Light Mode App".to_string(),
        _ => "Unknown".to_string(),
    }
}

fn sample_region_avg(rgba: &[u8], w: u32, x: u32, y: u32, rw: u32, rh: u32) -> (u8, u8, u8) {
    let mut sum = [0u64; 3];
    let mut count = 0u64;
    let step = 4;
    for py in (y..y + rh).step_by(step) {
        for px in (x..x + rw).step_by(step) {
            let idx = ((py * w + px) * 4) as usize;
            if idx + 2 < rgba.len() {
                sum[0] += rgba[idx] as u64;
                sum[1] += rgba[idx + 1] as u64;
                sum[2] += rgba[idx + 2] as u64;
                count += 1;
            }
        }
    }
    if count == 0 { return (0, 0, 0); }
    ((sum[0] / count) as u8, (sum[1] / count) as u8, (sum[2] / count) as u8)
}
