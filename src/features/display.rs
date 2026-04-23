use crate::features::{Frame, screenshot};
use minifb::{Key, Window, WindowOptions};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

/// Run the display window on a dedicated OS thread.
///
/// Features:
///   - Aspect-ratio-preserving letterbox
///   - Always-on-top (PiP mode)
///   - Hotkeys: S = screenshot, F = fullscreen toggle, Q/Esc = quit
pub fn run_display(mut frame_rx: broadcast::Receiver<Arc<Frame>>, pip_mode: bool) {
    let init_w = 960;
    let init_h = 540;

    let opts = WindowOptions {
        resize: true,
        scale_mode: minifb::ScaleMode::AspectRatioStretch,
        topmost: pip_mode,
        ..WindowOptions::default()
    };

    let title = if pip_mode {
        "ios-remote [PiP]"
    } else {
        "ios-remote — USB Mirror"
    };

    let mut window = match Window::new(title, init_w, init_h, opts) {
        Ok(w) => w,
        Err(e) => {
            tracing::error!(error = %e, "Failed to create window");
            return;
        }
    };

    window.set_target_fps(60);
    info!(pip = pip_mode, "Display window opened");

    let mut buffer: Vec<u32> = vec![0x00222222; init_w * init_h]; // dark gray bg
    let mut width = init_w;
    let mut height = init_h;
    let mut latest_frame: Option<Arc<Frame>> = None;

    while window.is_open() && !window.is_key_down(Key::Escape) && !window.is_key_down(Key::Q) {
        // Drain all pending frames, keep the latest. H.264-only frames
        // (rgba empty, h264_nalu set) are produced by the encoder loopback and
        // must be ignored here or the window goes black.
        while let Ok(frame) = frame_rx.try_recv() {
            if frame.rgba.is_empty() {
                continue;
            }
            width = frame.width as usize;
            height = frame.height as usize;
            buffer = rgba_to_rgb32(&frame.rgba, width, height);
            latest_frame = Some(frame);
        }

        // Hotkeys
        if window.is_key_released(Key::S)
            && let Some(ref frame) = latest_frame
        {
            match screenshot::save_frame(frame) {
                Ok(path) => info!(file = %path, "Screenshot saved (hotkey)"),
                Err(e) => tracing::warn!(error = %e, "Screenshot failed"),
            }
        }

        window
            .update_with_buffer(&buffer, width, height)
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "Display update failed");
            });
    }

    info!("Display window closed");
}

/// Convert RGBA [u8] to RGB32 [u32] for minifb (0x00RRGGBB).
fn rgba_to_rgb32(rgba: &[u8], width: usize, height: usize) -> Vec<u32> {
    let pixel_count = width * height;
    let mut buf = Vec::with_capacity(pixel_count);
    for chunk in rgba.chunks_exact(4).take(pixel_count) {
        let r = chunk[0] as u32;
        let g = chunk[1] as u32;
        let b = chunk[2] as u32;
        buf.push((r << 16) | (g << 8) | b);
    }
    buf.resize(pixel_count, 0);
    buf
}

/// Convert YUV420 planar to RGBA packed.
pub fn yuv420_to_rgba(
    y_plane: &[u8],
    u_plane: &[u8],
    v_plane: &[u8],
    width: usize,
    height: usize,
    y_stride: usize,
    u_stride: usize,
    v_stride: usize,
) -> Vec<u8> {
    let mut rgba = vec![255u8; width * height * 4]; // alpha = 255

    for row in 0..height {
        for col in 0..width {
            let y_idx = row * y_stride + col;
            let uv_row = row / 2;
            let uv_col = col / 2;
            let u_idx = uv_row * u_stride + uv_col;
            let v_idx = uv_row * v_stride + uv_col;

            let y = y_plane.get(y_idx).copied().unwrap_or(0) as f32;
            let u = u_plane.get(u_idx).copied().unwrap_or(128) as f32 - 128.0;
            let v = v_plane.get(v_idx).copied().unwrap_or(128) as f32 - 128.0;

            let r = (y + 1.402 * v).clamp(0.0, 255.0) as u8;
            let g = (y - 0.344136 * u - 0.714136 * v).clamp(0.0, 255.0) as u8;
            let b = (y + 1.772 * u).clamp(0.0, 255.0) as u8;

            let out_idx = (row * width + col) * 4;
            rgba[out_idx] = r;
            rgba[out_idx + 1] = g;
            rgba[out_idx + 2] = b;
            // rgba[out_idx + 3] = 255 already set
        }
    }

    rgba
}
