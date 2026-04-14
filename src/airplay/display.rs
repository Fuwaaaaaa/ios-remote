use minifb::{Key, Window, WindowOptions};
use std::sync::mpsc;
use tracing::info;

const DEFAULT_WIDTH: usize = 1920;
const DEFAULT_HEIGHT: usize = 1080;

/// Run the display window on its own thread.
///
/// minifb requires running on the main thread or a dedicated OS thread
/// (not a tokio task). This function blocks the calling thread.
pub fn run_display(frame_rx: mpsc::Receiver<(u32, u32, Vec<u32>)>) {
    let mut window = Window::new(
        "ios-remote — AirPlay Mirror",
        DEFAULT_WIDTH,
        DEFAULT_HEIGHT,
        WindowOptions {
            resize: true,
            scale_mode: minifb::ScaleMode::AspectRatioStretch,
            ..WindowOptions::default()
        },
    )
    .expect("failed to create display window");

    // Cap at ~60fps
    window.set_target_fps(60);

    let mut buffer = vec![0u32; DEFAULT_WIDTH * DEFAULT_HEIGHT];
    let mut width = DEFAULT_WIDTH;
    let mut height = DEFAULT_HEIGHT;

    info!("Display window opened ({}x{})", width, height);

    while window.is_open() && !window.is_key_down(Key::Escape) {
        // Check for new frames (non-blocking)
        while let Ok((w, h, pixels)) = frame_rx.try_recv() {
            width = w as usize;
            height = h as usize;
            buffer = pixels;
        }

        window
            .update_with_buffer(&buffer, width, height)
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "Display update failed");
            });
    }

    info!("Display window closed");
}

/// Convert YUV420 planar to RGBA packed (u32 per pixel, 0x00RRGGBB).
pub fn yuv420_to_rgb_buffer(
    y_plane: &[u8],
    u_plane: &[u8],
    v_plane: &[u8],
    width: usize,
    height: usize,
    y_stride: usize,
    u_stride: usize,
    v_stride: usize,
) -> Vec<u32> {
    let mut buf = vec![0u32; width * height];

    for row in 0..height {
        for col in 0..width {
            let y_idx = row * y_stride + col;
            let uv_row = row / 2;
            let uv_col = col / 2;
            let u_idx = uv_row * u_stride + uv_col;
            let v_idx = uv_row * v_stride + uv_col;

            let y = y_plane[y_idx] as f32;
            let u = u_plane[u_idx] as f32 - 128.0;
            let v = v_plane[v_idx] as f32 - 128.0;

            let r = (y + 1.402 * v).clamp(0.0, 255.0) as u32;
            let g = (y - 0.344136 * u - 0.714136 * v).clamp(0.0, 255.0) as u32;
            let b = (y + 1.772 * u).clamp(0.0, 255.0) as u32;

            buf[row * width + col] = (r << 16) | (g << 8) | b;
        }
    }

    buf
}
