use super::Frame;
use chrono::Local;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn};

/// Timelapse: capture one frame every N seconds for long recordings.
///
/// Produces a sequence of PNGs that can be combined into a video:
///   ffmpeg -framerate 30 -i timelapse/frame_%06d.png -c:v libx264 timelapse.mp4
pub async fn run_timelapse(mut rx: broadcast::Receiver<Arc<Frame>>, interval_secs: u64) {
    let dir = format!("timelapse/{}", Local::now().format("%Y%m%d_%H%M%S"));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        warn!(error = %e, "Failed to create timelapse directory");
        return;
    }

    info!(dir = %dir, interval = interval_secs, "Timelapse started");
    let mut frame_num = 0u64;

    loop {
        // Wait for interval
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;

        // Get latest frame; skip silently if the channel is empty or lagged.
        if let Ok(frame) = rx.try_recv() {
            let path = format!("{}/frame_{:06}.png", dir, frame_num);
            if let Some(img) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
                frame.width,
                frame.height,
                frame.rgba.to_vec(),
            ) {
                let _ = img.save(&path);
                frame_num += 1;
                if frame_num.is_multiple_of(10) {
                    info!(frames = frame_num, "Timelapse progress");
                }
            }
        }

        // Drain extra frames to stay current
        while rx.try_recv().is_ok() {}
    }
}

/// Export all frames from the FrameBus as a PNG sequence.
pub async fn export_frame_sequence(mut rx: broadcast::Receiver<Arc<Frame>>, max_frames: u64) {
    let dir = format!("frames/{}", Local::now().format("%Y%m%d_%H%M%S"));
    let _ = std::fs::create_dir_all(&dir);
    info!(dir = %dir, max = max_frames, "Frame sequence export started");

    let mut count = 0u64;
    loop {
        match rx.recv().await {
            Ok(frame) => {
                let path = format!("{}/frame_{:06}.png", dir, count);
                if let Some(img) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
                    frame.width,
                    frame.height,
                    frame.rgba.to_vec(),
                ) {
                    let _ = img.save(&path);
                }
                count += 1;
                if count >= max_frames {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {}
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
    info!(frames = count, "Frame sequence export complete");
}
