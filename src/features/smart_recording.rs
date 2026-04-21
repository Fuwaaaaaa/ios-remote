use super::{frame_analysis, Frame};
use chrono::Local;
use std::io::Write;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn};

/// Motion-triggered recording: only record when the screen changes.
///
/// Saves H.264 NALUs only when motion is detected above a threshold.
/// Dramatically reduces file size for mostly-static screens.
pub async fn motion_recording(
    mut rx: broadcast::Receiver<Arc<Frame>>,
    motion_threshold: f64,
) {
    let dir = "recordings";
    let _ = std::fs::create_dir_all(dir);

    let filename = format!("{}/motion_{}.h264", dir, Local::now().format("%Y%m%d_%H%M%S"));
    let mut file = match std::fs::File::create(&filename) {
        Ok(f) => f,
        Err(e) => { warn!(error = %e, "Failed to create motion recording file"); return; }
    };

    info!(file = %filename, threshold = motion_threshold, "Motion recording started");

    let mut prev_frame: Option<Arc<Frame>> = None;
    let mut recording_frames = 0u64;
    let mut skipped_frames = 0u64;

    loop {
        match rx.recv().await {
            Ok(frame) => {
                let should_record = match &prev_frame {
                    Some(prev) => {
                        let score = frame_analysis::motion_score(prev, &frame);
                        score > motion_threshold
                    }
                    None => true, // Always record first frame
                };

                if should_record {
                    if let Some(ref nalu) = frame.h264_nalu {
                        let _ = file.write_all(&[0x00, 0x00, 0x00, 0x01]);
                        let _ = file.write_all(nalu);
                        recording_frames += 1;
                    }
                } else {
                    skipped_frames += 1;
                }

                if (recording_frames + skipped_frames).is_multiple_of(300) {
                    info!(
                        recorded = recording_frames,
                        skipped = skipped_frames,
                        "Motion recording stats"
                    );
                }

                prev_frame = Some(frame);
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!(dropped = n, "Motion recorder lagging");
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

/// Scheduled recording: record during a specific time window.
pub async fn scheduled_recording(
    rx: broadcast::Receiver<Arc<Frame>>,
    start_time: chrono::NaiveTime,
    end_time: chrono::NaiveTime,
) {
    info!(start = %start_time, end = %end_time, "Scheduled recording configured");

    loop {
        let now = Local::now().time();
        if now >= start_time && now <= end_time {
            info!("Scheduled recording: active window, starting...");
            super::recording::run(rx).await;
            return;
        }

        // Wait 10 seconds before checking again
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}

/// Screenshot comparison: diff two frames side by side.
pub fn compare_frames(a: &Frame, b: &Frame) -> Frame {
    let w = a.width.max(b.width);
    let h = a.height;

    // Side by side: frame A | divider | frame B
    let total_w = w * 2 + 4; // 4px divider
    let mut rgba = vec![0u8; (total_w * h * 4) as usize];

    // Copy frame A on the left
    for y in 0..a.height.min(h) {
        for x in 0..a.width.min(w) {
            let src = ((y * a.width + x) * 4) as usize;
            let dst = ((y * total_w + x) * 4) as usize;
            if src + 3 < a.rgba.len() && dst + 3 < rgba.len() {
                rgba[dst..dst + 4].copy_from_slice(&a.rgba[src..src + 4]);
            }
        }
    }

    // Red divider
    for y in 0..h {
        for dx in 0..4u32 {
            let x = w + dx;
            let idx = ((y * total_w + x) * 4) as usize;
            if idx + 2 < rgba.len() {
                rgba[idx] = 255;
                rgba[idx + 1] = 0;
                rgba[idx + 2] = 0;
                rgba[idx + 3] = 255;
            }
        }
    }

    // Copy frame B on the right
    for y in 0..b.height.min(h) {
        for x in 0..b.width.min(w) {
            let src = ((y * b.width + x) * 4) as usize;
            let dst = ((y * total_w + (w + 4 + x)) * 4) as usize;
            if src + 3 < b.rgba.len() && dst + 3 < rgba.len() {
                rgba[dst..dst + 4].copy_from_slice(&b.rgba[src..src + 4]);
            }
        }
    }

    Frame {
        width: total_w,
        height: h,
        rgba,
        timestamp_us: 0,
        h264_nalu: None,
    }
}
