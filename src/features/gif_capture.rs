use super::Frame;
use chrono::Local;
use std::collections::VecDeque;
use std::fs::File;
use tracing::info;

/// GIF capture: save the last N seconds as an animated GIF.
///
/// Maintains a rolling buffer of recent frames and exports them
/// as an animated GIF on demand.

pub struct GifCapture {
    /// Rolling buffer of recent frames (RGBA downscaled).
    buffer: VecDeque<GifFrame>,
    /// Max buffer duration in seconds.
    max_seconds: u32,
    /// Target FPS for GIF (lower = smaller file).
    gif_fps: u32,
    /// Downscale factor (2 = half resolution).
    downscale: u32,
}

struct GifFrame {
    rgba: Vec<u8>,
    width: u16,
    height: u16,
}

impl GifCapture {
    pub fn new(max_seconds: u32, gif_fps: u32) -> Self {
        Self {
            buffer: VecDeque::new(),
            max_seconds,
            gif_fps,
            downscale: 2,
        }
    }

    /// Add a frame to the rolling buffer (called every Nth source frame).
    pub fn push_frame(&mut self, frame: &Frame) {
        let dw = (frame.width / self.downscale) as u16;
        let dh = (frame.height / self.downscale) as u16;

        let downscaled = downscale_rgba(&frame.rgba, frame.width, frame.height, self.downscale);

        self.buffer.push_back(GifFrame {
            rgba: downscaled,
            width: dw,
            height: dh,
        });

        // Trim to max duration
        let max_frames = (self.max_seconds * self.gif_fps) as usize;
        while self.buffer.len() > max_frames {
            self.buffer.pop_front();
        }
    }

    /// Export the buffer as an animated GIF.
    pub fn save(&self) -> Result<String, String> {
        if self.buffer.is_empty() {
            return Err("No frames in GIF buffer".to_string());
        }

        let dir = "gifs";
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;

        let filename = format!("{}/gif_{}.gif", dir, Local::now().format("%Y%m%d_%H%M%S"));
        let file = File::create(&filename).map_err(|e| e.to_string())?;

        let first = &self.buffer[0];
        let mut encoder = gif::Encoder::new(file, first.width, first.height, &[])
            .map_err(|e| e.to_string())?;

        encoder.set_repeat(gif::Repeat::Infinite).map_err(|e| e.to_string())?;

        let delay = (100 / self.gif_fps.max(1)) as u16; // centiseconds per frame

        for gf in &self.buffer {
            // Convert RGBA to indexed color (simple quantization)
            let rgb: Vec<u8> = gf.rgba.chunks(4).flat_map(|px| &px[..3]).copied().collect();

            let mut gif_frame = gif::Frame::from_rgb(gf.width, gf.height, &rgb);
            gif_frame.delay = delay;

            encoder.write_frame(&gif_frame).map_err(|e| e.to_string())?;
        }

        info!(
            file = %filename,
            frames = self.buffer.len(),
            "GIF saved"
        );

        Ok(filename)
    }
}

fn downscale_rgba(rgba: &[u8], w: u32, h: u32, factor: u32) -> Vec<u8> {
    let nw = w / factor;
    let nh = h / factor;
    let mut out = vec![0u8; (nw * nh * 4) as usize];

    for y in 0..nh {
        for x in 0..nw {
            let sx = x * factor;
            let sy = y * factor;
            let src_idx = ((sy * w + sx) * 4) as usize;
            let dst_idx = ((y * nw + x) * 4) as usize;
            if src_idx + 3 < rgba.len() && dst_idx + 3 < out.len() {
                out[dst_idx..dst_idx + 4].copy_from_slice(&rgba[src_idx..src_idx + 4]);
            }
        }
    }

    out
}
