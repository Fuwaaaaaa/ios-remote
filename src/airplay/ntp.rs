use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use tracing::{debug, info};

/// NTP time synchronization for AirPlay mirroring.
///
/// AirPlay uses NTP to synchronize the master clock between sender (iPhone)
/// and receiver. The iPhone sends NTP requests and we respond with our
/// timestamps so it can calculate the clock offset.
///
/// NTP packet: 32 bytes
///   - Bytes 0-3:   Reference timestamp (seconds since 1900-01-01)
///   - Bytes 4-7:   Reference timestamp (fraction)
///   - Bytes 8-11:  Receive timestamp (seconds)
///   - Bytes 12-15: Receive timestamp (fraction)
///   - Bytes 16-19: Transmit timestamp (seconds)
///   - Bytes 20-23: Transmit timestamp (fraction)
///   - Bytes 24-31: (padding/unused in AirPlay variant)
const NTP_EPOCH_OFFSET: u64 = 2_208_988_800; // seconds between 1900 and 1970

pub struct NtpServer {
    port: u16,
}

impl NtpServer {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let socket = UdpSocket::bind(("0.0.0.0", self.port)).await?;
        info!(port = self.port, "NTP server listening");

        let mut buf = [0u8; 128];

        loop {
            let (len, peer) = socket.recv_from(&mut buf).await?;
            if len < 32 {
                continue;
            }

            let now = ntp_now();

            // Build NTP response
            let mut resp = [0u8; 32];

            // Copy client's transmit timestamp to our reference timestamp (bytes 0-7)
            resp[0..8].copy_from_slice(&buf[24..32]);

            // Receive timestamp = now (bytes 8-15)
            resp[8..12].copy_from_slice(&now.0.to_be_bytes());
            resp[12..16].copy_from_slice(&now.1.to_be_bytes());

            // Transmit timestamp = now (bytes 16-23)
            resp[16..20].copy_from_slice(&now.0.to_be_bytes());
            resp[20..24].copy_from_slice(&now.1.to_be_bytes());

            socket.send_to(&resp, peer).await?;
            debug!(peer = %peer, "NTP response sent");
        }
    }
}

/// Get current time as NTP timestamp (seconds since 1900, fraction).
fn ntp_now() -> (u32, u32) {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let secs = since_epoch.as_secs() + NTP_EPOCH_OFFSET;
    let frac = ((since_epoch.subsec_nanos() as u64) << 32) / 1_000_000_000;

    (secs as u32, frac as u32)
}
