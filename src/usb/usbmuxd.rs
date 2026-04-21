use bytes::{BufMut, BytesMut};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, info};

/// usbmuxd wire protocol client.
///
/// usbmuxd runs on localhost:27015 (Windows) or /var/run/usbmuxd (Unix).
/// It multiplexes USB communication with iOS devices.
///
/// Protocol: each message has a 16-byte header:
///   - u32 length (including header)
///   - u32 version (1)
///   - u32 message type
///   - u32 tag (request identifier)
///
/// Message body is a binary plist (version 1) or XML plist.
const USBMUXD_PORT: u16 = 27015; // Windows (Apple Mobile Device Service)

// Message types
const MSG_RESULT: u32 = 1;
const MSG_CONNECT: u32 = 2;
const MSG_LISTEN: u32 = 3;
const MSG_DEVICE_ADD: u32 = 4;
const MSG_DEVICE_REMOVE: u32 = 5;
const MSG_PLIST: u32 = 8;

#[derive(Debug, Clone)]
pub struct UsbDevice {
    pub device_id: u32,
    pub udid: String,
    pub connection_type: String,
}

pub struct UsbmuxdClient {
    stream: TcpStream,
    tag: u32,
}

impl UsbmuxdClient {
    /// Connect to the local usbmuxd daemon.
    pub async fn connect() -> anyhow::Result<Self> {
        let stream = TcpStream::connect(("127.0.0.1", USBMUXD_PORT))
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Cannot connect to usbmuxd (127.0.0.1:{}). \
                 Is iTunes or Apple Devices app installed? Error: {}",
                    USBMUXD_PORT,
                    e
                )
            })?;

        info!("Connected to usbmuxd at 127.0.0.1:{}", USBMUXD_PORT);

        Ok(Self { stream, tag: 0 })
    }

    /// List connected iOS devices.
    pub async fn list_devices(&mut self) -> anyhow::Result<Vec<UsbDevice>> {
        // Send ListDevices plist request
        let request = plist_xml(&[
            ("MessageType", "ListDevices"),
            ("ClientVersionString", "ios-remote"),
            ("ProgName", "ios-remote"),
        ]);

        self.send_plist(&request).await?;
        let response = self.recv_plist().await?;

        // Parse device list from response
        let mut devices = Vec::new();

        if let Some(plist::Value::Array(device_list)) = response.get("DeviceList") {
            for entry in device_list {
                if let Some(props) = entry
                    .as_dictionary()
                    .and_then(|d| d.get("Properties"))
                    .and_then(|p| p.as_dictionary())
                {
                    let device_id = props
                        .get("DeviceID")
                        .and_then(|v| v.as_unsigned_integer())
                        .unwrap_or(0) as u32;

                    let udid = props
                        .get("SerialNumber")
                        .and_then(|v| v.as_string())
                        .unwrap_or("unknown")
                        .to_string();

                    let conn_type = props
                        .get("ConnectionType")
                        .and_then(|v| v.as_string())
                        .unwrap_or("USB")
                        .to_string();

                    devices.push(UsbDevice {
                        device_id,
                        udid,
                        connection_type: conn_type,
                    });
                }
            }
        }

        debug!(count = devices.len(), "Devices found");
        Ok(devices)
    }

    /// Connect to a specific port on a device (TCP over USB tunnel).
    pub async fn connect_to_device(&mut self, device_id: u32, port: u16) -> anyhow::Result<()> {
        let request = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>MessageType</key><string>Connect</string>
    <key>ClientVersionString</key><string>ios-remote</string>
    <key>ProgName</key><string>ios-remote</string>
    <key>DeviceID</key><integer>{}</integer>
    <key>PortNumber</key><integer>{}</integer>
</dict>
</plist>"#,
            device_id,
            htons(port) // usbmuxd expects network byte order port
        );

        self.send_plist(request.as_bytes()).await?;
        let response = self.recv_plist().await?;

        let result = response
            .get("Number")
            .and_then(|v| v.as_unsigned_integer())
            .unwrap_or(999);

        if result == 0 {
            info!(device_id, port, "USB tunnel connected");
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Connect failed with result code {}",
                result
            ))
        }
    }

    /// Get the raw TCP stream for direct communication after Connect.
    pub fn into_stream(self) -> TcpStream {
        self.stream
    }

    /// Get a mutable reference to the underlying stream.
    pub fn stream_mut(&mut self) -> &mut TcpStream {
        &mut self.stream
    }

    async fn send_plist(&mut self, body: &[u8]) -> anyhow::Result<()> {
        self.tag += 1;
        let total_len = 16 + body.len() as u32;

        let mut header = BytesMut::with_capacity(16);
        header.put_u32_le(total_len);
        header.put_u32_le(1); // version
        header.put_u32_le(MSG_PLIST); // type
        header.put_u32_le(self.tag);

        self.stream.write_all(&header).await?;
        self.stream.write_all(body).await?;
        self.stream.flush().await?;

        Ok(())
    }

    async fn recv_plist(&mut self) -> anyhow::Result<plist::Dictionary> {
        // Read 16-byte header
        let mut header = [0u8; 16];
        self.stream.read_exact(&mut header).await?;

        let length = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let _version = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        let _msg_type = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
        let _tag = u32::from_le_bytes([header[12], header[13], header[14], header[15]]);

        let body_len = length.saturating_sub(16) as usize;
        if body_len == 0 {
            return Ok(plist::Dictionary::new());
        }

        let mut body = vec![0u8; body_len];
        self.stream.read_exact(&mut body).await?;

        // Try to parse as XML plist
        let cursor = std::io::Cursor::new(&body);
        match plist::Value::from_reader(cursor) {
            Ok(plist::Value::Dictionary(dict)) => Ok(dict),
            Ok(_) => Ok(plist::Dictionary::new()),
            Err(_) => {
                // Try as binary plist
                let cursor = std::io::Cursor::new(&body);
                match plist::Value::from_reader(cursor) {
                    Ok(plist::Value::Dictionary(dict)) => Ok(dict),
                    _ => {
                        debug!(len = body_len, "Could not parse usbmuxd response as plist");
                        Ok(plist::Dictionary::new())
                    }
                }
            }
        }
    }
}

/// Build a simple XML plist from key-value pairs.
fn plist_xml(pairs: &[(&str, &str)]) -> Vec<u8> {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
"#,
    );
    for (key, value) in pairs {
        xml.push_str(&format!(
            "    <key>{}</key><string>{}</string>\n",
            key, value
        ));
    }
    xml.push_str("</dict>\n</plist>");
    xml.into_bytes()
}

/// Convert port to network byte order (big-endian u16 stored as u32).
fn htons(port: u16) -> u32 {
    ((port & 0xFF) as u32) << 8 | ((port >> 8) & 0xFF) as u32
}
