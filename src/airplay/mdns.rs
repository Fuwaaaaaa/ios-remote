use crate::error::Error;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use tracing::info;

/// Advertises an AirPlay mirroring service via mDNS (Bonjour).
///
/// iPhones scan for `_airplay._tcp.local.` to discover receivers.
/// The TXT record carries capability flags telling iOS what this
/// receiver supports.
pub struct MdnsAdvertiser {
    daemon: ServiceDaemon,
    service: ServiceInfo,
}

impl MdnsAdvertiser {
    pub fn new(name: &str, port: u16) -> Result<Self, Error> {
        let daemon =
            ServiceDaemon::new().map_err(|e| Error::Mdns(e.to_string()))?;

        let device_id = "AA:BB:CC:DD:EE:FF";
        let instance_name = format!("{}@{}", device_id, name);

        let properties = [
            ("deviceid", device_id),
            ("features", "0x5A7FFFF7,0x1E"),
            ("model", "AppleTV3,2"),
            ("srcvers", "220.68"),
            ("vv", "2"),
            ("pk", ""),
            ("pi", ""),
            ("flags", "0x44"),
        ];

        let service = ServiceInfo::new(
            "_airplay._tcp.local.",
            &instance_name,
            &format!("{}.local.", name),
            "",
            port,
            &properties[..],
        )
        .map_err(|e| Error::Mdns(e.to_string()))?;

        Ok(Self { daemon, service })
    }

    pub async fn run(&self) {
        match self.daemon.register(self.service.clone()) {
            Ok(_) => {
                info!(
                    name = %self.service.get_fullname(),
                    "mDNS: AirPlay service registered"
                );
            }
            Err(e) => {
                tracing::error!(error = %e, "mDNS: failed to register");
                return;
            }
        }

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    }
}
