use tokio::net::UdpSocket;
use tracing::{debug, info};

/// AirPlay audio receiver.
///
/// AirPlay mirroring sends AAC-ELD encoded audio as RTP packets over UDP.
/// Audio is separate from the video stream and uses its own port.
///
/// RTP header (12 bytes):
///   - Byte 0: version(2) + padding(1) + extension(1) + CSRC count(4)
///   - Byte 1: marker(1) + payload type(7)
///   - Bytes 2-3: sequence number
///   - Bytes 4-7: timestamp
///   - Bytes 8-11: SSRC
///
/// After the RTP header, the payload is AAC-ELD encoded audio data.
/// In encrypted mode, the payload is AES-128-CBC encrypted.

pub struct AudioReceiver {
    port: u16,
    decryption_key: Option<[u8; 16]>,
    decryption_iv: Option<[u8; 16]>,
}

impl AudioReceiver {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            decryption_key: None,
            decryption_iv: None,
        }
    }

    pub fn set_encryption(&mut self, key: [u8; 16], iv: [u8; 16]) {
        self.decryption_key = Some(key);
        self.decryption_iv = Some(iv);
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let socket = UdpSocket::bind(("0.0.0.0", self.port)).await?;
        info!(port = self.port, "Audio receiver listening (RTP/UDP)");

        let mut buf = [0u8; 2048]; // max RTP packet size
        let mut packet_count: u64 = 0;

        loop {
            let (len, _peer) = socket.recv_from(&mut buf).await?;
            if len < 12 {
                continue; // too short for RTP header
            }

            packet_count += 1;

            // Parse RTP header
            let _version = (buf[0] >> 6) & 0x03;
            let _marker = (buf[1] >> 7) & 0x01;
            let payload_type = buf[1] & 0x7F;
            let seq = u16::from_be_bytes([buf[2], buf[3]]);
            let timestamp = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
            let _ssrc = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);

            let payload = &buf[12..len];

            // Decrypt if encryption is active
            let audio_data = if let (Some(_key), Some(_iv)) = (&self.decryption_key, &self.decryption_iv) {
                // AES-128-CBC decrypt
                // Only decrypt full 16-byte blocks; trailing bytes are unencrypted
                let _block_count = payload.len() / 16;
                // TODO: implement AES-CBC decryption of audio payload
                payload.to_vec()
            } else {
                payload.to_vec()
            };

            if packet_count % 500 == 1 {
                debug!(
                    packets = packet_count,
                    seq,
                    timestamp,
                    payload_type,
                    payload_len = audio_data.len(),
                    "Audio RTP packet"
                );
            }

            // TODO: decode AAC-ELD and play via audio output
            // For now, we receive and log. Full audio playback requires:
            // 1. AAC-ELD decoder (e.g., fdk-aac or platform audio API)
            // 2. Audio output sink (e.g., cpal crate for cross-platform audio)
        }
    }
}
