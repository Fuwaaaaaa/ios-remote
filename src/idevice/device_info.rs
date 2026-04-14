use serde::Serialize;

/// iPhone device information retrieved via lockdownd.
#[derive(Debug, Serialize)]
pub struct DeviceInfo {
    pub name: String,
    pub model: String,
    pub ios_version: String,
    pub serial: String,
    pub udid: String,
    pub battery_level: u8,
    pub battery_charging: bool,
    pub storage_total_gb: f64,
    pub storage_free_gb: f64,
    pub wifi_address: String,
}

/// Retrieve device info via idevice crate.
///
/// Requires USB connection and "Trust This Computer" approval.
pub async fn get_device_info() -> Result<DeviceInfo, String> {
    // TODO: Implement with idevice crate when enabled
    Err("idevice crate not yet enabled — add to Cargo.toml".to_string())
}

/// List installed applications via installation_proxy.
#[derive(Debug, Serialize)]
pub struct AppInfo {
    pub bundle_id: String,
    pub name: String,
    pub version: String,
    pub size_mb: f64,
}

pub async fn list_apps() -> Result<Vec<AppInfo>, String> {
    Err("idevice crate not yet enabled".to_string())
}
