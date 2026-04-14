use super::Frame;
use chrono::Local;
use image::{ImageBuffer, Rgba};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn};

/// Screenshot service: saves the latest frame as PNG on demand.
///
/// Listens for frames and keeps the latest. When triggered (via the display
/// window's Ctrl+S hotkey or an API call), saves to ./screenshots/.
pub async fn run(mut rx: broadcast::Receiver<Arc<Frame>>) {
    let dir = "screenshots";
    if let Err(e) = std::fs::create_dir_all(dir) {
        warn!(error = %e, "Failed to create screenshots directory");
        return;
    }

    // Just keep consuming frames — actual save is triggered by save_latest()
    loop {
        match rx.recv().await {
            Ok(_) => {} // latest frame is tracked by FrameBus
            Err(broadcast::error::RecvError::Lagged(_)) => {}
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

/// Save a frame as PNG to ./screenshots/.
pub fn save_frame(frame: &Frame) -> Result<String, String> {
    let dir = "screenshots";
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;

    let filename = format!(
        "{}/ss_{}.png",
        dir,
        Local::now().format("%Y%m%d_%H%M%S_%3f")
    );

    let img: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_raw(
        frame.width,
        frame.height,
        frame.rgba.clone(),
    )
    .ok_or("Failed to create image buffer")?;

    img.save(&filename).map_err(|e| e.to_string())?;

    info!(file = %filename, "Screenshot saved");
    Ok(filename)
}
