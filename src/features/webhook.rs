use serde::{Deserialize, Serialize};
use tracing::info;

/// Webhook triggers: POST to external URLs on events.

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub url: String,
    pub events: Vec<WebhookEvent>,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum WebhookEvent {
    Connected,
    Disconnected,
    NotificationDetected,
    QrDetected,
    RecordingStarted,
    RecordingStopped,
    ScreenshotTaken,
    MacroCompleted,
}

pub struct WebhookManager {
    hooks: Vec<WebhookConfig>,
}

impl WebhookManager {
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    pub fn add(&mut self, hook: WebhookConfig) {
        self.hooks.push(hook);
    }

    /// Fire all hooks matching the event.
    pub fn fire(&self, event: WebhookEvent, payload: &serde_json::Value) {
        for hook in &self.hooks {
            if !hook.enabled || !hook.events.contains(&event) {
                continue;
            }
            let url = hook.url.clone();
            let body = serde_json::json!({
                "event": format!("{:?}", event),
                "data": payload,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });
            // Fire-and-forget
            std::thread::spawn(move || {
                let _ = std::process::Command::new("curl")
                    .args([
                        "-s",
                        "-X",
                        "POST",
                        &url,
                        "-H",
                        "Content-Type: application/json",
                        "-d",
                        &body.to_string(),
                    ])
                    .output();
            });
            info!(url = %hook.url, event = ?event, "Webhook fired");
        }
    }
}

/// Batch screenshot: take screenshots at regular intervals.
pub async fn batch_screenshots(bus: super::FrameBus, interval_secs: u64, count: u64) {
    let dir = "screenshots/batch";
    let _ = std::fs::create_dir_all(dir);
    info!(interval = interval_secs, count, "Batch screenshot started");

    for _i in 0..count {
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        if let Some(frame) = bus.latest_frame() {
            let _ = super::screenshot::save_frame(&frame);
        }
    }
    info!("Batch screenshot complete");
}
