use plist::{Dictionary, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::info;

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

        let error = resp.get("Error").and_then(|v| v.as_string());
        if let Some(err) = error {
            return Err(anyhow::anyhow!(
                "StartService '{}' failed: {}",
                service_name,
                err
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

#[derive(Debug)]
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
