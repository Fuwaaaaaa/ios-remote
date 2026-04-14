use super::{Frame, FrameBus};
use crate::features::screenshot;
use chrono::Local;
use std::sync::Arc;
use tracing::info;

/// Monitor for notification banners and auto-capture them.
///
/// Runs as a background task, comparing consecutive frames to detect
/// when a notification banner appears at the top of the iPhone screen.
pub async fn run(bus: FrameBus) {
    let mut rx = bus.subscribe();
    let mut prev_frame: Option<Arc<Frame>> = None;
    let mut cooldown = 0u32; // avoid capturing the same notification multiple times

    loop {
        match rx.recv().await {
            Ok(frame) => {
                if cooldown > 0 {
                    cooldown -= 1;
                    prev_frame = Some(frame);
                    continue;
                }

                if let Some(ref prev) = prev_frame {
                    if super::frame_analysis::detect_notification_banner(prev, &frame) {
                        info!("Notification detected — auto-capturing");

                        let dir = "notifications";
                        let _ = std::fs::create_dir_all(dir);
                        let filename = format!(
                            "{}/notif_{}.png",
                            dir,
                            Local::now().format("%Y%m%d_%H%M%S_%3f")
                        );

                        // Crop top 15% as the notification region
                        let crop = crop_top(&frame, 0.15);
                        if let Err(e) = screenshot::save_frame(&crop) {
                            tracing::warn!(error = %e, "Failed to save notification capture");
                        }

                        cooldown = 30; // skip ~1 second of frames
                    }
                }

                prev_frame = Some(frame);
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}

fn crop_top(frame: &Frame, fraction: f64) -> Frame {
    let crop_h = ((frame.height as f64) * fraction) as u32;
    let w = frame.width;
    let stride = (w * 4) as usize;
    let cropped_pixels = &frame.rgba[..stride * crop_h as usize];

    Frame {
        width: w,
        height: crop_h,
        rgba: cropped_pixels.to_vec(),
        timestamp_us: frame.timestamp_us,
        h264_nalu: None,
    }
}
