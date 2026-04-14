use serde::Serialize;
use tracing::info;

/// Device manager: track connected USB devices.

#[derive(Debug, Clone, Serialize)]
pub struct ConnectedDevice {
    pub device_id: u32,
    pub udid: String,
    pub name: String,
    pub model: String,
    pub ios_version: String,
    pub connection_type: String,
}

pub struct DeviceManager {
    devices: Vec<ConnectedDevice>,
}

impl DeviceManager {
    pub fn new() -> Self {
        Self { devices: Vec::new() }
    }

    pub fn add(&mut self, dev: ConnectedDevice) {
        info!(udid = %dev.udid, name = %dev.name, "Device registered");
        self.devices.push(dev);
    }

    pub fn list(&self) -> &[ConnectedDevice] {
        &self.devices
    }

    pub fn find_by_udid(&self, udid: &str) -> Option<&ConnectedDevice> {
        self.devices.iter().find(|d| d.udid == udid)
    }
}
