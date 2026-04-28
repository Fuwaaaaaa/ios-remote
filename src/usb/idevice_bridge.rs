//! iOS 17+ bridge: wraps the `idevice` crate (jkcoxson) so the upstream
//! USB pipeline can talk to lockdownd with proper Pair record handling,
//! StartSession, and TLS upgrade. Compiled only with `--features ios17`.
//!
//! Public surface mirrors `super::lockdown::LockdownClient` so the future
//! dispatch site in `mod.rs` (Stage C-6) can pick a backend per detected
//! iOS major version.
//!
//! Stage C-1 .. C-3 of the plan at
//! %USERPROFILE%\.claude\plans\crystalline-seeking-naur.md.

use anyhow::Context;
use idevice::IdeviceService;
use idevice::lockdown::LockdownClient;
use idevice::provider::IdeviceProvider;
use idevice::usbmuxd::{Connection, UsbmuxdAddr, UsbmuxdConnection};
use tracing::{debug, info};

use super::lockdown::{DeviceInfo, ServiceInfo};

/// Adapter around `idevice::lockdown::LockdowndClient`. Holds a live
/// lockdown session (post-StartSession + TLS) bound to one device.
pub struct IdeviceBridge {
    lockdown: LockdownClient,
    udid: String,
}

impl IdeviceBridge {
    /// Connect to local usbmuxd, pick the device by UDID, open lockdownd,
    /// fetch the pair record from usbmuxd (no manual %ProgramData% read
    /// needed — usbmuxd serves it over the wire), and perform StartSession
    /// + TLS upgrade.
    pub async fn connect_by_udid(udid: &str, label: &str) -> anyhow::Result<Self> {
        let mut usbmuxd = UsbmuxdConnection::default()
            .await
            .context("connect to local usbmuxd (Apple Mobile Device Service)")?;

        let devices = usbmuxd
            .get_devices()
            .await
            .context("usbmuxd ListDevices")?;

        let dev = devices
            .into_iter()
            .find(|d| d.udid == udid && d.connection_type == Connection::Usb)
            .ok_or_else(|| {
                anyhow::anyhow!("Device {udid} not connected via USB (idevice bridge)")
            })?;

        let addr = UsbmuxdAddr::from_env_var().unwrap_or_default();
        let provider = dev.to_provider(addr, label);

        info!(udid, "Opening lockdownd via idevice bridge");
        let mut lockdown = LockdownClient::connect(&provider as &dyn IdeviceProvider)
            .await
            .context("idevice LockdownClient::connect")?;

        let pairing_file = provider.get_pairing_file().await.context(
            "usbmuxd ReadPairRecord (no pair record — pair the iPhone with this host first)",
        )?;

        lockdown
            .start_session(&pairing_file)
            .await
            .context("lockdownd StartSession + TLS upgrade")?;

        info!(udid, "lockdownd session established (StartSession + TLS)");
        Ok(Self {
            lockdown,
            udid: udid.to_string(),
        })
    }

    /// Read DeviceName / ProductType / ProductVersion / UniqueDeviceID via lockdownd.
    pub async fn device_info(&mut self) -> anyhow::Result<DeviceInfo> {
        let name = self
            .get_value_string(None, "DeviceName")
            .await
            .unwrap_or_else(|_| "iPhone".to_string());
        let model = self
            .get_value_string(None, "ProductType")
            .await
            .unwrap_or_else(|_| "unknown".to_string());
        let ios_version = self
            .get_value_string(None, "ProductVersion")
            .await
            .unwrap_or_else(|_| "unknown".to_string());
        let udid = self
            .get_value_string(None, "UniqueDeviceID")
            .await
            .unwrap_or_else(|_| self.udid.clone());

        Ok(DeviceInfo {
            name,
            model,
            ios_version,
            udid,
        })
    }

    /// Start a lockdownd service. Returns port + SSL flag mapped into the
    /// project's existing `ServiceInfo` shape.
    pub async fn start_service(&mut self, name: &str) -> anyhow::Result<ServiceInfo> {
        let (port, ssl) = self
            .lockdown
            .start_service(name)
            .await
            .with_context(|| format!("idevice start_service('{name}')"))?;
        info!(service = name, port, ssl, "Service started (idevice bridge)");
        Ok(ServiceInfo {
            port,
            enable_ssl: ssl,
        })
    }

    async fn get_value_string(
        &mut self,
        domain: Option<&str>,
        key: &str,
    ) -> anyhow::Result<String> {
        let value = self
            .lockdown
            .get_value(Some(key), domain)
            .await
            .with_context(|| format!("lockdownd GetValue {key:?}"))?;
        match value {
            plist::Value::String(s) => Ok(s),
            other => {
                debug!(?other, key, "GetValue returned non-string");
                Err(anyhow::anyhow!("GetValue {key} returned non-string"))
            }
        }
    }
}
