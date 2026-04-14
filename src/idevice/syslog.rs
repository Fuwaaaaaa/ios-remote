use tokio::sync::broadcast;
use tracing::info;

/// A single syslog entry from the iPhone.
#[derive(Clone, Debug)]
pub struct SyslogEntry {
    pub timestamp: String,
    pub process: String,
    pub pid: u32,
    pub message: String,
    pub level: LogLevel,
}

#[derive(Clone, Debug)]
pub enum LogLevel {
    Default,
    Info,
    Debug,
    Error,
    Fault,
}

/// Stream iPhone syslog entries in real-time via syslog_relay service.
///
/// Useful for debugging iOS apps without Xcode.
pub async fn stream_syslog(_tx: broadcast::Sender<SyslogEntry>) -> Result<(), String> {
    info!("Syslog relay: waiting for USB device connection");
    // TODO: Implement with idevice crate
    Err("idevice syslog relay not yet enabled".to_string())
}

/// Retrieve crash logs from the iPhone.
pub async fn get_crash_logs() -> Result<Vec<String>, String> {
    Err("idevice crash log retrieval not yet enabled".to_string())
}
