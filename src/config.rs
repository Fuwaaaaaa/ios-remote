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
    /// Display name for this receiver instance.
    pub name: String,
    /// Legacy listen port (unused in USB mode; retained for config compatibility).
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
    /// Bind address for HTTP servers (Web Dashboard, MJPEG). Default 127.0.0.1.
    /// Use `lan_access = true` (or CLI `--lan`) to switch to 0.0.0.0.
    pub bind_address: String,
    /// When true, forces bind_address to 0.0.0.0 and requires an API token.
    pub lan_access: bool,
    /// Bearer token required on every /api/* request. Auto-generated on first
    /// launch if None. Overridden by env var `IOS_REMOTE_API_TOKEN`.
    pub api_token: Option<String>,
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
                bind_address: "127.0.0.1".to_string(),
                lan_access: false,
                api_token: None,
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

    /// Ensure an API token exists. Preference order:
    ///   1. `IOS_REMOTE_API_TOKEN` environment variable
    ///   2. `config.network.api_token` from disk
    ///   3. Freshly generated 32-byte URL-safe token (persisted to disk)
    ///
    /// Returns the resolved token. Call this once on startup.
    pub fn resolve_api_token(&mut self) -> String {
        if let Ok(env_token) = std::env::var("IOS_REMOTE_API_TOKEN") {
            let trimmed = env_token.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        if let Some(existing) = &self.network.api_token
            && !existing.is_empty() {
                return existing.clone();
            }
        let token = generate_token();
        self.network.api_token = Some(token.clone());
        self.save();
        token
    }
}

/// Generate a 32-byte URL-safe random token.
fn generate_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut bytes);
    // URL-safe base64 without padding (0-9A-Za-z-_).
    const ALPHABET: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(32);
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | (bytes[i + 2] as u32);
        out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 6) & 0x3f) as usize] as char);
        out.push(ALPHABET[(n & 0x3f) as usize] as char);
        i += 3;
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // `AppConfig::{load, save}` hard-code the filename `ios-remote.toml` in
    // the CWD, so tests that touch disk must serialize access.
    static CWD_GUARD: Mutex<()> = Mutex::new(());

    fn with_tempdir<F: FnOnce()>(f: F) {
        let guard = CWD_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let original = std::env::current_dir().expect("cwd");
        let dir = std::env::temp_dir().join(format!(
            "ios-remote-cfg-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_current_dir(&dir).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        std::env::set_current_dir(&original).ok();
        std::fs::remove_dir_all(&dir).ok();
        drop(guard);
        if let Err(p) = result {
            std::panic::resume_unwind(p);
        }
    }

    #[test]
    fn default_bind_address_is_loopback() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.network.bind_address, "127.0.0.1");
        assert!(!cfg.network.lan_access);
        assert!(cfg.network.api_token.is_none());
    }

    #[test]
    fn save_and_load_round_trip_preserves_fields() {
        with_tempdir(|| {
            let mut original = AppConfig::default();
            original.network.api_token = Some("test-token-123".into());
            original.network.lan_access = true;
            original.save();

            let loaded = AppConfig::load();
            assert_eq!(loaded.network.api_token.as_deref(), Some("test-token-123"));
            assert!(loaded.network.lan_access);
            assert_eq!(loaded.network.bind_address, "127.0.0.1");
        });
    }

    #[test]
    fn resolve_api_token_generates_when_missing() {
        with_tempdir(|| {
            // Clear env for deterministic generation path.
            // SAFETY: process-global env mutation; serialized by CWD_GUARD.
            unsafe { std::env::remove_var("IOS_REMOTE_API_TOKEN") };
            let mut cfg = AppConfig::default();
            let resolved = cfg.resolve_api_token();
            assert!(resolved.len() >= 24, "token should be >= 24 chars");
            assert_eq!(cfg.network.api_token.as_deref(), Some(resolved.as_str()));
        });
    }

    #[test]
    fn generated_tokens_are_unique() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(a, b);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }
}
