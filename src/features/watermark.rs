/// Watermark: overlay a custom image or text on recordings and streams.

#[derive(Clone, Debug)]
pub struct Watermark {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub position: WatermarkPosition,
    pub opacity: f32,
}

#[derive(Clone, Debug)]
pub enum WatermarkPosition {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
    Custom(u32, u32),
}

impl Watermark {
    /// Load watermark from a PNG file.
    pub fn from_png(path: &str, position: WatermarkPosition, opacity: f32) -> Result<Self, String> {
        let img = image::open(path).map_err(|e| format!("Failed to load watermark: {}", e))?;
        let rgba_img = img.to_rgba8();
        let (w, h) = rgba_img.dimensions();

        Ok(Self {
            rgba: rgba_img.into_raw(),
            width: w,
            height: h,
            position,
            opacity,
        })
    }

    /// Create a text watermark.
    pub fn from_text(text: &str, position: WatermarkPosition, opacity: f32) -> Self {
        let char_w = 6u32;
        let char_h = 10u32;
        let w = text.len() as u32 * char_w + 8;
        let h = char_h + 8;
        let mut rgba = vec![0u8; (w * h * 4) as usize];

        // Semi-transparent dark background
        for i in 0..(w * h) as usize {
            rgba[i * 4 + 3] = 128;
        }

        // White text (simple blocks)
        for (i, _ch) in text.chars().enumerate() {
            let cx = 4 + i as u32 * char_w;
            let cy = 4u32;
            for dy in 0..char_h.min(7) {
                for dx in 0..5u32 {
                    let px = cx + dx;
                    let py = cy + dy;
                    if px < w && py < h {
                        let idx = ((py * w + px) * 4) as usize;
                        rgba[idx] = 255;
                        rgba[idx + 1] = 255;
                        rgba[idx + 2] = 255;
                        rgba[idx + 3] = 200;
                    }
                }
            }
        }

        Self { rgba, width: w, height: h, position, opacity }
    }

    /// Composite watermark onto an RGBA frame buffer.
    pub fn apply(&self, frame_rgba: &mut [u8], frame_w: u32, frame_h: u32) {
        let (ox, oy) = match self.position {
            WatermarkPosition::TopLeft => (8, 8),
            WatermarkPosition::TopRight => (frame_w.saturating_sub(self.width + 8), 8),
            WatermarkPosition::BottomLeft => (8, frame_h.saturating_sub(self.height + 8)),
            WatermarkPosition::BottomRight => (
                frame_w.saturating_sub(self.width + 8),
                frame_h.saturating_sub(self.height + 8),
            ),
            WatermarkPosition::Center => (
                (frame_w.saturating_sub(self.width)) / 2,
                (frame_h.saturating_sub(self.height)) / 2,
            ),
            WatermarkPosition::Custom(x, y) => (x, y),
        };

        for wy in 0..self.height {
            for wx in 0..self.width {
                let fx = ox + wx;
                let fy = oy + wy;
                if fx >= frame_w || fy >= frame_h { continue; }

                let w_idx = ((wy * self.width + wx) * 4) as usize;
                let f_idx = ((fy * frame_w + fx) * 4) as usize;

                if w_idx + 3 >= self.rgba.len() || f_idx + 3 >= frame_rgba.len() { continue; }

                let alpha = (self.rgba[w_idx + 3] as f32 / 255.0) * self.opacity;
                if alpha < 0.01 { continue; }

                frame_rgba[f_idx] = blend(frame_rgba[f_idx], self.rgba[w_idx], alpha);
                frame_rgba[f_idx + 1] = blend(frame_rgba[f_idx + 1], self.rgba[w_idx + 1], alpha);
                frame_rgba[f_idx + 2] = blend(frame_rgba[f_idx + 2], self.rgba[w_idx + 2], alpha);
            }
        }
    }
}

fn blend(bg: u8, fg: u8, alpha: f32) -> u8 {
    ((bg as f32 * (1.0 - alpha)) + (fg as f32 * alpha)).clamp(0.0, 255.0) as u8
}
