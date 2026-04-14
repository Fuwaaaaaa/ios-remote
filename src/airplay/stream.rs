use crate::airplay::SharedState;
use bytes::BytesMut;
use openh264::formats::YUVSource;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tracing::{debug, info, warn};

/// AirPlay mirroring stream packet header (128 bytes).
///
/// Reference: https://openairplay.github.io/airplay-spec/screen_mirroring/stream_packets.html
#[derive(Debug)]
pub struct StreamPacketHeader {
    /// Payload size in bytes.
    pub payload_size: u32,
    /// Payload type:
    ///   0 = H.264 codec data (SPS/PPS)
    ///   1 = H.264 video frame
    ///   2 = heartbeat (no payload)
    pub payload_type: u16,
    /// NTP timestamp (for synchronization).
    pub timestamp: u64,
}

impl StreamPacketHeader {
    pub fn parse(data: &[u8; 128]) -> Self {
        Self {
            payload_size: u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
            payload_type: u16::from_le_bytes([data[4], data[5]]),
            timestamp: u64::from_le_bytes([
                data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
            ]),
        }
    }
}

/// Frame data to send to the display thread.
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Listen for the H.264 video stream from the iPhone.
///
/// After SETUP + RECORD, the iPhone opens a TCP connection to our video data
/// port and sends a continuous stream of 128-byte headers + H.264 NAL units.
pub async fn listen_video(port: u16, _state: SharedState) -> anyhow::Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    info!(port, "Video stream listener ready");

    loop {
        let (mut stream, peer) = listener.accept().await?;
        info!(peer = %peer, "Video stream connected");

        tokio::spawn(async move {
            if let Err(e) = receive_video_stream(&mut stream).await {
                warn!(error = %e, "Video stream ended");
            }
        });
    }
}

async fn receive_video_stream(
    stream: &mut tokio::net::TcpStream,
) -> anyhow::Result<()> {
    let mut header_buf = [0u8; 128];
    let mut payload_buf = BytesMut::new();

    // Try to initialize OpenH264 decoder
    let mut decoder = match openh264::decoder::Decoder::new() {
        Ok(d) => {
            info!("H.264 decoder initialized");
            Some(d)
        }
        Err(e) => {
            warn!(error = %e, "Failed to init H.264 decoder — frames will be logged only");
            None
        }
    };

    let mut frame_count: u64 = 0;
    let mut sps_pps: Option<Vec<u8>> = None;

    loop {
        // Read 128-byte header
        stream.read_exact(&mut header_buf).await?;
        let header = StreamPacketHeader::parse(&header_buf);

        match header.payload_type {
            0 => {
                // H.264 codec data (SPS/PPS)
                if header.payload_size > 0 {
                    payload_buf.resize(header.payload_size as usize, 0);
                    stream.read_exact(&mut payload_buf).await?;
                    info!(
                        size = header.payload_size,
                        "Received H.264 codec data (SPS/PPS)"
                    );
                    sps_pps = Some(payload_buf.to_vec());

                    // Feed SPS/PPS to decoder
                    if let Some(ref mut dec) = decoder {
                        let _ = dec.decode(&payload_buf);
                    }
                }
            }
            1 => {
                // H.264 video frame
                if header.payload_size > 0 {
                    payload_buf.resize(header.payload_size as usize, 0);
                    stream.read_exact(&mut payload_buf).await?;
                    frame_count += 1;

                    if frame_count % 30 == 1 {
                        debug!(
                            frame = frame_count,
                            size = header.payload_size,
                            "Video frame received"
                        );
                    }

                    // Decode H.264 frame
                    if let Some(ref mut dec) = decoder {
                        match dec.decode(&payload_buf) {
                            Ok(Some(yuv)) => {
                                let (w, h) = yuv.dimensions();
                                debug!(
                                    frame = frame_count,
                                    width = w,
                                    height = h,
                                    "Decoded frame"
                                );
                                // TODO: convert YUV→RGB and send to display
                            }
                            Ok(None) => {
                                // Decoder needs more data
                            }
                            Err(e) => {
                                if frame_count % 100 == 1 {
                                    warn!(error = %e, "H.264 decode error");
                                }
                            }
                        }
                    }
                }
            }
            2 => {
                // Heartbeat — no payload
                debug!("Heartbeat");
            }
            other => {
                // Unknown type — skip payload
                if header.payload_size > 0 {
                    payload_buf.resize(header.payload_size as usize, 0);
                    stream.read_exact(&mut payload_buf).await?;
                }
                debug!(payload_type = other, "Unknown packet type");
            }
        }
    }
}
