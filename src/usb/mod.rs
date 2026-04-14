pub mod usbmuxd;
pub mod lockdown;
pub mod screen_capture;
pub mod device;

use crate::features::FrameBus;
use tracing::{info, warn};

/// USB-based iPhone connection manager.
///
/// Connects to iPhone via USB Type-C using the usbmuxd protocol:
///   1. Connect to usbmuxd daemon (Apple Mobile Device Service on Windows)
///   2. List connected devices
///   3. Establish lockdownd session
///   4. Start screen capture service
///   5. Receive frames → FrameBus → display
///
/// Requirements:
///   - iPhone connected via USB Type-C (or Lightning)
///   - iTunes or Apple Devices app installed (provides usbmuxd on Windows)
///   - "Trust This Computer" approved on iPhone
pub struct UsbReceiver {
    frame_bus: FrameBus,
}

impl UsbReceiver {
    pub fn new(frame_bus: FrameBus) -> Self {
        Self { frame_bus }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        info!("USB mode: connecting to usbmuxd...");

        // Step 1: Connect to usbmuxd
        let mut mux = usbmuxd::UsbmuxdClient::connect().await?;
        info!("Connected to usbmuxd");

        // Step 2: List devices
        let devices = mux.list_devices().await?;
        if devices.is_empty() {
            warn!("No iPhone connected via USB. Please connect with Type-C cable and tap 'Trust'.");
            info!("Waiting for device...");
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                let devs = mux.list_devices().await?;
                if !devs.is_empty() {
                    info!(udid = %devs[0].udid, "Device found!");
                    break;
                }
            }
        }

        let devices = mux.list_devices().await?;
        let device = &devices[0];
        info!(
            udid = %device.udid,
            device_id = device.device_id,
            "Connected to iPhone"
        );

        // Step 3: Start screen capture
        info!("Starting screen capture via USB...");
        let bus = self.frame_bus.clone();
        screen_capture::capture_loop(&mut mux, device.device_id, bus).await?;

        Ok(())
    }
}
