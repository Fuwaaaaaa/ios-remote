use crate::airplay::display::yuv420_to_rgba;
use crate::airplay::SharedState;
use crate::features::Frame;
use bytes::BytesMut;
use openh264::formats::YUVSource;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tracing::{debug, info, warn};

/// AirPlay mirroring stream packet header (128 bytes).
///
/// Ref: https://openairplay.github.io/airplay-spec/screen_mirroring/stream_packets.html
#[derive(Debug)]
struct StreamPacketHeader {
    payload_size: u32,
    /// 0 = H.264 codec data (SPS/PPS), 1 = video frame, 2 = heartbeat
    payload_type: u16,
    timestamp: u64,
}

impl StreamPacketHeader {
    fn parse(data: &[u8; 128]) -> Self {
        Self {
            payload_size: u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
            payload_type: u16::from_le_bytes([data[4], data[5]]),
            timestamp: u64::from_le_bytes([
                data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
            ]),
        }
    }
}

/// Listen for H.264 video from the iPhone and publish decoded frames.
pub async fn listen_video(port: u16, state: SharedState) -> anyhow::Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    info!(port, "Video stream listener ready");

    loop {
        let (mut stream, peer) = listener.accept().await?;
        info!(peer = %peer, "Video stream connected");

        let s = state.clone();
        tokio::spawn(async move {
            if let Err(e) = receive_stream(&mut stream, s).await {
                warn!(error = %e, "Video stream ended");
            }
        });
    }
}

async fn receive_stream(
    stream: &mut tokio::net::TcpStream,
    state: SharedState,
) -> anyhow::Result<()> {
    let mut header_buf = [0u8; 128];
    let mut payload_buf = BytesMut::new();

    let mut decoder = match openh264::decoder::Decoder::new() {
        Ok(d) => {
            info!("H.264 decoder initialized");
            Some(d)
        }
        Err(e) => {
            warn!(error = %e, "H.264 decoder init failed — raw mode only");
            None
        }
    };

    let frame_bus = {
        let s = state.lock().await;
        s.frame_bus.clone()
    };

    let mut frame_count: u64 = 0;
    let start = std::time::Instant::now();

    loop {
        stream.read_exact(&mut header_buf).await?;
        let header = StreamPacketHeader::parse(&header_buf);

        match header.payload_type {
            0 => {
                // SPS/PPS codec config
                if header.payload_size > 0 && header.payload_size < 10_000_000 {
                    payload_buf.resize(header.payload_size as usize, 0);
                    stream.read_exact(&mut payload_buf).await?;
                    info!(size = header.payload_size, "H.264 SPS/PPS received");

                    if let Some(ref mut dec) = decoder {
                        let _ = dec.decode(&payload_buf);
                    }
                }
            }
            1 => {
                // Video frame
                if header.payload_size > 0 && header.payload_size < 10_000_000 {
                    payload_buf.resize(header.payload_size as usize, 0);
                    stream.read_exact(&mut payload_buf).await?;
                    frame_count += 1;

                    let nalu_data = payload_buf.to_vec();

                    // Decode and publish
                    if let Some(ref mut dec) = decoder {
                        if let Ok(Some(yuv)) = dec.decode(&payload_buf) {
                            let (w, h) = yuv.dimensions();
                            let (ys, us, vs) = yuv.strides();
                            let rgba = yuv420_to_rgba(
                                yuv.y(),
                                yuv.u(),
                                yuv.v(),
                                w,
                                h,
                                ys,
                                us,
                                vs,
                            );

                            frame_bus.publish(Frame {
                                width: w as u32,
                                height: h as u32,
                                rgba,
                                timestamp_us: header.timestamp,
                                h264_nalu: Some(nalu_data),
                            });
                        }
                    } else {
                        // No decoder: publish frame with h264 data only (for recording)
                        frame_bus.publish(Frame {
                            width: 0,
                            height: 0,
                            rgba: vec![],
                            timestamp_us: header.timestamp,
                            h264_nalu: Some(nalu_data),
                        });
                    }

                    // Log FPS every 5 seconds
                    if frame_count % 150 == 0 {
                        let elapsed = start.elapsed().as_secs_f64();
                        let fps = frame_count as f64 / elapsed;
                        info!(frames = frame_count, fps = format!("{:.1}", fps), "Stream stats");
                    }
                }
            }
            2 => {
                debug!("Heartbeat");
            }
            _ => {
                if header.payload_size > 0 && header.payload_size < 10_000_000 {
                    payload_buf.resize(header.payload_size as usize, 0);
                    stream.read_exact(&mut payload_buf).await?;
                }
            }
        }
    }
}
