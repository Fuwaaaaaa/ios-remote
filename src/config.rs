use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use tracing::info;

const CONFIG_FILE: &str = "ios-remote.toml";
const HISTORY_FILE: &str = "connection_history.json";

/// Persistent application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub receiver: ReceiverSettings,
    pub display: DisplaySettings,
    pub recording: RecordingSettings,
    pub network: NetworkSettings,
    pub features: FeatureToggles,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiverSettings {
    /// Name shown on iPhone's AirPlay device list.
    pub name: String,
    /// RTSP listen port.
    pub port: u16,
    /// Maximum resolution to advertise (width).
    pub max_width: u32,
    /// Maximum resolution to advertise (height).
    pub max_height: u32,
    /// Maximum FPS to advertise.
    pub max_fps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplaySettings {
    /// Start in picture-in-picture mode.
    pub pip_mode: bool,
    /// Default window width.
    pub window_width: u32,
    /// Default window height.
    pub window_height: u32,
    /// Show FPS/latency overlay.
    pub show_stats: bool,
    /// Show touch overlay (ripple, trail).
    pub show_touch_overlay: bool,
    /// Dark background color (hex).
    pub background_color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingSettings {
    /// Auto-start recording on connect.
    pub auto_record: bool,
    /// Output directory.
    pub output_dir: String,
    /// Maximum recording duration in seconds (0 = unlimited).
    pub max_duration_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSettings {
    /// Bind address for all servers.
    pub bind_address: String,
    /// RTMP streaming URL (empty = disabled).
    pub rtmp_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureToggles {
    pub obs_virtual_camera: bool,
    pub notification_capture: bool,
    pub ocr: bool,
    pub ai_vision: bool,
    pub macros: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            receiver: ReceiverSettings {
                name: "ios-remote".to_string(),
                port: 7000,
                max_width: 1920,
                max_height: 1080,
                max_fps: 60,
            },
            display: DisplaySettings {
                pip_mode: false,
                window_width: 960,
                window_height: 540,
                show_stats: true,
                show_touch_overlay: true,
                background_color: "#222222".to_string(),
            },
            recording: RecordingSettings {
                auto_record: false,
                output_dir: "recordings".to_string(),
                max_duration_secs: 0,
            },
            network: NetworkSettings {
                bind_address: "0.0.0.0".to_string(),
                rtmp_url: String::new(),
            },
            features: FeatureToggles {
                obs_virtual_camera: false,
                notification_capture: true,
                ocr: false,
                ai_vision: false,
                macros: false,
            },
        }
    }
}

impl AppConfig {
    /// Load config from file, or create default if missing.
    pub fn load() -> Self {
        match fs::read_to_string(CONFIG_FILE) {
            Ok(content) => {
                match toml::from_str(&content) {
                    Ok(config) => {
                        info!(file = CONFIG_FILE, "Configuration loaded");
                        config
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Config parse error — using defaults");
                        Self::default()
                    }
                }
            }
            Err(_) => {
                let config = Self::default();
                config.save();
                info!(file = CONFIG_FILE, "Default configuration created");
                config
            }
        }
    }

    /// Save config to file.
    pub fn save(&self) {
        if let Ok(content) = toml::to_string_pretty(self) {
            let _ = fs::write(CONFIG_FILE, content);
        }
    }
}

// ─── Connection History ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionRecord {
    pub device_id: String,
    pub device_name: String,
    pub last_connected: DateTime<Utc>,
    pub connect_count: u32,
    pub total_duration_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConnectionHistory {
    pub records: Vec<ConnectionRecord>,
}

impl ConnectionHistory {
    pub fn load() -> Self {
        match fs::read_to_string(HISTORY_FILE) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(HISTORY_FILE, json);
        }
    }

    /// Record a new connection or update an existing one.
    pub fn record_connection(&mut self, device_id: &str, device_name: &str, duration_secs: u64) {
        if let Some(record) = self.records.iter_mut().find(|r| r.device_id == device_id) {
            record.last_connected = Utc::now();
            record.connect_count += 1;
            record.total_duration_secs += duration_secs;
            record.device_name = device_name.to_string();
        } else {
            self.records.push(ConnectionRecord {
                device_id: device_id.to_string(),
                device_name: device_name.to_string(),
                last_connected: Utc::now(),
                connect_count: 1,
                total_duration_secs: duration_secs,
            });
        }
        self.save();
        info!(device = %device_id, "Connection recorded");
    }

    /// Get recently connected devices, most recent first.
    pub fn recent(&self, limit: usize) -> Vec<&ConnectionRecord> {
        let mut sorted: Vec<_> = self.records.iter().collect();
        sorted.sort_by(|a, b| b.last_connected.cmp(&a.last_connected));
        sorted.truncate(limit);
        sorted
    }
}
