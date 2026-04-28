use plist::{Dictionary, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, info, warn};

/// Lockdownd client: iOS device management service.
///
/// lockdownd runs on port 62078 on the device and handles:
///   - Device info queries
///   - Service startup (screenshotr, etc.)
///   - Pairing verification
///
/// Communication: length-prefixed binary plist over the USB tunnel.
const LOCKDOWN_PORT: u16 = 62078;

pub struct LockdownClient {
    stream: TcpStream,
}

impl LockdownClient {
    /// Connect to lockdownd on a device via usbmuxd tunnel.
    pub async fn connect(
        _mux: &mut super::usbmuxd::UsbmuxdClient,
        device_id: u32,
    ) -> anyhow::Result<Self> {
        // Create a new usbmuxd connection for the tunnel
        let mut tunnel = super::usbmuxd::UsbmuxdClient::connect().await?;
        tunnel.connect_to_device(device_id, LOCKDOWN_PORT).await?;

        info!("Lockdownd connected via USB tunnel");
        Ok(Self {
            stream: tunnel.into_stream(),
        })
    }

    /// Query device information.
    pub async fn get_value(&mut self, domain: Option<&str>, key: &str) -> anyhow::Result<Value> {
        let mut req = Dictionary::new();
        req.insert("Request".to_string(), Value::String("GetValue".to_string()));
        req.insert("Key".to_string(), Value::String(key.to_string()));
        if let Some(d) = domain {
            req.insert("Domain".to_string(), Value::String(d.to_string()));
        }

        self.send_plist(&Value::Dictionary(req)).await?;
        let resp = self.recv_plist().await?;

        Ok(resp
            .get("Value")
            .cloned()
            .unwrap_or(Value::String("".to_string())))
    }

    /// Get basic device info (name, model, iOS version, etc.).
    pub async fn get_device_info(&mut self) -> anyhow::Result<DeviceInfo> {
        let name = self
            .get_value(None, "DeviceName")
            .await?
            .as_string()
            .unwrap_or("iPhone")
            .to_string();
        let model = self
            .get_value(None, "ProductType")
            .await?
            .as_string()
            .unwrap_or("unknown")
            .to_string();
        let ios_version = self
            .get_value(None, "ProductVersion")
            .await?
            .as_string()
            .unwrap_or("unknown")
            .to_string();
        let udid = self
            .get_value(None, "UniqueDeviceID")
            .await?
            .as_string()
            .unwrap_or("unknown")
            .to_string();

        Ok(DeviceInfo {
            name,
            model,
            ios_version,
            udid,
        })
    }

    /// Start a lockdownd service (e.g., "com.apple.mobile.screenshotr").
    ///
    /// On iOS 17+ this typically fails because the protocol stack here does not
    /// implement StartSession (with pairing record), the post-session TLS
    /// upgrade, or the Personalized Developer Disk Image mount required by
    /// services like screenshotr. When that happens, the failure path below
    /// dumps the full lockdownd response (Error / ErrorString / Domain / Type
    /// fields) so users can see exactly what the device complained about.
    pub async fn start_service(&mut self, service_name: &str) -> anyhow::Result<ServiceInfo> {
        let mut req = Dictionary::new();
        req.insert(
            "Request".to_string(),
            Value::String("StartService".to_string()),
        );
        req.insert(
            "Service".to_string(),
            Value::String(service_name.to_string()),
        );

        self.send_plist(&Value::Dictionary(req)).await?;
        let resp = self.recv_plist().await?;

        let error_field = resp
            .get("Error")
            .and_then(|v| v.as_string())
            .map(str::to_string);
        let error_string = resp
            .get("ErrorString")
            .and_then(|v| v.as_string())
            .map(str::to_string);
        let domain = resp
            .get("Domain")
            .and_then(|v| v.as_string())
            .map(str::to_string);
        let resp_type = resp
            .get("Type")
            .and_then(|v| v.as_string())
            .map(str::to_string);

        if error_field.is_some() || error_string.is_some() {
            warn!(
                service = service_name,
                error = error_field.as_deref().unwrap_or(""),
                error_string = error_string.as_deref().unwrap_or(""),
                domain = domain.as_deref().unwrap_or(""),
                resp_type = resp_type.as_deref().unwrap_or(""),
                "Lockdownd StartService rejected"
            );
            if tracing::enabled!(tracing::Level::DEBUG) {
                let mut xml = Vec::new();
                if Value::Dictionary(resp.clone())
                    .to_writer_xml(&mut xml)
                    .is_ok()
                {
                    debug!(
                        service = service_name,
                        body = %String::from_utf8_lossy(&xml),
                        "Lockdownd raw response"
                    );
                }
            }
            return Err(anyhow::anyhow!(
                "StartService '{}' failed: Error={} ErrorString={} (Domain={})",
                service_name,
                error_field.as_deref().unwrap_or("<none>"),
                error_string.as_deref().unwrap_or("<none>"),
                domain.as_deref().unwrap_or("<none>"),
            ));
        }

        let port = resp
            .get("Port")
            .and_then(|v| v.as_unsigned_integer())
            .unwrap_or(0) as u16;

        let ssl = resp
            .get("EnableServiceSSL")
            .and_then(|v| v.as_boolean())
            .unwrap_or(false);

        info!(service = service_name, port, ssl, "Service started");
        Ok(ServiceInfo {
            port,
            enable_ssl: ssl,
        })
    }

    /// Serialize an arbitrary lockdownd response back to XML. Used by `--diag`.
    pub fn dict_to_xml(dict: &Dictionary) -> String {
        let mut xml = Vec::new();
        if Value::Dictionary(dict.clone())
            .to_writer_xml(&mut xml)
            .is_ok()
        {
            String::from_utf8_lossy(&xml).into_owned()
        } else {
            String::new()
        }
    }

    /// Issue a raw lockdownd request and return the raw response dictionary.
    /// Exposed for diagnostics (`--diag`).
    pub async fn raw_request(&mut self, req: Dictionary) -> anyhow::Result<Dictionary> {
        self.send_plist(&Value::Dictionary(req)).await?;
        self.recv_plist().await
    }

    async fn send_plist(&mut self, value: &Value) -> anyhow::Result<()> {
        let mut body = Vec::new();
        value.to_writer_xml(&mut body)?;

        let len = body.len() as u32;
        self.stream.write_all(&len.to_be_bytes()).await?;
        self.stream.write_all(&body).await?;
        self.stream.flush().await?;

        Ok(())
    }

    async fn recv_plist(&mut self) -> anyhow::Result<Dictionary> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        if len == 0 || len > 1_000_000 {
            return Err(anyhow::anyhow!(
                "Invalid lockdownd response length: {}",
                len
            ));
        }

        let mut body = vec![0u8; len];
        self.stream.read_exact(&mut body).await?;

        let cursor = std::io::Cursor::new(&body);
        match plist::Value::from_reader(cursor) {
            Ok(Value::Dictionary(dict)) => Ok(dict),
            Ok(_) => Ok(Dictionary::new()),
            Err(e) => Err(anyhow::anyhow!("Plist parse error: {}", e)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub name: String,
    pub model: String,
    pub ios_version: String,
    pub udid: String,
}

#[derive(Debug)]
pub struct ServiceInfo {
    pub port: u16,
    pub enable_ssl: bool,
}

/// Extract the major iOS version (e.g., "26.4.1" → Some(26)).
///
/// Returns `None` when the input is empty, "unknown", or otherwise unparseable.
/// Tolerates pre-release suffixes ("17.0b3") and missing minor parts ("18").
pub fn parse_ios_major(version: &str) -> Option<u32> {
    let trimmed = version.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("unknown") {
        return None;
    }
    let head: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
    head.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::parse_ios_major;

    #[test]
    fn parses_common_ios_versions() {
        let cases = [
            ("16.7.10", Some(16)),
            ("17.0", Some(17)),
            ("18", Some(18)),
            ("26.4.1", Some(26)),
            ("17.0b3", Some(17)),
            ("", None),
            ("unknown", None),
            ("UNKNOWN", None),
            ("garbage", None),
            ("  17.5  ", Some(17)),
        ];
        for (input, want) in cases {
            assert_eq!(parse_ios_major(input), want, "input={input:?}");
        }
    }
}
