use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use tracing::info;

const CONFIG_FILE: &str = "ios-remote.toml";
const HISTORY_FILE: &str = "connection_history.json";

/// Persistent application configuration (`ios-remote.toml`).
///
/// Every section and every key is optional: missing values fall back to
/// their defaults, so a hand-written file with only the keys you care about
/// loads fine. Unknown keys (including ones removed in earlier releases, such
/// as `[features]` or `receiver.port`) are ignored.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppConfig {
    pub receiver: ReceiverSettings,
    pub display: DisplaySettings,
    pub recording: RecordingSettings,
    pub network: NetworkSettings,
    pub audio: AudioSettings,
    /// False when the on-disk file failed to parse. Such a config must never
    /// be written back — that would silently replace the user's file with
    /// defaults.
    #[serde(skip)]
    persist: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReceiverSettings {
    /// Name shown in the display window title. `--name` overrides it.
    pub name: String,
}

impl Default for ReceiverSettings {
    fn default() -> Self {
        Self {
            name: "ios-remote".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplaySettings {
    /// Start with the window always on top (same as `--pip`).
    pub pip_mode: bool,
    /// Initial window width in pixels.
    pub window_width: u32,
    /// Initial window height in pixels.
    pub window_height: u32,
    /// Show the FPS / resolution overlay at startup (toggle with F4).
    pub show_stats: bool,
    /// Background shown before the first frame arrives (`#RRGGBB`).
    pub background_color: String,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            pip_mode: false,
            window_width: 960,
            window_height: 540,
            show_stats: false,
            background_color: "#222222".to_string(),
        }
    }
}

impl DisplaySettings {
    /// `background_color` as 0x00RRGGBB, falling back to dark grey.
    pub fn background_rgb(&self) -> u32 {
        let hex = self.background_color.trim().trim_start_matches('#');
        if hex.len() == 6 {
            u32::from_str_radix(hex, 16).unwrap_or(0x0022_2222)
        } else {
            0x0022_2222
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RecordingSettings {
    /// Start recording as soon as the app starts (same as `--record`).
    pub auto_record: bool,
    /// Directory for session recordings; also where Replay looks for them.
    pub output_dir: String,
    /// Stop a recording automatically after this many seconds (0 = unlimited).
    pub max_duration_secs: u64,
}

impl Default for RecordingSettings {
    fn default() -> Self {
        Self {
            auto_record: false,
            output_dir: "recordings".to_string(),
            max_duration_secs: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkSettings {
    /// Bind address for the Web Dashboard / API. Default 127.0.0.1.
    /// Use `lan_access = true` (or CLI `--lan`) to switch to 0.0.0.0.
    pub bind_address: String,
    /// When true, forces bind_address to 0.0.0.0.
    pub lan_access: bool,
    /// Bearer token required on every /api/* request. Auto-generated on first
    /// launch if None. Overridden by env var `IOS_REMOTE_API_TOKEN`.
    pub api_token: Option<String>,
    /// RTMP URL to live-stream the H.264 feed to via ffmpeg (empty = off).
    pub rtmp_url: String,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".to_string(),
            lan_access: false,
            api_token: None,
            rtmp_url: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    /// One of `loopback`, `mic`, `off`. Defaults to `loopback` (WASAPI).
    pub source: String,
    /// Window size fed to Whisper per chunk, in seconds.
    pub chunk_secs: u32,
    /// Optional language hint (e.g. "ja") passed to Whisper / the OpenAI
    /// API. `None` = auto-detect.
    pub language: Option<String>,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            source: "loopback".to_string(),
            chunk_secs: 5,
            language: None,
        }
    }
}

impl AppConfig {
    /// Load config from file, or create default if missing.
    ///
    /// A file that fails to parse is reported and left untouched: the
    /// defaults are used for this run but never saved over it.
    pub fn load() -> Self {
        match fs::read_to_string(CONFIG_FILE) {
            Ok(content) => match toml::from_str::<Self>(&content) {
                Ok(mut config) => {
                    info!(file = CONFIG_FILE, "Configuration loaded");
                    config.persist = true;
                    config
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        file = CONFIG_FILE,
                        "Config parse error — using defaults for this run; the file is left unchanged"
                    );
                    Self::default()
                }
            },
            Err(_) => {
                let config = Self::default().into_persistent();
                config.save();
                info!(file = CONFIG_FILE, "Default configuration created");
                config
            }
        }
    }

    /// Save config to file. No-op for a config whose file failed to parse.
    pub fn save(&self) {
        if !self.persist {
            tracing::warn!(
                file = CONFIG_FILE,
                "Not saving configuration: the existing file could not be parsed"
            );
            return;
        }
        if let Ok(content) = toml::to_string_pretty(self) {
            let _ = fs::write(CONFIG_FILE, content);
        }
    }

    /// Mark a config (e.g. one received via `POST /api/config`) as safe to
    /// write to disk.
    pub fn into_persistent(mut self) -> Self {
        self.persist = true;
        self
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
            && !existing.is_empty()
        {
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
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
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
    /// Backing file. `None` (the `Default`) keeps the history in memory only,
    /// which is what tests and synthetic mode want.
    #[serde(skip)]
    path: Option<std::path::PathBuf>,
}

impl ConnectionHistory {
    /// Load `connection_history.json` from the working directory; later
    /// updates are written back to it.
    pub fn load() -> Self {
        let mut history: Self = match fs::read_to_string(HISTORY_FILE) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        };
        history.path = Some(std::path::PathBuf::from(HISTORY_FILE));
        history
    }

    pub fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, json);
        }
    }

    /// Add connected time to an existing record (called when a session ends).
    pub fn add_duration(&mut self, device_id: &str, duration_secs: u64) {
        if let Some(record) = self.records.iter_mut().find(|r| r.device_id == device_id) {
            record.total_duration_secs += duration_secs;
            self.save();
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
        sorted.sort_by_key(|r| std::cmp::Reverse(r.last_connected));
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

    /// Every key under `table`, as dotted paths (`display.pip_mode`).
    fn key_paths(table: &toml::Table, prefix: &str, out: &mut Vec<String>) {
        for (key, value) in table {
            let path = format!("{prefix}{key}");
            match value {
                toml::Value::Table(inner) => key_paths(inner, &format!("{path}."), out),
                _ => out.push(path),
            }
        }
    }

    #[test]
    fn readme_config_examples_parse_and_use_only_known_keys() {
        // v0.8.0 shipped a README example that failed to parse and listed keys
        // with no implementation. Unknown keys are silently ignored at load
        // time, so check them explicitly: re-serializing the parsed config
        // keeps only real fields.
        for (name, readme) in [
            ("README.md", include_str!("../README.md")),
            ("README.ja.md", include_str!("../README.ja.md")),
        ] {
            let block = readme
                // Leading newline: don't match the audio section's "### Configuration".
                .split("\n## Configuration")
                .nth(1)
                .and_then(|section| section.split("```toml").nth(1))
                .and_then(|block| block.split("```").next())
                .unwrap_or_else(|| panic!("{name}: no ```toml block under ## Configuration"));
            let cfg: AppConfig =
                toml::from_str(block).unwrap_or_else(|e| panic!("{name}: example must parse: {e}"));
            let example: toml::Table = toml::from_str(block).unwrap();
            let known: toml::Table = toml::from_str(&toml::to_string(&cfg).unwrap()).unwrap();
            let (mut example_keys, mut known_keys) = (Vec::new(), Vec::new());
            key_paths(&example, "", &mut example_keys);
            key_paths(&known, "", &mut known_keys);
            let unknown: Vec<_> = example_keys
                .iter()
                .filter(|k| !known_keys.contains(k))
                .collect();
            assert!(
                unknown.is_empty(),
                "{name}: keys with no effect: {unknown:?}"
            );
        }
    }

    #[test]
    fn readme_style_partial_config_loads() {
        // The README example lists only a few keys per section; that must
        // parse instead of falling back to defaults.
        let toml = r#"
            [receiver]
            name = "desk-mirror"

            [display]
            pip_mode = true

            [recording]
            output_dir = "captures"

            [network]
            api_token = "abc"
        "#;
        let cfg: AppConfig = toml::from_str(toml).expect("partial config must parse");
        assert_eq!(cfg.receiver.name, "desk-mirror");
        assert!(cfg.display.pip_mode);
        assert_eq!(cfg.display.window_width, 960, "missing keys use defaults");
        assert_eq!(cfg.recording.output_dir, "captures");
        assert_eq!(cfg.network.api_token.as_deref(), Some("abc"));
        assert_eq!(cfg.network.bind_address, "127.0.0.1");
    }

    #[test]
    fn legacy_keys_from_older_releases_are_ignored() {
        let toml = r#"
            [receiver]
            name = "old"
            port = 7000
            max_fps = 60

            [features]
            ocr = true
        "#;
        let cfg: AppConfig = toml::from_str(toml).expect("legacy config must parse");
        assert_eq!(cfg.receiver.name, "old");
    }

    #[test]
    fn unparseable_config_is_never_overwritten() {
        with_tempdir(|| {
            // SAFETY: process-global env mutation; serialized by CWD_GUARD.
            unsafe { std::env::remove_var("IOS_REMOTE_API_TOKEN") };
            let broken = "[network\napi_token = \"keep-me\"\n";
            std::fs::write(CONFIG_FILE, broken).unwrap();

            let mut cfg = AppConfig::load();
            // Token generation normally persists — it must not here.
            let token = cfg.resolve_api_token();
            assert!(!token.is_empty());
            assert_eq!(std::fs::read_to_string(CONFIG_FILE).unwrap(), broken);
        });
    }

    #[test]
    fn background_color_parses_hex() {
        let mut d = DisplaySettings::default();
        assert_eq!(d.background_rgb(), 0x222222);
        d.background_color = "#00d4ff".into();
        assert_eq!(d.background_rgb(), 0x00d4ff);
        d.background_color = "not-a-color".into();
        assert_eq!(d.background_rgb(), 0x222222);
    }

    #[test]
    fn save_and_load_round_trip_preserves_fields() {
        with_tempdir(|| {
            let mut original = AppConfig::default().into_persistent();
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
        assert!(
            a.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
    }
}
