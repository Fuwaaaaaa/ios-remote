use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

/// Multi-device manager: track multiple connected iPhones.
///
/// Each device gets its own SessionState with independent mDNS name,
/// RTSP port, video port, and display window.

#[derive(Debug)]
pub struct DeviceEntry {
    pub device_id: String,
    pub name: String,
    pub rtsp_port: u16,
    pub video_port: u16,
    pub connected: bool,
}

pub struct DeviceManager {
    devices: Arc<Mutex<HashMap<String, DeviceEntry>>>,
    base_port: u16,
}

impl DeviceManager {
    pub fn new(base_port: u16) -> Self {
        Self {
            devices: Arc::new(Mutex::new(HashMap::new())),
            base_port,
        }
    }

    /// Register a new device, assigning unique ports.
    pub async fn register(&self, device_id: &str, name: &str) -> DeviceEntry {
        let mut devices = self.devices.lock().await;
        let idx = devices.len() as u16;

        let entry = DeviceEntry {
            device_id: device_id.to_string(),
            name: name.to_string(),
            rtsp_port: self.base_port + idx * 10,
            video_port: self.base_port + idx * 10 + 1,
            connected: true,
        };

        info!(
            device = %device_id,
            rtsp_port = entry.rtsp_port,
            video_port = entry.video_port,
            "Device registered"
        );

        devices.insert(device_id.to_string(), DeviceEntry {
            device_id: entry.device_id.clone(),
            name: entry.name.clone(),
            rtsp_port: entry.rtsp_port,
            video_port: entry.video_port,
            connected: true,
        });

        entry
    }

    /// List all registered devices.
    pub async fn list(&self) -> Vec<(String, String, bool)> {
        let devices = self.devices.lock().await;
        devices.values().map(|d| {
            (d.device_id.clone(), d.name.clone(), d.connected)
        }).collect()
    }

    /// Mark a device as disconnected.
    pub async fn disconnect(&self, device_id: &str) {
        let mut devices = self.devices.lock().await;
        if let Some(entry) = devices.get_mut(device_id) {
            entry.connected = false;
            info!(device = %device_id, "Device disconnected");
        }
    }
}
