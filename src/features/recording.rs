use super::Frame;
use chrono::Local;
use std::fs;
use std::io::Write;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn};

/// Record H.264 NAL units to a raw .h264 file.
///
/// We save raw NALUs instead of muxing to MP4 in real-time for simplicity
/// and zero-overhead. Convert to MP4 afterwards with:
///   ffmpeg -i recording.h264 -c copy recording.mp4
pub async fn run(mut rx: broadcast::Receiver<Arc<Frame>>) {
    let dir = "recordings";
    if let Err(e) = fs::create_dir_all(dir) {
        warn!(error = %e, "Failed to create recordings directory");
        return;
    }

    let filename = format!(
        "{}/rec_{}.h264",
        dir,
        Local::now().format("%Y%m%d_%H%M%S")
    );

    let mut file = match fs::File::create(&filename) {
        Ok(f) => f,
        Err(e) => {
            warn!(error = %e, "Failed to create recording file");
            return;
        }
    };

    info!(file = %filename, "Recording started");
    let mut frame_count: u64 = 0;

    loop {
        match rx.recv().await {
            Ok(frame) => {
                if let Some(ref nalu) = frame.h264_nalu {
                    // Write start code + NALU
                    let _ = file.write_all(&[0x00, 0x00, 0x00, 0x01]);
                    let _ = file.write_all(nalu);
                    frame_count += 1;

                    if frame_count % 300 == 0 {
                        info!(frames = frame_count, "Recording in progress");
                    }
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!(dropped = n, "Recorder falling behind — dropped frames");
            }
            Err(broadcast::error::RecvError::Closed) => {
                break;
            }
        }
    }

    info!(frames = frame_count, file = %filename, "Recording stopped");
}
