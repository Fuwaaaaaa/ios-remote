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

        let devices = usbmuxd.get_devices().await.context("usbmuxd ListDevices")?;

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
        info!(
            service = name,
            port, ssl, "Service started (idevice bridge)"
        );
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
///
/// **What Stage C-7 must add (frame capture loop):**
/// 1. Re-connect to the screenshotr port returned by `start_service` over
///    a fresh usbmuxd tunnel and re-do the TLS handshake using the same
///    pair record (the `idevice` crate exposes a service-stream helper —
///    do *not* hand-roll TLS).
/// 2. Speak the DLMessage framing (`DLMessageVersionExchange` →
///    `DLMessageProcessMessage` with `ScreenShotRequest`) — same wire
///    format as the legacy `screen_capture::capture_loop`, just over the
///    TLS-wrapped socket. The existing `send_dl_message` / `recv_message`
///    helpers in `screen_capture.rs` can be reused once they are
///    abstracted over `AsyncRead + AsyncWrite + Unpin`.
/// 3. Decode the returned PNG and `frame_bus.publish(Frame { … })` —
///    identical to the iOS ≤16 path.
/// 4. Periodic FPS log + reconnect-on-error semantics matching
///    `capture_loop`.
///
/// **Stages C-4 (CoreDeviceProxy tunnel) and C-5 (Personalized DDI mount)
/// gate this on real iOS 17+ devices**: until they ship,
/// `start_service('com.apple.mobile.screenshotr')` is expected to fail at
/// the lockdownd level and the bridge can't even open the socket
/// described in step 1. See `docs/IOS17_BRIDGE.md` for the staging order.
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
            "iOS 17+ bridge start_service('{SCREENSHOTR_SERVICE}') — on iOS 17+ \
             this service typically requires CoreDeviceProxy tunnel (Stage C-4) \
             followed by Personalized DDI mount (Stage C-5) before lockdownd \
             will hand it out; until both ship, this is the expected failure \
             surface and the retry loop will keep cycling"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::IdeviceBridge;

    /// Without a real iPhone of the given UDID — and regardless of whether
    /// usbmuxd happens to be running on the test host — `connect_by_udid`
    /// must return `Err` and never panic. Two graceful-failure surfaces are
    /// in scope: (1) no usbmuxd → "connect to local usbmuxd …" context, or
    /// (2) usbmuxd present, no matching device → "Device … not connected
    /// via USB". Both are acceptable; the invariant under test is that the
    /// bridge does not crash on hardware-absence.
    #[tokio::test]
    async fn connect_by_udid_fails_gracefully_without_device() {
        let result = IdeviceBridge::connect_by_udid(
            "00000000-FFFFFFFFFFFFFFFF-NOT-A-REAL-UDID",
            "ios-remote-test",
        )
        .await;
        assert!(
            result.is_err(),
            "connect_by_udid must return Err for a bogus UDID; got Ok"
        );
    }
}
