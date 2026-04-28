pub mod device;
pub mod diag;
#[cfg(feature = "ios17")]
pub mod idevice_bridge;
pub mod lockdown;
pub mod screen_capture;
pub mod usbmuxd;

use crate::features::FrameBus;
use lockdown::DeviceInfo;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};
use usbmuxd::{UsbDevice, UsbmuxdClient};

/// USB-based iPhone connection manager.
///
/// Connects to iPhone via USB Type-C using the usbmuxd protocol:
///   1. Connect to usbmuxd daemon (Apple Mobile Device Service on Windows)
///   2. List connected devices, optionally filter by UDID
///   3. Establish lockdownd session
///   4. Start screen capture service
///   5. Receive frames → FrameBus → display
///
/// The receiver is reconnect-aware: if the device disappears or the screenshotr
/// stream errors out, it reconnects with exponential backoff capped at 16 s.
///
/// Requirements:
///   - iPhone connected via USB Type-C (or Lightning)
///   - iTunes or Apple Devices app installed (provides usbmuxd on Windows)
///   - "Trust This Computer" approved on iPhone
pub struct UsbReceiver {
    frame_bus: FrameBus,
    /// Preferred device UDID. If `None`, picks the first enumerated device.
    preferred_udid: Option<String>,
    /// Most recently observed device info. Populated after each successful
    /// lockdownd `GetValue` round-trip and used to enrich stall warnings.
    last_device: Arc<Mutex<Option<DeviceInfo>>>,
}

impl UsbReceiver {
    pub fn new(frame_bus: FrameBus) -> Self {
        Self {
            frame_bus,
            preferred_udid: None,
            last_device: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_udid(mut self, udid: Option<String>) -> Self {
        self.preferred_udid = udid;
        self
    }

    /// Run forever, reconnecting on any transient error.
    pub async fn run(&self) -> anyhow::Result<()> {
        info!("USB mode: connecting to usbmuxd...");

        let mut backoff = Duration::from_secs(1);
        const MAX_BACKOFF: Duration = Duration::from_secs(16);
        let mut stall_logged_at: Option<std::time::Instant> = None;

        loop {
            match self.connect_and_run_once().await {
                Ok(()) => {
                    // capture_loop returned Ok → device disconnected cleanly
                    warn!("Device disconnected — waiting for reconnect");
                    backoff = Duration::from_secs(1);
                }
                Err(e) => {
                    warn!(error = %e, "USB session ended — will retry");
                }
            }

            // Surface a single "still waiting" warning after ~30 s of consecutive
            // failures so users don't think the process has frozen.
            match stall_logged_at {
                None => stall_logged_at = Some(std::time::Instant::now()),
                Some(t) if t.elapsed() > Duration::from_secs(30) => {
                    let snapshot = self.last_device.lock().ok().and_then(|g| g.clone());
                    let unsupported = snapshot
                        .as_ref()
                        .and_then(|d| lockdown::parse_ios_major(&d.ios_version))
                        .map(|m| m >= 17)
                        .unwrap_or(false);
                    match snapshot {
                        Some(d) if unsupported => warn!(
                            udid = %d.udid,
                            model = %d.model,
                            ios = %d.ios_version,
                            "Still waiting for iPhone. iOS 17+ is NOT supported by this build \
                             (no Pairing/StartSession/TLS/DDI). Run `--diag` for details."
                        ),
                        Some(d) => warn!(
                            udid = %d.udid,
                            model = %d.model,
                            ios = %d.ios_version,
                            "Still waiting for iPhone (last seen). Checklist: cable seated, \
                             'Trust This Computer' tapped, Apple Mobile Device Service running."
                        ),
                        None => warn!(
                            "Still waiting for iPhone. Checklist: cable seated, \
                             'Trust This Computer' tapped, Apple Mobile Device Service running."
                        ),
                    }
                    stall_logged_at = Some(std::time::Instant::now());
                }
                _ => {}
            }

            sleep(backoff).await;
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    }

    /// One connect→list→capture cycle. Returns when the session ends for any
    /// reason (Ok = clean disconnect, Err = protocol error).
    async fn connect_and_run_once(&self) -> anyhow::Result<()> {
        let mut mux = UsbmuxdClient::connect().await?;
        let devices = mux.list_devices().await?;

        if devices.is_empty() {
            return Err(anyhow::anyhow!("No iPhone connected"));
        }

        let device = self.pick_device(&devices)?;
        info!(
            udid = %device.udid,
            device_id = device.device_id,
            conn = %device.connection_type,
            "Connected to iPhone"
        );

        info!("Starting screen capture via USB...");
        let bus = self.frame_bus.clone();
        screen_capture::capture_loop(
            &mut mux,
            device.device_id,
            bus,
            Arc::clone(&self.last_device),
        )
        .await
    }

    fn pick_device<'a>(&self, devices: &'a [UsbDevice]) -> anyhow::Result<&'a UsbDevice> {
        if let Some(want) = &self.preferred_udid {
            return devices.iter().find(|d| &d.udid == want).ok_or_else(|| {
                anyhow::anyhow!(
                    "Requested UDID {} not connected. Available: [{}]",
                    want,
                    devices
                        .iter()
                        .map(|d| d.udid.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            });
        }
        if devices.len() > 1 {
            warn!(
                count = devices.len(),
                "Multiple devices connected — using first. Pass --device <UDID> to pick: [{}]",
                devices
                    .iter()
                    .map(|d| d.udid.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        devices
            .first()
            .ok_or_else(|| anyhow::anyhow!("No iPhone connected"))
    }
}

/// Enumerate attached devices and print them, then exit. Used by `--list-devices`.
pub async fn print_device_list() -> anyhow::Result<()> {
    let mut mux = UsbmuxdClient::connect().await?;
    let devices = mux.list_devices().await?;
    if devices.is_empty() {
        println!("No iPhone connected.");
        return Ok(());
    }
    println!("Connected devices ({}):", devices.len());
    for d in &devices {
        println!("  {}  [{}]  id={}", d.udid, d.connection_type, d.device_id);
    }
    Ok(())
}
