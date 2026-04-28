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
use tracing::{debug, info, warn};

use crate::features::FrameBus;

use super::lockdown::{DeviceInfo, ServiceInfo};

const SCREENSHOTR_SERVICE: &str = "com.apple.mobile.screenshotr";

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

/// iOS 17+ entry point — Stage C-6 dispatch target.
///
/// Connect via the bridge (Pair record + StartSession + TLS upgrade),
/// double-check device info, and probe `screenshotr`. Each step is logged
/// individually so a real-device test surfaces exactly how far the bridge
/// got — even without `--diag`. The actual frame loop over the TLS-wrapped
/// service socket is Stage C-7 and is not implemented yet, so this function
/// always returns `Err` (with a different message for "probe succeeded but
/// no v2 capture loop" vs "probe failed at step X").
pub async fn run_v2(dev_info: &DeviceInfo, _frame_bus: &FrameBus) -> anyhow::Result<()> {
    info!(
        udid = %dev_info.udid,
        ios = %dev_info.ios_version,
        model = %dev_info.model,
        "iOS 17+ bridge path activated (`--features ios17`)"
    );

    let mut bridge = IdeviceBridge::connect_by_udid(&dev_info.udid, "ios-remote")
        .await
        .context("idevice bridge connect_by_udid")?;

    match bridge.device_info().await {
        Ok(info) => info!(
            name = %info.name,
            model = %info.model,
            ios = %info.ios_version,
            "Bridge device_info via TLS-wrapped lockdownd: OK"
        ),
        Err(e) => warn!(error = %e, "Bridge device_info failed (non-fatal probe)"),
    }

    match bridge.start_service(SCREENSHOTR_SERVICE).await {
        Ok(svc) => {
            warn!(
                port = svc.port,
                ssl = svc.enable_ssl,
                "screenshotr start_service succeeded via bridge — but the v2 \
                 capture loop is not implemented yet (Stage C-7). Stop the \
                 process and report this success in the issue tracker."
            );
            anyhow::bail!(
                "iOS 17+ bridge reached start_service('{}') = (port={}, ssl={}); \
                 v2 capture loop not yet implemented",
                SCREENSHOTR_SERVICE,
                svc.port,
                svc.enable_ssl,
            )
        }
        Err(e) => Err(e).context(format!(
            "iOS 17+ bridge start_service('{SCREENSHOTR_SERVICE}') — likely needs \
             Personalized DDI mount (Stage C-5) or a different service path"
        )),
    }
}
