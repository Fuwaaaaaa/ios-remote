use chrono::{DateTime, Local};
use serde::Serialize;
use std::fs;
use tracing::info;

/// Connection timeline: visual log of all events during a session.

#[derive(Debug, Clone, Serialize)]
pub struct TimelineEvent {
    pub timestamp: DateTime<Local>,
    pub event_type: EventType,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub enum EventType {
    Connected,
    Disconnected,
    StreamStarted,
    StreamStopped,
    ScreenshotTaken,
    RecordingStarted,
    RecordingStopped,
    NotificationDetected,
    QrDetected,
    MacroExecuted,
    Error,
    Custom,
}

pub struct Timeline {
    events: Vec<TimelineEvent>,
    max_events: usize,
}

impl Timeline {
    pub fn new(max_events: usize) -> Self {
        Self {
            events: Vec::new(),
            max_events,
        }
    }

    pub fn push(&mut self, event_type: EventType, description: &str) {
        let event = TimelineEvent {
            timestamp: Local::now(),
            event_type,
            description: description.to_string(),
        };

        info!(
            event = %description,
            "Timeline: {:?}",
            event.event_type
        );

        self.events.push(event);
        if self.events.len() > self.max_events {
            self.events.remove(0);
        }
    }

    pub fn recent(&self, count: usize) -> &[TimelineEvent] {
        let start = self.events.len().saturating_sub(count);
        &self.events[start..]
    }

    /// Export timeline to JSON file.
    pub fn export(&self, path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&self.events).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())
    }

    /// Generate ASCII timeline visualization.
    pub fn ascii_view(&self, last_n: usize) -> String {
        let events = self.recent(last_n);
        let mut out = String::new();
        out.push_str("┌─── Timeline ───────────────────────────────────\n");

        for event in events {
            let time = event.timestamp.format("%H:%M:%S");
            let icon = match event.event_type {
                EventType::Connected => "🟢",
                EventType::Disconnected => "🔴",
                EventType::StreamStarted => "▶",
                EventType::StreamStopped => "⏹",
                EventType::ScreenshotTaken => "📷",
                EventType::RecordingStarted => "⏺",
                EventType::RecordingStopped => "⏹",
                EventType::NotificationDetected => "🔔",
                EventType::QrDetected => "📱",
                EventType::MacroExecuted => "⚡",
                EventType::Error => "❌",
                EventType::Custom => "•",
            };
            out.push_str(&format!("│ {} {} {}\n", time, icon, event.description));
        }

        out.push_str("└─────────────────────────────────────────────────\n");
        out
    }
}
